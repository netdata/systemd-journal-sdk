mod chain;
use chain::OwnedChain;

mod config;
pub use config::{Config, LogIdentityMode, LogOpenMode, RetentionPolicy, RotationPolicy};

use crate::{Result, WriterError};
use itoa::Buffer as ItoaBuffer;
use journal_common::{Microseconds, RealtimeClock, load_boot_id, load_machine_id, monotonic_now};
use journal_core::error::JournalError;
use journal_core::file::mmap::MmapMut;
use journal_core::file::{
    Compression, EntryField, EntryWriteOptions, FieldNamePolicy, JournalFile, JournalFileOptions,
    JournalWriter, StructuredField,
};
use journal_registry::repository;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const STACK_ENTRY_REF_LIMIT: usize = 128;
const SOURCE_REALTIME_PREFIX: &[u8] = b"_SOURCE_REALTIME_TIMESTAMP=";
const DERIVED_ROTATION_FRACTION: u64 = 20;
const JOURNAL_FILE_SIZE_MIN: u64 = 512 * 1024;
const PAGE_SIZE: u64 = 4096;
const JOURNAL_COMPACT_SIZE_MAX: u64 = u32::MAX as u64;

fn log_writer_field_name_policy(policy: FieldNamePolicy) -> FieldNamePolicy {
    if policy == FieldNamePolicy::Raw {
        FieldNamePolicy::Raw
    } else {
        FieldNamePolicy::Journald
    }
}

fn filter_raw_items_for_journal_app<'a>(items: &'a [&'a [u8]]) -> Result<Vec<&'a [u8]>> {
    let mut filtered = Vec::with_capacity(items.len());
    for item in items.iter().copied() {
        let Some(pos) = item.iter().position(|&b| b == b'=') else {
            return Err(JournalError::InvalidField.into());
        };
        if pos == 0 {
            return Err(JournalError::InvalidField.into());
        }
        if is_journal_app_field_name(&item[..pos]) {
            filtered.push(item);
        }
    }
    Ok(filtered)
}

fn filter_structured_fields_for_journal_app<'a>(
    fields: &'a [StructuredField<'a>],
) -> Vec<StructuredField<'a>> {
    fields
        .iter()
        .copied()
        .filter(|field| is_journal_app_field_name(field.name))
        .collect()
}

