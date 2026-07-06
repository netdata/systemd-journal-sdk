use super::helpers::*;
use super::*;

pub(super) struct RotationState {
    pub(super) size: Option<(u64, u64)>,      // (max, current)
    pub(super) count: Option<(usize, usize)>, // (max, current)
}

impl RotationState {
    pub(super) fn new(rotation_policy: &RotationPolicy) -> Self {
        Self {
            size: rotation_policy.size_of_journal_file.map(|max| (max, 0)),
            count: rotation_policy.number_of_entries.map(|max| (max, 0)),
        }
    }

    pub(super) fn should_rotate(&self) -> bool {
        self.size.is_some_and(|(max, current)| current >= max)
            || self.count.is_some_and(|(max, current)| current >= max)
    }

    pub(super) fn update(&mut self, journal_writer: &JournalWriter) {
        if let Some((_, ref mut current)) = self.size {
            *current = journal_writer.current_file_size();
        }
        if let Some((_, ref mut current)) = self.count {
            *current += 1;
        }
    }

    pub(super) fn observe_existing(&mut self, journal_writer: &JournalWriter, entries: u64) {
        if let Some((_, ref mut current)) = self.size {
            *current = journal_writer.current_file_size();
        }
        if let Some((_, ref mut current)) = self.count {
            *current = entries as usize;
        }
    }

    pub(super) fn reset(&mut self) {
        if let Some((_, ref mut current)) = self.size {
            *current = 0;
        }
        if let Some((_, ref mut current)) = self.count {
            *current = 0;
        }
    }
}

/// Groups a journal file and its writer together
pub(super) struct ActiveFile {
    pub(super) repository_file: repository::File,
    pub(super) journal_file: JournalFile<MmapMut>,
    pub(super) writer: JournalWriter,
}

