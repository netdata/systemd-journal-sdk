mod chain;
use chain::OwnedChain;

mod config;
pub use config::{Config, LogIdentityMode, LogOpenMode, RetentionPolicy, RotationPolicy};

mod helpers;
mod startup;
use helpers::*;
use startup::{ActiveFile, RotationState, build_startup_state};

use crate::{Result, WriterError};
use itoa::Buffer as ItoaBuffer;
pub use journal_common::EntryTimestamps;
use journal_common::{Microseconds, RealtimeClock};
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

/// Tracks rotation state for size and count limits.
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

    fn append_rotation_reason(&self) -> LogLifecycleReason {
        if self.active_file.is_none() {
            LogLifecycleReason::Append
        } else {
            LogLifecycleReason::Rotation
        }
    }

    fn prepare_append_for_realtime(&mut self, entry_realtime: u64) -> Result<()> {
        self.apply_retention_on_open()?;
        let opened_first_active = self.active_file.is_none();
        if self.should_rotate_for_realtime(entry_realtime) {
            self.rotate(entry_realtime, self.append_rotation_reason())?;
            if opened_first_active {
                self.retention_on_open_applied = true;
            }
        }
        self.apply_retention_on_open()
    }

    fn raw_items_for_policy<'a>(&self, items: &'a [&'a [u8]]) -> Result<Option<Vec<&'a [u8]>>> {
        if self.config.field_name_policy != FieldNamePolicy::JournalApp {
            return Ok(None);
        }
        let filtered_items = filter_raw_items_for_journal_app(items)?;
        if filtered_items.is_empty() {
            return Err(WriterError::EmptyEntry);
        }
        Ok(Some(filtered_items))
    }

    fn structured_fields_for_policy<'a>(
        &self,
        fields: &'a [StructuredField<'a>],
    ) -> Result<Option<Vec<StructuredField<'a>>>> {
        if self.config.field_name_policy != FieldNamePolicy::JournalApp {
            return Ok(None);
        }
        let filtered_fields = filter_structured_fields_for_journal_app(fields);
        if filtered_fields.is_empty() {
            return Err(WriterError::EmptyEntry);
        }
        Ok(Some(filtered_fields))
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

        let desired_monotonic = timestamp_override
            .and_then(|ts| ts.entry_monotonic_usec)
            .ok_or_else(|| {
                WriterError::InvalidConfig("entry monotonic timestamp is required".to_string())
            })?;

        let monotonic = if desired_monotonic > self.last_monotonic_usec {
            desired_monotonic
        } else {
            self.last_monotonic_usec.saturating_add(1)
        };
        self.last_monotonic_usec = monotonic;

        Ok((realtime, monotonic))
    }

    fn require_entry_monotonic(timestamps: &EntryTimestamps) -> Result<()> {
        if timestamps.entry_monotonic_usec.is_none() {
            return Err(WriterError::InvalidConfig(
                "entry monotonic timestamp is required".to_string(),
            ));
        }
        Ok(())
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
        config: Config,
        lifecycle_observer: Option<Arc<dyn LogLifecycleObserver>>,
        artifact_sizer: Option<Arc<dyn LogArtifactSizer>>,
    ) -> Result<Self> {
        let startup = build_startup_state(path, config)?;

        let mut log = Log {
            configured_dir: path.to_path_buf(),
            chain: startup.chain,
            config: startup.config,
            active_file: startup.active_file,
            rotation_state: startup.rotation_state,
            boot_id: startup.boot_id,
            seqnum_id: startup.seqnum_id,
            current_seqnum: startup.current_seqnum,
            clock: startup.clock,
            last_monotonic_usec: startup.last_monotonic_usec,
            lifecycle_observer,
            artifact_sizer,
            retention_on_open_applied: false,
            boot_id_field: format!("_BOOT_ID={}", startup.boot_id.as_simple()).into_bytes(),
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
    /// This compatibility method always returns an error under the strict
    /// writer contract. Use [`Log::write_entry_with_timestamps`] and provide
    /// an explicit entry monotonic timestamp.
    #[deprecated(
        since = "0.7.2",
        note = "use write_entry_with_timestamps and provide an explicit entry monotonic timestamp"
    )]
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
        Self::require_entry_monotonic(&timestamps)?;

        let entry_realtime = self.peek_entry_realtime(&timestamps);
        self.prepare_append_for_realtime(entry_realtime)?;
        let filtered_items = self.raw_items_for_policy(items)?;
        let write_items = filtered_items.as_deref().unwrap_or(items);

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
    ///
    /// This compatibility method always returns an error under the strict
    /// writer contract. Use [`Log::write_fields_with_timestamps`] and provide
    /// an explicit entry monotonic timestamp.
    #[deprecated(
        since = "0.7.2",
        note = "use write_fields_with_timestamps and provide an explicit entry monotonic timestamp"
    )]
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
    /// Entry monotonic timestamp is required. Entry realtime and monotonic
    /// overrides use the same clamping rules as
    /// [`Log::write_entry_with_timestamps`].
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
        Self::require_entry_monotonic(&timestamps)?;

        let entry_realtime = self.peek_entry_realtime(&timestamps);
        self.prepare_append_for_realtime(entry_realtime)?;
        let filtered_fields = self.structured_fields_for_policy(fields)?;
        let write_fields = filtered_fields.as_deref().unwrap_or(fields);

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

    fn update_active_file_size(&mut self) {
        if let Some(active_file) = &self.active_file {
            self.chain.update_file_size(
                &active_file.repository_file,
                active_file.current_file_size(),
            );
        }
    }

    fn prepare_initial_rotation(&mut self) -> Result<()> {
        self.update_active_file_size();
        if self.active_file.is_none() && self.config.strict_systemd_naming {
            self.chain.archive_existing_active_file()?;
        }
        Ok(())
    }

    fn archive_rotated_file(&mut self, old_file: &ActiveFile) -> Result<repository::File> {
        if !self.config.strict_systemd_naming {
            return Ok(old_file.repository_file.clone());
        }
        let old_header = old_file.journal_file.journal_header_ref();
        self.chain.archive_file(
            &old_file.repository_file,
            uuid::Uuid::from_bytes(old_header.seqnum_id),
            old_header.head_entry_seqnum,
            old_header.head_entry_realtime,
        )
    }

    fn rotate_existing_active_file(
        &mut self,
        mut old_file: ActiveFile,
        max_file_size: Option<u64>,
        head_realtime: u64,
    ) -> Result<(ActiveFile, LogLifecycleEvent)> {
        use journal_core::file::JournalState;

        old_file.journal_file.journal_header_mut().state = JournalState::Archived as u8;
        old_file.journal_file.sync()?;
        let archived = self.archive_rotated_file(&old_file)?;
        let new_file = old_file.rotate(
            &mut self.chain,
            max_file_size,
            head_realtime,
            self.config.compression,
            self.config.compression_threshold,
            self.config.strict_systemd_naming,
            self.config.live_publish_every_entries,
            self.config.file_mode,
        )?;
        let active = new_file.repository_file.clone();
        Ok((new_file, LogLifecycleEvent::Rotated { archived, active }))
    }

    fn create_initial_active_file(
        &mut self,
        max_file_size: Option<u64>,
        head_realtime: u64,
        reason: LogLifecycleReason,
    ) -> Result<(ActiveFile, LogLifecycleEvent)> {
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
            self.config.file_mode,
        )?;
        let active = new_file.repository_file.clone();
        Ok((new_file, LogLifecycleEvent::Created { active, reason }))
    }

    fn emit_lifecycle_event(&self, event: &LogLifecycleEvent) {
        if let Some(observer) = &self.lifecycle_observer {
            observer.on_event(event);
        }
    }

    fn protected_active_file(&self) -> Option<repository::File> {
        self.active_file
            .as_ref()
            .map(|active_file| active_file.repository_file.clone())
    }

    #[tracing::instrument(skip_all, fields(active_file))]
    fn rotate(&mut self, head_realtime: u64, reason: LogLifecycleReason) -> Result<()> {
        self.prepare_initial_rotation()?;
        let max_file_size = self.config.rotation_policy.size_of_journal_file;
        let (new_file, lifecycle_event) = if let Some(old_file) = self.active_file.take() {
            self.rotate_existing_active_file(old_file, max_file_size, head_realtime)?
        } else {
            self.create_initial_active_file(max_file_size, head_realtime, reason)?
        };

        tracing::Span::current().record("new_file", new_file.repository_file.path());

        self.active_file = Some(new_file);
        self.rotation_state.reset();
        self.update_active_file_size();
        self.emit_lifecycle_event(&lifecycle_event);

        // Retention runs after the post-rotation current file is known, so the
        // tracked current file counts in the envelope and is never deleted.
        let protected_file = self.protected_active_file();
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
    /// use journal_log_writer::{Config, EntryTimestamps, Log, RotationPolicy, RetentionPolicy};
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
    ///     machine_id: Some("00112233445566778899aabbccddeeff".parse()?),
    ///     namespace: None,
    ///     source: journal_registry::Source::System,
    /// };
    /// let config = Config::new(origin, RotationPolicy::default(), RetentionPolicy::default())
    ///     .with_boot_id("ffeeddccbbaa99887766554433221100".parse()?);
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
    /// let timestamps = EntryTimestamps::default()
    ///     .with_entry_realtime_usec(1_700_000_000_000_000)
    ///     .with_entry_monotonic_usec(1);
    /// log.write_structured_with_timestamps(&entry, timestamps)?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "serde-api")]
    #[deprecated(
        since = "0.7.2",
        note = "use write_structured_with_timestamps and provide an explicit entry monotonic timestamp"
    )]
    pub fn write_structured<T: serde::Serialize>(&mut self, value: &T) -> Result<()> {
        self.write_structured_with_timestamps(value, EntryTimestamps::default())
    }

    /// Writes a journal entry from a serializable value with explicit timestamps.
    #[cfg(feature = "serde-api")]
    pub fn write_structured_with_timestamps<T: serde::Serialize>(
        &mut self,
        value: &T,
        timestamps: EntryTimestamps,
    ) -> Result<()> {
        // Serialize to JSON value
        let json_value = serde_json::to_value(value).map_err(|e| {
            WriterError::Serialization(format!("failed to serialize to JSON: {}", e))
        })?;

        // Flatten the JSON structure - requires a JSON object (Map)
        let flattened = if let serde_json::Value::Object(map) = json_value {
            flatten_json_map(&map)
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

        self.write_entry_with_timestamps(&field_refs, timestamps)
    }
}

#[cfg(all(test, feature = "serde-api"))]
mod serde_api_tests;

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
