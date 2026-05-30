use super::mmap::MmapMut;
use crate::error::{JournalError, Result};
use crate::file::{
    hash::jenkins_hash64_parts, normalize_compress_threshold, CompactDataFields, CompactEntryItem,
    Compression, DataObject, DataObjectHeader, DataPayloadType, EntryObjectHeader,
    FieldObjectHeader, HashTable, HashableObjectMut, HeaderIncompatibleFlags, JournalFile,
    JournalHeader, ObjectFlags, ObjectHeader, ObjectType, PayloadParts, RegularEntryItem,
    DEFAULT_COMPRESS_THRESHOLD,
};
use crate::seal::TAG_LENGTH;
use rustc_hash::FxHashMap;
use std::io::Cursor;
use std::num::NonZeroU64;

const OBJECT_ALIGNMENT: u64 = 8;
const JOURNAL_COMPACT_SIZE_MAX: u64 = u32::MAX as u64;
const FILE_SIZE_INCREASE: u64 = 8 * 1024 * 1024;
const FIELD_CACHE_MAX_ENTRIES: usize = 1024;
const FIELD_CACHE_MAX_PAYLOAD_LEN: usize = 128;
fn round_up_to_file_size_increment(value: u64) -> Result<u64> {
    value
        .checked_add(FILE_SIZE_INCREASE - 1)
        .map(|v| v & !(FILE_SIZE_INCREASE - 1))
        .ok_or(JournalError::ObjectExceedsFileBounds)
}

#[derive(Debug, Clone, Copy)]
struct EntryItem {
    offset: NonZeroU64,
    hash: u64,
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

fn accept_entry_field(field: EntryField<'_>, policy: FieldNamePolicy) -> Result<bool> {
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
    tail_object_offset: NonZeroU64,
    append_offset: NonZeroU64,
    next_seqnum: u64,
    num_written_objects: u64,
    first_tag_written: bool,
    entry_items: Vec<EntryItem>,
    field_cache: FieldCache,
    first_entry_monotonic: Option<u64>,
    boot_id: uuid::Uuid,
    compression: Compression,
    compress_threshold: usize,
    live_publish_every_entries: u64,
    entries_since_live_publication: u64,
    seal: Option<crate::seal::SealState>,
}

impl JournalWriter {
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
        let append_offset = {
            let header = journal_file.journal_header_ref();

            let Some(tail_object_offset) = header.tail_object_offset else {
                return Err(JournalError::InvalidMagicNumber);
            };

            let tail_object = journal_file.object_header_ref(tail_object_offset)?;

            tail_object_offset.saturating_add(tail_object.size)
        };

        let seal = journal_file
            .seal_options
            .as_ref()
            .map(|opts| crate::seal::SealState::new(opts))
            .transpose()?;

        let mut writer = Self {
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
        Self::new_with_compression(
            journal_file,
            self.next_seqnum,
            self.boot_id,
            self.compression,
            self.compress_threshold,
        )
    }

    // ------------------------------------------------------------------
    // Forward Secure Sealing helpers
    // ------------------------------------------------------------------

    fn ensure_first_tag(&mut self, journal_file: &mut JournalFile<MmapMut>) -> Result<()> {
        if !self.first_tag_written && self.seal.is_some() {
            self.append_first_tag(journal_file)?;
            self.first_tag_written = true;
        }
        Ok(())
    }

    fn append_first_tag(&mut self, journal_file: &mut JournalFile<MmapMut>) -> Result<()> {
        self.hmac_put_header(journal_file)?;
        let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
        let (dht_offset, fht_offset) = {
            let header = journal_file.journal_header_ref();
            (
                header
                    .data_hash_table_offset
                    .map(|o| o.get() - object_header_size),
                header
                    .field_hash_table_offset
                    .map(|o| o.get() - object_header_size),
            )
        };
        // systemd journal-authenticate.c:478-487: field hash table first, then data hash table.
        if let Some(fht_offset) = fht_offset {
            self.hmac_put_hash_table_object(journal_file, NonZeroU64::new(fht_offset).unwrap())?;
        }
        if let Some(dht_offset) = dht_offset {
            self.hmac_put_hash_table_object(journal_file, NonZeroU64::new(dht_offset).unwrap())?;
        }
        self.append_tag(journal_file)
    }

    fn append_tag(&mut self, journal_file: &mut JournalFile<MmapMut>) -> Result<()> {
        let tag_offset = self.append_offset;

        // Increment n_tags BEFORE computing the HMAC, matching systemd's
        // journal_file_tag_seqnum() which increments n_tags first.
        let seqnum = {
            let header = journal_file.journal_header_mut();
            header.n_tags += 1;
            header.n_tags
        };

        let epoch = self.seal.as_ref().unwrap().epoch();

        // Build the tag object in a local buffer: header + seqnum + epoch + tag
        let object_header_size = std::mem::size_of::<ObjectHeader>() as usize;
        let tag_meta_size = 16; // seqnum(8) + epoch(8)
        let total_size = object_header_size + tag_meta_size + TAG_LENGTH;
        let aligned_size = (total_size + 7) & !7;
        let mut buf = vec![0u8; aligned_size];

        // Object header
        buf[0] = ObjectType::Tag as u8;
        // flags, reserved remain zero
        buf[8..16].copy_from_slice(&(total_size as u64).to_le_bytes());

        // seqnum and epoch (little-endian)
        buf[object_header_size..object_header_size + 8].copy_from_slice(&seqnum.to_le_bytes());
        buf[object_header_size + 8..object_header_size + 16].copy_from_slice(&epoch.to_le_bytes());

        if let Some(ref mut seal) = self.seal {
            seal.hmac_put_object_bytes(&buf, ObjectType::Tag, total_size as u64, false);
            let digest = seal.hmac_finalize();
            buf[object_header_size + 16..object_header_size + 16 + TAG_LENGTH]
                .copy_from_slice(&digest);
            seal.hmac_reset();
        }

        // Write the complete tag object to the file
        {
            let mut tag_guard = journal_file.tag_mut(tag_offset, true)?;
            tag_guard.header.seqnum = seqnum;
            tag_guard.header.epoch = epoch;
            let digest = &buf[object_header_size + 16..object_header_size + 16 + TAG_LENGTH];
            tag_guard.header.tag.copy_from_slice(digest);
        }

        self.object_added(journal_file, tag_offset, total_size as u64)?;

        Ok(())
    }