fn is_journal_app_field_name(field_name: &[u8]) -> bool {
    if field_name.is_empty() || field_name.len() > 64 {
        return false;
    }
    if field_name[0] == b'_' || field_name[0].is_ascii_digit() {
        return false;
    }
    field_name
        .iter()
        .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

fn create_chain(
    path: &Path,
    source: journal_registry::Source,
    machine_id: uuid::Uuid,
) -> Result<OwnedChain> {
    if path.exists() && !path.is_dir() {
        return Err(WriterError::NotADirectory(path.display().to_string()));
    }

    if path.to_str().is_none() {
        return Err(WriterError::InvalidPath(
            "path contains invalid UTF-8".to_string(),
        ));
    }

    let path = PathBuf::from(path).join(machine_id.as_simple().to_string());
    if path.to_str().is_none() {
        return Err(WriterError::InvalidPath(
            "path with machine ID contains invalid UTF-8".to_string(),
        ));
    }

    std::fs::create_dir_all(&path)?;

    path.canonicalize()
        .map_err(|e| WriterError::NotADirectory(format!("failed to canonicalize path: {}", e)))?;
    if path.to_str().is_none() {
        return Err(WriterError::InvalidPath(
            "canonicalized path contains invalid UTF-8".to_string(),
        ));
    }

    OwnedChain::new(path, machine_id, source)
}

fn resolve_machine_id(config: &Config) -> Result<uuid::Uuid> {
    match config.identity_mode {
        LogIdentityMode::Strict => config.origin.machine_id.ok_or_else(|| {
            WriterError::MachineId("strict identity requires machine id".to_string())
        }),
        LogIdentityMode::Auto => Ok(config
            .origin
            .machine_id
            .or_else(|| load_machine_id().ok())
            .unwrap_or_else(uuid::Uuid::new_v4)),
    }
}

fn resolve_boot_id(config: &Config) -> Result<uuid::Uuid> {
    match config.identity_mode {
        LogIdentityMode::Strict => config
            .boot_id
            .ok_or_else(|| WriterError::MachineId("strict identity requires boot id".to_string())),
        LogIdentityMode::Auto => Ok(config
            .boot_id
            .or_else(|| load_boot_id().ok())
            .unwrap_or_else(uuid::Uuid::new_v4)),
    }
}

fn validate_config(config: &Config) -> Result<()> {
    if config.rotation_policy.size_of_journal_file == Some(0) {
        return Err(WriterError::InvalidConfig(
            "rotation max file size must be greater than 0".to_string(),
        ));
    }
    if config.rotation_policy.number_of_entries == Some(0) {
        return Err(WriterError::InvalidConfig(
            "rotation max entries must be greater than 0".to_string(),
        ));
    }
    if config
        .rotation_policy
        .duration_of_journal_file
        .is_some_and(|duration| duration.is_zero())
    {
        return Err(WriterError::InvalidConfig(
            "rotation max duration must be greater than 0".to_string(),
        ));
    }
    if config.retention_policy.number_of_journal_files == Some(0) {
        return Err(WriterError::InvalidConfig(
            "retention max files must be greater than 0".to_string(),
        ));
    }
    if config.retention_policy.size_of_journal_files == Some(0) {
        return Err(WriterError::InvalidConfig(
            "retention max bytes must be greater than 0".to_string(),
        ));
    }
    if config
        .retention_policy
        .duration_of_journal_files
        .is_some_and(|duration| duration.is_zero())
    {
        return Err(WriterError::InvalidConfig(
            "retention max age must be greater than 0".to_string(),
        ));
    }
    Ok(())
}

fn align_to(value: u64, alignment: u64) -> u64 {
    value.saturating_add(alignment.saturating_sub(1)) & !(alignment.saturating_sub(1))
}

fn normalize_derived_max_file_size(size: u64, compact: bool) -> u64 {
    let mut size = align_to(size.max(1), PAGE_SIZE);
    if compact && size > JOURNAL_COMPACT_SIZE_MAX {
        size = JOURNAL_COMPACT_SIZE_MAX;
    }
    size.max(JOURNAL_FILE_SIZE_MIN)
}

fn derive_rotation_policy(config: &Config) -> RotationPolicy {
    let mut rotation = config.rotation_policy;
    if rotation.size_of_journal_file.is_none()
        && let Some(retention_size) = config.retention_policy.size_of_journal_files
    {
        rotation.size_of_journal_file = Some(normalize_derived_max_file_size(
            retention_size / DERIVED_ROTATION_FRACTION,
            config.compact,
        ));
    }
    if rotation.duration_of_journal_file.is_none()
        && let Some(retention_duration) = config.retention_policy.duration_of_journal_files
    {
        let fraction = u128::from(DERIVED_ROTATION_FRACTION);
        let micros = retention_duration
            .as_micros()
            .saturating_add(fraction.saturating_sub(1))
            / fraction;
        let micros = micros.max(1).min(u128::from(u64::MAX)) as u64;
        rotation.duration_of_journal_file = Some(std::time::Duration::from_micros(micros));
    }
    rotation
}

/// Tracks rotation state for size and count limits.
struct RotationState {
    size: Option<(u64, u64)>,      // (max, current)
    count: Option<(usize, usize)>, // (max, current)
}

impl RotationState {
    fn new(rotation_policy: &RotationPolicy) -> Self {
        Self {
            size: rotation_policy.size_of_journal_file.map(|max| (max, 0)),
            count: rotation_policy.number_of_entries.map(|max| (max, 0)),
        }
    }

    fn should_rotate(&self) -> bool {
        self.size.is_some_and(|(max, current)| current >= max)
            || self.count.is_some_and(|(max, current)| current >= max)
    }

    fn update(&mut self, journal_writer: &JournalWriter) {
        if let Some((_, ref mut current)) = self.size {
            *current = journal_writer.current_file_size();
        }
        if let Some((_, ref mut current)) = self.count {
            *current += 1;
        }
    }

    fn observe_existing(&mut self, journal_writer: &JournalWriter, entries: u64) {
        if let Some((_, ref mut current)) = self.size {
            *current = journal_writer.current_file_size();
        }
        if let Some((_, ref mut current)) = self.count {
            *current = entries as usize;
        }
    }

    fn reset(&mut self) {
        if let Some((_, ref mut current)) = self.size {
            *current = 0;
        }
        if let Some((_, ref mut current)) = self.count {
            *current = 0;
        }
    }
}

/// Groups a journal file and its writer together
struct ActiveFile {
    repository_file: repository::File,
    journal_file: JournalFile<MmapMut>,
    writer: JournalWriter,
}

impl ActiveFile {
    /// Opens an existing online journal file for append.
    fn open(repository_file: repository::File, fallback_boot_id: uuid::Uuid) -> Result<Self> {
        use journal_core::file::JournalState;

        let mut journal_file =
            JournalFile::<MmapMut>::open_for_append(&repository_file, 8 * 1024 * 1024)?;
        journal_file.journal_header_mut().state = JournalState::Online as u8;
        let header = journal_file.journal_header_ref();
        let next_seqnum = header.tail_entry_seqnum.saturating_add(1);
        let mut boot_id = uuid::Uuid::from_bytes(header.tail_entry_boot_id);
        if boot_id.is_nil() {
            boot_id = fallback_boot_id;
        }
        let writer = JournalWriter::new(&mut journal_file, next_seqnum, boot_id)?;

        Ok(Self {
            repository_file,
            journal_file,
            writer,
        })
    }

    /// Creates a new journal file with the given parameters
    fn create(
        chain: &mut OwnedChain,
        seqnum_id: uuid::Uuid,
        boot_id: uuid::Uuid,
        next_seqnum: u64,
        max_file_size: Option<u64>,
        _head_realtime: u64,
        compression: Compression,
        compression_threshold: usize,
        compact: bool,
        strict_systemd_naming: bool,
        live_publish_every_entries: u64,
    ) -> Result<Self> {
        let head_seqnum = next_seqnum;

        let repository_file = if strict_systemd_naming {
            chain.create_active_file()?
        } else {
            chain.create_chain_file(seqnum_id, head_seqnum, _head_realtime)?
        };

        let options = JournalFileOptions::new(chain.machine_id, boot_id, seqnum_id)
            .with_window_size(8 * 1024 * 1024)
            .with_compact(compact)
            .with_optimized_buckets(None, max_file_size)
            .with_keyed_hash(true)
            .with_compression(compression)
            .with_compress_threshold(compression_threshold);

        let mut journal_file = JournalFile::create(&repository_file, options)?;
        let mut writer = JournalWriter::new_with_compression(
            &mut journal_file,
            head_seqnum,
            boot_id,
            compression,
            compression_threshold,
        )?;
        writer.set_live_publish_every_entries(live_publish_every_entries);

        Ok(Self {
            repository_file,
            journal_file,
            writer,
        })
    }

    /// Creates a successor file, inheriting settings from this file
    fn rotate(
        self,
        chain: &mut OwnedChain,
        max_file_size: Option<u64>,
        _head_realtime: u64,
        compression: Compression,
        compression_threshold: usize,
        strict_systemd_naming: bool,
        live_publish_every_entries: u64,
    ) -> Result<Self> {
        let next_seqnum = self.writer.next_seqnum();
        let boot_id = self.writer.boot_id();

        let head_seqnum = next_seqnum;

        let seqnum_id = uuid::Uuid::from_bytes(self.journal_file.journal_header_ref().seqnum_id);
        let repository_file = if strict_systemd_naming {
            chain.create_active_file()?
        } else {
            chain.create_chain_file(seqnum_id, head_seqnum, _head_realtime)?
        };

        let mut old_journal_file = self.journal_file;
        old_journal_file.release_writer_lock()?;
        let mut journal_file =
            old_journal_file.create_successor(&repository_file, max_file_size)?;
        let mut writer = JournalWriter::new_with_compression(
            &mut journal_file,
            head_seqnum,
            boot_id,
            compression,
            compression_threshold,
        )?;
        writer.set_live_publish_every_entries(live_publish_every_entries);

        Ok(Self {
            repository_file,
            journal_file,
            writer,
        })
    }

    fn write_entry_fields<'a>(
        &mut self,
        fields: impl IntoIterator<Item = EntryField<'a>>,
        realtime: u64,
        monotonic: u64,
        options: EntryWriteOptions,
    ) -> Result<()> {
        self.writer.add_entry_fields_with_options(
            &mut self.journal_file,
            fields,
            realtime,
            monotonic,
            options,
        )?;
        Ok(())
    }

    /// Gets the current file size
    fn current_file_size(&self) -> u64 {
        self.writer.current_file_size()
    }
}

