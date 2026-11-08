use super::file_payload::ResolvedDataLinkState;
use super::mmap::MmapMut;
use crate::error::{JournalError, Result};
use crate::file::{
    Compression, DEFAULT_COMPRESS_THRESHOLD, DataObject, DataPayloadType, EntryObjectHeader,
    FieldObjectHeader, HashableObjectMut, HeaderIncompatibleFlags, JournalFile, JournalHeader,
    ObjectFlags, ObjectType, PayloadParts, hash::jenkins_hash64_parts,
    normalize_compress_threshold, valid_monotonic, valid_realtime,
};
use rustc_hash::FxHashMap;
use std::io::Cursor;
use std::num::NonZeroU64;

pub(super) const OBJECT_ALIGNMENT: u64 = 8;
pub(super) const JOURNAL_COMPACT_SIZE_MAX: u64 = u32::MAX as u64;
const FILE_SIZE_INCREASE: u64 = 8 * 1024 * 1024;
/// The highest object end an append may reach: the arena grows in 8 MiB steps
/// and the file must stay below 4 GiB.
pub(super) const JOURNAL_APPEND_CEILING: u64 = JOURNAL_COMPACT_SIZE_MAX & !(FILE_SIZE_INCREASE - 1);
/// Entries with up to this many fields are buffered on the stack for the
/// growth pre-check.
const STACK_ENTRY_FIELDS: usize = 64;
#[cfg(test)]
thread_local! {
    /// Runs after each entry item is prepared, before the ENTRY object, so
    /// tests can observe a writer mid-append.
    pub(super) static AFTER_ENTRY_ITEM: std::cell::RefCell<Option<Box<dyn FnMut()>>> =
        const { std::cell::RefCell::new(None) };
}

const FIELD_CACHE_MAX_ENTRIES: usize = 1024;
const FIELD_CACHE_MAX_PAYLOAD_LEN: usize = 128;
pub(super) fn round_up_to_file_size_increment(value: u64) -> Result<u64> {
    value
        .checked_add(FILE_SIZE_INCREASE - 1)
        .map(|v| v & !(FILE_SIZE_INCREASE - 1))
        .ok_or(JournalError::ObjectExceedsFileBounds)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EntryItem {
    pub(super) offset: NonZeroU64,
    pub(super) hash: u64,
    pub(super) link_state: Option<ResolvedDataLinkState>,
}

#[derive(Debug, Clone, Copy)]
pub struct StructuredField<'a> {
    /// Field name without the `=` separator.
    pub name: &'a [u8],
    /// Field value bytes. Values may contain NUL bytes and `=` bytes.
    pub value: &'a [u8],
}

