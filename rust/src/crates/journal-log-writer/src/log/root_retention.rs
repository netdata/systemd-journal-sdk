//! Dedicated-root retention. Callers own provenance checks and writer exclusion.
use super::*;
use journal_core::file::{JournalHeader, JournalState, read_retention_header};
use journal_registry::Source;
use std::collections::{BTreeSet, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Header metadata for one owned source journal. Times are journal realtime
/// microseconds. `active` describes strict active naming, including retired hosts.
#[derive(Debug, Clone)]
pub struct RootRetentionFile {
    pub path: PathBuf,
    pub machine_id: Uuid,
    pub seqnum_id: Uuid,
    pub bytes: u64,
    pub entries: u64,
    pub head_seqnum: u64,
    pub tail_seqnum: u64,
    pub head_realtime: u64,
    pub tail_realtime: u64,
    pub active: bool,
    header: JournalHeader,
}

/// Directory-visible full file lengths, including preallocation. Unlinked files
/// still pinned by readers are not counted.
#[derive(Debug, Clone, Default)]
pub struct RootRetentionInventory {
    pub files: Vec<RootRetentionFile>,
    pub bytes: u64,
}

/// Latest maintenance outcome, independent of a healthy automatic append result.
#[derive(Debug, Clone)]
pub struct RootRetentionResult {
    pub attempted_at: SystemTime,
    pub last_successful_at: Option<SystemTime>,
    pub inventory: RootRetentionInventory,
    pub inventory_valid: bool,
    pub deleted_files: usize,
    pub error: Option<Arc<WriterError>>,
}

fn invalid(path: &Path, reason: &str) -> WriterError {
    WriterError::InvalidPath(format!("{}: {reason}", path.display()))
}

pub(super) fn validate_source(source: &Source) -> Result<String> {
    let name = super::chain::source_basename(source);
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
    {
        return Err(WriterError::InvalidConfig(
            "invalid root retention source".into(),
        ));
    }
    Ok(name)
}

fn owned_name(name: &str, source: &str) -> bool {
    name.starts_with(&format!("{source}@")) || name.starts_with(&format!("{source}.journal"))
}

fn archive_name(source: &str, header: &JournalHeader) -> String {
    format!(
        "{source}@{}-{:016x}-{:016x}.journal",
        Uuid::from_bytes(header.seqnum_id).simple(),
        header.head_entry_seqnum,
        header.head_entry_realtime
    )
}

/// Inspects an owned root/source using directory entries, stat and fixed-size
/// headers only. Missing roots, symlinks, quarantine candidates, malformed
/// ownership and ambiguous identities return errors, never partial inventories.
/// This is not index/payload verification. Exclude all writers and renames and
/// verify recovered active files before enabling root retention.
pub fn inspect_root_retention(root: &Path, source: &Source) -> Result<RootRetentionInventory> {
    let source = validate_source(source)?;
    if !std::fs::symlink_metadata(root)?.is_dir() {
        return Err(invalid(root, "root is not a directory"));
    }
    let mut inventory = RootRetentionInventory::default();
    let mut identities = HashSet::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if let Some(machine) = classify_root_entry(&entry, &source)? {
            inspect_root_machine(
                &entry.path(),
                &source,
                machine,
                &mut inventory,
                &mut identities,
            )?;
        }
    }
    inventory.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(inventory)
}

fn classify_root_entry(entry: &std::fs::DirEntry, source: &str) -> Result<Option<Uuid>> {
    let path = entry.path();
    let name = entry.file_name();
    let name = name.to_string_lossy();
    let machine = Uuid::parse_str(&name)
        .ok()
        .filter(|id| id.simple().to_string() == name);
    let kind = entry.file_type()?;
    if !kind.is_dir() {
        if machine.is_some() || kind.is_symlink() || owned_name(&name, source) {
            return Err(invalid(&path, "unexpected root entry"));
        }
        return Ok(None);
    }
    if let Some(machine) = machine.filter(|id| !id.is_nil()) {
        return Ok(Some(machine));
    }
    // Metadata directories are allowed only when they contain no source candidates.
    for child in std::fs::read_dir(&path)? {
        let child = child?;
        if owned_name(&child.file_name().to_string_lossy(), source) {
            return Err(invalid(&path, "invalid machine directory"));
        }
    }
    Ok(None)
}