pub struct Log {
    configured_dir: PathBuf,
    chain: OwnedChain,
    config: Config,
    active_file: Option<ActiveFile>,
    rotation_state: RotationState,
    boot_id: uuid::Uuid,
    seqnum_id: uuid::Uuid,
    current_seqnum: u64,
    clock: RealtimeClock,
    last_monotonic_usec: u64,
    lifecycle_observer: Option<Arc<dyn LogLifecycleObserver>>,
    artifact_sizer: Option<Arc<dyn LogArtifactSizer>>,
    retention_on_open_applied: bool,
    boot_id_field: Vec<u8>,
    source_realtime_field: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLifecycleReason {
    Append,
    EagerOpen,
    Rotation,
    Retention,
}

#[derive(Debug, Clone)]
pub enum LogLifecycleEvent {
    Created {
        active: repository::File,
        reason: LogLifecycleReason,
    },
    Rotated {
        archived: repository::File,
        active: repository::File,
    },
    RetainedDeleted {
        files: Vec<repository::File>,
    },
}

pub trait LogLifecycleObserver: Send + Sync {
    fn on_event(&self, event: &LogLifecycleEvent);
}

pub trait LogArtifactSizer: Send + Sync {
    fn journal_artifact_size(&self, journal_path: &Path) -> Result<u64>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EntryTimestamps {
    /// Optional source timestamp for `_SOURCE_REALTIME_TIMESTAMP` field injection.
    pub source_realtime_usec: Option<u64>,
    /// Optional journal entry realtime timestamp override.
    pub entry_realtime_usec: Option<u64>,
    /// Optional journal entry monotonic timestamp override.
    pub entry_monotonic_usec: Option<u64>,
}

impl EntryTimestamps {
    pub fn with_source_realtime_usec(mut self, ts: u64) -> Self {
        self.source_realtime_usec = Some(ts);
        self
    }

    pub fn with_entry_realtime_usec(mut self, ts: u64) -> Self {
        self.entry_realtime_usec = Some(ts);
        self
    }

    pub fn with_entry_monotonic_usec(mut self, ts: u64) -> Self {
        self.entry_monotonic_usec = Some(ts);
        self
    }
}

impl Log {
    fn duration_to_micros(duration: std::time::Duration) -> u64 {
        duration.as_micros().try_into().unwrap_or(u64::MAX)
    }

    fn peek_entry_realtime(&self, timestamps: &EntryTimestamps) -> u64 {
        let candidate = timestamps
            .entry_realtime_usec
            .unwrap_or_else(|| Microseconds::now().get());
        let last_seen = self.clock.last_seen().get();
        if candidate > last_seen {
            candidate
        } else {
            last_seen.saturating_add(1)
        }
    }

    fn should_rotate_for_realtime(&self, realtime: u64) -> bool {
        let Some(active_file) = &self.active_file else {
            return true;
        };
        if self.rotation_state.should_rotate() {
            return true;
        }
        let Some(max_duration) = self.config.rotation_policy.duration_of_journal_file else {
            return false;
        };
        let header = active_file.journal_file.journal_header_ref();
        header.n_entries > 0
            && header.head_entry_realtime > 0
            && realtime.saturating_sub(header.head_entry_realtime)
                >= Self::duration_to_micros(max_duration)
    }