    fn maybe_append_tag(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        realtime: u64,
    ) -> Result<()> {
        let need_evolve = if let Some(ref seal) = self.seal {
            seal.need_evolve(realtime)?
        } else {
            false
        };
        if !need_evolve {
            return Ok(());
        }

        // Finalize the running HMAC (accumulated from all objects since the
        // last tag) by appending the tag for the current epoch.
        self.append_tag(journal_file)?;

        // Evolve across intervals, appending intermediate tags.
        loop {
            let goal = if let Some(ref seal) = self.seal {
                seal.goal_epoch(realtime)?
            } else {
                break;
            };
            let epoch = if let Some(ref seal) = self.seal {
                seal.epoch()
            } else {
                break;
            };
            if epoch >= goal {
                break;
            }
            if let Some(ref mut seal) = self.seal {
                seal.evolve_state();
            }
            let new_epoch = if let Some(ref seal) = self.seal {
                seal.epoch()
            } else {
                break;
            };
            if new_epoch < goal {
                self.append_tag(journal_file)?;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn hmac_put_header(&mut self, journal_file: &JournalFile<MmapMut>) -> Result<()> {
        if self.seal.is_none() {
            return Ok(());
        }
        // Serialize the header to on-disk bytes and HMAC the immutable ranges.
        let header = journal_file.journal_header_ref();
        let bytes = zerocopy::IntoBytes::as_bytes(header);
        if let Some(ref mut seal) = self.seal {
            seal.hmac_put_header_ranges(bytes);
        }
        Ok(())
    }

    fn hmac_put_hash_table_object(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        offset: NonZeroU64,
    ) -> Result<()> {
        if self.seal.is_none() {
            return Ok(());
        }
        // Hash table objects: only the object header is immutable.
        // Read only the object header (16 bytes), not the entire hash table.
        let object_header_size = std::mem::size_of::<ObjectHeader>() as u64;
        let bytes = journal_file.read_bytes_at(offset.get(), object_header_size)?;
        if let Some(ref mut seal) = self.seal {
            let typ = if bytes.is_empty() {
                ObjectType::Unused
            } else {
                match bytes[0] {
                    4 => ObjectType::DataHashTable,
                    5 => ObjectType::FieldHashTable,
                    _ => ObjectType::Unused,
                }
            };
            seal.hmac_put_object_bytes(&bytes, typ, object_header_size, false);
        }
        Ok(())
    }

    fn hmac_put_object(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        offset: u64,
        object_type: ObjectType,
    ) -> Result<()> {
        if self.seal.is_none() {
            return Ok(());
        }
        let is_compact = Self::is_compact(journal_file);
        let offset_nz = NonZeroU64::new(offset).unwrap();
        let oh = journal_file.object_header_ref(offset_nz)?;
        let size = oh.size as usize;
        let bytes = journal_file.read_bytes_at(offset, size as u64)?;
        if let Some(ref mut seal) = self.seal {
            seal.hmac_put_object_bytes(&bytes, object_type, size as u64, is_compact);
        }
        Ok(())
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
        let header = journal_file.journal_header_ref();
        assert!(header.has_incompatible_flag(HeaderIncompatibleFlags::KeyedHash));
        let entry_seqnum = options.seqnum.unwrap_or(self.next_seqnum);
        if entry_seqnum == 0 || entry_seqnum == u64::MAX || entry_seqnum < self.next_seqnum {
            return Err(JournalError::InvalidField);
        }

        // Write the data/field objects while computing the entry's xor-hash
        // and storing each data object's offset/hash
        let mut xor_hash = 0;
        {
            self.entry_items.clear();
            let mut publication_ready = false;
            for field in fields {
                if !accept_entry_field(field, options.field_name_policy)? {
                    continue;
                }
                if !publication_ready {
                    self.ensure_first_tag(journal_file)?;
                    self.maybe_append_tag(journal_file, realtime)?;
                    publication_ready = true;
                }
                let entry_item = self.add_data(journal_file, field)?;
                self.entry_items.push(entry_item);

                // Per journal file format spec: xor_hash always uses Jenkins lookup3,
                // even for files with HEADER_INCOMPATIBLE_KEYED_HASH flag set
                xor_hash ^= jenkins_hash64_parts(field.payload_parts().iter());
            }
            if self.entry_items.is_empty() {
                return Err(JournalError::InvalidField);
            }

            if !self
                .entry_items
                .windows(2)
                .all(|items| items[0].offset <= items[1].offset)
            {
                self.entry_items
                    .sort_unstable_by(|a, b| a.offset.cmp(&b.offset));
            }
            if !options.trusted_unique_payloads {
                self.entry_items.dedup_by(|a, b| a.offset == b.offset);
            }
        }

        // write the entry itself
        let entry_offset = self.append_offset;
        let is_compact = Self::is_compact(journal_file);
        let entry_payload_size = self.entry_items.len() as u64 * Self::entry_item_size(is_compact);
        Self::ensure_compact_object_fits(
            is_compact,
            entry_offset,
            std::mem::size_of::<EntryObjectHeader>() as u64 + entry_payload_size,
        )?;
        let entry_size = {
            let size = Some(entry_payload_size);
            let mut entry_guard = journal_file.entry_mut(entry_offset, size)?;

            entry_guard.header.seqnum = entry_seqnum;
            entry_guard.header.xor_hash = xor_hash;
            entry_guard.header.boot_id = *self.boot_id.as_bytes();
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

        self.append_to_entry_array(journal_file, entry_offset)?;
        for entry_item_index in 0..self.entry_items.len() {
            self.link_data_to_entry(journal_file, entry_offset, entry_item_index)?;
        }

        self.entry_added(
            journal_file.journal_header_mut(),
            entry_offset,
            entry_seqnum,
            realtime,
            monotonic,
        );
        self.publish_after_entry(journal_file)?;

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

    fn object_added(
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
        header.tail_entry_boot_id = *self.boot_id.as_bytes();
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

        match journal_file.find_data_offset_parts(hash, payload)? {
            Some(data_offset) => {
                let entry_item = EntryItem {
                    offset: data_offset,
                    hash,
                };
                Ok(entry_item)
            }
            None => {
                let data_offset = self.append_offset;
                let stored_payload = self.stored_data_payload(payload);
                let is_compact = Self::is_compact(journal_file);
                Self::ensure_compact_object_fits(
                    is_compact,
                    data_offset,
                    Self::data_object_size(is_compact, stored_payload.len() as u64),
                )?;
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

                journal_file.data_hash_table_set_tail_offset(hash, data_offset)?;
                Self::update_data_hash_chain_depth(journal_file, hash)?;
                journal_file.journal_header_mut().n_data += 1;

                {
                    let field_offset = self.add_field(journal_file, field_name)?;

                    {
                        let head_data_offset = {
                            let field_guard = journal_file.field_ref(field_offset)?;
                            field_guard.header.head_data_offset
                        };

                        let mut data_guard = journal_file.data_mut(data_offset, None)?;
                        data_guard.header.next_field_offset = head_data_offset;
                    }

                    {
                        let mut field_guard = journal_file.field_mut(field_offset, None)?;
                        field_guard.header.head_data_offset = Some(data_offset);
                    }
                }

                let entry_item = EntryItem {
                    offset: data_offset,
                    hash,
                };
                Ok(entry_item)
            }
        }
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

    fn allocate_new_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        capacity: NonZeroU64,
    ) -> Result<NonZeroU64> {
        // let new_capacity = previous_capacity.saturating_mul(NonZeroU64::new(2).unwrap());

        let array_offset = self.append_offset;
        let is_compact = Self::is_compact(journal_file);
        Self::ensure_compact_object_fits(
            is_compact,
            array_offset,
            std::mem::size_of::<crate::file::OffsetArrayObjectHeader>() as u64
                + capacity.get() * Self::offset_array_item_size(is_compact),
        )?;
        let array_size = {
            let array_guard = journal_file.offset_array_mut(array_offset, Some(capacity))?;

            array_guard.header.object_header.aligned_size()
        };
        self.hmac_put_object(journal_file, array_offset.get(), ObjectType::EntryArray)?;
        self.object_added(journal_file, array_offset, array_size)?;
        journal_file.journal_header_mut().n_entry_arrays += 1;

        Ok(array_offset)
    }

    fn next_entry_array_capacity(index: u64, previous_capacity: u64) -> u64 {
        let mut capacity = previous_capacity;
        if index > capacity {
            capacity = (index + 1) * 2;
        } else {
            capacity *= 2;
        }
        capacity.max(4)
    }

    fn update_data_hash_chain_depth(
        journal_file: &mut JournalFile<MmapMut>,
        hash: u64,
    ) -> Result<()> {
        let depth = Self::current_data_hash_chain_depth(journal_file, hash)?;
        let max_depth = journal_file
            .journal_header_ref()
            .data_hash_chain_depth
            .max(depth);
        journal_file.journal_header_mut().data_hash_chain_depth = max_depth;
        Ok(())
    }

    fn current_data_hash_chain_depth(
        journal_file: &JournalFile<MmapMut>,
        hash: u64,
    ) -> Result<u64> {
        let Some(hash_table) = journal_file.data_hash_table_ref() else {
            return Err(JournalError::MissingHashTable);
        };
        let mut depth = 0;
        let mut object_offset = hash_table.hash_item_ref(hash).head_hash_offset;
        while let Some(offset) = object_offset {
            let object = journal_file.data_ref(offset)?;
            object_offset = object.header.next_hash_offset;
            if object_offset.is_some() {
                depth += 1;
            }
        }
        Ok(depth)
    }

    fn current_field_hash_chain_depth(
        journal_file: &JournalFile<MmapMut>,
        hash: u64,
    ) -> Result<u64> {
        let Some(hash_table) = journal_file.field_hash_table_ref() else {
            return Err(JournalError::MissingHashTable);
        };
        let mut depth = 0;
        let mut object_offset = hash_table.hash_item_ref(hash).head_hash_offset;
        while let Some(offset) = object_offset {
            let object = journal_file.field_ref(offset)?;
            object_offset = object.header.next_hash_offset;
            if object_offset.is_some() {
                depth += 1;
            }
        }
        Ok(depth)
    }

    fn append_to_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
    ) -> Result<()> {
        let is_compact = Self::is_compact(journal_file);
        Self::ensure_compact_offset(is_compact, entry_offset)?;
        let entry_array_offset = journal_file.journal_header_ref().entry_array_offset;

        if entry_array_offset.is_none() {
            journal_file.journal_header_mut().entry_array_offset = {
                let array_offset =
                    self.allocate_new_array(journal_file, NonZeroU64::new(4).unwrap())?;
                let mut array_guard = journal_file.offset_array_mut(array_offset, None)?;
                array_guard.set(0, entry_offset)?;
                Some(array_offset)
            };
            let header = journal_file.journal_header_mut();
            let array_offset = header.entry_array_offset.unwrap();
            header.tail_entry_array_offset = array_offset.get() as u32;
            header.tail_entry_array_n_entries = 1;
            return Ok(());
        }

        let entry_count = journal_file.journal_header_ref().n_entries;
        let mut tail_offset = NonZeroU64::new(
            journal_file
                .journal_header_ref()
                .tail_entry_array_offset
                .into(),
        );
        if tail_offset.is_none() {
            let mut offset = entry_array_offset.unwrap();
            let mut remaining = entry_count;
            loop {
                let array_guard = journal_file.offset_array_ref(offset)?;
                let capacity = array_guard.capacity() as u64;
                if remaining < capacity || array_guard.header.next_offset_array.is_none() {
                    tail_offset = Some(offset);
                    break;
                }
                remaining -= capacity;
                offset = array_guard.header.next_offset_array.unwrap();
            }
        }

        let tail_offset = tail_offset.ok_or(JournalError::EmptyOffsetArrayList)?;
        let tail_capacity = {
            let tail_guard = journal_file.offset_array_ref(tail_offset)?;
            tail_guard.capacity() as u64
        };
        let mut tail_entries = journal_file.journal_header_ref().tail_entry_array_n_entries as u64;
        if tail_entries == 0 {
            tail_entries = entry_count;
            let mut offset = entry_array_offset.unwrap();
            while offset != tail_offset {
                let array_guard = journal_file.offset_array_ref(offset)?;
                tail_entries -= array_guard.capacity() as u64;
                offset = array_guard
                    .header
                    .next_offset_array
                    .ok_or(JournalError::InvalidOffsetArrayOffset)?;
            }
        }

        if tail_entries < tail_capacity {
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.set(tail_entries as usize, entry_offset)?;
            drop(tail_guard);
            let header = journal_file.journal_header_mut();
            header.tail_entry_array_offset = tail_offset.get() as u32;
            header.tail_entry_array_n_entries = (tail_entries + 1) as u32;
            return Ok(());
        }

        let new_capacity = Self::next_entry_array_capacity(entry_count, tail_capacity);
        let new_array_offset =
            self.allocate_new_array(journal_file, NonZeroU64::new(new_capacity).unwrap())?;
        {
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.header.next_offset_array = Some(new_array_offset);
        }
        {
            let mut new_array_guard = journal_file.offset_array_mut(new_array_offset, None)?;
            new_array_guard.set(0, entry_offset)?;
        }
        let header = journal_file.journal_header_mut();
        header.tail_entry_array_offset = new_array_offset.get() as u32;
        header.tail_entry_array_n_entries = 1;

        Ok(())
    }

    fn append_to_data_entry_array(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        mut array_offset: NonZeroU64,
        entry_offset: NonZeroU64,
        current_count: u64,
    ) -> Result<(NonZeroU64, u64)> {
        let is_compact = Self::is_compact(journal_file);
        Self::ensure_compact_offset(is_compact, entry_offset)?;

        // Navigate to the tail of the array chain
        let mut current_index = 0u64;
        #[allow(unused_assignments)]
        let mut tail_offset = array_offset;

        loop {
            let array_guard = journal_file.offset_array_ref(array_offset)?;
            let capacity = array_guard.capacity() as u64;

            if current_index + capacity >= current_count {
                // This is the tail array
                tail_offset = array_offset;
                break;
            }

            current_index += capacity;

            let Some(next_offset) = array_guard.header.next_offset_array else {
                // This shouldn't happen if counts are correct
                return Err(JournalError::InvalidOffsetArrayOffset);
            };

            array_offset = next_offset;
        }

        // Try to add to the tail array
        let tail_capacity = {
            let tail_guard = journal_file.offset_array_ref(tail_offset)?;
            tail_guard.capacity() as u64
        };

        let entries_in_tail = current_count - current_index;

        if entries_in_tail < tail_capacity {
            // There's space in the tail array
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.set(entries_in_tail as usize, entry_offset)?;
            Ok((tail_offset, entries_in_tail + 1))
        } else {
            // Need to create a new array
            let new_capacity = NonZeroU64::new(Self::next_entry_array_capacity(
                current_count,
                tail_capacity,
            ))
            .unwrap();
            let new_array_offset = self.allocate_new_array(journal_file, new_capacity)?;

            // Link the old tail to the new array
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.header.next_offset_array = Some(new_array_offset);
            drop(tail_guard);

            // Add entry to the new array
            let mut new_array_guard = journal_file.offset_array_mut(new_array_offset, None)?;
            new_array_guard.set(0, entry_offset)?;
            Ok((new_array_offset, 1))
        }
    }

    fn append_to_data_entry_array_tail(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        tail_offset: NonZeroU64,
        tail_entries: u64,
        entry_offset: NonZeroU64,
        current_count: u64,
    ) -> Result<Option<(NonZeroU64, u64)>> {
        if tail_entries == 0 || tail_entries > current_count {
            return Ok(None);
        }

        let tail_capacity = {
            let tail_guard = match journal_file.offset_array_ref(tail_offset) {
                Ok(guard) => guard,
                Err(_) => return Ok(None),
            };
            if tail_guard.header.next_offset_array.is_some() {
                return Ok(None);
            }
            tail_guard.capacity() as u64
        };

        if tail_entries > tail_capacity {
            return Ok(None);
        }

        if tail_entries < tail_capacity {
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.set(tail_entries as usize, entry_offset)?;
            return Ok(Some((tail_offset, tail_entries + 1)));
        }

        let new_capacity = NonZeroU64::new(Self::next_entry_array_capacity(
            current_count,
            tail_capacity,
        ))
        .unwrap();
        let new_array_offset = self.allocate_new_array(journal_file, new_capacity)?;

        {
            let mut tail_guard = journal_file.offset_array_mut(tail_offset, None)?;
            tail_guard.header.next_offset_array = Some(new_array_offset);
        }
        {
            let mut new_array_guard = journal_file.offset_array_mut(new_array_offset, None)?;
            new_array_guard.set(0, entry_offset)?;
        }

        Ok(Some((new_array_offset, 1)))
    }

    fn link_data_to_entry(
        &mut self,
        journal_file: &mut JournalFile<MmapMut>,
        entry_offset: NonZeroU64,
        entry_item_index: usize,
    ) -> Result<()> {
        let data_offset = self.entry_items[entry_item_index].offset;
        let mut data_guard = journal_file.data_mut(data_offset, None)?;

        match data_guard.header.n_entries {
            None => {
                data_guard.header.entry_offset = Some(entry_offset);
                data_guard.header.n_entries = NonZeroU64::new(1);
            }
            Some(n_entries) => {
                match n_entries.get() {
                    0 => {
                        unreachable!();
                    }
                    1 => {
                        drop(data_guard);

                        // Create new entry array with initial capacity
                        let array_capacity = NonZeroU64::new(4).unwrap();
                        let array_offset = self.allocate_new_array(journal_file, array_capacity)?;

                        // Load new array and set its first entry offset
                        {
                            let mut array_guard =
                                journal_file.offset_array_mut(array_offset, None)?;
                            array_guard.set(0, entry_offset)?;
                        }

                        // Update data object to point to the array
                        let is_compact = Self::is_compact(journal_file);
                        let mut data_guard = journal_file.data_mut(data_offset, None)?;
                        data_guard.header.entry_array_offset = Some(array_offset);
                        if is_compact {
                            Self::set_compact_data_tail(&mut data_guard, array_offset, 1)?;
                        }
                        data_guard.header.n_entries = NonZeroU64::new(2);
                    }
                    x => {
                        // There's already an entry array, append to it
                        let current_count = x - 1;
                        let array_offset = data_guard.header.entry_array_offset.unwrap();
                        let is_compact = Self::is_compact(journal_file);
                        let compact_tail = Self::compact_data_tail(&data_guard);

                        // Drop the data guard to avoid borrow conflicts
                        drop(data_guard);

                        let tail_result = match (is_compact, compact_tail) {
                            (true, Some((tail_offset, tail_entries))) => self
                                .append_to_data_entry_array_tail(
                                    journal_file,
                                    tail_offset,
                                    tail_entries,
                                    entry_offset,
                                    current_count,
                                )?,
                            _ => None,
                        };
                        let (tail_offset, tail_entries) = match tail_result {
                            Some(result) => result,
                            None => self.append_to_data_entry_array(
                                journal_file,
                                array_offset,
                                entry_offset,
                                current_count,
                            )?,
                        };

                        // Update the count
                        let mut data_guard = journal_file.data_mut(data_offset, None)?;
                        if is_compact {
                            Self::set_compact_data_tail(
                                &mut data_guard,
                                tail_offset,
                                tail_entries,
                            )?;
                        }
                        data_guard.header.n_entries = NonZeroU64::new(x + 1);
                    }
                }
            }
        }

        Ok(())
    }

    fn compact_data_tail(data_guard: &DataObject<&mut [u8]>) -> Option<(NonZeroU64, u64)> {
        match &data_guard.payload {
            DataPayloadType::Compact { compact_fields, .. } => {
                let tail_offset = NonZeroU64::new(compact_fields.tail_entry_array_offset as u64)?;
                let tail_entries = compact_fields.tail_entry_array_n_entries as u64;
                (tail_entries != 0).then_some((tail_offset, tail_entries))
            }
            DataPayloadType::Regular(_) => None,
        }
    }

    fn is_compact(journal_file: &JournalFile<MmapMut>) -> bool {
        journal_file
            .journal_header_ref()
            .has_incompatible_flag(HeaderIncompatibleFlags::Compact)
    }

    fn entry_item_size(is_compact: bool) -> u64 {
        if is_compact {
            std::mem::size_of::<CompactEntryItem>() as u64
        } else {
            std::mem::size_of::<RegularEntryItem>() as u64
        }
    }

    fn offset_array_item_size(is_compact: bool) -> u64 {
        if is_compact {
            std::mem::size_of::<u32>() as u64
        } else {
            std::mem::size_of::<u64>() as u64
        }
    }

    fn data_object_size(is_compact: bool, payload_size: u64) -> u64 {
        let mut size = std::mem::size_of::<DataObjectHeader>() as u64 + payload_size;
        if is_compact {
            size += std::mem::size_of::<CompactDataFields>() as u64;
        }
        size
    }

    fn ensure_compact_offset(is_compact: bool, offset: NonZeroU64) -> Result<()> {
        if is_compact && offset.get() > JOURNAL_COMPACT_SIZE_MAX {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        Ok(())
    }

    fn ensure_compact_object_fits(
        is_compact: bool,
        offset: NonZeroU64,
        object_size: u64,
    ) -> Result<()> {
        if !is_compact {
            return Ok(());
        }

        let end_offset = offset
            .get()
            .checked_add(object_size)
            .ok_or(JournalError::ObjectExceedsFileBounds)?;
        let aligned_end = (end_offset + (OBJECT_ALIGNMENT - 1)) & !(OBJECT_ALIGNMENT - 1);
        if offset.get() > JOURNAL_COMPACT_SIZE_MAX || aligned_end > JOURNAL_COMPACT_SIZE_MAX {
            return Err(JournalError::ObjectExceedsFileBounds);
        }
        Ok(())
    }

    fn set_compact_data_tail(
        data_guard: &mut DataObject<&mut [u8]>,
        tail_offset: NonZeroU64,
        tail_entries: u64,
    ) -> Result<()> {
        match &mut data_guard.payload {
            DataPayloadType::Compact { compact_fields, .. } => {
                compact_fields.tail_entry_array_offset = u32::try_from(tail_offset.get())
                    .map_err(|_| JournalError::ObjectExceedsFileBounds)?;
                compact_fields.tail_entry_array_n_entries = u32::try_from(tail_entries)
                    .map_err(|_| JournalError::ObjectExceedsFileBounds)?;
                Ok(())
            }
            DataPayloadType::Regular(_) => Err(JournalError::InvalidObjectType),
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
mod tests {
    use super::{
        zstd_frame_with_content_size, FieldCache, PayloadParts, FIELD_CACHE_MAX_ENTRIES,
        FIELD_CACHE_MAX_PAYLOAD_LEN,
    };
    use std::io::{Cursor, Read};
    use std::num::NonZeroU64;

    #[test]
    fn field_cache_hits_exact_field_names() {
        let mut cache = FieldCache::new();
        let offset = NonZeroU64::new(8).unwrap();

        cache.insert(b"FIELD", offset);

        assert_eq!(cache.get(b"FIELD"), Some(offset));
        assert_eq!(cache.get(b"OTHER"), None);
    }

    #[test]
    fn field_cache_skips_oversized_field_names() {
        let mut cache = FieldCache::new();
        let offset = NonZeroU64::new(16).unwrap();
        let oversized = vec![b'x'; FIELD_CACHE_MAX_PAYLOAD_LEN + 1];

        cache.insert(&oversized, offset);

        assert!(cache.get(&oversized).is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn field_cache_stays_bounded_after_capacity_is_exceeded() {
        let mut cache = FieldCache::new();

        for index in 0..FIELD_CACHE_MAX_ENTRIES {
            let key = format!("FIELD_{index}");
            cache.insert(key.as_bytes(), NonZeroU64::new((index + 1) as u64).unwrap());
        }

        assert_eq!(cache.len(), FIELD_CACHE_MAX_ENTRIES);

        cache.insert(b"FIELD_OVERFLOW", NonZeroU64::new(9_999).unwrap());

        assert_eq!(
            cache.get(b"FIELD_OVERFLOW"),
            Some(NonZeroU64::new(9_999).unwrap())
        );
        assert!(cache.get(b"FIELD_0").is_none());
        assert!(cache.len() <= FIELD_CACHE_MAX_ENTRIES);
    }

    #[test]
    fn zstd_frame_with_content_size_adds_decodable_frame_size() {
        let payload: Vec<u8> = (0..275usize)
            .map(|index| (index % 26) as u8 + b'A')
            .collect();
        let frame = ruzstd::encoding::compress_to_vec(
            Cursor::new(payload.as_slice()),
            ruzstd::encoding::CompressionLevel::Fastest,
        );

        assert_eq!(&frame[..4], &[0x28, 0xb5, 0x2f, 0xfd]);
        assert_eq!(frame[4] >> 6, 0);
        assert_eq!(frame[4] & (1 << 5), 0);

        let patched = zstd_frame_with_content_size(frame, payload.len());

        assert_eq!(&patched[..4], &[0x28, 0xb5, 0x2f, 0xfd]);
        assert_eq!(patched[4] >> 6, 1);
        assert_ne!(patched[4] & (1 << 5), 0);
        assert_eq!(
            u16::from_le_bytes([patched[5], patched[6]]) as usize + 256,
            payload.len()
        );

        let mut decoder = ruzstd::decoding::StreamingDecoder::new(patched.as_slice()).unwrap();
        let mut decoded = Vec::new();
        decoder.read_to_end(&mut decoded).unwrap();

        assert_eq!(decoded, payload);
    }

    #[test]
    fn zstd_frame_with_content_size_leaves_unsupported_frames_unchanged() {
        let invalid = vec![0, 1, 2, 3, 4, 5];
        assert_eq!(zstd_frame_with_content_size(invalid.clone(), 16), invalid);

        let payload = b"FRAME_CONTENT_SIZE_ALREADY_SET";
        let frame = ruzstd::encoding::compress_to_vec(
            Cursor::new(payload.as_slice()),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        let patched = zstd_frame_with_content_size(frame.clone(), payload.len());
        assert_eq!(
            zstd_frame_with_content_size(patched.clone(), payload.len()),
            patched
        );

        let mut dictionary_frame = frame;
        dictionary_frame[4] |= 1;
        assert_eq!(
            zstd_frame_with_content_size(dictionary_frame.clone(), payload.len()),
            dictionary_frame
        );
    }

    // ------------------------------------------------------------------
    // Sealed writer tests
    // ------------------------------------------------------------------

    use super::{
        EntryField, EntryWriteOptions, FieldNamePolicy, JournalFile, JournalWriter, StructuredField,
    };
    use crate::file::{
        normalize_compress_threshold, Compression, HeaderCompatibleFlags, JournalFileOptions,
        MmapMut, ObjectFlags, DEFAULT_COMPRESS_THRESHOLD, MIN_COMPRESS_THRESHOLD,
    };
    use crate::seal::SealOptions;
    #[cfg(unix)]
    use std::os::unix::fs::FileExt;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    fn test_uuid(n: u8) -> uuid::Uuid {
        let mut bytes = [0u8; 16];
        bytes[15] = n;
        uuid::Uuid::from_bytes(bytes)
    }

    fn test_seal_opts() -> SealOptions {
        SealOptions::new([0u8; 12], 1_000_000, 1_000_000)
    }

    fn write_test_bytes_at(
        file: &mut std::fs::File,
        bytes: &[u8],
        offset: u64,
    ) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            file.write_all_at(bytes, offset)
        }

        #[cfg(not(unix))]
        {
            use std::io::{Seek, SeekFrom, Write};

            file.seek(SeekFrom::Start(offset))?;
            file.write_all(bytes)
        }
    }

    fn zstd_writer(threshold: usize) -> JournalWriter {
        JournalWriter {
            tail_object_offset: NonZeroU64::new(8).unwrap(),
            append_offset: NonZeroU64::new(16).unwrap(),
            next_seqnum: 1,
            num_written_objects: 0,
            first_tag_written: false,
            entry_items: Vec::new(),
            field_cache: FieldCache::new(),
            first_entry_monotonic: None,
            boot_id: test_uuid(4),
            compression: Compression::Zstd,
            compress_threshold: normalize_compress_threshold(threshold),
            live_publish_every_entries: 1,
            entries_since_live_publication: 0,
            seal: None,
        }
    }

    fn payload_with_total_len(len: usize) -> Vec<u8> {
        let mut payload = b"F=".to_vec();
        payload.resize(len, b'A');
        payload
    }

    #[test]
    fn compression_threshold_matches_systemd_default_boundary() {
        let writer = zstd_writer(DEFAULT_COMPRESS_THRESHOLD);
        let below = payload_with_total_len(DEFAULT_COMPRESS_THRESHOLD - 1);
        let exact = payload_with_total_len(DEFAULT_COMPRESS_THRESHOLD);

        let below_payload = writer.stored_data_payload(PayloadParts::raw(&below));
        let stored_exact = writer.stored_data_payload(PayloadParts::raw(&exact));

        assert_eq!(below_payload.object_flags(), 0);
        assert_eq!(
            stored_exact.object_flags(),
            ObjectFlags::CompressedZstd as u8
        );
        assert!(stored_exact.len() < exact.len());
    }

    #[test]
    fn compression_threshold_clamps_to_systemd_minimum() {
        assert_eq!(
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).compress_threshold(),
            DEFAULT_COMPRESS_THRESHOLD
        );
        assert_eq!(normalize_compress_threshold(0), MIN_COMPRESS_THRESHOLD);
        assert_eq!(normalize_compress_threshold(1), MIN_COMPRESS_THRESHOLD);
        assert_eq!(
            normalize_compress_threshold(MIN_COMPRESS_THRESHOLD),
            MIN_COMPRESS_THRESHOLD
        );
        assert_eq!(zstd_writer(1).compress_threshold, MIN_COMPRESS_THRESHOLD);
        assert_eq!(
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_compress_threshold(1)
                .compress_threshold(),
            MIN_COMPRESS_THRESHOLD
        );

        let writer = zstd_writer(1);
        let small = payload_with_total_len(MIN_COMPRESS_THRESHOLD - 1);
        let small_payload = writer.stored_data_payload(PayloadParts::raw(&small));
        assert_eq!(small_payload.object_flags(), 0);

        let payload = payload_with_total_len(DEFAULT_COMPRESS_THRESHOLD);
        let stored_payload = writer.stored_data_payload(PayloadParts::raw(&payload));
        assert_eq!(
            stored_payload.object_flags(),
            ObjectFlags::CompressedZstd as u8
        );
    }

    fn verification_key(opts: &SealOptions) -> String {
        let seed_hex = opts
            .seed
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        let start = opts.start_usec / opts.interval_usec;
        format!(
            "{seed_hex}/{start:x}-{interval:x}",
            interval = opts.interval_usec
        )
    }

    fn journalctl_available() -> bool {
        Command::new("journalctl").arg("--version").output().is_ok()
    }

    fn write_raw_test_journal(path: &Path, fields: &[&[u8]]) -> Vec<u8> {
        let repo_file =
            crate::repository::File::from_path(path).expect("test journal path should parse");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_file_id(test_uuid(5)),
        )
        .expect("create raw journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(&mut journal_file, fields, 1_700_000_060_000_000, 100)
            .expect("write raw entry");
        journal_file.sync().expect("sync raw journal");
        drop(journal_file);
        std::fs::read(path).expect("read raw journal")
    }

    fn write_structured_test_journal(
        path: &Path,
        fields: &[StructuredField<'_>],
        options: EntryWriteOptions,
    ) -> Vec<u8> {
        let repo_file =
            crate::repository::File::from_path(path).expect("test journal path should parse");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_file_id(test_uuid(5)),
        )
        .expect("create structured journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry_structured_with_options(
                &mut journal_file,
                fields,
                1_700_000_060_000_000,
                100,
                options,
            )
            .expect("write structured entry");
        journal_file.sync().expect("sync structured journal");
        drop(journal_file);
        std::fs::read(path).expect("read structured journal")
    }

    fn write_entry_fields_test_journal(
        path: &Path,
        fields: &[EntryField<'_>],
        options: EntryWriteOptions,
    ) -> (Vec<u8>, usize, Vec<Vec<u8>>) {
        let repo_file =
            crate::repository::File::from_path(path).expect("test journal path should parse");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_file_id(test_uuid(5)),
        )
        .expect("create entry-fields journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry_fields_with_options(
                &mut journal_file,
                fields.iter().copied(),
                1_700_000_060_000_000,
                100,
                options,
            )
            .expect("write entry fields");

        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect entry offsets");
        let entry_offset = entry_offsets[0];
        let entry_item_count = {
            let entry = journal_file.entry_ref(entry_offset).expect("entry ref");
            entry.items.len()
        };
        let payloads = journal_file
            .entry_data_objects(entry_offset)
            .expect("entry data iterator")
            .map(|item| item.map(|object| object.raw_payload().to_vec()))
            .collect::<crate::error::Result<Vec<_>>>()
            .expect("read payloads");

        journal_file.sync().expect("sync entry-fields journal");
        drop(journal_file);
        (
            std::fs::read(path).expect("read entry-fields journal"),
            entry_item_count,
            payloads,
        )
    }

    #[test]
    fn entry_seqnum_override_preserves_gaps() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 10, test_uuid(4)).expect("create writer");

        for (idx, seqnum) in [10, 12, 20].into_iter().enumerate() {
            let payload = format!("MESSAGE=seqnum-{seqnum}");
            writer
                .add_entry_fields_with_options(
                    &mut journal_file,
                    [EntryField::raw(payload.as_bytes())],
                    1_700_000_060_000_000 + idx as u64,
                    idx as u64 + 1,
                    EntryWriteOptions::default().seqnum(seqnum),
                )
                .expect("write entry with seqnum override");
        }
        assert!(
            writer
                .add_entry_fields_with_options(
                    &mut journal_file,
                    [EntryField::raw(b"MESSAGE=backwards")],
                    1_700_000_060_000_010,
                    10,
                    EntryWriteOptions::default().seqnum(19),
                )
                .is_err(),
            "writer accepted a backwards seqnum override"
        );

        let header = journal_file.journal_header_ref();
        assert_eq!(header.head_entry_seqnum, 10);
        assert_eq!(header.tail_entry_seqnum, 20);
        assert_eq!(writer.next_seqnum(), 21);

        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect entry offsets");
        let seqnums = entry_offsets
            .iter()
            .map(|offset| {
                journal_file
                    .entry_ref(*offset)
                    .expect("entry ref")
                    .header
                    .seqnum
            })
            .collect::<Vec<_>>();
        assert_eq!(seqnums, vec![10, 12, 20]);
    }

    #[derive(Clone)]
    struct TestField {
        raw: Vec<u8>,
        name: Vec<u8>,
        value: Vec<u8>,
    }

    impl TestField {
        fn new(name: Vec<u8>, value: Vec<u8>) -> Self {
            let mut raw = Vec::with_capacity(name.len() + 1 + value.len());
            raw.extend_from_slice(&name);
            raw.push(b'=');
            raw.extend_from_slice(&value);
            Self { raw, name, value }
        }

        fn from_str(name: &str, value: impl AsRef<[u8]>) -> Self {
            Self::new(name.as_bytes().to_vec(), value.as_ref().to_vec())
        }

        fn structured(&self) -> StructuredField<'_> {
            StructuredField::new(&self.name, &self.value)
        }
    }

    fn make_raw_structured_identity_rows(rows: usize) -> Vec<Vec<TestField>> {
        let mut all = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut fields = Vec::with_capacity(18);
            fields.push(TestField::from_str("TEST_ID", "raw-structured-identity"));
            fields.push(TestField::from_str("PERF_PROFILE", "mixed-cardinality"));
            fields.push(TestField::from_str("EMPTY_VALUE", b""));
            fields.push(TestField::from_str(
                "BINARY_VALUE",
                [0, b'=', (row & 0xff) as u8, 0xff],
            ));

            for offset in 0..6 {
                fields.push(TestField::from_str(
                    &format!("LOW_CARD_{offset:02}"),
                    format!("low-{offset:02}-{:02}", row % 16),
                ));
            }
            for offset in 0..4 {
                fields.push(TestField::from_str(
                    &format!("MED_CARD_{offset:02}"),
                    format!("medium-{offset:02}-{:04}", row % 257),
                ));
            }
            for offset in 0..4 {
                fields.push(TestField::from_str(
                    &format!("HIGH_CARD_{offset:02}"),
                    format!("high-{offset:02}-{row:06}"),
                ));
            }
            all.push(fields);
        }
        all
    }

    fn write_rows_raw_test_journal(
        path: &Path,
        rows: &[Vec<TestField>],
        options: EntryWriteOptions,
    ) -> Vec<u8> {
        let repo_file =
            crate::repository::File::from_path(path).expect("test journal path should parse");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_file_id(test_uuid(5)),
        )
        .expect("create raw corpus journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        let mut entry_fields = Vec::with_capacity(32);
        for (index, row) in rows.iter().enumerate() {
            entry_fields.clear();
            entry_fields.extend(
                row.iter()
                    .map(|field| EntryField::raw(field.raw.as_slice())),
            );
            writer
                .add_entry_fields_with_options(
                    &mut journal_file,
                    entry_fields.iter().copied(),
                    1_700_000_060_000_000 + index as u64 * 500,
                    100 + index as u64 * 50,
                    options,
                )
                .expect("write raw corpus entry");
        }
        journal_file.sync().expect("sync raw corpus journal");
        drop(journal_file);
        std::fs::read(path).expect("read raw corpus journal")
    }

    fn write_rows_structured_test_journal(
        path: &Path,
        rows: &[Vec<TestField>],
        options: EntryWriteOptions,
    ) -> Vec<u8> {
        let repo_file =
            crate::repository::File::from_path(path).expect("test journal path should parse");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_file_id(test_uuid(5)),
        )
        .expect("create structured corpus journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        let mut structured_fields = Vec::with_capacity(32);
        for (index, row) in rows.iter().enumerate() {
            structured_fields.clear();
            structured_fields.extend(row.iter().map(TestField::structured));
            writer
                .add_entry_structured_with_options(
                    &mut journal_file,
                    &structured_fields,
                    1_700_000_060_000_000 + index as u64 * 500,
                    100 + index as u64 * 50,
                    options,
                )
                .expect("write structured corpus entry");
        }
        journal_file.sync().expect("sync structured corpus journal");
        drop(journal_file);
        std::fs::read(path).expect("read structured corpus journal")
    }

    fn write_rows_structured_with_live_mode(
        path: &Path,
        rows: &[Vec<TestField>],
        live_publish_every_entries: u64,
    ) -> Vec<u8> {
        let repo_file =
            crate::repository::File::from_path(path).expect("test journal path should parse");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_file_id(test_uuid(5)),
        )
        .expect("create live publication mode journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer.set_live_publish_every_entries(live_publish_every_entries);

        let mut structured_fields = Vec::with_capacity(32);
        for (index, row) in rows.iter().enumerate() {
            structured_fields.clear();
            structured_fields.extend(row.iter().map(TestField::structured));
            writer
                .add_entry_structured_with_options(
                    &mut journal_file,
                    &structured_fields,
                    1_700_000_060_000_000 + index as u64 * 500,
                    100 + index as u64 * 50,
                    EntryWriteOptions::default().trusted_unique_payloads(true),
                )
                .expect("write live publication mode entry");
        }
        journal_file
            .sync()
            .expect("sync live publication mode journal");
        drop(journal_file);
        std::fs::read(path).expect("read live publication mode journal")
    }

    #[test]
    fn structured_writer_matches_raw_payload_writer_bytes() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let raw_path = journal_dir.join("raw.journal");
        let structured_path = journal_dir.join("structured.journal");

        let raw_fields = [
            b"MESSAGE=structured parity".as_slice(),
            b"PRIORITY=6".as_slice(),
            b"BINARY=\x00=\x01\xfe\xff".as_slice(),
        ];
        let structured_fields = [
            StructuredField::new(b"MESSAGE", b"structured parity"),
            StructuredField::new(b"PRIORITY", b"6"),
            StructuredField::new(b"BINARY", b"\x00=\x01\xfe\xff"),
        ];

        let raw_bytes = write_raw_test_journal(&raw_path, &raw_fields);
        let structured_bytes = write_structured_test_journal(
            &structured_path,
            &structured_fields,
            EntryWriteOptions::default(),
        );

        assert_eq!(structured_bytes, raw_bytes);

        if !journalctl_available() {
            eprintln!("journalctl not available; skipping structured stock verify");
            return;
        }
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--file")
            .arg(&structured_path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for structured file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn mixed_entry_fields_match_raw_payload_writer_bytes() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let raw_path = journal_dir.join("raw.journal");
        let mixed_path = journal_dir.join("mixed.journal");

        let raw_fields = [
            EntryField::raw(b"MESSAGE=mixed entry"),
            EntryField::raw(b"PRIORITY=6"),
            EntryField::raw(b"BINARY=\x00=\x01\xfe\xff"),
        ];
        let mixed_fields = [
            EntryField::raw(b"MESSAGE=mixed entry"),
            EntryField::structured(b"PRIORITY", b"6"),
            EntryField::structured(b"BINARY", b"\x00=\x01\xfe\xff"),
        ];

        let (raw_bytes, _, _) =
            write_entry_fields_test_journal(&raw_path, &raw_fields, EntryWriteOptions::default());
        let (mixed_bytes, _, _) = write_entry_fields_test_journal(
            &mixed_path,
            &mixed_fields,
            EntryWriteOptions::default(),
        );

        assert_eq!(mixed_bytes, raw_bytes);
    }

    #[test]
    fn structured_writer_preserves_binary_field_values() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry_structured(
                &mut journal_file,
                &[
                    StructuredField::new(b"MESSAGE", b"binary structured"),
                    StructuredField::new(b"BINARY", b"\x00=\x01\xfe\xff"),
                ],
                1_700_000_060_000_000,
                100,
            )
            .expect("write structured binary entry");
        journal_file.sync().expect("sync journal");

        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect entry offsets");
        let payloads = journal_file
            .entry_data_objects(entry_offsets[0])
            .expect("entry data iterator")
            .map(|item| item.map(|object| object.raw_payload().to_vec()))
            .collect::<crate::error::Result<Vec<_>>>()
            .expect("read payloads");

        assert!(payloads.iter().any(|p| p == b"MESSAGE=binary structured"));
        assert!(payloads.iter().any(|p| p == b"BINARY=\x00=\x01\xfe\xff"));
    }

    #[test]
    fn structured_writer_rejects_invalid_field_names_before_writing() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

        assert!(writer
            .add_entry_structured(
                &mut journal_file,
                &[StructuredField::new(b"not-valid", b"value")],
                1_700_000_060_000_000,
                100,
            )
            .is_err());
        assert_eq!(journal_file.journal_header_ref().n_entries, 0);
    }

    #[test]
    fn writer_field_name_policies_cover_journald_app_and_raw() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");

        let journald_path = journal_dir.join("journald.journal");
        let repo_file =
            crate::repository::File::from_path(&journald_path).expect("test journal path");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry_structured(
                &mut journal_file,
                &[
                    StructuredField::new(b"MESSAGE", b"trusted fields"),
                    StructuredField::new(b"_HOSTNAME", b"synthetic-host"),
                ],
                1_700_002_111_000_000,
                1,
            )
            .expect("journald policy accepts protected fields");
        assert_eq!(journal_file.journal_header_ref().n_entries, 1);

        let app_path = journal_dir.join("journal-app.journal");
        let repo_file = crate::repository::File::from_path(&app_path).expect("test journal path");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(5), test_uuid(6), test_uuid(7)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(8)).expect("create writer");
        writer
            .add_entry_structured_with_options(
                &mut journal_file,
                &[
                    StructuredField::new(b"MESSAGE", b"app valid"),
                    StructuredField::new(b"_HOSTNAME", b"drop-host"),
                    StructuredField::new(b"lowercase", b"drop-lowercase"),
                ],
                1_700_002_112_000_000,
                1,
                EntryWriteOptions::default().field_name_policy(FieldNamePolicy::JournalApp),
            )
            .expect("journal-app policy drops invalid fields");
        assert_eq!(journal_file.journal_header_ref().n_entries, 1);
        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect journal-app entry offsets");
        let payloads = journal_file
            .entry_data_objects(entry_offsets[0])
            .expect("journal-app entry data iterator")
            .map(|item| item.map(|object| object.raw_payload().to_vec()))
            .collect::<crate::error::Result<Vec<_>>>()
            .expect("read journal-app payloads");
        assert!(payloads.iter().any(|p| p == b"MESSAGE=app valid"));
        assert!(!payloads.iter().any(|p| p.starts_with(b"_HOSTNAME=")));
        assert!(!payloads.iter().any(|p| p.starts_with(b"lowercase=")));
        assert!(writer
            .add_entry_structured_with_options(
                &mut journal_file,
                &[StructuredField::new(b"_HOSTNAME", b"drop-only")],
                1_700_002_112_000_001,
                2,
                EntryWriteOptions::default().field_name_policy(FieldNamePolicy::JournalApp),
            )
            .is_err());

        let raw_path = journal_dir.join("raw.journal");
        let repo_file = crate::repository::File::from_path(&raw_path).expect("test journal path");
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(9), test_uuid(10), test_uuid(11)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(12)).expect("create writer");
        let long_name = vec![b'a'; 1024];
        writer
            .add_entry_fields_with_options(
                &mut journal_file,
                [
                    EntryField::structured(b"lowercase", b"ok"),
                    EntryField::structured(b"foo.bar", b"dot"),
                    EntryField::structured(b"field name", b"space"),
                    EntryField::structured(long_name.as_slice(), b"long"),
                    EntryField::structured(b"BINARY", b"a\0=b"),
                ],
                1_700_002_113_000_000,
                1,
                EntryWriteOptions::default().field_name_policy(FieldNamePolicy::Raw),
            )
            .expect("raw policy accepts structure-only names");
        let mut entry_offsets = Vec::new();
        journal_file
            .entry_offsets(&mut entry_offsets)
            .expect("collect raw entry offsets");
        let payloads = journal_file
            .entry_data_objects(entry_offsets[0])
            .expect("raw entry data iterator")
            .map(|item| item.map(|object| object.raw_payload().to_vec()))
            .collect::<crate::error::Result<Vec<_>>>()
            .expect("read raw payloads");
        assert!(payloads.iter().any(|p| p == b"lowercase=ok"));
        assert!(payloads.iter().any(|p| p == b"foo.bar=dot"));
        assert!(payloads.iter().any(|p| p == b"field name=space"));
        assert!(payloads
            .iter()
            .any(|p| p == &format!("{}=long", "a".repeat(1024)).into_bytes()));
        assert!(payloads.iter().any(|p| p == b"BINARY=a\0=b"));
        assert!(writer
            .add_entry_fields_with_options(
                &mut journal_file,
                [EntryField::structured(b"BAD=NAME", b"bad")],
                1_700_002_113_000_001,
                2,
                EntryWriteOptions::default().field_name_policy(FieldNamePolicy::Raw),
            )
            .is_err());
    }

