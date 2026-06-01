use crate::error::{Result, WriterError};
use crate::log::RetentionPolicy;
use journal_common::Microseconds;
use journal_core::JournalFile;
use journal_core::collections::HashMap;
use journal_core::file::{JournalState, Mmap};
use journal_registry::repository;
use journal_registry::repository::{File, Source};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[allow(unused_imports)]
use tracing::{error, info, instrument};

fn source_basename(source: &Source) -> String {
    match source {
        Source::System => "system".to_string(),
        Source::User(uid) => format!("user-{uid}"),
        Source::Remote(host) => format!("remote-{host}"),
        Source::Unknown(name) => name.clone(),
    }
}

fn create_strict_systemd_active_file(
    path: &PathBuf,
    source_name: &str,
) -> Option<repository::File> {
    repository::File::from_path(&path.join(format!("{source_name}.journal")))
}

// Helper function to create a File with archived status
fn create_chain_file(
    path: &PathBuf,
    source_name: &str,
    seqnum_id: Uuid,
    head_seqnum: u64,
    head_realtime: u64,
) -> Option<repository::File> {
    // Format the path using the same logic as journal_registry
    let filename = format!(
        "{}@{}-{:016x}-{:016x}.journal",
        source_name,
        seqnum_id.simple(),
        head_seqnum,
        head_realtime
    );

    repository::File::from_path(&path.join(filename))
}

/// Manages a directory of journal files with automatic cleanup.
///
/// Scans the directory for existing files, tracks their sizes, and enforces retention
/// policies. Typically not used directly - see [`JournalLog`](crate::JournalLog) instead.
#[derive(Debug)]
pub(super) struct OwnedChain {
    pub(super) path: PathBuf,
    pub(super) machine_id: Uuid,
    pub(super) source: Source,
    pub(super) source_name: String,

    pub(super) inner: repository::Chain,
    pub(super) file_sizes: HashMap<File, u64>,
    pub(super) total_size: u64,
}

pub(super) struct RetentionOutcome {
    pub(super) deleted_files: Vec<repository::File>,
    pub(super) error: Option<WriterError>,
}

struct TailState {
    state: u8,
    seqnum_id: Uuid,
    tail_seqnum: u64,
    tail_realtime: u64,
    tail_boot_id: Uuid,
    tail_monotonic: u64,
    head_realtime: u64,
}