    fn apply_retention(&mut self, protected_file: Option<&repository::File>) -> Result<()> {
        if let Some(sizer) = &self.artifact_sizer {
            self.chain.refresh_retained_sizes(|file| {
                sizer.journal_artifact_size(Path::new(file.path()))
            })?;
        }
        let retention = self
            .chain
            .retain(&self.config.retention_policy, protected_file);
        let deleted_files = retention.deleted_files;
        if !deleted_files.is_empty()
            && let Some(observer) = &self.lifecycle_observer
        {
            observer.on_event(&LogLifecycleEvent::RetainedDeleted {
                files: deleted_files,
            });
        }
        if let Some(error) = retention.error {
            return Err(error);
        }

        Ok(())
    }

    fn apply_retention_on_open(&mut self) -> Result<()> {
        if self.retention_on_open_applied || self.active_file.is_none() {
            return Ok(());
        }
        self.enforce_retention()?;
        self.retention_on_open_applied = true;
        Ok(())
    }

    /// Captures both realtime and monotonic timestamps, similar to systemd's dual_timestamp_now().
    ///
    /// Returns (realtime_usec, monotonic_usec) where:
    /// - realtime: microseconds since Unix epoch (CLOCK_REALTIME), monotonically increasing
    /// - monotonic: microseconds since boot (CLOCK_MONOTONIC)
    fn capture_dual_timestamp(
        &mut self,
        timestamp_override: Option<&EntryTimestamps>,
    ) -> Result<(u64, u64)> {
        let realtime = match timestamp_override.and_then(|ts| ts.entry_realtime_usec) {
            Some(ts) => self.clock.observe(Microseconds::new(ts)).get(),
            None => self.clock.now().get(),
        };

        let desired_monotonic = match timestamp_override.and_then(|ts| ts.entry_monotonic_usec) {
            Some(ts) => ts,
            None => monotonic_now().map_err(WriterError::Io)?.get(),
        };

        let monotonic = if desired_monotonic > self.last_monotonic_usec {
            desired_monotonic
        } else {
            self.last_monotonic_usec.saturating_add(1)
        };
        self.last_monotonic_usec = monotonic;

        Ok((realtime, monotonic))
    }

    /// Creates a new journal log.
    pub fn new(path: &Path, config: Config) -> Result<Self> {
        Self::new_inner(path, config, None, None)
    }

    pub fn new_with_lifecycle_observer(
        path: &Path,
        config: Config,
        observer: Arc<dyn LogLifecycleObserver>,
    ) -> Result<Self> {
        Self::new_inner(path, config, Some(observer), None)
    }

    pub fn new_with_hooks(
        path: &Path,
        config: Config,
        observer: Option<Arc<dyn LogLifecycleObserver>>,
        artifact_sizer: Option<Arc<dyn LogArtifactSizer>>,
    ) -> Result<Self> {
        Self::new_inner(path, config, observer, artifact_sizer)
    }

    fn new_inner(
        path: &Path,
        mut config: Config,
        lifecycle_observer: Option<Arc<dyn LogLifecycleObserver>>,
        artifact_sizer: Option<Arc<dyn LogArtifactSizer>>,
    ) -> Result<Self> {
        validate_config(&config)?;
        config.rotation_policy = derive_rotation_policy(&config);
        let machine_id = resolve_machine_id(&config)?;
        let mut chain = create_chain(path, config.origin.source.clone(), machine_id)?;

        let tail_identity = chain.tail_identity(config.strict_systemd_naming)?;
        let mut boot_id = resolve_boot_id(&config)?;
        let (mut seqnum_id, mut current_seqnum) =
            tail_identity.unwrap_or_else(|| (uuid::Uuid::new_v4(), 0));
        let mut active_file = None;
        if config.strict_systemd_naming
            && let Some(repository_file) = chain.online_chain_file()?
        {
            use journal_core::file::JournalState;

            let mut opened = ActiveFile::open(repository_file.clone(), boot_id)?;
            let n_entries = opened.journal_file.journal_header_ref().n_entries;
            if n_entries == 0 {
                opened.journal_file.release_writer_lock()?;
                match std::fs::remove_file(repository_file.path()) {
                    Ok(()) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err.into()),
                }
                chain.remove_tracked_file(&repository_file);
            } else {
                chain.update_file_size(&repository_file, opened.current_file_size());
                opened.journal_file.journal_header_mut().state = JournalState::Archived as u8;
                opened.journal_file.sync()?;
                opened.journal_file.release_writer_lock()?;
            }
        }
        let existing_active_file = if config.strict_systemd_naming {
            chain.existing_active_file()
        } else {
            chain.online_chain_file()?
        };
        if let Some(repository_file) = existing_active_file {
            let mut opened = ActiveFile::open(repository_file, boot_id)?;
            let n_entries = opened.journal_file.journal_header_ref().n_entries;
            if n_entries == 0 {
                let repository_file = opened.repository_file.clone();
                opened.journal_file.release_writer_lock()?;
                match std::fs::remove_file(repository_file.path()) {
                    Ok(()) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err.into()),
                }
                chain.remove_tracked_file(&repository_file);
            } else {
                let header = opened.journal_file.journal_header_ref();
                boot_id = opened.writer.boot_id();
                seqnum_id = uuid::Uuid::from_bytes(header.seqnum_id);
                current_seqnum = header.tail_entry_seqnum;
                active_file = Some(opened);
            }
        }
        let mut rotation_state = RotationState::new(&config.rotation_policy);
        if let Some(active_file) = &active_file {
            rotation_state.observe_existing(
                &active_file.writer,
                active_file.journal_file.journal_header_ref().n_entries,
            );
        }
        // When there is no tail monotonic timestamp for this boot we start at 0.
        // The first clamped write becomes 1us if an override asks for 0, preserving strict monotonicity.
        let last_monotonic_usec = chain
            .tail_monotonic_for_boot(boot_id, config.strict_systemd_naming)?
            .unwrap_or(0);