fn inspect_root_machine(
    path: &Path,
    source: &str,
    machine: Uuid,
    inventory: &mut RootRetentionInventory,
    identities: &mut HashSet<[u8; 16]>,
) -> Result<()> {
    let mut targets = HashSet::new();
    for candidate in std::fs::read_dir(path)? {
        let candidate = candidate?;
        let name = candidate.file_name();
        let name = name.to_string_lossy();
        if !owned_name(&name, source) {
            continue;
        }
        let path = candidate.path();
        if !candidate.file_type()?.is_file() {
            return Err(invalid(&path, "nonregular source candidate"));
        }
        let (file, target) = inspect_root_file(path, &name, source, machine)?;
        if !targets.insert(target) {
            return Err(invalid(&file.path, "ambiguous archive identity"));
        }
        if !identities.insert(file.header.file_id) {
            return Err(invalid(&file.path, "duplicate file identity"));
        }
        inventory.bytes = inventory.bytes.saturating_add(file.bytes);
        inventory.files.push(file);
    }
    Ok(())
}

fn inspect_root_file(
    path: PathBuf,
    name: &str,
    source: &str,
    machine: Uuid,
) -> Result<(RootRetentionFile, String)> {
    let (header, bytes) = read_retention_header(&path)?;
    if header.machine_id != *machine.as_bytes() {
        return Err(invalid(&path, "machine directory disagrees with header"));
    }
    let active = name == format!("{source}.journal");
    let target = archive_name(source, &header);
    if !active
        && (name != target || header.state != JournalState::Archived as u8 || header.n_entries == 0)
    {
        return Err(invalid(&path, "archive name/state disagrees with header"));
    }
    Ok((
        RootRetentionFile {
            path,
            machine_id: machine,
            seqnum_id: Uuid::from_bytes(header.seqnum_id),
            bytes,
            entries: header.n_entries,
            head_seqnum: header.head_entry_seqnum,
            tail_seqnum: header.tail_entry_seqnum,
            head_realtime: header.head_entry_realtime,
            tail_realtime: header.tail_entry_realtime,
            active,
            header,
        },
        target,
    ))
}

fn expired(file: &RootRetentionFile, policy: RetentionPolicy, now: SystemTime) -> bool {
    let (Some(age), Ok(stamp)) = (
        policy.duration_of_journal_files,
        now.duration_since(UNIX_EPOCH),
    ) else {
        return false;
    };
    let age = age.as_micros().max(1);
    let stamp = stamp.as_micros();
    stamp >= age && u128::from(file.tail_realtime) <= stamp - age
}

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    std::fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct RootFaults {
    unlink_after: Option<usize>,
    prune_sync: bool,
    archive_sync: bool,
    empty_sync: bool,
}

#[cfg(test)]
fn injected_error() -> WriterError {
    std::io::Error::other("injected root maintenance I/O failure").into()
}

impl Log {
    fn require_root_retention(&self) -> Result<()> {
        if !self.config.root_retention {
            return Err(WriterError::InvalidConfig(
                "root retention is not enabled".into(),
            ));
        }
        self.validate_root_hooks()
    }

    pub(super) fn validate_root_hooks(&self) -> Result<()> {
        if self.config.root_retention && self.artifact_sizer.is_some() {
            return Err(WriterError::InvalidConfig(
                "root retention does not support artifact sizing hooks".into(),
            ));
        }
        Ok(())
    }