fn read_u64_le(buf: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        buf.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn read_uuid(buf: &[u8], offset: usize) -> Option<Uuid> {
    Some(Uuid::from_bytes(
        buf.get(offset..offset + 16)?.try_into().ok()?,
    ))
}

fn tail_state_from_raw_header(file: &repository::File) -> Option<TailState> {
    const MIN_TAIL_HEADER_SIZE: usize = 208;
    let mut handle = std::fs::File::open(file.path()).ok()?;
    let mut buf = [0u8; MIN_TAIL_HEADER_SIZE];
    handle.read_exact(&mut buf).ok()?;
    if &buf[0..8] != b"LPKSHHRH" {
        return None;
    }

    Some(TailState {
        state: buf[16],
        seqnum_id: read_uuid(&buf, 72)?,
        tail_seqnum: read_u64_le(&buf, 160)?,
        tail_realtime: read_u64_le(&buf, 192)?,
        tail_boot_id: read_uuid(&buf, 56)?,
        tail_monotonic: read_u64_le(&buf, 200)?,
        head_realtime: read_u64_le(&buf, 184)?,
    })
}

impl OwnedChain {
    pub(super) fn new(path: PathBuf, machine_id: Uuid, source: Source) -> Result<Self> {
        #[cfg(debug_assertions)]
        {
            debug_assert!(path.exists() && path.is_dir());

            if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
                debug_assert_eq!(Ok(machine_id), Uuid::try_parse(filename));
            }
        }

        let source_name = source_basename(&source);
        let mut chain = Self {
            path,
            machine_id,
            source,
            source_name,
            inner: repository::Chain::default(),
            file_sizes: HashMap::default(),
            total_size: 0,
        };

        for entry in std::fs::read_dir(&chain.path)? {
            let Ok(file_path) = entry.map(|e| e.path()) else {
                continue;
            };

            let Some(file) = repository::File::from_path(&file_path) else {
                continue;
            };
            if file.origin().source != chain.source {
                continue;
            }

            let Some(size) = committed_journal_size(&file)
                .or_else(|| std::fs::metadata(file.path()).map(|m| m.len()).ok())
            else {
                continue;
            };

            chain.total_size += size;
            chain.file_sizes.insert(file.clone(), size);
            chain.inner.insert_file(file);
        }

        Ok(chain)
    }

    pub(super) fn tail_identity(&self, include_active: bool) -> Result<Option<(Uuid, u64)>> {
        Ok(self
            .tail_state(include_active)?
            .map(|tail| (tail.seqnum_id, tail.tail_seqnum)))
    }

    pub(super) fn online_chain_file(&self) -> Result<Option<repository::File>> {
        let mut selected: Option<(repository::File, u64, u64)> = None;
        for entry in std::fs::read_dir(&self.path)? {
            let Ok(entry) = entry else {
                continue;
            };
            let file_path = entry.path();
            let Some(file) = repository::File::from_path(&file_path) else {
                continue;
            };
            if file.origin().source != self.source || !file.is_archived() {
                continue;
            }

            let window_size = 4096;
            let candidate = match JournalFile::<Mmap>::open(&file, window_size) {
                Ok(jf) => {
                    let header = jf.journal_header_ref();
                    TailState {
                        state: header.state,
                        seqnum_id: Uuid::from_bytes(header.seqnum_id),
                        tail_seqnum: header.tail_entry_seqnum,
                        tail_realtime: header.tail_entry_realtime,
                        tail_boot_id: Uuid::from_bytes(header.tail_entry_boot_id),
                        tail_monotonic: header.tail_entry_monotonic,
                        head_realtime: header.head_entry_realtime,
                    }
                }
                Err(_) => {
                    let Some(raw_state) = tail_state_from_raw_header(&file) else {
                        continue;
                    };
                    raw_state
                }
            };
            if candidate.state != JournalState::Online as u8 {
                continue;
            }

            let replace = selected
                .as_ref()
                .is_none_or(|(_, tail_seqnum, head_realtime)| {
                    candidate.tail_seqnum > *tail_seqnum
                        || (candidate.tail_seqnum == *tail_seqnum
                            && candidate.head_realtime > *head_realtime)
                });
            if replace {
                selected = Some((file, candidate.tail_seqnum, candidate.head_realtime));
            }
        }

        Ok(selected.map(|(file, _, _)| file))
    }

    pub(super) fn tail_realtime(&self, include_active: bool) -> Result<Option<Microseconds>> {
        let Some(tail) = self.tail_state(include_active)? else {
            return Ok(None);
        };
        let realtime = tail.tail_realtime;
        if realtime == 0 {
            Ok(None)
        } else {
            Ok(Some(Microseconds::new(realtime)))
        }
    }

    pub(super) fn tail_monotonic_for_boot(
        &self,
        boot_id: Uuid,
        include_active: bool,
    ) -> Result<Option<u64>> {
        let Some(tail) = self.tail_state(include_active)? else {
            return Ok(None);
        };

        if tail.tail_boot_id != boot_id {
            return Ok(None);
        }

        let monotonic = tail.tail_monotonic;
        if monotonic == 0 {
            Ok(None)
        } else {
            Ok(Some(monotonic))
        }
    }

    fn tail_state(&self, include_active: bool) -> Result<Option<TailState>> {
        let mut selected = None;
        for entry in std::fs::read_dir(&self.path)? {
            let Ok(entry) = entry else {
                continue;
            };
            let file_path = entry.path();
            let Some(file) = repository::File::from_path(&file_path) else {
                continue;
            };
            if file.origin().source != self.source || (!include_active && !file.is_archived()) {
                continue;
            }

            let window_size = 4096;
            let candidate = match JournalFile::<Mmap>::open(&file, window_size) {
                Ok(jf) => {
                    let header = jf.journal_header_ref();
                    TailState {
                        state: header.state,
                        seqnum_id: Uuid::from_bytes(header.seqnum_id),
                        tail_seqnum: header.tail_entry_seqnum,
                        tail_realtime: header.tail_entry_realtime,
                        tail_boot_id: Uuid::from_bytes(header.tail_entry_boot_id),
                        tail_monotonic: header.tail_entry_monotonic,
                        head_realtime: header.head_entry_realtime,
                    }
                }
                Err(_) => {
                    let Some(raw_state) = tail_state_from_raw_header(&file) else {
                        continue;
                    };
                    raw_state
                }
            };
            let replace = selected.as_ref().is_none_or(|current: &TailState| {
                candidate.tail_seqnum > current.tail_seqnum
                    || (candidate.tail_seqnum == current.tail_seqnum
                        && candidate.head_realtime > current.head_realtime)
            });
            if replace {
                selected = Some(candidate);
            }
        }
        Ok(selected)
    }

    /// Registers a new journal file with the directory.
    pub(super) fn create_active_file(&mut self) -> Result<repository::File> {
        let Some(file) = create_strict_systemd_active_file(&self.path, &self.source_name) else {
            return Err(WriterError::FileCreation(format!(
                "failed to create journal file in {}",
                self.path.display()
            )));
        };
        self.inner.insert_file(file.clone());
        Ok(file)
    }

    pub(super) fn existing_active_file(&self) -> Option<repository::File> {
        let file = create_strict_systemd_active_file(&self.path, &self.source_name)?;
        Path::new(file.path()).exists().then_some(file)
    }

    pub(super) fn remove_tracked_file(&mut self, file: &repository::File) {
        if let Some(size) = self.file_sizes.remove(file) {
            self.total_size = self.total_size.saturating_sub(size);
        }
        self.inner.remove_file(file);
    }

    pub(super) fn dispose_replaceable_active_file(
        &mut self,
        file: &repository::File,
    ) -> Result<()> {
        self.dispose_file(file)
    }

    fn dispose_file(&mut self, file: &repository::File) -> Result<()> {
        let source = Path::new(file.path());
        if !source.exists() {
            self.remove_tracked_file(file);
            return Ok(());
        }

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let source_text = file.path();
        let stem = source_text.strip_suffix(".journal").unwrap_or(source_text);
        let mut target = PathBuf::from(format!(
            "{stem}@{:016x}-{:016x}.journal~",
            stamp as u64,
            Uuid::new_v4().as_u128() as u64
        ));
        while target.exists() {
            target = PathBuf::from(format!(
                "{stem}@{:016x}-{:016x}.journal~",
                stamp as u64,
                Uuid::new_v4().as_u128() as u64
            ));
        }
        match std::fs::rename(source, &target) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                self.remove_tracked_file(file);
                return Ok(());
            }
            Err(err) => return Err(err.into()),
        }
        sync_directory(&self.path)?;
        self.remove_tracked_file(file);
        Ok(())
    }

    pub(super) fn create_chain_file(
        &mut self,
        seqnum_id: Uuid,
        head_seqnum: u64,
        head_realtime: u64,
    ) -> Result<repository::File> {
        let Some(file) = create_chain_file(
            &self.path,
            &self.source_name,
            seqnum_id,
            head_seqnum,
            head_realtime,
        ) else {
            return Err(WriterError::FileCreation(format!(
                "failed to create journal file in {}",
                self.path.display()
            )));
        };
        self.inner.insert_file(file.clone());
        Ok(file)
    }

    pub(super) fn archive_existing_active_file(&mut self) -> Result<Option<repository::File>> {
        let Some(file) = self.inner.back().filter(|file| file.is_active()).cloned() else {
            return Ok(None);
        };

        if !Path::new(file.path()).exists() {
            self.file_sizes.remove(&file);
            self.inner.remove_file(&file);
            return Ok(None);
        }

        let (seqnum_id, head_seqnum, head_realtime, n_entries) = {
            let window_size = 4096;
            let jf = JournalFile::<Mmap>::open(&file, window_size)?;
            let header = jf.journal_header_ref();
            (
                Uuid::from_bytes(header.seqnum_id),
                header.head_entry_seqnum,
                header.head_entry_realtime,
                header.n_entries,
            )
        };

        if n_entries == 0 {
            return Ok(None);
        }

        self.archive_file(&file, seqnum_id, head_seqnum, head_realtime)
            .map(Some)
    }

    pub(super) fn archive_file(
        &mut self,
        file: &repository::File,
        seqnum_id: Uuid,
        head_seqnum: u64,
        head_realtime: u64,
    ) -> Result<repository::File> {
        let Some(archived) = create_chain_file(
            &self.path,
            &self.source_name,
            seqnum_id,
            head_seqnum,
            head_realtime,
        ) else {
            return Err(WriterError::FileCreation(format!(
                "failed to create archived journal file name in {}",
                self.path.display()
            )));
        };

        let renamed = file.path() != archived.path() && std::path::Path::new(file.path()).exists();
        if renamed {
            std::fs::rename(file.path(), archived.path())?;
            sync_directory(&self.path)?;
        }

        let size = self.file_sizes.remove(file).unwrap_or(0);
        self.inner.remove_file(file);
        self.file_sizes.insert(archived.clone(), size);
        self.inner.insert_file(archived.clone());
        Ok(archived)
    }

    /// Updates the tracked size of a file in the chain
    pub(super) fn update_file_size(&mut self, file: &File, new_size: u64) {
        let old_size = self.file_sizes.get(file).copied().unwrap_or(0);
        self.file_sizes.insert(file.clone(), new_size);
        self.total_size = self
            .total_size
            .saturating_sub(old_size)
            .saturating_add(new_size);
    }

    pub(super) fn refresh_retained_sizes<F>(&mut self, mut artifact_size: F) -> Result<()>
    where
        F: FnMut(&File) -> Result<u64>,
    {
        let files: Vec<_> = self.file_sizes.keys().cloned().collect();
        let mut total_size = 0u64;
        for file in files {
            let journal_size = committed_journal_size(&file)
                .or_else(|| std::fs::metadata(file.path()).map(|m| m.len()).ok())
                .unwrap_or(0);
            let size = journal_size.saturating_add(artifact_size(&file)?);
            self.file_sizes.insert(file, size);
            total_size = total_size.saturating_add(size);
        }
        self.total_size = total_size;
        Ok(())
    }

    /// Retains the files that satisfy retention policy limits.
    #[tracing::instrument(skip_all, fields(reason))]
    pub(super) fn retain(
        &mut self,
        retention_policy: &RetentionPolicy,
        protected_file: Option<&repository::File>,
    ) -> RetentionOutcome {
        let mut deleted_files = Vec::new();
        let mut error = None;

        // Remove by file count limit
        if let Some(max_files) = retention_policy.number_of_journal_files {
            while self.inner.len() > max_files {
                let reason = format!("num_files({}) > max_files({})", self.inner.len(), max_files);
                tracing::Span::current().record("reason", reason);
                match self.delete_oldest_file(protected_file) {
                    Ok(Some(file)) => deleted_files.push(file),
                    Ok(None) => break,
                    Err(err) => {
                        error = Some(err);
                        break;
                    }
                }
            }
        }

        // Remove by total size limit
        if error.is_none()
            && let Some(max_total_size) = retention_policy.size_of_journal_files
        {
            while self.total_size > max_total_size && !self.inner.is_empty() {
                let reason = format!(
                    "total_size({}) > max_size({})",
                    self.total_size, max_total_size
                );
                tracing::Span::current().record("reason", reason);
                match self.delete_oldest_file(protected_file) {
                    Ok(Some(file)) => deleted_files.push(file),
                    Ok(None) => break,
                    Err(err) => {
                        error = Some(err);
                        break;
                    }
                }
            }
        }

        // Remove by entry age limit
        if error.is_none()
            && let Some(max_entry_age) = retention_policy.duration_of_journal_files
        {
            let age_retention = self.delete_files_older_than(max_entry_age, protected_file);
            deleted_files.extend(age_retention.deleted_files);
            error = age_retention.error;
        }

        if error.is_none()
            && !deleted_files.is_empty()
            && let Err(err) = sync_directory(&self.path)
        {
            error = Some(err.into());
        }

        RetentionOutcome {
            deleted_files,
            error,
        }
    }

    /// Remove the oldest file
    #[tracing::instrument(skip_all)]
    fn delete_oldest_file(
        &mut self,
        protected_file: Option<&repository::File>,
    ) -> Result<Option<repository::File>> {
        let mut skipped = Vec::new();
        let file = loop {
            let Some(file) = self.inner.pop_front() else {
                for file in skipped {
                    self.inner.insert_file(file);
                }
                return Ok(None);
            };
            if protected_file.is_some_and(|protected| protected == &file) {
                skipped.push(file);
                continue;
            }
            break file;
        };

        for file in skipped {
            self.inner.insert_file(file);
        }

        info!("deleting {}", file.path());

        let file_size = self.file_sizes.get(&file).copied().unwrap_or(0);

        // Remove from filesystem
        match std::fs::remove_file(file.path()) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                info!("journal file {:?} was already removed", file.path());
            }
            Err(err) => {
                error!("failed to remove journal file {:?}: {}", file.path(), err);
                self.inner.insert_file(file);
                return Err(err.into());
            }
        }

        self.file_sizes.remove(&file);
        self.total_size = self.total_size.saturating_sub(file_size);
        Ok(Some(file))
    }

    /// Remove files older than the specified cutoff time
    #[tracing::instrument(skip(self))]
    fn delete_files_older_than(
        &mut self,
        max_entry_age: std::time::Duration,
        protected_file: Option<&repository::File>,
    ) -> RetentionOutcome {
        let cutoff_time = Microseconds::now()
            .get()
            .saturating_sub(max_entry_age.as_micros() as u64);
        let mut deleted_files = Vec::new();
        let mut failed_files = Vec::new();
        let mut first_error = None;

        let mut protected_files = Vec::new();
        for file in self.inner.drain(cutoff_time) {
            if protected_file.is_some_and(|protected| protected == &file) {
                protected_files.push(file);
                continue;
            }
            info!("deleting {}", file.path());
            let file_size = self.file_sizes.get(&file).copied().unwrap_or(0);

            match std::fs::remove_file(file.path()) {
                Ok(()) => {
                    self.file_sizes.remove(&file);
                    self.total_size = self.total_size.saturating_sub(file_size);
                    deleted_files.push(file);
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    info!("journal file {:?} was already removed", file.path());
                    self.file_sizes.remove(&file);
                    self.total_size = self.total_size.saturating_sub(file_size);
                    deleted_files.push(file);
                }
                Err(err) => {
                    error!("failed to remove journal file {:?}: {}", file.path(), err);
                    if first_error.is_none() {
                        first_error = Some(err.into());
                    }
                    failed_files.push(file);
                }
            }
        }

        for file in failed_files {
            self.inner.insert_file(file);
        }
        for file in protected_files {
            self.inner.insert_file(file);
        }

        RetentionOutcome {
            deleted_files,
            error: first_error,
        }
    }
}

