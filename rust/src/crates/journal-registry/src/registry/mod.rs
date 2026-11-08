//! Journal file registry with monitoring and metadata tracking
//!
//! This module provides the complete infrastructure for tracking journal files,
//! including file system monitoring, metadata management, and file collection.

pub mod error;
pub use error::RegistryError;

use crate::registry::error::Result;
use crate::repository::{Repository as BaseRepository, scan_journal_files};
use crate::{File, FileInfo, TimeRange};
use journal_common::Seconds;
use journal_common::collections::{HashMap, HashSet};
use notify::{
    Event,
    event::{EventKind, ModifyKind, RenameMode},
};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, trace, warn};

mod monitor;
pub use monitor::Monitor;

// ============================================================================
// Repository with Metadata
// ============================================================================

/// Repository that tracks journal files with metadata
///
/// This wraps the base repository and automatically maintains time range metadata
/// for each file. Metadata starts as Unknown and can be updated when computed.
struct Repository {
    base: BaseRepository,
    file_metadata: HashMap<File, FileInfo>,
}

impl Repository {
    /// Create a new empty repository
    fn new() -> Self {
        Self {
            base: BaseRepository::default(),
            file_metadata: HashMap::default(),
        }
    }

    /// Insert a file to the repository
    fn insert(&mut self, file: File) -> Result<()> {
        let file_info = FileInfo {
            file: file.clone(),
            time_range: TimeRange::Unknown,
        };

        self.base.insert(file.clone())?;
        self.file_metadata.insert(file, file_info);

        Ok(())
    }

    /// Remove a file from the repository
    fn remove(&mut self, file: &File) -> Result<()> {
        self.base.remove(file)?;
        self.file_metadata.remove(file);
        Ok(())
    }

    /// Keep only the files for which `keep` returns true
    fn retain_files(&mut self, mut keep: impl FnMut(&File) -> bool) {
        self.base.retain_files(&mut keep);
        self.file_metadata.retain(|file, _| keep(file));
    }

    /// Find files in a time range
    ///
    /// Uses indexed metadata when available to filter out files that don't
    /// overlap with the requested time range. Falls back to base repository
    /// logic for files with unknown time ranges.
    fn find_files_in_range(&self, start: Seconds, end: Seconds) -> Vec<FileInfo> {
        let files: Vec<File> = self.base.find_files_in_range(start, end);

        files
            .into_iter()
            .filter_map(|file| {
                let file_info =
                    self.file_metadata
                        .get(&file)
                        .cloned()
                        .unwrap_or_else(|| FileInfo {
                            file: file.clone(),
                            time_range: TimeRange::Unknown,
                        });

                // Filter based on time range metadata if available
                let include = match file_info.time_range {
                    TimeRange::Unknown => {
                        // Don't know the time range yet, include it to be safe
                        true
                    }
                    TimeRange::Active { end: _file_end, .. } => {
                        // We could have the following check: file_end >= start
                        // However, imagine someone panning outside of the
                        // active's file time range and then going back to
                        // `now` with a small histogram time-range...
                        true
                    }
                    TimeRange::Bounded {
                        start: file_start,
                        end: file_end,
                        ..
                    } => {
                        // Archived file: check for exact overlap
                        // File range [file_start, file_end) overlaps with [start, end) if:
                        // file_start < end && file_end > start
                        file_start.0 < end.0 && file_end.0 > start.0
                    }
                };

                if include { Some(file_info) } else { None }
            })
            .collect()
    }

    /// Update time range metadata for a file
    fn update_file_info(&mut self, file_info: FileInfo) {
        let file = file_info.file.clone();
        self.file_metadata.insert(file, file_info);
    }
}

impl Default for Repository {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Registry
// ============================================================================

/// Journal file parsing needs absolute paths, and notify reports paths in the
/// form they were watched. Resolve once, lexically, and drop trailing and
/// repeated separators so each directory has a single key.
fn absolute_directory(path: &str) -> Result<String> {
    std::path::absolute(path)?
        .components()
        .collect::<PathBuf>()
        .into_os_string()
        .into_string()
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "directory path is not valid UTF-8 once made absolute",
            )
            .into()
        })
}