    /// Inspects the root and verifies that its active path still names the owned
    /// live descriptor and journal identities. Available even after writer
    /// failure; inspection errors alone never poison the writer.
    pub fn inspect_root_retention(&self) -> Result<RootRetentionInventory> {
        self.require_root_retention()?;
        if let Some(active) = &self.active_file {
            if !active
                .journal_file
                .names_same_file(Path::new(active.repository_file.path()))?
            {
                return Err(invalid(
                    Path::new(active.repository_file.path()),
                    "active path no longer names the live writer file",
                ));
            }
        }
        let inventory = inspect_root_retention(&self.configured_dir, &self.config.origin.source)?;
        if let Some(active) = &self.active_file {
            let path = Path::new(active.repository_file.path());
            let Some(file) = inventory.files.iter().find(|file| file.path == path) else {
                return Err(invalid(path, "live writer is missing from root inventory"));
            };
            if !file.active
                || (
                    file.header.file_id,
                    file.header.machine_id,
                    file.header.seqnum_id,
                ) != active.identity
            {
                return Err(invalid(
                    path,
                    "active journal identity disagrees with live writer",
                ));
            }
        }
        Ok(inventory)
    }

    /// Returns an independent policy copy.
    pub fn root_retention_policy(&self) -> RetentionPolicy {
        self.config.retention_policy
    }

    /// Validates and installs a policy without enforcing it. Derived allocation
    /// geometry is recomputed from the original caller rotation policy.
    pub fn set_root_retention_policy(&mut self, policy: RetentionPolicy) -> Result<()> {
        self.require_root_retention()?;
        self.ensure_healthy()?;
        let mut config = self.config.clone();
        config.rotation_policy = self.root_rotation;
        config.retention_policy = policy;
        config = startup::normalize_config(config)?;
        self.rotation_state =
            startup::rotation_state_for_active(&config.rotation_policy, self.active_file.as_ref());
        self.config = config;
        Ok(())
    }

    pub fn last_root_retention_result(&self) -> Option<&RootRetentionResult> {
        self.root_result.as_ref()
    }

    /// Applies full-length/tail-time retention across all machine directories.
    /// An expired live file or changed allocation geometry is finalized with a
    /// lazy successor. Explicit errors are also stored in the latest outcome.
    pub fn maintain_root_retention(&mut self, now: SystemTime) -> Result<RootRetentionResult> {
        self.require_root_retention()?;
        self.ensure_healthy()?;
        self.maintain_root_retention_inner(now, true)
    }

    pub(super) fn automatic_root_retention(&mut self, expire_live: bool) -> Result<()> {
        let result = self.maintain_root_retention_inner(SystemTime::now(), expire_live);
        if self.is_poisoned() {
            result.map(|_| ())
        } else {
            Ok(())
        }
    }

    fn maintain_root_retention_inner(
        &mut self,
        now: SystemTime,
        expire_live: bool,
    ) -> Result<RootRetentionResult> {
        let mut result = RootRetentionResult {
            attempted_at: now,
            last_successful_at: self.root_result.as_ref().and_then(|r| r.last_successful_at),
            inventory: RootRetentionInventory::default(),
            inventory_valid: false,
            deleted_files: 0,
            error: None,
        };
        let attempt = (|| {
            let inventory = self.inspect_root_retention()?;
            self.finalize_root_actives(&inventory, now, expire_live, &mut result)?;
            let mut inventory = self.inspect_root_retention()?;
            let prune = self.prune_root(&mut inventory, now, &mut result);
            let sample = self.inspect_root_retention();
            match sample {
                Ok(inventory) => {
                    result.inventory = inventory;
                    result.inventory_valid = true;
                }
                Err(error) => return prune.and(Err(error)),
            }
            prune
        })();
        match attempt {
            Ok(()) => result.last_successful_at = Some(now),
            Err(error) => result.error = Some(Arc::new(error)),
        }
        let error = result.error.clone();
        self.root_result = Some(result.clone());
        match error {
            Some(error) => Err(WriterError::RootRetention(error)),
            None => Ok(result),
        }
    }