fn committed_journal_size(file: &repository::File) -> Option<u64> {
    let window_size = 4096;
    let jf = JournalFile::<Mmap>::open(file, window_size).ok()?;
    let header = jf.journal_header_ref();
    let tail_object_offset = header.tail_object_offset?;
    let tail_object = jf.object_header_ref(tail_object_offset).ok()?;
    Some(align8(
        tail_object_offset.get().saturating_add(tail_object.size),
    ))
}

fn align8(value: u64) -> u64 {
    value.saturating_add(7) & !7
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn delete_oldest_file_preserves_accounting_on_remove_error() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("create temp dir");
        let machine_id = Uuid::new_v4();
        let path = tmp.path().join(machine_id.to_string());
        fs::create_dir(&path).expect("create machine-id dir");

        let mut chain =
            OwnedChain::new(path.clone(), machine_id, Source::System).expect("create chain");
        let file = create_chain_file(&path, "system", machine_id, 1, 1).expect("create chain file");
        fs::write(file.path(), b"journal").expect("write journal file");

        let file_size = fs::metadata(file.path()).expect("stat journal file").len();
        chain.inner.insert_file(file.clone());
        chain.file_sizes.insert(file.clone(), file_size);
        chain.total_size = file_size;

        fs::set_permissions(&path, fs::Permissions::from_mode(0o555))
            .expect("make directory read-only");

        let delete_result = chain.delete_oldest_file(None);

        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("restore directory permissions");

        match delete_result {
            Ok(Some(_)) => panic!("expected deletion to fail, but the oldest file was removed"),
            Ok(None) => panic!("expected the oldest file to be selected for deletion"),
            Err(err) => assert!(matches!(err, WriterError::Io(_))),
        }

        assert_eq!(chain.inner.len(), 1);
        assert_eq!(chain.file_sizes.get(&file), Some(&file_size));
        assert_eq!(chain.total_size, file_size);
    }

    #[test]
    fn delete_files_older_than_reports_successes_and_preserves_failed_file() {
        use std::fs;

        let tmp = tempfile::tempdir().expect("create temp dir");
        let machine_id = Uuid::new_v4();
        let path = tmp.path().join(machine_id.to_string());
        fs::create_dir(&path).expect("create machine-id dir");

        let mut chain =
            OwnedChain::new(path.clone(), machine_id, Source::System).expect("create chain");
        let deletable_file = create_chain_file(&path, "system", machine_id, 1, 1)
            .expect("create deletable chain file");
        let failed_file =
            create_chain_file(&path, "system", machine_id, 2, 2).expect("create failed chain file");

        fs::write(deletable_file.path(), b"journal").expect("write deletable journal file");
        fs::create_dir(failed_file.path()).expect("create directory at failed journal path");

        let deletable_size = fs::metadata(deletable_file.path())
            .expect("stat deletable journal file")
            .len();
        let failed_size = fs::metadata(failed_file.path())
            .expect("stat failed journal path")
            .len();
        chain.inner.insert_file(deletable_file.clone());
        chain.inner.insert_file(failed_file.clone());
        chain
            .file_sizes
            .insert(deletable_file.clone(), deletable_size);
        chain.file_sizes.insert(failed_file.clone(), failed_size);
        chain.total_size = deletable_size + failed_size;

        let retention = chain.delete_files_older_than(std::time::Duration::from_secs(1), None);

        assert_eq!(retention.deleted_files, vec![deletable_file.clone()]);
        match retention.error {
            Some(WriterError::Io(_)) => {}
            Some(err) => panic!("expected I/O error, got {err:?}"),
            None => panic!("expected deletion failure to be reported"),
        }

        assert_eq!(chain.inner.len(), 1);
        assert!(!chain.file_sizes.contains_key(&deletable_file));
        assert_eq!(chain.file_sizes.get(&failed_file), Some(&failed_size));
        assert_eq!(chain.total_size, failed_size);
        assert!(!PathBuf::from(deletable_file.path()).exists());
        assert!(PathBuf::from(failed_file.path()).is_dir());
    }
}