impl<'a> StructuredField<'a> {
    /// Creates a structured journal field from a name and binary-safe value.
    pub fn new(name: &'a [u8], value: &'a [u8]) -> Self {
        Self { name, value }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EntryField<'a> {
    /// Full `KEY=value` payload, matching systemd's low-level writer shape.
    Raw(&'a [u8]),
    /// Split field name and value, avoiding `KEY=value` reconstruction for
    /// already-structured producers.
    Structured(StructuredField<'a>),
}

impl<'a> EntryField<'a> {
    /// Creates a raw full-field entry item from a `KEY=value` byte payload.
    pub fn raw(payload: &'a [u8]) -> Self {
        Self::Raw(payload)
    }

    /// Creates a structured entry item from a name and binary-safe value.
    pub fn structured(name: &'a [u8], value: &'a [u8]) -> Self {
        Self::Structured(StructuredField::new(name, value))
    }

    /// Length of the full `KEY=value` payload.
    fn payload_len(self) -> usize {
        match self {
            Self::Raw(payload) => payload.len(),
            Self::Structured(field) => field.name.len() + 1 + field.value.len(),
        }
    }

    fn payload_parts(self) -> PayloadParts<'a> {
        match self {
            Self::Raw(payload) => PayloadParts::raw(payload),
            Self::Structured(field) => PayloadParts::structured(field.name, field.value),
        }
    }

    fn field_name(self) -> Option<&'a [u8]> {
        match self {
            Self::Raw(payload) => payload
                .iter()
                .position(|&b| b == b'=')
                .map(|pos| &payload[..pos]),
            Self::Structured(field) => Some(field.name),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FieldNamePolicy {
    /// Trusted journald-compatible field names. Protected `_...` names are
    /// allowed.
    #[default]
    Journald,
    /// Journal DATA structure capability only. Stock systemd tooling
    /// compatibility is not guaranteed for names outside JOURNALD.
    Raw,
    /// Untrusted application input accepted by journald. Invalid or protected
    /// caller fields are dropped.
    JournalApp,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EntryWriteOptions {
    /// Skips duplicate DATA reference elimination for this ENTRY.
    ///
    /// Set this only when the caller guarantees that the entry contains no
    /// duplicate full `KEY=value` payloads after field-name policy filtering.
    /// Offset sorting by DATA object offset is always performed regardless of
    /// this flag.
    /// Misuse can write duplicate DATA offsets into one ENTRY object. Keep the
    /// default `false` unless the producer owns and enforces that invariant.
    pub trusted_unique_payloads: bool,
    /// Field-name validation policy for caller-provided fields.
    pub field_name_policy: FieldNamePolicy,
    /// Optional low-level ENTRY seqnum override.
    ///
    /// This is for exact journal regeneration and must be monotonically
    /// increasing relative to previously written entries. Leave unset for the
    /// normal systemd-style auto-incrementing sequence.
    pub seqnum: Option<u64>,
    /// Optional low-level ENTRY boot ID override.
    ///
    /// This is for exact journal regeneration of multi-boot files. Leave unset
    /// for the normal writer-wide boot ID.
    pub boot_id: Option<uuid::Uuid>,
}

impl EntryWriteOptions {
    /// Enables or disables the trusted unique-payload fast path.
    ///
    /// See [`EntryWriteOptions::trusted_unique_payloads`] for the caller
    /// invariant required before enabling this option.
    pub fn trusted_unique_payloads(mut self, enabled: bool) -> Self {
        self.trusted_unique_payloads = enabled;
        self
    }

    /// Selects the field-name validation policy for caller-provided fields.
    pub fn field_name_policy(mut self, policy: FieldNamePolicy) -> Self {
        self.field_name_policy = policy;
        self
    }

    /// Uses a caller-provided ENTRY seqnum for this entry.
    pub fn seqnum(mut self, seqnum: u64) -> Self {
        self.seqnum = Some(seqnum);
        self
    }

    /// Uses a caller-provided ENTRY boot ID for this entry.
    pub fn boot_id(mut self, boot_id: uuid::Uuid) -> Self {
        self.boot_id = Some(boot_id);
        self
    }
}

fn is_journal_field_name_valid(field_name: &[u8], allow_protected: bool) -> bool {
    if field_name.is_empty() || field_name.len() > 64 {
        return false;
    }
    if field_name[0] == b'_' && !allow_protected {
        return false;
    }
    if field_name[0].is_ascii_digit() {
        return false;
    }
    field_name
        .iter()
        .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

fn is_raw_field_name_valid(field_name: &[u8]) -> bool {
    !field_name.is_empty() && !field_name.contains(&b'=')
}

#[doc(hidden)]
pub fn accept_entry_field(field: EntryField<'_>, policy: FieldNamePolicy) -> Result<bool> {
    let Some(field_name) = field.field_name() else {
        return Err(JournalError::InvalidField);
    };
    let valid = match policy {
        FieldNamePolicy::Raw => is_raw_field_name_valid(field_name),
        FieldNamePolicy::Journald => is_journal_field_name_valid(field_name, true),
        FieldNamePolicy::JournalApp => is_journal_field_name_valid(field_name, false),
    };
    if valid {
        return Ok(true);
    }
    if matches!(policy, FieldNamePolicy::JournalApp) {
        return Ok(false);
    }
    Err(JournalError::InvalidField)
}

#[derive(Debug)]
struct FieldCache {
    entries: FxHashMap<Box<[u8]>, NonZeroU64>,
}

impl FieldCache {
    fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
        }
    }

    fn get(&self, payload: &[u8]) -> Option<NonZeroU64> {
        self.entries.get(payload).copied()
    }

    fn insert(&mut self, payload: &[u8], offset: NonZeroU64) {
        if payload.len() > FIELD_CACHE_MAX_PAYLOAD_LEN {
            return;
        }

        if self.entries.len() >= FIELD_CACHE_MAX_ENTRIES && self.entries.get(payload).is_none() {
            self.entries.clear();
        }

        self.entries
            .insert(payload.to_vec().into_boxed_slice(), offset);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

enum StoredDataPayload<'a> {
    Uncompressed(PayloadParts<'a>),
    Compressed(Vec<u8>, u8),
}

impl StoredDataPayload<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Uncompressed(payload) => payload.len(),
            Self::Compressed(payload, _) => payload.len(),
        }
    }

    fn object_flags(&self) -> u8 {
        match self {
            Self::Uncompressed(_) => 0,
            Self::Compressed(_, flags) => *flags,
        }
    }

    fn copy_to_data_object(&self, data: &mut DataObject<&mut [u8]>) {
        match self {
            Self::Uncompressed(payload) => match &mut data.payload {
                DataPayloadType::Regular(dst) => payload.copy_to_slice(dst),
                DataPayloadType::Compact { payload: dst, .. } => payload.copy_to_slice(dst),
            },
            Self::Compressed(payload, _) => data.set_payload(payload),
        }
    }
}

pub struct JournalWriter {
    poisoned: bool,
    #[cfg(test)]
    fail_append_stage: u8,
    pub(super) tail_object_offset: NonZeroU64,
    pub(super) append_offset: NonZeroU64,
    next_seqnum: u64,
    num_written_objects: u64,
    pub(super) first_tag_written: bool,
    pub(super) entry_items: Vec<EntryItem>,
    field_cache: FieldCache,
    first_entry_monotonic: Option<u64>,
    boot_id: uuid::Uuid,
    compression: Compression,
    compress_threshold: usize,
    live_publish_every_entries: u64,
    entries_since_live_publication: u64,
    pub(super) seal: Option<crate::seal::SealState>,
    /// Reserved growth of the entry being appended; entry arrays stop doubling
    /// when doubling would eat into it.
    pub(super) entry_growth_bound: u64,
}

impl JournalWriter {
    /// True after a mutating failure. Drop resources without publishing a clean state.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    #[cfg(test)]
    fn fail_append_at(&self, stage: u8) -> Result<()> {
        if self.fail_append_stage == stage {
            return Err(JournalError::Io(std::io::Error::other(
                "injected append failure",
            )));
        }
        Ok(())
    }

    /// Get current file size in bytes
    pub fn current_file_size(&self) -> u64 {
        self.append_offset.get()
    }

    /// Get the monotonic timestamp of the first entry written to this file
    pub fn first_entry_monotonic(&self) -> Option<u64> {
        self.first_entry_monotonic
    }

    /// Get the next sequence number that will be written
    pub fn next_seqnum(&self) -> u64 {
        self.next_seqnum
    }

    /// Get the boot ID for this writer
    pub fn boot_id(&self) -> uuid::Uuid {
        self.boot_id
    }

    /// Sets how often the writer explicitly publishes live-reader visibility.
    ///
    /// `1` is the default and matches systemd-style publication after every
    /// appended entry. `0` disables this explicit publication; closed-file
    /// verification and reads after sync/close are unchanged, but stock
    /// follow-reader visibility while the writer is active is not guaranteed.
    /// Values greater than `1` publish after every N appended entries.
    pub fn set_live_publish_every_entries(&mut self, entries: u64) {
        self.live_publish_every_entries = entries;
        self.entries_since_live_publication = 0;
    }

    /// Returns the configured live-reader publication cadence.
    ///
    /// See [`JournalWriter::set_live_publish_every_entries`] for the meaning of
    /// `0`, `1`, and larger values.
    pub fn live_publish_every_entries(&self) -> u64 {
        self.live_publish_every_entries
    }

    pub fn new(
        journal_file: &mut JournalFile<MmapMut>,
        next_seqnum: u64,
        boot_id: uuid::Uuid,
    ) -> Result<Self> {
        let compression = match journal_file.journal_header_ref() {
            header if header.has_incompatible_flag(HeaderIncompatibleFlags::CompressedZstd) => {
                Compression::Zstd
            }
            header if header.has_incompatible_flag(HeaderIncompatibleFlags::CompressedXz) => {
                Compression::Xz
            }
            header if header.has_incompatible_flag(HeaderIncompatibleFlags::CompressedLz4) => {
                Compression::Lz4
            }
            _ => Compression::None,
        };

        Self::new_with_compression(
            journal_file,
            next_seqnum,
            boot_id,
            compression,
            DEFAULT_COMPRESS_THRESHOLD,
        )
    }

    pub fn new_with_compression(
        journal_file: &mut JournalFile<MmapMut>,
        next_seqnum: u64,
        boot_id: uuid::Uuid,
        compression: Compression,
        compress_threshold: usize,
    ) -> Result<Self> {
        let current_header_size = std::mem::size_of::<JournalHeader>() as u64;
        let header = journal_file.journal_header_ref();
        if !header.has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash)
            || header.header_size < current_header_size
        {
            return Err(JournalError::UnsupportedJournalFile);
        }

        journal_file.validate_committed_arena()?;
        header.validate_empty_entry_metadata()?;

        let append_offset = {
            let header = journal_file.journal_header_ref();

            let Some(tail_object_offset) = header.tail_object_offset else {
                return Err(JournalError::InvalidMagicNumber);
            };

            let tail_object = journal_file.object_header_ref(tail_object_offset)?;

            tail_object_offset
                .checked_add(tail_object.aligned_size())
                .ok_or(JournalError::ObjectExceedsFileBounds)?
        };

        let seal = journal_file
            .seal_options
            .as_ref()
            .map(|opts| crate::seal::SealState::new(opts))
            .transpose()?;

        let mut writer = Self {
            poisoned: false,
            #[cfg(test)]
            fail_append_stage: 0,
            tail_object_offset: journal_file
                .journal_header_ref()
                .tail_object_offset
                .unwrap(),
            append_offset,
            next_seqnum,
            num_written_objects: 0,
            first_tag_written: false,
            entry_items: Vec::with_capacity(128),
            field_cache: FieldCache::new(),
            first_entry_monotonic: None,
            boot_id,
            compression,
            compress_threshold: normalize_compress_threshold(compress_threshold),
            live_publish_every_entries: 1,
            entries_since_live_publication: 0,
            seal,
            entry_growth_bound: 0,
        };

        if writer.seal.is_some() && journal_file.journal_header_ref().n_tags == 0 {
            writer.ensure_first_tag(journal_file)?;
            {
                let header = journal_file.journal_header_mut();
                header.n_objects += writer.num_written_objects;
                header.tail_object_offset = Some(writer.tail_object_offset);
            }
            writer.num_written_objects = 0;
        }

        Ok(writer)
    }

    /// Creates a successor writer for a new journal file
    pub fn create_successor(&self, journal_file: &mut JournalFile<MmapMut>) -> Result<Self> {
        if self.poisoned {
            return Err(JournalError::WriterPoisoned);
        }
        Self::new_with_compression(
            journal_file,
            self.next_seqnum,
            self.boot_id,
            self.compression,
            self.compress_threshold,
        )
    }

    pub fn add_entry(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        items: &[&[u8]],
        realtime: u64,
        monotonic: u64,
    ) -> Result<()> {
        self.add_entry_fields_with_options(
            journal_file,
            items.iter().copied().map(EntryField::raw),
            realtime,
            monotonic,
            EntryWriteOptions::default(),
        )
    }

    pub fn add_entry_structured(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        fields: &[StructuredField<'_>],
        realtime: u64,
        monotonic: u64,
    ) -> Result<()> {
        self.add_entry_fields_with_options(
            journal_file,
            fields.iter().copied().map(EntryField::Structured),
            realtime,
            monotonic,
            EntryWriteOptions::default(),
        )
    }

    pub fn add_entry_structured_with_options(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        fields: &[StructuredField<'_>],
        realtime: u64,
        monotonic: u64,
        options: EntryWriteOptions,
    ) -> Result<()> {
        self.add_entry_fields_with_options(
            journal_file,
            fields.iter().copied().map(EntryField::Structured),
            realtime,
            monotonic,
            options,
        )
    }

    pub fn add_entry_fields<'a>(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        fields: impl IntoIterator<Item = EntryField<'a>>,
        realtime: u64,
        monotonic: u64,
    ) -> Result<()> {
        self.add_entry_fields_with_options(
            journal_file,
            fields,
            realtime,
            monotonic,
            EntryWriteOptions::default(),
        )
    }

    pub fn add_entry_fields_with_options<'a>(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        fields: impl IntoIterator<Item = EntryField<'a>>,
        realtime: u64,
        monotonic: u64,
        options: EntryWriteOptions,
    ) -> Result<()> {
        if self.poisoned {
            return Err(JournalError::WriterPoisoned);
        }
        self.ensure_keyed_append(journal_file)?;
        let entry_seqnum = self.entry_seqnum_for_options(options)?;
        let entry_boot_id = options.boot_id.unwrap_or(self.boot_id);
        let monotonic = self.clamp_same_boot_monotonic(journal_file, entry_boot_id, monotonic)?;
        if !valid_realtime(realtime) || !valid_monotonic(monotonic) {
            return Err(JournalError::TimestampOutOfRange);
        }
        if let Some(seal) = &self.seal {
            seal.need_evolve(realtime)?;
        }
        // The growth pre-check needs every field before the first write.
        let mut stack = [EntryField::Raw(&[]); STACK_ENTRY_FIELDS];
        let mut heap = Vec::new();
        let mut count = 0;
        for field in fields {
            if count < STACK_ENTRY_FIELDS {
                stack[count] = field;
            } else {
                if heap.is_empty() {
                    heap.extend_from_slice(&stack);
                }
                heap.push(field);
            }
            count += 1;
        }
        let fields = if heap.is_empty() {
            &stack[..count]
        } else {
            &heap[..]
        };
        let bound = self.bound_entry_growth(journal_file, fields, realtime)?;
        let append_offset = self.append_offset.get();
        if append_offset > JOURNAL_APPEND_CEILING || bound > JOURNAL_APPEND_CEILING - append_offset
        {
            return Err(JournalError::JournalFileFull);
        }
        self.entry_growth_bound = bound;
        let result = self.add_bounded_entry(
            journal_file,
            fields,
            entry_seqnum,
            entry_boot_id,
            realtime,
            monotonic,
            options,
        );
        self.entry_growth_bound = 0;
        result
    }

    /// Bounds the bytes one entry can append from payload sizes alone, without
    /// lookups: every field as a new DATA and FIELD object (a field name is
    /// shorter than its payload) with a minimum entry array, the ENTRY object, a
    /// minimum global entry array, and the TAGs it may write. Saturates above
    /// `JOURNAL_APPEND_CEILING`.
    pub(super) fn bound_entry_growth(
        &self,
        journal_file: &JournalFile<MmapMut>,
        fields: &[EntryField<'_>],
        realtime: u64,
    ) -> Result<u64> {
        const OVER: u64 = JOURNAL_APPEND_CEILING + 1;
        let align = |n: u64| (n + (OBJECT_ALIGNMENT - 1)) & !(OBJECT_ALIGNMENT - 1);
        let is_compact = journal_file
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        let (entry_item, array_item) = if is_compact { (4, 4) } else { (16, 8) };
        let min_array = align(
            std::mem::size_of::<super::object::OffsetArrayObjectHeader>() as u64 + 4 * array_item,
        );
        let field_header = std::mem::size_of::<FieldObjectHeader>() as u64;
        let mut bound = align(
            std::mem::size_of::<EntryObjectHeader>() as u64 + fields.len() as u64 * entry_item,
        ) + min_array;
        for field in fields {
            let n = field.payload_len() as u64;
            if n > JOURNAL_APPEND_CEILING || bound > JOURNAL_APPEND_CEILING {
                return Ok(OVER);
            }
            bound +=
                align(Self::data_object_size(is_compact, n)) + align(field_header + n) + min_array;
        }
        if let Some(seal) = &self.seal {
            // add_entry_fields_with_options validated the epoch.
            let mut tags = seal.goal_epoch(realtime)? - seal.epoch();
            if !self.first_tag_written {
                tags += 1;
            }
            let tag = align(
                (std::mem::size_of::<super::object::ObjectHeader>() + 16 + crate::seal::TAG_LENGTH)
                    as u64,
            );
            if tags > JOURNAL_APPEND_CEILING / tag {
                return Ok(OVER);
            }
            bound += tags * tag;
        }
        Ok(bound)
    }

    /// Returns the doubled capacity, or the 4-slot minimum the growth bound
    /// reserved when doubling could leave the rest of the entry without room
    /// below the ceiling. Readers use each array's own size.
    pub(super) fn entry_array_capacity(
        &self,
        journal_file: &JournalFile<MmapMut>,
        capacity: u64,
    ) -> u64 {
        if capacity <= 4 || self.entry_growth_bound == 0 {
            return capacity;
        }
        let is_compact = journal_file
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact);
        let align = |n: u64| (n + (OBJECT_ALIGNMENT - 1)) & !(OBJECT_ALIGNMENT - 1);
        let reserve = align(self.append_offset.get()) + self.entry_growth_bound;
        let Some(room) = JOURNAL_APPEND_CEILING.checked_sub(reserve) else {
            return 4;
        };
        let item = if is_compact { 4 } else { 8 };
        if capacity > room / item
            || align(
                std::mem::size_of::<super::object::OffsetArrayObjectHeader>() as u64
                    + capacity * item,
            ) > room
        {
            return 4;
        }
        capacity
    }

    #[allow(clippy::too_many_arguments)]
    fn add_bounded_entry(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        fields: &[EntryField<'_>],
        entry_seqnum: u64,
        entry_boot_id: uuid::Uuid,
        realtime: u64,
        monotonic: u64,
        options: EntryWriteOptions,
    ) -> Result<()> {
        let xor_hash =
            self.prepare_entry_items(journal_file, fields.iter().copied(), realtime, options)?;
        #[cfg(test)]
        self.fail_append_at(1)?;
        let entry_offset = self.write_entry_object(
            journal_file,
            entry_seqnum,
            entry_boot_id,
            realtime,
            monotonic,
            xor_hash,
        )?;
        #[cfg(test)]
        self.fail_append_at(2)?;
        self.publish_entry_links(journal_file, entry_offset)?;
        #[cfg(test)]
        self.fail_append_at(3)?;
        self.entry_added(
            journal_file.journal_header_mut(),
            entry_offset,
            entry_seqnum,
            entry_boot_id,
            realtime,
            monotonic,
        );
        #[cfg(test)]
        self.fail_append_at(4)?;
        self.publish_after_entry(journal_file)?;
        self.poisoned = false;
        Ok(())
    }

    fn ensure_keyed_append(&self, journal_file: &JournalFile<MmapMut>) -> Result<()> {
        let header = journal_file.journal_header_ref();
        if header.has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash) {
            return Ok(());
        }
        Err(JournalError::UnsupportedJournalFile)
    }

    fn entry_seqnum_for_options(&self, options: EntryWriteOptions) -> Result<u64> {
        let entry_seqnum = options.seqnum.unwrap_or(self.next_seqnum);
        if entry_seqnum == 0 || entry_seqnum == u64::MAX || entry_seqnum < self.next_seqnum {
            return Err(JournalError::InvalidField);
        }
        Ok(entry_seqnum)
    }

    fn clamp_same_boot_monotonic(
        &self,
        journal_file: &JournalFile<MmapMut>,
        entry_boot_id: uuid::Uuid,
        monotonic: u64,
    ) -> Result<u64> {
        let header = journal_file.journal_header_ref();
        if header.n_entries == 0
            || header.tail_entry_boot_id != *entry_boot_id.as_bytes()
            || monotonic > header.tail_entry_monotonic
        {
            return Ok(monotonic);
        }
        header
            .tail_entry_monotonic
            .checked_add(1)
            .ok_or(JournalError::InvalidField)
    }

    fn prepare_entry_items<'a>(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        fields: impl IntoIterator<Item = EntryField<'a>>,
        realtime: u64,
        options: EntryWriteOptions,
    ) -> Result<u64> {
        let mut xor_hash = 0;
        self.entry_items.clear();
        let mut publication_ready = false;
        for field in fields {
            if !accept_entry_field(field, options.field_name_policy)? {
                continue;
            }
            self.ensure_entry_publication_ready(journal_file, realtime, &mut publication_ready)?;
            xor_hash ^= self.add_entry_field_item(journal_file, field)?;
            #[cfg(test)]
            AFTER_ENTRY_ITEM.with(|hook| {
                if let Some(hook) = hook.borrow_mut().as_mut() {
                    hook();
                }
            });
        }
        self.finish_entry_items(options.trusted_unique_payloads)?;
        Ok(xor_hash)
    }

    fn ensure_entry_publication_ready(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        realtime: u64,
        publication_ready: &mut bool,
    ) -> Result<()> {
        if *publication_ready {
            return Ok(());
        }
        // Set before any potentially partial I/O; only a fully published append clears it.
        // Seal state is publication state too, even before the next file write.
        if let Some(seal) = &self.seal {
            if !self.first_tag_written || seal.need_evolve(realtime)? {
                self.poisoned = true;
            }
        }
        self.ensure_first_tag(journal_file)?;
        self.maybe_append_tag(journal_file, realtime)?;
        *publication_ready = true;
        Ok(())
    }

    fn add_entry_field_item(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        field: EntryField<'_>,
    ) -> Result<u64> {
        let entry_item = self.add_data(journal_file, field)?;
        self.entry_items.push(entry_item);
        Ok(jenkins_hash64_parts(field.payload_parts().iter()))
    }

    fn finish_entry_items(&mut self, trusted_unique_payloads: bool) -> Result<()> {
        if self.entry_items.is_empty() {
            return Err(JournalError::InvalidField);
        }
        if !self.entry_items_are_sorted() {
            self.entry_items
                .sort_unstable_by(|a, b| a.offset.cmp(&b.offset));
        }
        if !trusted_unique_payloads {
            self.entry_items.dedup_by(|a, b| a.offset == b.offset);
        } else {
            for index in 1..self.entry_items.len() {
                if self.entry_items[index - 1].offset == self.entry_items[index].offset {
                    self.entry_items[index].link_state = None;
                }
            }
        }
        Ok(())
    }

    fn entry_items_are_sorted(&self) -> bool {
        self.entry_items
            .windows(2)
            .all(|items| items[0].offset <= items[1].offset)
    }

    fn write_entry_object(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_seqnum: u64,
        entry_boot_id: uuid::Uuid,
        realtime: u64,
        monotonic: u64,
        xor_hash: u64,
    ) -> Result<NonZeroU64> {
        let entry_offset = self.append_offset;
        let is_compact = Self::is_compact(journal_file);
        let entry_payload_size = self.entry_items.len() as u64 * Self::entry_item_size(is_compact);
        Self::ensure_compact_object_fits(
            is_compact,
            entry_offset,
            std::mem::size_of::<EntryObjectHeader>() as u64 + entry_payload_size,
        )?;
        self.poisoned = true;
        let entry_size = {
            let size = Some(entry_payload_size);
            let mut entry_guard = journal_file.entry_mut(entry_offset, size)?;

            entry_guard.header.seqnum = entry_seqnum;
            entry_guard.header.xor_hash = xor_hash;
            entry_guard.header.boot_id = *entry_boot_id.as_bytes();
            entry_guard.header.monotonic = monotonic;
            entry_guard.header.realtime = realtime;

            // set each entry item
            for (index, entry_item) in self.entry_items.iter().enumerate() {
                Self::ensure_compact_offset(is_compact, entry_item.offset)?;
                let item_hash = (!is_compact).then_some(entry_item.hash);
                entry_guard.items.set(index, entry_item.offset, item_hash);
            }

            entry_guard.header.object_header.aligned_size()
        };
        self.hmac_put_object(journal_file, entry_offset.get(), ObjectType::Entry)?;
        self.object_added(journal_file, entry_offset, entry_size)?;
        Ok(entry_offset)
    }

    fn publish_entry_links(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        self.append_to_entry_array(journal_file, entry_offset)?;
        for entry_item_index in 0..self.entry_items.len() {
            self.link_data_to_entry(journal_file, entry_offset, entry_item_index)?;
        }
        Ok(())
    }

    fn publish_after_entry(&mut self, journal_file: &mut JournalFile<MmapMut>) -> Result<()> {
        match self.live_publish_every_entries {
            0 => Ok(()),
            1 => journal_file.post_change(),
            interval => {
                self.entries_since_live_publication += 1;
                if self.entries_since_live_publication >= interval {
                    self.entries_since_live_publication = 0;
                    journal_file.post_change()
                } else {
                    Ok(())
                }
            }
        }
    }

    pub(super) fn object_added(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        object_offset: NonZeroU64,
        object_size: u64,
    ) -> Result<()> {
        self.tail_object_offset = object_offset;
        self.append_offset = object_offset
            .checked_add(object_size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        self.num_written_objects += 1;

        let header = journal_file.journal_header_mut();
        let old_size = header
            .header_size
            .checked_add(header.arena_size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        if self.append_offset.get() > old_size {
            let new_size = round_up_to_file_size_increment(self.append_offset.get())?;
            // The append pre-check keeps entries below this; a file at 4 GiB
            // would truncate the tail entry array hint and compact offsets.
            if new_size > JOURNAL_COMPACT_SIZE_MAX {
                return Err(JournalError::ObjectExceedsFileBounds);
            }
            header.arena_size = new_size
                .checked_sub(header.header_size)
                .ok_or(JournalError::ObjectExceedsFileBounds)?;
        }

        Ok(())
    }

    fn entry_added(
        &mut self,
        header: &mut JournalHeader,
        entry_offset: NonZeroU64,
        entry_seqnum: u64,
        entry_boot_id: uuid::Uuid,
        realtime: u64,
        monotonic: u64,
    ) {
        header.n_objects += self.num_written_objects;
        header.tail_object_offset = Some(self.tail_object_offset);

        if header.head_entry_seqnum == 0 {
            header.head_entry_seqnum = entry_seqnum;
        }
        if header.head_entry_realtime == 0 {
            header.head_entry_realtime = realtime;
        }
        if self.first_entry_monotonic.is_none() {
            self.first_entry_monotonic = Some(monotonic);
        }

        header.tail_entry_seqnum = entry_seqnum;
        header.tail_entry_realtime = realtime;
        header.tail_entry_monotonic = monotonic;
        header.tail_entry_boot_id = *entry_boot_id.as_bytes();
        header.tail_entry_offset = entry_offset.get();
        header.n_entries += 1;

        self.next_seqnum = entry_seqnum + 1;
        self.num_written_objects = 0;
    }

    fn add_data(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        field: EntryField<'_>,
    ) -> Result<EntryItem> {
        let payload = field.payload_parts();
        let field_name = field.field_name().ok_or(JournalError::InvalidField)?;
        let hash = journal_file.hash_parts(payload);
        if let Some((data_offset, link_state)) =
            journal_file.find_data_with_link_state_parts(hash, payload)?
        {
            return Ok(Self::entry_item(data_offset, hash, link_state));
        }
        self.add_new_data(journal_file, payload, field_name, hash)
    }

    fn entry_item(offset: NonZeroU64, hash: u64, link_state: ResolvedDataLinkState) -> EntryItem {
        EntryItem {
            offset,
            hash,
            link_state: Some(link_state),
        }
    }

    fn add_new_data<'a>(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        payload: PayloadParts<'a>,
        field_name: &'a [u8],
        hash: u64,
    ) -> Result<EntryItem> {
        let data_offset = self.write_new_data_object(journal_file, payload, hash)?;
        self.publish_new_data_object(journal_file, data_offset, hash)?;
        self.link_data_to_field(journal_file, data_offset, field_name)?;
        Ok(Self::entry_item(
            data_offset,
            hash,
            ResolvedDataLinkState::empty(),
        ))
    }

    fn write_new_data_object<'a>(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        payload: PayloadParts<'a>,
        hash: u64,
    ) -> Result<NonZeroU64> {
        let data_offset = self.append_offset;
        let stored_payload = self.stored_data_payload(payload);
        self.ensure_data_object_fits(journal_file, data_offset, stored_payload.len() as u64)?;
        self.poisoned = true;
        let data_size = {
            let mut data_guard =
                journal_file.data_mut(data_offset, Some(stored_payload.len() as u64))?;
            data_guard.header.hash = hash;
            stored_payload.copy_to_data_object(&mut data_guard);
            data_guard.header.object_header.flags = stored_payload.object_flags();
            data_guard.header.object_header.aligned_size()
        };
        self.hmac_put_object(journal_file, data_offset.get(), ObjectType::Data)?;
        self.object_added(journal_file, data_offset, data_size)?;
        Ok(data_offset)
    }

    fn ensure_data_object_fits(
        &self,
        journal_file: &JournalFile<MmapMut>,
        data_offset: NonZeroU64,
        payload_size: u64,
    ) -> Result<()> {
        let is_compact = Self::is_compact(journal_file);
        Self::ensure_compact_object_fits(
            is_compact,
            data_offset,
            Self::data_object_size(is_compact, payload_size),
        )
    }

    fn publish_new_data_object(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        data_offset: NonZeroU64,
        hash: u64,
    ) -> Result<()> {
        journal_file.data_hash_table_set_tail_offset(hash, data_offset)?;
        Self::update_data_hash_chain_depth(journal_file, hash)?;
        journal_file.journal_header_mut().n_data += 1;
        Ok(())
    }

    fn link_data_to_field(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        data_offset: NonZeroU64,
        field_name: &[u8],
    ) -> Result<()> {
        let field_offset = self.add_field(journal_file, field_name)?;
        let head_data_offset = {
            let field_guard = journal_file.field_ref(field_offset)?;
            field_guard.header.head_data_offset
        };
        {
            let mut data_guard = journal_file.data_mut(data_offset, None)?;
            data_guard.header.next_field_offset = head_data_offset;
        }
        let mut field_guard = journal_file.field_mut(field_offset, None)?;
        field_guard.header.head_data_offset = Some(data_offset);
        Ok(())
    }

    fn stored_data_payload<'a>(&self, payload: PayloadParts<'a>) -> StoredDataPayload<'a> {
        if payload.len() >= self.compress_threshold {
            let full_payload;
            let payload_bytes = if let Some(raw) = payload.as_single_slice() {
                raw
            } else {
                // Structured payloads need a contiguous buffer only when compression is
                // enabled and the payload is large enough to attempt compression.
                full_payload = payload.to_vec();
                full_payload.as_slice()
            };
            match self.compression {
                Compression::Zstd => {
                    let compressed = ruzstd::encoding::compress_to_vec(
                        Cursor::new(payload_bytes),
                        ruzstd::encoding::CompressionLevel::Fastest,
                    );
                    let compressed = zstd_frame_with_content_size(compressed, payload_bytes.len());
                    if compressed.len() < payload_bytes.len() {
                        return StoredDataPayload::Compressed(
                            compressed,
                            ObjectFlags::CompressedZstd as u8,
                        );
                    }
                }
                Compression::Xz => {
                    if payload_bytes.len() >= 80 {
                        if let Ok(compressed) = xz_compress(payload_bytes) {
                            if compressed.len() < payload_bytes.len() {
                                return StoredDataPayload::Compressed(
                                    compressed,
                                    ObjectFlags::CompressedXz as u8,
                                );
                            }
                        }
                    }
                }
                Compression::Lz4 => {
                    if payload_bytes.len() >= 9 {
                        let compressed = lz4_compress(payload_bytes);
                        if compressed.len() < payload_bytes.len() {
                            return StoredDataPayload::Compressed(
                                compressed,
                                ObjectFlags::CompressedLz4 as u8,
                            );
                        }
                    }
                }
                Compression::None => {}
            }
        }