/// Watched directories that need a notify watch: those not inside another
/// watched directory, whose recursive watch already covers them.
fn outermost(watched: &HashSet<String>) -> HashSet<String> {
    watched
        .iter()
        .filter(|dir| {
            !watched
                .iter()
                .any(|other| other != *dir && Path::new(dir.as_str()).starts_with(other))
        })
        .cloned()
        .collect()
}

/// Internal state for Registry
struct RegistryInner {
    repository: Repository,
    /// Directories callers asked to watch, as normalized absolute paths
    watched_directories: HashSet<String>,
    /// The watched directories that hold a notify watch. notify keys watches
    /// by path and drops every watch below a removed one, so overlapping
    /// watches are never handed to it.
    armed_directories: HashSet<String>,
    monitor: Monitor,
}

impl RegistryInner {
    /// Move notify watches to `target`, returning the directories newly armed.
    ///
    /// Removes before adding, so a nested watch is gone before its parent's
    /// recursive watch is added. A failed removal means notify's state is
    /// unreliable, so the watcher is rebuilt. On failure the previous watches
    /// are restored.
    fn rearm(&mut self, target: HashSet<String>) -> Result<Vec<String>> {
        let to_remove: Vec<String> = self
            .armed_directories
            .difference(&target)
            .cloned()
            .collect();
        let to_add: Vec<String> = target
            .difference(&self.armed_directories)
            .cloned()
            .collect();

        let previous = self.armed_directories.clone();

        for dir in &to_remove {
            if let Err(e) = self.disarm(dir) {
                warn!(
                    "removing notify watch for {} failed ({}); rebuilding the watcher",
                    dir, e
                );
                return match self.rebuild(&target) {
                    Ok(()) => Ok(target.into_iter().collect()),
                    Err(e) => {
                        self.restore(&previous);
                        Err(e)
                    }
                };
            }
        }
        for dir in &to_add {
            if let Err(e) = self.monitor.watch_directory(dir) {
                self.restore(&previous);
                return Err(e);
            }
        }

        self.armed_directories = target;
        Ok(to_add)
    }