        // Initialize clock with last entry timestamp if available
        let clock =
            if let Some(tail_realtime) = chain.tail_realtime(config.strict_systemd_naming)? {
                RealtimeClock::with_initial(tail_realtime)
            } else {
                RealtimeClock::new()
            };

        let mut log = Log {
            configured_dir: path.to_path_buf(),
            chain,
            config,
            active_file,
            rotation_state,
            boot_id,
            seqnum_id,
            current_seqnum,
            clock,
            last_monotonic_usec,
            lifecycle_observer,
            artifact_sizer,
            retention_on_open_applied: false,
            boot_id_field: format!("_BOOT_ID={}", boot_id.as_simple()).into_bytes(),
            source_realtime_field: Vec::with_capacity(SOURCE_REALTIME_PREFIX.len() + 20),
        };
        if log.config.open_mode == LogOpenMode::Eager && log.active_file.is_none() {
            let realtime = log.peek_entry_realtime(&EntryTimestamps::default());
            log.rotate(realtime, LogLifecycleReason::EagerOpen)?;
            log.retention_on_open_applied = true;
        }
        log.apply_retention_on_open()?;
        Ok(log)
    }

    pub fn with_lifecycle_observer(mut self, observer: Arc<dyn LogLifecycleObserver>) -> Self {
        self.lifecycle_observer = Some(observer);
        self
    }

    pub fn with_artifact_sizer(mut self, sizer: Arc<dyn LogArtifactSizer>) -> Self {
        self.artifact_sizer = Some(sizer);
        self
    }

    /// Writes a journal entry.
    ///
    /// If `source_realtime_usec` is provided, a `_SOURCE_REALTIME_TIMESTAMP` field will be added
    /// to record the original timestamp from the source (in microseconds since Unix epoch).
    /// This is useful when ingesting logs from external sources that have their own timestamps.
    pub fn write_entry(
        &mut self,
        items: &[&[u8]],
        source_realtime_usec: Option<u64>,
    ) -> Result<()> {
        self.write_entry_with_timestamps(
            items,
            EntryTimestamps {
                source_realtime_usec,
                ..EntryTimestamps::default()
            },
        )
    }

    /// Writes a journal entry with optional source and entry timestamp overrides.
    ///
    /// Overrides are safe by construction:
    /// - entry realtime is clamped to strict monotonic progression (`last + 1us` floor)
    /// - entry monotonic is also clamped to strict monotonic progression (`last + 1us` floor)
    pub fn write_entry_with_timestamps(
        &mut self,
        items: &[&[u8]],
        timestamps: EntryTimestamps,
    ) -> Result<()> {
        if items.is_empty() {
            return Err(WriterError::EmptyEntry);
        }

        let entry_realtime = self.peek_entry_realtime(&timestamps);
        self.apply_retention_on_open()?;
        let opened_first_active = self.active_file.is_none();
        if self.should_rotate_for_realtime(entry_realtime) {
            let reason = if self.active_file.is_none() {
                LogLifecycleReason::Append
            } else {
                LogLifecycleReason::Rotation
            };
            self.rotate(entry_realtime, reason)?;
            if opened_first_active {
                self.retention_on_open_applied = true;
            }
        }
        self.apply_retention_on_open()?;

        let filtered_items;
        let write_items = if self.config.field_name_policy == FieldNamePolicy::JournalApp {
            filtered_items = filter_raw_items_for_journal_app(items)?;
            if filtered_items.is_empty() {
                return Err(WriterError::EmptyEntry);
            }
            filtered_items.as_slice()
        } else {
            items
        };

        let (realtime, monotonic) = self.capture_dual_timestamp(Some(&timestamps))?;
        self.write_raw_entry_fields(
            write_items,
            timestamps.source_realtime_usec,
            realtime,
            monotonic,
            self.low_level_entry_options(EntryWriteOptions::default()),
        )?;

        let active_file = self.active_file.as_ref().unwrap();
        self.rotation_state.update(&active_file.writer);
        self.current_seqnum += 1;

        Ok(())
    }