        StoredDataPayload::Uncompressed(payload)
    }

    fn add_field(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        payload: &[u8],
    ) -> Result<NonZeroU64> {
        self.ensure_first_tag(journal_file)?;

        if let Some(field_offset) = self.field_cache.get(payload) {
            return Ok(field_offset);
        }

        let hash = journal_file.hash(payload);

        match journal_file.find_field_offset(hash, payload)? {
            Some(field_offset) => {
                self.field_cache.insert(payload, field_offset);
                Ok(field_offset)
            }
            None => {
                // We will have to write the new field object at the current
                // tail offset
                let field_offset = self.append_offset;
                let is_compact = Self::is_compact(journal_file);
                Self::ensure_compact_object_fits(
                    is_compact,
                    field_offset,
                    std::mem::size_of::<FieldObjectHeader>() as u64 + payload.len() as u64,
                )?;
                let field_size = {
                    let mut field_guard =
                        journal_file.field_mut(field_offset, Some(payload.len() as u64))?;

                    field_guard.header.hash = hash;
                    field_guard.set_payload(payload);
                    field_guard.header.object_header.aligned_size()
                };
                self.hmac_put_object(journal_file, field_offset.get(), ObjectType::Field)?;
                self.object_added(journal_file, field_offset, field_size)?;

                // Update hash table
                journal_file.field_hash_table_set_tail_offset(hash, field_offset)?;
                let depth = Self::current_field_hash_chain_depth(journal_file, hash)?;
                let max_depth = journal_file
                    .journal_header_ref()
                    .field_hash_chain_depth
                    .max(depth);
                journal_file.journal_header_mut().field_hash_chain_depth = max_depth;
                journal_file.journal_header_mut().n_fields += 1;

                self.field_cache.insert(payload, field_offset);

                // Return the offset where we wrote the newly added data object
                Ok(field_offset)
            }
        }
    }
}