    fn finalize_root_actives(
        &mut self,
        inventory: &RootRetentionInventory,
        now: SystemTime,
        expire_live: bool,
        result: &mut RootRetentionResult,
    ) -> Result<()> {
        for file in &inventory.files {
            if !file.active {
                continue;
            }
            let live = self.active_path().is_some_and(|path| path == file.path);
            if live {
                let desired = startup::configured_file_options(
                    &self.config,
                    self.chain.machine_id,
                    self.boot_id,
                    self.seqnum_id,
                )
                .data_hash_table_size();
                let allocation_changed = self
                    .active_file
                    .as_ref()
                    .unwrap()
                    .journal_file
                    .journal_header_ref()
                    .data_hash_table_size
                    .map(|size| size.get())
                    != Some(desired);
                if !(expire_live
                    && file.entries > 0
                    && expired(file, self.config.retention_policy, now))
                    && !allocation_changed
                {
                    continue;
                }
                self.archive_root_active(LogLifecycleReason::Retention, &mut result.deleted_files)?;
            } else {
                self.finalize_retired_root_active(file, &mut result.deleted_files)?;
            }
        }
        Ok(())
    }

    /// Finalizes the current writer without creating a successor. Counts empty
    /// unlinks before syncing their directory. Uncertain archive failures poison.
    pub(super) fn archive_root_active(
        &mut self,
        reason: LogLifecycleReason,
        deleted_files: &mut usize,
    ) -> Result<()> {
        let Some(active) = self.active_file.as_ref() else {
            return Ok(());
        };
        let path = Path::new(active.repository_file.path());
        if !active.journal_file.names_same_file(path)? {
            return Err(invalid(
                path,
                "active path no longer names the live writer file",
            ));
        }
        let (header, _) = read_retention_header(path)?;
        if (header.file_id, header.machine_id, header.seqnum_id) != active.identity {
            return Err(invalid(
                path,
                "active journal identity disagrees with live writer",
            ));
        }
        if header.n_entries > 0 {
            let target = path
                .parent()
                .unwrap()
                .join(archive_name(&self.chain.source_name, &header));
            match std::fs::symlink_metadata(&target) {
                Ok(_) => return Err(invalid(&target, "archive target already exists")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        if active.journal_file.journal_header_ref().n_entries == 0 {
            let file = active.repository_file.clone();
            std::fs::remove_file(file.path())?;
            *deleted_files += 1;
            self.active_file.take();
            self.retention_on_open_applied = false;
            self.rotation_state.reset();
            self.chain.remove_tracked_file(&file);
            self.sync_empty_root_directory(&self.chain.path)?;
            return Ok(());
        }
        // The operation owns the outgoing file from the first mutation onward.
        // If rename succeeds but sync fails, a poisoned Log must not retain a
        // live-path assertion for the old name. Inspection can still scan evidence.
        let mut active = self.active_file.take().unwrap();
        self.retention_on_open_applied = false;
        self.rotation_state.reset();
        self.poisoned = true;
        active.journal_file.journal_header_mut().state = JournalState::Archived as u8;
        #[cfg(test)]
        if self.root_faults.archive_sync {
            return Err(injected_error());
        }
        sync_archive_journal_file(self.config.sync_on_archive, &mut active.journal_file)?;
        let header = *active.journal_file.journal_header_ref();
        let archived = self.chain.archive_file(
            &active.repository_file,
            Uuid::from_bytes(header.seqnum_id),
            header.head_entry_seqnum,
            header.head_entry_realtime,
        )?;
        self.poisoned = false;
        self.emit_lifecycle_event(&LogLifecycleEvent::Archived { archived, reason });
        Ok(())
    }

    fn finalize_retired_root_active(
        &mut self,
        file: &RootRetentionFile,
        deleted_files: &mut usize,
    ) -> Result<()> {
        // Recovery callers verified this file. Use the low-level append/archive
        // lifecycle, never a recursive high-level Log with its own retention.
        let repository_file = repository::File::from_path(&file.path)
            .ok_or_else(|| invalid(&file.path, "invalid active path"))?;
        // Mapping validates existing bytes without writing them. In particular,
        // a denied write-open must not poison the unrelated current writer.
        let journal_file =
            JournalFile::<MmapMut>::open_for_append(&repository_file, 8 * 1024 * 1024)?;
        self.poisoned = true;
        let mut active = ActiveFile::activate_opened(repository_file, journal_file, self.boot_id)?;
        if file.entries == 0 {
            active.journal_file.sync()?;
            drop(active);
            self.poisoned = false;
            std::fs::remove_file(&file.path)?;
            *deleted_files += 1;
            self.sync_empty_root_directory(file.path.parent().unwrap())?;
            return Ok(());
        }
        active.journal_file.journal_header_mut().state = JournalState::Archived as u8;
        #[cfg(test)]
        if self.root_faults.archive_sync {
            return Err(injected_error());
        }
        sync_archive_journal_file(self.config.sync_on_archive, &mut active.journal_file)?;
        let target = file
            .path
            .parent()
            .unwrap()
            .join(archive_name(&self.chain.source_name, &file.header));
        std::fs::rename(&file.path, &target)?;
        sync_directory(target.parent().unwrap())?;
        self.poisoned = false;
        let archived = repository::File::from_path(&target)
            .ok_or_else(|| invalid(&target, "invalid archive path"))?;
        self.emit_lifecycle_event(&LogLifecycleEvent::Archived {
            archived,
            reason: LogLifecycleReason::Retention,
        });
        Ok(())
    }

    fn remove_root_file(&mut self, path: &Path) -> Result<()> {
        #[cfg(test)]
        if let Some(remaining) = &mut self.root_faults.unlink_after {
            if *remaining == 0 {
                return Err(injected_error());
            }
            *remaining -= 1;
        }
        std::fs::remove_file(path)?;
        Ok(())
    }

    fn sync_empty_root_directory(&self, path: &Path) -> Result<()> {
        #[cfg(test)]
        if self.root_faults.empty_sync {
            return Err(injected_error());
        }
        sync_directory(path)
    }

    fn sync_pruned_directory(&self, path: &Path) -> Result<()> {
        #[cfg(test)]
        if self.root_faults.prune_sync {
            return Err(injected_error());
        }
        sync_directory(path)
    }

    fn prune_root(
        &mut self,
        inventory: &mut RootRetentionInventory,
        now: SystemTime,
        result: &mut RootRetentionResult,
    ) -> Result<()> {
        inventory.files.sort_by(|a, b| {
            a.tail_realtime
                .cmp(&b.tail_realtime)
                .then(a.path.cmp(&b.path))
        });
        let mut count = inventory.files.len();
        let mut directories = BTreeSet::new();
        let mut deleted = Vec::new();
        let mut error = None;
        for file in &inventory.files {
            if file.active {
                continue;
            }
            let policy = self.config.retention_policy;
            let over = policy
                .size_of_journal_files
                .is_some_and(|max| inventory.bytes > max)
                || policy
                    .number_of_journal_files
                    .is_some_and(|max| count > max);
            if !over && !expired(file, policy, now) {
                continue;
            }
            if let Err(err) = self.remove_root_file(&file.path) {
                error = Some(err);
                break;
            }
            result.deleted_files += 1;
            count -= 1;
            inventory.bytes = inventory.bytes.saturating_sub(file.bytes);
            directories.insert(file.path.parent().unwrap().to_path_buf());
            if let Some(repository_file) = repository::File::from_path(&file.path) {
                if file.machine_id == self.chain.machine_id {
                    self.chain.remove_tracked_file(&repository_file);
                }
                deleted.push(repository_file);
            }
        }
        for dir in directories {
            if let Err(err) = self.sync_pruned_directory(&dir) {
                if error.is_none() {
                    error = Some(err);
                }
            }
        }
        if !deleted.is_empty() {
            self.emit_lifecycle_event(&LogLifecycleEvent::RetainedDeleted { files: deleted });
        }
        match error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn config() -> Config {
        Config::new(
            journal_registry::Origin {
                machine_id: Some(Uuid::from_bytes([1; 16])),
                namespace: None,
                source: Source::Unknown("history".into()),
            },
            RotationPolicy::default().with_number_of_entries(1),
            RetentionPolicy::default(),
        )
        .with_boot_id(Uuid::from_bytes([2; 16]))
        .with_strict_systemd_naming(true)
        .with_root_retention(true)
    }

    fn append(log: &mut Log, step: u64) {
        log.write_entry_with_timestamps(
            &[b"MESSAGE=record"],
            EntryTimestamps::default()
                .with_entry_realtime_usec(Microseconds::now().get() + step)
                .with_entry_monotonic_usec(step),
        )
        .unwrap();
    }

    fn assert_empty_unlink_counted_after_sync_failure(retired: bool) {
        let _guard = super::super::tests::ARCHIVE_SYNC_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut log = Log::new(dir.path(), config().with_open_mode(LogOpenMode::Eager)).unwrap();
        let removed = if retired {
            append(&mut log, 1);
            let mut retired_config = config()
                .with_root_retention(false)
                .with_open_mode(LogOpenMode::Eager);
            retired_config.origin.machine_id = Some(Uuid::from_bytes([3; 16]));
            let mut retired = Log::new(dir.path(), retired_config).unwrap();
            let mut active = retired.active_file.take().unwrap();
            active.journal_file.sync().unwrap();
            let path = PathBuf::from(active.repository_file.path());
            drop(active);
            drop(retired);
            path
        } else {
            log.set_root_retention_policy(
                RetentionPolicy::default().with_size_of_journal_files(8 * 1024 * 1024),
            )
            .unwrap();
            log.active_path().unwrap().to_path_buf()
        };
        let successful = log.last_root_retention_result().unwrap().last_successful_at;
        log.root_faults.empty_sync = true;
        assert!(log.maintain_root_retention(SystemTime::now()).is_err());
        assert!(
            !removed.exists(),
            "unlink must succeed before the injected sync error"
        );
        let result = log.last_root_retention_result().unwrap();
        assert_eq!(
            result.deleted_files, 1,
            "successful unlink must be accounted even when sync fails"
        );
        assert_eq!(result.last_successful_at, successful);
        assert!(result.error.is_some());
        assert!(!result.inventory_valid);
        assert!(!log.is_poisoned());
        log.root_faults.empty_sync = false;
        log.maintain_root_retention(SystemTime::now()).unwrap();
        assert_eq!(log.last_root_retention_result().unwrap().deleted_files, 0);
        append(&mut log, 2);
        log.close_without_retention().unwrap();
    }

    #[test]
    fn live_empty_unlink_is_counted_before_sync_failure() {
        assert_empty_unlink_counted_after_sync_failure(false);
    }

    #[test]
    fn retired_empty_unlink_is_counted_before_sync_failure() {
        assert_empty_unlink_counted_after_sync_failure(true);
    }

    #[test]
    fn safe_unlink_failure_is_stored_and_does_not_fail_automatic_append() {
        let _guard = super::super::tests::ARCHIVE_SYNC_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut log = Log::new(
            dir.path(),
            config()
                .with_retention_policy(RetentionPolicy::default().with_number_of_journal_files(1)),
        )
        .unwrap();
        append(&mut log, 1);
        let successful = log.last_root_retention_result().unwrap().last_successful_at;
        log.root_faults.unlink_after = Some(0);
        append(&mut log, 2);
        let result = log.last_root_retention_result().unwrap();
        assert!(result.error.is_some());
        assert!(result.inventory_valid);
        assert_eq!(result.inventory.files.len(), 2);
        assert_eq!(result.last_successful_at, successful);
        assert!(!log.is_poisoned());
        log.root_faults.unlink_after = None;
        let result = log.maintain_root_retention(SystemTime::now()).unwrap();
        assert_eq!(result.deleted_files, 1);
        assert_eq!(result.inventory.files.len(), 1);
        log.close_without_retention().unwrap();
    }

    #[test]
    fn partial_unlink_and_directory_sync_errors_resample_and_preserve_chain_accounting() {
        let _guard = super::super::tests::ARCHIVE_SYNC_TEST_LOCK.lock().unwrap();
        for sync_failure in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = Log::new(dir.path(), config()).unwrap();
            for step in 1..=3 {
                append(&mut log, step);
            }
            let successful = log.last_root_retention_result().unwrap().last_successful_at;
            log.set_root_retention_policy(
                RetentionPolicy::default().with_number_of_journal_files(1),
            )
            .unwrap();
            if sync_failure {
                log.root_faults.prune_sync = true;
            } else {
                log.root_faults.unlink_after = Some(1);
            }
            let error = log.maintain_root_retention(SystemTime::now()).unwrap_err();
            let WriterError::RootRetention(error) = error else {
                panic!("missing typed wrapper")
            };
            let result = log.last_root_retention_result().unwrap();
            assert!(Arc::ptr_eq(&error, result.error.as_ref().unwrap()));
            assert!(result.inventory_valid);
            assert_eq!(result.deleted_files, if sync_failure { 2 } else { 1 });
            assert_eq!(
                result.inventory.files.len(),
                if sync_failure { 1 } else { 2 }
            );
            assert_eq!(result.last_successful_at, successful);
            assert_eq!(log.chain.inner.len(), result.inventory.files.len());
            assert!(!log.is_poisoned());
            log.root_faults = RootFaults::default();
            log.maintain_root_retention(SystemTime::now()).unwrap();
            log.close_without_retention().unwrap();
        }
    }

    #[test]
    fn archive_mutation_failure_poison_preserves_files_and_inspection() {
        let _guard = super::super::tests::ARCHIVE_SYNC_TEST_LOCK.lock().unwrap();
        for explicit_close in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = Log::new(dir.path(), config()).unwrap();
            append(&mut log, 1);
            append(&mut log, 2);
            log.set_root_retention_policy(
                RetentionPolicy::default().with_duration_of_journal_files(Duration::from_secs(1)),
            )
            .unwrap();
            log.root_faults.archive_sync = true;
            assert!(
                log.maintain_root_retention(SystemTime::now() + Duration::from_secs(2))
                    .is_err()
            );
            assert!(log.is_poisoned());
            let result = log.last_root_retention_result().unwrap();
            assert!(!result.inventory_valid);
            assert!(result.error.is_some());
            assert_eq!(result.deleted_files, 0);
            let inventory = log.inspect_root_retention().unwrap();
            assert_eq!(inventory.files.len(), 2);
            let evidence: Vec<_> = inventory
                .files
                .iter()
                .map(|file| (file.path.clone(), std::fs::read(&file.path).unwrap()))
                .collect();
            assert!(log.sync().is_err());
            assert!(
                log.write_entry_with_timestamps(
                    &[b"MESSAGE=blocked"],
                    EntryTimestamps::default().with_entry_monotonic_usec(3)
                )
                .is_err()
            );
            if explicit_close {
                assert!(log.close().is_err());
            } else {
                drop(log);
            }
            for (path, bytes) in evidence {
                assert_eq!(std::fs::read(path).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn empty_recovered_current_active_is_discarded_on_lazy_startup() {
        let _guard = super::super::tests::ARCHIVE_SYNC_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut log = Log::new(dir.path(), config().with_open_mode(LogOpenMode::Eager)).unwrap();
        let mut active = log.active_file.take().unwrap();
        active.journal_file.sync().unwrap();
        let path = PathBuf::from(active.repository_file.path());
        drop(active);
        drop(log);
        assert!(path.exists());
        let log = Log::new(dir.path(), config()).unwrap();
        assert!(log.active_path().is_none());
        assert!(!path.exists());
        log.close_without_retention().unwrap();
    }

    #[test]
    fn retired_archive_mutation_failure_remains_fatal() {
        let _guard = super::super::tests::ARCHIVE_SYNC_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut log = Log::new(dir.path(), config()).unwrap();
        append(&mut log, 1);
        let mut retired_config = config().with_root_retention(false);
        retired_config.origin.machine_id = Some(Uuid::from_bytes([3; 16]));
        let mut retired = Log::new(dir.path(), retired_config).unwrap();
        append(&mut retired, 1);
        let mut retired_active = retired.active_file.take().unwrap();
        retired_active.journal_file.journal_header_mut().state = JournalState::Offline as u8;
        retired_active.journal_file.sync().unwrap();
        let path = PathBuf::from(retired_active.repository_file.path());
        drop(retired_active);
        drop(retired);
        assert_eq!(
            std::fs::read(&path).unwrap()[16],
            JournalState::Offline as u8
        );
        log.root_faults.archive_sync = true;
        assert!(log.maintain_root_retention(SystemTime::now()).is_err());
        assert!(log.is_poisoned());
        assert_eq!(
            std::fs::read(&path).unwrap()[16],
            JournalState::Archived as u8
        );
        assert_eq!(log.inspect_root_retention().unwrap().files.len(), 2);
        let before = std::fs::read(&path).unwrap();
        assert!(log.close().is_err());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn failed_archive_after_rename_remains_inspectable_and_preserves_evidence() {
        let _guard = super::super::tests::ARCHIVE_SYNC_TEST_LOCK.lock().unwrap();
        for explicit_close in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = Log::new(dir.path(), config()).unwrap();
            append(&mut log, 1);
            let original = log.active_path().unwrap().to_path_buf();
            log.set_root_retention_policy(
                RetentionPolicy::default().with_duration_of_journal_files(Duration::from_secs(1)),
            )
            .unwrap();
            let successful = log.last_root_retention_result().unwrap().last_successful_at;
            log.chain.fail_archive_directory_sync = true;
            assert!(
                log.maintain_root_retention(SystemTime::now() + Duration::from_secs(2))
                    .is_err()
            );
            assert!(log.is_poisoned());
            assert!(
                !original.exists(),
                "failure must occur after the actual rename"
            );
            let standalone =
                inspect_root_retention(dir.path(), &Source::Unknown("history".into())).unwrap();
            assert_eq!(standalone.files.len(), 1);
            assert!(!standalone.files[0].active);
            let live = log
                .inspect_root_retention()
                .expect("failed writer remains inspectable");
            assert_eq!(live.files[0].path, standalone.files[0].path);
            let result = log.last_root_retention_result().unwrap();
            assert!(result.error.is_some());
            assert_eq!(result.deleted_files, 0);
            assert_eq!(result.last_successful_at, successful);
            let path = live.files[0].path.clone();
            let bytes = std::fs::read(&path).unwrap();
            assert!(log.sync().is_err());
            if explicit_close {
                assert!(log.close().is_err());
            } else {
                drop(log);
            }
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }

    struct ForbiddenSizer;
    impl LogArtifactSizer for ForbiddenSizer {
        fn journal_artifact_size(&self, _: &Path) -> Result<u64> {
            panic!("unexpected sizer")
        }
    }

    #[test]
    fn invalid_builder_combo_close_and_drop_preserve_active_evidence() {
        let _guard = super::super::tests::ARCHIVE_SYNC_TEST_LOCK.lock().unwrap();
        for explicit_close in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut log = Log::new(dir.path(), config()).unwrap();
            append(&mut log, 1);
            log.sync().unwrap();
            let path = log.active_path().unwrap().to_path_buf();
            let bytes = std::fs::read(&path).unwrap();
            let mut log = log.with_artifact_sizer(Arc::new(ForbiddenSizer));
            assert!(matches!(log.sync(), Err(WriterError::InvalidConfig(_))));
            assert!(matches!(
                log.set_root_retention_policy(RetentionPolicy::default()),
                Err(WriterError::InvalidConfig(_))
            ));
            if explicit_close {
                assert!(matches!(log.close(), Err(WriterError::InvalidConfig(_))));
            } else {
                drop(log);
            }
            assert_eq!(std::fs::read(path).unwrap(), bytes);
            assert_eq!(
                inspect_root_retention(dir.path(), &Source::Unknown("history".into()))
                    .unwrap()
                    .files
                    .len(),
                1
            );
        }
    }
}