impl ActiveFile {
    /// Opens an existing online journal file for append.
    pub(super) fn open(
        repository_file: repository::File,
        fallback_boot_id: uuid::Uuid,
    ) -> Result<Self> {
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
    pub(super) fn create(
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
        file_mode: u32,
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
            .with_file_mode(file_mode)
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
    pub(super) fn rotate(
        self,
        chain: &mut OwnedChain,
        max_file_size: Option<u64>,
        _head_realtime: u64,
        compression: Compression,
        compression_threshold: usize,
        strict_systemd_naming: bool,
        live_publish_every_entries: u64,
        file_mode: u32,
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

        let old_journal_file = self.journal_file;
        let mut journal_file = old_journal_file.create_successor_with_file_mode(
            &repository_file,
            max_file_size,
            file_mode,
        )?;
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

    pub(super) fn write_entry_fields<'a>(
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
    pub(super) fn current_file_size(&self) -> u64 {
        self.writer.current_file_size()
    }
}

pub(super) fn replaceable_active_open_error(err: &WriterError) -> bool {
    matches!(
        err,
        WriterError::Journal(JournalError::UnsupportedJournalFile)
    )
}

pub(super) fn open_existing_active_file(
    chain: &mut OwnedChain,
    repository_file: repository::File,
    boot_id: uuid::Uuid,
) -> Result<Option<ActiveFile>> {
    match ActiveFile::open(repository_file.clone(), boot_id) {
        Ok(opened) => {
            if opened.journal_file.journal_header_ref().n_entries == 0 {
                match std::fs::remove_file(repository_file.path()) {
                    Ok(()) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err.into()),
                }
                chain.remove_tracked_file(&repository_file);
                Ok(None)
            } else {
                Ok(Some(opened))
            }
        }
        Err(err) if replaceable_active_open_error(&err) => {
            chain.dispose_replaceable_active_file(&repository_file)?;
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

pub(super) fn initial_sequence_identity(
    chain: &OwnedChain,
    config: &Config,
) -> Result<(uuid::Uuid, uuid::Uuid, u64)> {
    let boot_id = resolve_boot_id(config)?;
    let (seqnum_id, current_seqnum) = chain
        .tail_identity(config.strict_systemd_naming)?
        .unwrap_or_else(|| (uuid::Uuid::new_v4(), 0));
    Ok((boot_id, seqnum_id, current_seqnum))
}

pub(super) fn replace_strict_online_chain_file(
    chain: &mut OwnedChain,
    strict_systemd_naming: bool,
    boot_id: uuid::Uuid,
    sync_on_archive: bool,
) -> Result<()> {
    if !strict_systemd_naming {
        return Ok(());
    }
    let Some(repository_file) = chain.online_chain_file()? else {
        return Ok(());
    };
    let Some(mut opened) = open_existing_active_file(chain, repository_file.clone(), boot_id)?
    else {
        return Ok(());
    };

    use journal_core::file::JournalState;

    chain.update_file_size(&repository_file, opened.current_file_size());
    opened.journal_file.journal_header_mut().state = JournalState::Archived as u8;
    super::sync_archive_journal_file(sync_on_archive, &mut opened.journal_file)?;
    Ok(())
}

pub(super) fn existing_active_file_for_config(
    chain: &mut OwnedChain,
    strict_systemd_naming: bool,
) -> Result<Option<repository::File>> {
    if strict_systemd_naming {
        Ok(chain.existing_active_file())
    } else {
        chain.online_chain_file()
    }
}

pub(super) fn open_existing_active_for_config(
    chain: &mut OwnedChain,
    config: &Config,
    boot_id: uuid::Uuid,
) -> Result<Option<ActiveFile>> {
    let Some(repository_file) =
        existing_active_file_for_config(chain, config.strict_systemd_naming)?
    else {
        return Ok(None);
    };
    open_existing_active_file(chain, repository_file, boot_id)
}

pub(super) fn adopt_active_file_identity(
    active_file: &ActiveFile,
    boot_id: &mut uuid::Uuid,
    seqnum_id: &mut uuid::Uuid,
    current_seqnum: &mut u64,
) {
    let header = active_file.journal_file.journal_header_ref();
    *boot_id = active_file.writer.boot_id();
    *seqnum_id = uuid::Uuid::from_bytes(header.seqnum_id);
    *current_seqnum = header.tail_entry_seqnum;
}

pub(super) fn rotation_state_for_active(
    rotation_policy: &RotationPolicy,
    active_file: Option<&ActiveFile>,
) -> RotationState {
    let mut rotation_state = RotationState::new(rotation_policy);
    if let Some(active_file) = active_file {
        rotation_state.observe_existing(
            &active_file.writer,
            active_file.journal_file.journal_header_ref().n_entries,
        );
    }
    rotation_state
}

pub(super) fn initial_log_clock(
    chain: &OwnedChain,
    strict_systemd_naming: bool,
) -> Result<RealtimeClock> {
    if let Some(tail_realtime) = chain.tail_realtime(strict_systemd_naming)? {
        Ok(RealtimeClock::with_initial(tail_realtime))
    } else {
        Ok(RealtimeClock::new())
    }
}

pub(super) fn initial_last_monotonic(
    chain: &OwnedChain,
    boot_id: uuid::Uuid,
    strict_systemd_naming: bool,
) -> Result<u64> {
    Ok(chain
        .tail_monotonic_for_boot(boot_id, strict_systemd_naming)?
        .unwrap_or(0))
}

pub(super) struct StartupActiveFile {
    pub(super) active_file: Option<ActiveFile>,
    pub(super) boot_id: uuid::Uuid,
    pub(super) seqnum_id: uuid::Uuid,
    pub(super) current_seqnum: u64,
}

pub(super) struct StartupState {
    pub(super) chain: OwnedChain,
    pub(super) config: Config,
    pub(super) active_file: Option<ActiveFile>,
    pub(super) rotation_state: RotationState,
    pub(super) boot_id: uuid::Uuid,
    pub(super) seqnum_id: uuid::Uuid,
    pub(super) current_seqnum: u64,
    pub(super) clock: RealtimeClock,
    pub(super) last_monotonic_usec: u64,
}

pub(super) fn normalize_config(mut config: Config) -> Result<Config> {
    validate_config(&config)?;
    config.rotation_policy = derive_rotation_policy(&config);
    Ok(config)
}

pub(super) fn create_startup_chain(path: &Path, config: &Config) -> Result<OwnedChain> {
    let machine_id = resolve_machine_id(config)?;
    create_chain(path, config.origin.source.clone(), machine_id)
}

pub(super) fn open_startup_active_file(
    chain: &mut OwnedChain,
    config: &Config,
) -> Result<StartupActiveFile> {
    let (mut boot_id, mut seqnum_id, mut current_seqnum) =
        initial_sequence_identity(chain, config)?;
    replace_strict_online_chain_file(
        chain,
        config.strict_systemd_naming,
        boot_id,
        config.sync_on_archive,
    )?;
    let active_file = open_existing_active_for_config(chain, config, boot_id)?;
    if let Some(active_file) = &active_file {
        adopt_active_file_identity(
            active_file,
            &mut boot_id,
            &mut seqnum_id,
            &mut current_seqnum,
        );
    }

    Ok(StartupActiveFile {
        active_file,
        boot_id,
        seqnum_id,
        current_seqnum,
    })
}

pub(super) fn build_startup_state(path: &Path, config: Config) -> Result<StartupState> {
    let config = normalize_config(config)?;
    let mut chain = create_startup_chain(path, &config)?;
    let startup_active = open_startup_active_file(&mut chain, &config)?;
    let rotation_state =
        rotation_state_for_active(&config.rotation_policy, startup_active.active_file.as_ref());
    let last_monotonic_usec =
        initial_last_monotonic(&chain, startup_active.boot_id, config.strict_systemd_naming)?;
    let clock = initial_log_clock(&chain, config.strict_systemd_naming)?;

    Ok(StartupState {
        chain,
        config,
        active_file: startup_active.active_file,
        rotation_state,
        boot_id: startup_active.boot_id,
        seqnum_id: startup_active.seqnum_id,
        current_seqnum: startup_active.current_seqnum,
        clock,
        last_monotonic_usec,
    })
}