fn zstd_frame_with_content_size(frame: Vec<u8>, content_size: usize) -> Vec<u8> {
    const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
    const SINGLE_SEGMENT_FLAG: u8 = 1 << 5;
    const CONTENT_CHECKSUM_FLAG: u8 = 1 << 2;

    if frame.len() < 6 || frame[0..4] != ZSTD_MAGIC {
        return frame;
    }

    let descriptor = frame[4];
    let dictionary_id_flag = descriptor & 0x03;
    let frame_content_size_flag = descriptor >> 6;
    if dictionary_id_flag != 0
        || frame_content_size_flag != 0
        || (descriptor & SINGLE_SEGMENT_FLAG) != 0
    {
        return frame;
    }

    let (new_frame_content_size_flag, frame_content_size) = if content_size <= 255 {
        (0u8, vec![content_size as u8])
    } else if content_size <= 65_791 {
        (1u8, ((content_size - 256) as u16).to_le_bytes().to_vec())
    } else if u32::try_from(content_size).is_ok() {
        (2u8, (content_size as u32).to_le_bytes().to_vec())
    } else {
        (3u8, (content_size as u64).to_le_bytes().to_vec())
    };

    let mut patched = Vec::with_capacity(frame.len() + frame_content_size.len() - 1);
    patched.extend_from_slice(&frame[..4]);
    patched.push(
        (new_frame_content_size_flag << 6)
            | SINGLE_SEGMENT_FLAG
            | (descriptor & CONTENT_CHECKSUM_FLAG),
    );
    patched.extend_from_slice(&frame_content_size);
    patched.extend_from_slice(&frame[6..]);
    patched
}

fn xz_compress(payload: &[u8]) -> std::io::Result<Vec<u8>> {
    use lzma_rust2::{XzOptions, XzWriter};
    use std::io::Write;

    let mut options = XzOptions::with_preset(0);
    options.set_check_sum_type(lzma_rust2::CheckType::None);
    let mut writer = XzWriter::new(Vec::new(), options)?;
    writer.write_all(payload)?;
    writer.finish()
}

fn lz4_compress(payload: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(payload);
    let mut out = Vec::with_capacity(8 + compressed.len());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&compressed);
    out
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