    /// Writes a journal entry from structured field names and binary-safe values.
    ///
    /// This is the preferred path when the producer already has split field
    /// names and values. If `source_realtime_usec` is provided, a
    /// `_SOURCE_REALTIME_TIMESTAMP` field is added.
    pub fn write_fields(
        &mut self,
        fields: &[StructuredField<'_>],
        source_realtime_usec: Option<u64>,
    ) -> Result<()> {
        self.write_fields_with_timestamps(
            fields,
            EntryTimestamps {
                source_realtime_usec,
                ..EntryTimestamps::default()
            },
        )
    }

    /// Writes structured fields with optional source and entry timestamp overrides.
    ///
    /// Entry realtime and monotonic overrides use the same monotonic clamping
    /// rules as [`Log::write_entry_with_timestamps`].
    pub fn write_fields_with_timestamps(
        &mut self,
        fields: &[StructuredField<'_>],
        timestamps: EntryTimestamps,
    ) -> Result<()> {
        self.write_fields_with_options(fields, timestamps, EntryWriteOptions::default())
    }

    /// Writes structured fields with explicit low-level entry write options.
    ///
    /// Use this only when the caller can satisfy any invariants required by the
    /// selected [`EntryWriteOptions`], especially no duplicate full `KEY=value`
    /// payloads when `trusted_unique_payloads` is enabled.
    pub fn write_fields_with_options(
        &mut self,
        fields: &[StructuredField<'_>],
        timestamps: EntryTimestamps,
        options: EntryWriteOptions,
    ) -> Result<()> {
        if fields.is_empty() {
            return Err(WriterError::EmptyEntry);
        }

        let entry_realtime = self.peek_entry_realtime(&timestamps);
        self.apply_retention_on_open()?;
        let opened_first_active = self.active_file.is_none();
        if self.should_rotate_for_realtime(entry_realtime) {
            let reason = if self.active_file.is_none() {
                LogLifecycleReason::Append
            } else {
                LogLifecycleReason::Rotation
            };
            self.rotate(entry_realtime, reason)?;
            if opened_first_active {
                self.retention_on_open_applied = true;
            }
        }
        self.apply_retention_on_open()?;

        let filtered_fields;
        let write_fields = if self.config.field_name_policy == FieldNamePolicy::JournalApp {
            filtered_fields = filter_structured_fields_for_journal_app(fields);
            if filtered_fields.is_empty() {
                return Err(WriterError::EmptyEntry);
            }
            filtered_fields.as_slice()
        } else {
            fields
        };

        let (realtime, monotonic) = self.capture_dual_timestamp(Some(&timestamps))?;
        self.write_structured_entry_fields(
            write_fields,
            timestamps.source_realtime_usec,
            realtime,
            monotonic,
            self.low_level_entry_options(options),
        )?;

        let active_file = self.active_file.as_ref().unwrap();
        self.rotation_state.update(&active_file.writer);
        self.current_seqnum += 1;

        Ok(())
    }

    fn write_raw_entry_fields(
        &mut self,
        items: &[&[u8]],
        source_realtime_usec: Option<u64>,
        realtime: u64,
        monotonic: u64,
        options: EntryWriteOptions,
    ) -> Result<()> {
        let source_field = if let Some(timestamp_usec) = source_realtime_usec {
            self.prepare_source_realtime_field(timestamp_usec);
            Some(self.source_realtime_field.as_slice())
        } else {
            None
        };

        let total_items = items.len() + 1 + usize::from(source_field.is_some());
        if total_items <= STACK_ENTRY_REF_LIMIT {
            let mut refs = [EntryField::raw(&[]); STACK_ENTRY_REF_LIMIT];
            let mut len = 0usize;
            refs[len] = EntryField::raw(self.boot_id_field.as_slice());
            len += 1;
            if let Some(source_field) = source_field {
                refs[len] = EntryField::raw(source_field);
                len += 1;
            }
            for item in items {
                refs[len] = EntryField::raw(item);
                len += 1;
            }
            self.active_file.as_mut().unwrap().write_entry_fields(
                refs[..len].iter().copied(),
                realtime,
                monotonic,
                options,
            )?;
        } else {
            let mut refs = Vec::with_capacity(total_items);
            refs.push(EntryField::raw(self.boot_id_field.as_slice()));
            if let Some(source_field) = source_field {
                refs.push(EntryField::raw(source_field));
            }
            refs.extend(items.iter().copied().map(EntryField::raw));
            self.active_file.as_mut().unwrap().write_entry_fields(
                refs.iter().copied(),
                realtime,
                monotonic,
                options,
            )?;
        }

        Ok(())
    }

    fn write_structured_entry_fields(
        &mut self,
        fields: &[StructuredField<'_>],
        source_realtime_usec: Option<u64>,
        realtime: u64,
        monotonic: u64,
        options: EntryWriteOptions,
    ) -> Result<()> {
        let source_field = if let Some(timestamp_usec) = source_realtime_usec {
            self.prepare_source_realtime_field(timestamp_usec);
            Some(self.source_realtime_field.as_slice())
        } else {
            None
        };

        let total_items = fields.len() + 1 + usize::from(source_field.is_some());
        if total_items <= STACK_ENTRY_REF_LIMIT {
            let mut refs = [EntryField::raw(&[]); STACK_ENTRY_REF_LIMIT];
            let mut len = 0usize;
            refs[len] = EntryField::raw(self.boot_id_field.as_slice());
            len += 1;
            if let Some(source_field) = source_field {
                refs[len] = EntryField::raw(source_field);
                len += 1;
            }
            for field in fields {
                refs[len] = EntryField::Structured(*field);
                len += 1;
            }
            self.active_file.as_mut().unwrap().write_entry_fields(
                refs[..len].iter().copied(),
                realtime,
                monotonic,
                options,
            )?;
        } else {
            let mut refs = Vec::with_capacity(total_items);
            refs.push(EntryField::raw(self.boot_id_field.as_slice()));
            if let Some(source_field) = source_field {
                refs.push(EntryField::raw(source_field));
            }
            refs.extend(fields.iter().copied().map(EntryField::Structured));
            self.active_file.as_mut().unwrap().write_entry_fields(
                refs.iter().copied(),
                realtime,
                monotonic,
                options,
            )?;
        }

        Ok(())
    }

    fn low_level_entry_options(&self, options: EntryWriteOptions) -> EntryWriteOptions {
        options.field_name_policy(log_writer_field_name_policy(self.config.field_name_policy))
    }

    fn prepare_source_realtime_field(&mut self, timestamp_usec: u64) {
        self.source_realtime_field.clear();
        self.source_realtime_field
            .extend_from_slice(SOURCE_REALTIME_PREFIX);
        let mut buffer = ItoaBuffer::new();
        self.source_realtime_field
            .extend_from_slice(buffer.format(timestamp_usec).as_bytes());
    }

    /// Syncs all written data to disk, ensuring durability.
    ///
    /// This should be called after writing a batch of log entries to ensure
    /// they are persisted to disk before acknowledging the request.
    pub fn sync(&mut self) -> Result<()> {
        if let Some(active_file) = &mut self.active_file {
            active_file.journal_file.sync()?;
        }
        Ok(())
    }

    /// Archives and closes the active file.
    ///
    /// In strict systemd naming mode this renames `<source>.journal` to the
    /// chain filename before retention, matching the explicit close behavior of
    /// the other SDK implementations. `Drop` remains best-effort for callers
    /// that do not explicitly close.
    pub fn close(mut self) -> Result<()> {
        use journal_core::file::JournalState;

        let Some(mut active_file) = self.active_file.take() else {
            return Ok(());
        };

        let n_entries = active_file.journal_file.journal_header_ref().n_entries;
        if self.config.strict_systemd_naming && n_entries == 0 {
            active_file.journal_file.release_writer_lock()?;
            match std::fs::remove_file(active_file.repository_file.path()) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
            self.chain.remove_tracked_file(&active_file.repository_file);
            return Ok(());
        }

        self.chain.update_file_size(
            &active_file.repository_file,
            active_file.current_file_size(),
        );
        active_file.journal_file.journal_header_mut().state = JournalState::Archived as u8;
        active_file.journal_file.sync()?;

        let protected_file = if self.config.strict_systemd_naming {
            let header = active_file.journal_file.journal_header_ref();
            self.chain.archive_file(
                &active_file.repository_file,
                uuid::Uuid::from_bytes(header.seqnum_id),
                header.head_entry_seqnum,
                header.head_entry_realtime,
            )?
        } else {
            active_file.repository_file.clone()
        };
        active_file.journal_file.release_writer_lock()?;

        self.apply_retention(Some(&protected_file))?;

        Ok(())
    }

    pub fn active_file(&self) -> Option<&repository::File> {
        self.active_file
            .as_ref()
            .map(|active_file| &active_file.repository_file)
    }

    pub fn active_path(&self) -> Option<&Path> {
        self.active_file
            .as_ref()
            .map(|active_file| Path::new(active_file.repository_file.path()))
    }

    pub fn configured_directory(&self) -> &Path {
        &self.configured_dir
    }

    pub fn journal_directory(&self) -> &Path {
        &self.chain.path
    }

    pub fn machine_id(&self) -> uuid::Uuid {
        self.chain.machine_id
    }

    pub fn boot_id(&self) -> uuid::Uuid {
        self.boot_id
    }

    pub fn source(&self) -> &journal_registry::Source {
        &self.chain.source
    }

    /// Applies the configured retention policy without requiring a rotation or
    /// close. The current active file is counted in retention envelopes and is
    /// protected from deletion.
    pub fn enforce_retention(&mut self) -> Result<()> {
        let protected_file = if let Some(active_file) = &self.active_file {
            self.chain.update_file_size(
                &active_file.repository_file,
                active_file.current_file_size(),
            );
            Some(active_file.repository_file.clone())
        } else {
            None
        };
        self.apply_retention(protected_file.as_ref())
    }

    #[tracing::instrument(skip_all, fields(active_file))]
    fn rotate(&mut self, head_realtime: u64, reason: LogLifecycleReason) -> Result<()> {
        use journal_core::file::JournalState;

        // Update chain with current file size before rotating
        if let Some(active_file) = &self.active_file {
            self.chain.update_file_size(
                &active_file.repository_file,
                active_file.current_file_size(),
            );
        } else if self.config.strict_systemd_naming {
            self.chain.archive_existing_active_file()?;
        }

        // Create new file (either initial or rotated)
        let max_file_size = self.config.rotation_policy.size_of_journal_file;
        let (new_file, lifecycle_event) = if let Some(mut old_file) = self.active_file.take() {
            // Set the old file's state to ARCHIVED before creating successor
            old_file.journal_file.journal_header_mut().state = JournalState::Archived as u8;
            old_file.journal_file.sync()?;
            let archived = if self.config.strict_systemd_naming {
                let old_header = old_file.journal_file.journal_header_ref();
                self.chain.archive_file(
                    &old_file.repository_file,
                    uuid::Uuid::from_bytes(old_header.seqnum_id),
                    old_header.head_entry_seqnum,
                    old_header.head_entry_realtime,
                )?
            } else {
                old_file.repository_file.clone()
            };
            let new_file = old_file.rotate(
                &mut self.chain,
                max_file_size,
                head_realtime,
                self.config.compression,
                self.config.compression_threshold,
                self.config.strict_systemd_naming,
                self.config.live_publish_every_entries,
            )?;
            let active = new_file.repository_file.clone();
            (
                new_file,
                Some(LogLifecycleEvent::Rotated { archived, active }),
            )
        } else {
            let new_file = ActiveFile::create(
                &mut self.chain,
                self.seqnum_id,
                self.boot_id,
                self.current_seqnum + 1,
                max_file_size,
                head_realtime,
                self.config.compression,
                self.config.compression_threshold,
                self.config.compact,
                self.config.strict_systemd_naming,
                self.config.live_publish_every_entries,
            )?;
            let active = new_file.repository_file.clone();
            (
                new_file,
                Some(LogLifecycleEvent::Created { active, reason }),
            )
        };

        tracing::Span::current().record("new_file", new_file.repository_file.path());

        self.active_file = Some(new_file);
        self.rotation_state.reset();
        if let Some(active_file) = &self.active_file {
            self.chain.update_file_size(
                &active_file.repository_file,
                active_file.current_file_size(),
            );
        }
        if let Some(event) = lifecycle_event
            && let Some(observer) = &self.lifecycle_observer
        {
            observer.on_event(&event);
        }

        // Retention runs after the post-rotation current file is known, so the
        // tracked current file counts in the envelope and is never deleted.
        let protected_file = self
            .active_file
            .as_ref()
            .map(|active_file| &active_file.repository_file);
        let protected_file = protected_file.cloned();
        self.apply_retention(protected_file.as_ref())?;

        Ok(())
    }

    /// Writes a journal entry from a serializable value.
    ///
    /// This method serializes the value to JSON, flattens it, and writes it to the journal.
    /// The flattened structure converts nested JSON into KEY=VALUE pairs suitable for journal entries.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use serde::Serialize;
    /// use journal_log_writer::{Log, Config, RotationPolicy, RetentionPolicy};
    /// use journal_registry::Origin;
    /// use std::path::Path;
    ///
    /// #[derive(Serialize)]
    /// struct LogEntry {
    ///     message: String,
    ///     level: String,
    ///     user: User,
    /// }
    ///
    /// #[derive(Serialize)]
    /// struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let origin = Origin {
    ///     machine_id: None,
    ///     namespace: None,
    ///     source: journal_registry::Source::System,
    /// };
    /// let config = Config::new(origin, RotationPolicy::default(), RetentionPolicy::default());
    /// let mut log = Log::new(Path::new("/tmp/test-journal"), config)?;
    ///
    /// let entry = LogEntry {
    ///     message: "User logged in".to_string(),
    ///     level: "INFO".to_string(),
    ///     user: User {
    ///         id: 42,
    ///         name: "alice".to_string(),
    ///     },
    /// };
    ///
    /// // This will write fields like:
    /// // MESSAGE=User logged in
    /// // LEVEL=INFO
    /// // USER_ID=42
    /// // USER_NAME=alice
    /// log.write_structured(&entry)?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "serde-api")]
    pub fn write_structured<T: serde::Serialize>(&mut self, value: &T) -> Result<()> {
        use flatten_serde_json::flatten;

        // Serialize to JSON value
        let json_value = serde_json::to_value(value).map_err(|e| {
            WriterError::Serialization(format!("failed to serialize to JSON: {}", e))
        })?;

        // Flatten the JSON structure - requires a JSON object (Map)
        let flattened = if let serde_json::Value::Object(map) = json_value {
            flatten(&map)
        } else {
            // If not an object, return error
            return Err(WriterError::Serialization(
                "value must be a JSON object, not a primitive or array".to_string(),
            ));
        };

        // Convert to journal field format (KEY=VALUE)
        let mut fields: Vec<Vec<u8>> = Vec::with_capacity(flattened.len());

        for (key, value) in flattened.iter() {
            // Convert key to uppercase and replace dots with underscores
            // (journal convention)
            let journal_key = key.to_uppercase().replace('.', "_");

            // Format as KEY=VALUE
            let field = match value {
                serde_json::Value::String(s) => {
                    format!("{}={}", journal_key, s)
                }
                serde_json::Value::Number(n) => {
                    format!("{}={}", journal_key, n)
                }
                serde_json::Value::Bool(b) => {
                    format!("{}={}", journal_key, if *b { "true" } else { "false" })
                }
                serde_json::Value::Null => {
                    format!("{}=", journal_key)
                }
                // Arrays and objects should be flattened already, but just in case
                _ => {
                    format!("{}={}", journal_key, value)
                }
            };

            fields.push(field.into_bytes());
        }

        // Convert Vec<Vec<u8>> to Vec<&[u8]> for write_entry
        let field_refs: Vec<&[u8]> = fields.iter().map(|f| f.as_slice()).collect();

        self.write_entry(&field_refs, None)
    }
}

impl Drop for Log {
    fn drop(&mut self) {
        use journal_core::file::JournalState;

        if let Some(ref mut active_file) = self.active_file {
            // Keep the active path stable on close so file-backed readers that
            // already follow system.journal can finish. The next writer startup
            // archives this stale active file before creating a fresh one.
            active_file.journal_file.journal_header_mut().state = JournalState::Archived as u8;

            // Best/Last-effort sync just to be on the cautious side.
            let _ = active_file.journal_file.sync();
        }
    }
}