    #[test]
    fn trusted_unique_payloads_keeps_unique_entry_output_identical() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let default_path = journal_dir.join("default.journal");
        let trusted_path = journal_dir.join("trusted.journal");

        let fields = [
            StructuredField::new(b"MESSAGE", b"trusted unique"),
            StructuredField::new(b"PRIORITY", b"6"),
            StructuredField::new(b"SYSLOG_IDENTIFIER", b"journal-core-test"),
        ];
        let default_bytes =
            write_structured_test_journal(&default_path, &fields, EntryWriteOptions::default());
        let trusted_bytes = write_structured_test_journal(
            &trusted_path,
            &fields,
            EntryWriteOptions::default().trusted_unique_payloads(true),
        );

        assert_eq!(trusted_bytes, default_bytes);
    }

    #[test]
    fn structured_writer_deduplicates_duplicate_payloads_by_default() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let default_path = journal_dir.join("default.journal");
        let trusted_path = journal_dir.join("trusted.journal");

        let fields = [
            EntryField::structured(b"MESSAGE", b"duplicate"),
            EntryField::structured(b"MESSAGE", b"duplicate"),
            EntryField::structured(b"PRIORITY", b"6"),
        ];

        let (_, default_count, default_payloads) =
            write_entry_fields_test_journal(&default_path, &fields, EntryWriteOptions::default());
        assert_eq!(default_count, 2);
        assert_eq!(
            default_payloads
                .iter()
                .filter(|payload| payload.as_slice() == b"MESSAGE=duplicate")
                .count(),
            1
        );

        let (_, trusted_count, trusted_payloads) = write_entry_fields_test_journal(
            &trusted_path,
            &fields,
            EntryWriteOptions::default().trusted_unique_payloads(true),
        );
        assert_eq!(trusted_count, 3);
        assert_eq!(
            trusted_payloads
                .iter()
                .filter(|payload| payload.as_slice() == b"MESSAGE=duplicate")
                .count(),
            2
        );
    }

    #[test]
    fn structured_writer_matches_raw_payload_writer_bytes_across_deterministic_corpus() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let raw_path = journal_dir.join("raw.journal");
        let structured_path = journal_dir.join("structured.journal");
        let rows = make_raw_structured_identity_rows(512);
        let options = EntryWriteOptions::default().trusted_unique_payloads(true);

        let raw_bytes = write_rows_raw_test_journal(&raw_path, &rows, options);
        let structured_bytes = write_rows_structured_test_journal(&structured_path, &rows, options);

        assert_eq!(structured_bytes, raw_bytes);
    }

    #[test]
    fn live_publication_modes_preserve_closed_file_bytes() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let immediate_path = journal_dir.join("immediate.journal");
        let disabled_path = journal_dir.join("disabled.journal");
        let every_n_path = journal_dir.join("every-n.journal");
        let rows = make_raw_structured_identity_rows(65);

        let immediate_bytes = write_rows_structured_with_live_mode(&immediate_path, &rows, 1);
        let disabled_bytes = write_rows_structured_with_live_mode(&disabled_path, &rows, 0);
        let every_n_bytes = write_rows_structured_with_live_mode(&every_n_path, &rows, 8);

        assert_eq!(disabled_bytes, immediate_bytes);
        assert_eq!(every_n_bytes, immediate_bytes);
    }

    #[test]
    fn sealed_writer_basic_passes_stock_verify() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping sealed writer stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[
                    b"MESSAGE=hello sealed world".as_slice(),
                    b"PRIORITY=6".as_slice(),
                ],
                1_500_000,
                100,
            )
            .expect("write sealed entry");
        journal_file.sync().expect("sync journal");

        let key = verification_key(&seal);
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(&key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for sealed file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn sealed_writer_interval_crossing_passes_stock_verify() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping interval crossing stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

        // Entry in epoch 0 (realtime == start)
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=epoch0".as_slice()],
                1_000_000,
                100,
            )
            .expect("write epoch 0");
        // Entry in epoch 1 (crosses interval)
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=epoch1".as_slice()],
                2_000_000,
                200,
            )
            .expect("write epoch 1");
        // Entry in epoch 2
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=epoch2".as_slice()],
                3_000_000,
                300,
            )
            .expect("write epoch 2");
        journal_file.sync().expect("sync journal");

        let key = verification_key(&seal);
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(&key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for interval-crossing sealed file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn sealed_writer_wrong_key_fails_stock_verify() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping wrong key verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_seal(seal),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=hello".as_slice()],
                1_500_000,
                100,
            )
            .expect("write sealed entry");
        journal_file.sync().expect("sync journal");

        let wrong_key = "000000000000000000000001/1-f4240";
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(wrong_key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify with wrong key");
        assert!(
            !output.status.success(),
            "journalctl verify should fail with wrong key, but succeeded"
        );
    }

    #[test]
    fn sealed_writer_tampered_data_fails_stock_verify() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping tamper verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=hello".as_slice()],
                1_500_000,
                100,
            )
            .expect("write sealed entry");
        journal_file.sync().expect("sync journal");

        // Tamper with a byte in the DATA object payload area
        use std::fs::OpenOptions;
        let mut f = OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("open for tamper");
        write_test_bytes_at(&mut f, &[0xff], 512).expect("tamper write");
        drop(f);

        let key = verification_key(&seal);
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(&key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify tampered");
        assert!(
            !output.status.success(),
            "journalctl verify should fail with tampered data, but succeeded"
        );
    }

    #[test]
    fn unsealed_writer_does_not_set_sealed_flags() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let journal_file: JournalFile<MmapMut> = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create unsealed journal");
        let header = journal_file.journal_header_ref();
        assert!(
            !header.has_compatible_flag(HeaderCompatibleFlags::Sealed),
            "unsealed writer set SEALED flag"
        );
        assert!(
            !header.has_compatible_flag(HeaderCompatibleFlags::SealedContinuous),
            "unsealed writer set SEALED_CONTINUOUS flag"
        );
    }

    #[test]
    fn sealed_writer_first_entry_future_epoch_passes_stock_verify() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping first-entry future-epoch stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

        // Write the first entry at epoch 2 (realtime = start + 2 * interval = 3_000_000).
        // This exercises FSS epoch-evolution during the first-tag path.
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=future epoch first entry".as_slice()],
                3_000_000,
                100,
            )
            .expect("write first entry at future epoch");
        journal_file.sync().expect("sync journal");

        let key = verification_key(&seal);
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(&key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for first-entry future-epoch sealed file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn sealed_writer_entry_before_start_rejected() {
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

        // Stock verification rejects entries older than the first tag epoch,
        // so writers must reject this input instead of producing an invalid file.
        assert!(
            writer
                .add_entry(
                    &mut journal_file,
                    &[b"MESSAGE=before sealing start".as_slice()],
                    500_000,
                    100,
                )
                .is_err(),
            "expected before-start entry to be rejected"
        );
    }

    #[test]
    fn sealed_writer_multi_interval_gap_passes_stock_verify() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping multi-interval gap stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=epoch0".as_slice()],
                1_000_000,
                100,
            )
            .expect("write epoch 0");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=epoch5".as_slice()],
                6_000_000,
                200,
            )
            .expect("write epoch 5");
        journal_file.sync().expect("sync journal");

        let key = verification_key(&seal);
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(&key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for multi-interval gap sealed file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn sealed_writer_unaligned_start_uses_systemd_epoch_boundary() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping unaligned-start stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = SealOptions::new([0u8; 12], 1_000_000, 1_702_717);
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

        for (idx, realtime) in [1_702_717, 2_100_000, 2_800_000].into_iter().enumerate() {
            let payload = format!("MESSAGE=unaligned-start-{idx}");
            writer
                .add_entry(
                    &mut journal_file,
                    &[payload.as_bytes()],
                    realtime,
                    (idx + 1) as u64,
                )
                .expect("write unaligned-start entry");
        }
        journal_file.sync().expect("sync journal");

        let key = verification_key(&seal);
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(&key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for unaligned-start sealed file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn sealed_writer_empty_file_passes_stock_verify() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping empty sealed stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_seal(seal.clone()),
        )
        .expect("create sealed journal");
        let _writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        journal_file.sync().expect("sync journal");

        let key = verification_key(&seal);
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(&key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for empty sealed file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn compact_sealed_writer_passes_stock_verify() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping compact+sealed stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let seal = test_seal_opts();
        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_compact(true)
                .with_seal(seal.clone()),
        )
        .expect("create compact sealed journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[
                    b"MESSAGE=compact sealed entry".as_slice(),
                    b"PRIORITY=6".as_slice(),
                ],
                1_500_000,
                100,
            )
            .expect("write compact sealed entry");
        journal_file.sync().expect("sync journal");

        let key = verification_key(&seal);
        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--verify-key")
            .arg(&key)
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for compact+sealed file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn compact_writer_grows_arena_past_initial_allocation() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping compact arena growth stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(true),
        )
        .expect("create compact journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");

        for index in 0..10u8 {
            let mut payload = b"BLOB=".to_vec();
            payload.resize(payload.len() + 1024 * 1024, index);
            writer
                .add_entry(
                    &mut journal_file,
                    &[payload.as_slice()],
                    2_000_000 + u64::from(index),
                    100 + u64::from(index),
                )
                .expect("write large compact entry");
        }
        journal_file.sync().expect("sync compact journal");

        let header = journal_file.journal_header_ref();
        assert!(
            header.header_size + header.arena_size > super::FILE_SIZE_INCREASE,
            "arena size did not grow past the initial allocation"
        );

        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for grown compact file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn writer_initial_arena_covers_large_hash_tables() {
        if !journalctl_available() {
            eprintln!("journalctl not available; skipping large hash table stock verify");
            return;
        }
        let dir = TempDir::new().expect("create temp dir");
        let journal_dir = dir.path().join("journals");
        std::fs::create_dir_all(&journal_dir).expect("create journal dir");
        let path = journal_dir.join("system.journal");
        let repo_file =
            crate::repository::File::from_path(&path).expect("test journal path should parse");

        let mut journal_file = JournalFile::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_compact(true)
                .with_data_hash_table_buckets(600_000)
                .with_field_hash_table_buckets(1_023),
        )
        .expect("create journal with large hash tables");
        let mut writer =
            JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=large hash table".as_slice()],
                1_700_000_060_000_000,
                1,
            )
            .expect("write entry after large hash table initialization");
        journal_file.sync().expect("sync journal");

        let header = journal_file.journal_header_ref();
        assert!(
            header.header_size + header.arena_size > super::FILE_SIZE_INCREASE,
            "initial arena did not cover large hash tables"
        );

        let output = Command::new("journalctl")
            .arg("--verify")
            .arg("--file")
            .arg(&path)
            .output()
            .expect("run journalctl verify");
        assert!(
            output.status.success(),
            "journalctl verify failed for large-hash-table file: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