    /// Replace all notify watches with a fresh watcher armed for `target`.
    /// Every directory in it must then be reconciled.
    ///
    /// One failing directory does not cost the others their watch: all are
    /// attempted, `armed_directories` records exactly those that succeeded,
    /// and the first error is returned.
    fn rebuild(&mut self, target: &HashSet<String>) -> Result<()> {
        self.armed_directories.clear();
        self.monitor.reset()?;
        let mut first_error = None;
        for dir in target {
            match self.monitor.watch_directory(dir) {
                Ok(()) => {
                    self.armed_directories.insert(dir.clone());
                }
                Err(e) => {
                    warn!("failed to arm notify watch for {}: {}", dir, e);
                    first_error.get_or_insert(e);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Return to the `previous` notify watches after a failed `rearm`, and
    /// rescan them for events lost meanwhile. Best effort, because the
    /// original error is the one returned; a directory that cannot be re-armed
    /// is logged and stays out of `armed_directories`.
    fn restore(&mut self, previous: &HashSet<String>) {
        if let Err(e) = self.rebuild(previous) {
            error!("failed to restore notify watches: {}", e);
        }
        for dir in self.armed_directories.clone() {
            self.reconcile_logged(&dir);
        }
    }

    /// Remove a notify watch; one notify already dropped counts as removed.
    fn disarm(&mut self, dir: &str) -> Result<()> {
        match self.monitor.unwatch_directory(dir) {
            Err(RegistryError::Notify(e)) if matches!(e.kind, notify::ErrorKind::WatchNotFound) => {
                warn!("notify watch for {} was already gone", dir);
                Ok(())
            }
            result => result,
        }
    }

    /// Make the registered files under `dir` match the disk. Needed wherever a
    /// notify watch changed hands, because events in between are lost.
    fn reconcile(&mut self, dir: &str) -> Result<()> {
        let files = scan_journal_files(dir)?;
        info!("found {} journal files in {}", files.len(), dir);

        let found: HashSet<&str> = files.iter().map(File::path).collect();
        self.repository
            .retain_files(|file| !file.as_path().starts_with(dir) || found.contains(file.path()));
        drop(found);

        for file in files {
            // Inserting resets the time range, and indexing will not revisit
            // a file it has already covered.
            if self.repository.file_metadata.contains_key(&file) {
                continue;
            }
            trace!("adding file to repository: {:?}", file.path());
            if let Err(e) = self.repository.insert(file) {
                error!("failed to insert file into repository: {}", e);
            }
        }
        Ok(())
    }

    fn reconcile_logged(&mut self, dir: &str) {
        if let Err(e) = self.reconcile(dir) {
            error!("failed to rescan {}: {}", dir, e);
        }
    }
}

/// Coordinates file monitoring and repository management (thread-safe)
#[derive(Clone)]
pub struct Registry {
    inner: Arc<RwLock<RegistryInner>>,
}

impl Registry {
    /// Create a new registry with the given monitor
    pub fn new(monitor: Monitor) -> Self {
        let inner = RegistryInner {
            repository: Repository::new(),
            watched_directories: HashSet::default(),
            armed_directories: HashSet::default(),
            monitor,
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    /// Watch a directory for journal files
    ///
    /// Starts monitoring, then scans the directory to discover existing files.
    /// A relative directory is resolved against the working directory at call
    /// time, lexically (symlinks and `..` are kept); an empty one is refused.
    /// Trailing separators are ignored. Directories may overlap: a directory
    /// inside or around one already watched is supported. On error the
    /// registry is left unchanged, except that a watch which could not be
    /// restored is logged and stays disarmed. After success, a failed rescan of
    /// a directory whose watch changed hands is logged, not returned, because
    /// the requested watch is in place.
    pub fn watch_directory(&self, path: &str) -> Result<()> {
        let dir = absolute_directory(path)?;
        let mut inner = self.inner.write();

        if inner.watched_directories.contains(&dir) {
            warn!("Directory {} is already being watched", dir);
            return Ok(());
        }

        let mut watched = inner.watched_directories.clone();
        watched.insert(dir.clone());
        let previous = inner.armed_directories.clone();
        let armed = inner.rearm(outermost(&watched))?;

        info!("scanning directory: {}", dir);
        if let Err(e) = inner.reconcile(&dir) {
            if inner.armed_directories != previous {
                inner.restore(&previous);
            }
            return Err(e);
        }
        inner.watched_directories = watched;
        // A rebuilt watcher re-armed other directories as well.
        for other in armed.iter().filter(|other| **other != dir) {
            inner.reconcile_logged(other);
        }

        info!(
            "now watching directory: {} (total directories: {})",
            dir,
            inner.watched_directories.len()
        );
        Ok(())
    }

    /// Stop watching a directory and forget its files
    ///
    /// The directory is resolved as in [`Registry::watch_directory`]. Every
    /// registered file under it is forgotten unless another watched directory
    /// still covers it; files on disk are never touched. On error the registry
    /// is left unchanged, except that a watch which could not be restored is
    /// logged and stays disarmed. After success, a failed rescan of a directory
    /// whose watch changed hands is logged, not returned, because the requested
    /// unwatch is done.
    pub fn unwatch_directory(&self, path: &str) -> Result<()> {
        let dir = absolute_directory(path)?;
        let mut inner = self.inner.write();

        if !inner.watched_directories.contains(&dir) {
            warn!("directory {} is not being watched", dir);
            return Ok(());
        }

        let mut watched = inner.watched_directories.clone();
        watched.remove(&dir);
        let rearmed = inner.rearm(outermost(&watched))?;

        inner.repository.retain_files(|file| {
            let path = file.as_path();
            !path.starts_with(&dir) || watched.iter().any(|other| path.starts_with(other))
        });
        inner.watched_directories = watched;
        for nested in &rearmed {
            inner.reconcile_logged(nested);
        }

        info!("stopped watching directory: {}", dir);
        Ok(())
    }

    /// Process a filesystem event from the monitor
    ///
    /// Handles creates, deletes, and renames. Call this for each event from the receiver.
    pub fn process_event(&self, event: Event) -> Result<()> {
        let mut inner = self.inner.write();

        match event.kind {
            EventKind::Create(_) => {
                for path in &event.paths {
                    Self::insert_event_path(&mut inner, path);
                }
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    Self::remove_event_path(&mut inner, path);
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                Self::process_rename_event(&mut inner, &event.paths);
            }
            EventKind::Modify(ModifyKind::Name(rename_mode)) => {
                info!(
                    "unhandled modify event: '{:?}', expecting rename event for newly archived file",
                    rename_mode
                );
            }

            // Ignore other events (content modifications, access, etc.)
            _ => {}
        }
        Ok(())
    }

    /// Events queued before an unwatch must not bring its files back.
    fn is_watched(inner: &RegistryInner, path: &Path) -> bool {
        let watched = inner
            .watched_directories
            .iter()
            .any(|dir| path.starts_with(dir));
        if !watched {
            trace!("ignoring event outside watched directories: {:?}", path);
        }
        watched
    }

    fn insert_event_path(inner: &mut RegistryInner, path: &Path) {
        if !Self::is_watched(inner, path) {
            return;
        }
        trace!("adding file to repository: {:?}", path);
        let Some(file) = File::from_path(path) else {
            warn!("path is not a valid journal file: {:?}", path);
            return;
        };
        if let Err(e) = inner.repository.insert(file) {
            error!("failed to insert file: {}", e);
        }
    }

    fn remove_event_path(inner: &mut RegistryInner, path: &Path) {
        trace!("removing file from repository: {:?}", path);
        let Some(file) = File::from_path(path) else {
            warn!("path is not a valid journal file: {:?}", path);
            return;
        };
        if let Err(e) = inner.repository.remove(&file) {
            error!("failed to remove file: {}", e);
        }
    }

    fn process_rename_event(inner: &mut RegistryInner, paths: &[PathBuf]) {
        let Some((old_path, new_path)) = paths.first().zip(paths.get(1)) else {
            error!("rename event with unexpected path count: {:#?}", paths);
            return;
        };
        info!("rename event: {:?} -> {:?}", old_path, new_path);
        Self::remove_renamed_old_path(inner, old_path);
        Self::insert_renamed_new_path(inner, new_path);
    }

    fn remove_renamed_old_path(inner: &mut RegistryInner, old_path: &Path) {
        let Some(old_file) = File::from_path(old_path) else {
            return;
        };
        info!("removing old file: {:?}", old_file.path());
        if let Err(e) = inner.repository.remove(&old_file) {
            error!("failed to remove old file: {}", e);
        }
    }

    fn insert_renamed_new_path(inner: &mut RegistryInner, new_path: &Path) {
        if !Self::is_watched(inner, new_path) {
            return;
        }
        let Some(new_file) = File::from_path(new_path) else {
            return;
        };
        info!("inserting new file: {:?}", new_file.path());
        if let Err(e) = inner.repository.insert(new_file) {
            error!("failed to insert new file: {}", e);
        }
    }

    /// Find files overlapping with a time range
    ///
    /// Uses indexed metadata to filter efficiently. Files with unknown or active time
    /// ranges are always included.
    pub fn find_files_in_range(&self, start: Seconds, end: Seconds) -> Result<Vec<FileInfo>> {
        let inner = self.inner.read();
        Ok(inner.repository.find_files_in_range(start, end))
    }

    /// Update time range metadata after indexing a file
    pub fn update_time_range(
        &self,
        file: &File,
        start_time: Seconds,
        end_time: Seconds,
        indexed_at: Seconds,
        online: bool,
    ) {
        let mut inner = self.inner.write();

        let time_range = if online {
            TimeRange::Active {
                start: start_time,
                end: end_time,
                indexed_at: indexed_at,
            }
        } else {
            TimeRange::Bounded {
                start: start_time,
                end: end_time,
                indexed_at: indexed_at,
            }
        };

        let file_info = FileInfo {
            file: file.clone(),
            time_range,
        };
        inner.repository.update_file_info(file_info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_arms_every_directory_it_can_and_returns_the_failure() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut good = HashSet::default();
        for i in 0..8 {
            let path = dir.path().join(format!("watch-{i}"));
            std::fs::create_dir(&path).unwrap();
            good.insert(path.to_str().unwrap().to_string());
        }
        let missing = dir.path().join("missing").to_str().unwrap().to_string();
        let mut target = good.clone();
        target.insert(missing);

        let (monitor, _events) = Monitor::new().unwrap();
        let mut inner = RegistryInner {
            repository: Repository::new(),
            watched_directories: HashSet::default(),
            armed_directories: HashSet::default(),
            monitor,
        };
        assert!(inner.rebuild(&target).is_err());
        assert_eq!(inner.armed_directories, good);
    }
}
