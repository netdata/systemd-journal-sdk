//! Pure-Rust systemd journal reader and writer SDK.
//!
//! This crate provides a public Rust layer over the imported Netdata journal
//! reader/writer crates. It intentionally keeps the low-level file parsing in
//! the imported implementation and adds byte-safe entries, directory reading,
//! export/JSON formatting, and a libsystemd-style facade.

mod facade;
mod parse;
mod verify_graph;

pub use parse::{ParseError, ParsedCursor, parse_cursor, parse_match_bytes, parse_match_string};

use hmac::{Hmac, Mac};
use journal_core::fss::{RECOMMENDED_SECPAR, gen_mk, gen_state0, get_key, seek};
use journal_core::seal::TAG_LENGTH;
use ouroboros::self_referencing;
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

pub use facade::{
    ERR_END_OF_ENTRIES, ERR_INVALID_CURSOR, ERR_NO_ENTRY, ERR_UNSUPPORTED, Error as FacadeError,
    OutputMode, SdJournal, SdJournalAddConjunction, SdJournalAddDisjunction, SdJournalAddMatch,
    SdJournalClose, SdJournalEnumerateAvailableData, SdJournalEnumerateAvailableUnique,
    SdJournalEnumerateField, SdJournalEnumerateFields, SdJournalFlushMatches, SdJournalGetCursor,
    SdJournalGetData, SdJournalGetEntry, SdJournalGetMonotonicUsec, SdJournalGetRealtimeUsec,
    SdJournalGetSeqnum, SdJournalListBoots, SdJournalNext, SdJournalNextSkip, SdJournalOpen,
    SdJournalOpenDirectory, SdJournalOpenDirectoryWithOptions, SdJournalOpenFile,
    SdJournalOpenFileWithOptions, SdJournalOpenFiles, SdJournalOpenFilesWithOptions,
    SdJournalPrevious, SdJournalPreviousSkip, SdJournalProcessOutput, SdJournalQueryUnique,
    SdJournalQueryUniqueState, SdJournalRestartData, SdJournalRestartFields,
    SdJournalRestartUnique, SdJournalSeekCursor, SdJournalSeekHead, SdJournalSeekRealtimeUsec,
    SdJournalSeekTail, SdJournalSetOutputMode, SdJournalTestCursor, SdJournalVisitUniqueValues,
};
pub use journal_core::error::JournalError;
use journal_core::file::file::DataPayloadReadContext;
pub use journal_core::file::{
    BucketUtilization, Compression, Direction, EntryItemsType, ExperimentalMmapStrategy,
    FieldNamePolicy, HashableObject, JournalFile, JournalReader, Location, Mmap,
};
pub use journal_log_writer::{
    Config, EntryTimestamps, Log, LogLifecycleEvent, LogLifecycleObserver, RetentionPolicy,
    RotationPolicy, WriterError,
};
pub use journal_registry::{Origin, Source};

pub type Result<T> = std::result::Result<T, SdkError>;

#[derive(Debug)]
pub enum SdkError {
    Journal(JournalError),
    InvalidPath(String),
    InvalidCursor(String),
    NoEntry,
    DecompressionFailed(String),
    Unsupported(&'static str),
    VerificationError(String),
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Journal(err) => write!(f, "{err}"),
            Self::InvalidPath(path) => write!(f, "invalid path: {path}"),
            Self::InvalidCursor(cursor) => write!(f, "invalid cursor: {cursor}"),
            Self::NoEntry => write!(f, "no entry at current position"),
            Self::DecompressionFailed(err) => write!(f, "decompression failed: {err}"),
            Self::Unsupported(op) => write!(f, "unsupported operation: {op}"),
            Self::VerificationError(msg) => {
                write!(f, "journal verification failed: corrupt file: {msg}")
            }
        }
    }
}

impl std::error::Error for SdkError {}

impl From<JournalError> for SdkError {
    fn from(err: JournalError) -> Self {
        Self::Journal(err)
    }
}

impl From<std::io::Error> for SdkError {
    fn from(err: std::io::Error) -> Self {
        Self::Journal(JournalError::Io(err))
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub value: Vec<u8>,
}

impl Field {
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.as_bytes().to_vec(),
        }
    }

    pub fn with_bytes(name: &str, value: Vec<u8>) -> Self {
        Self {
            name: name.to_string(),
            value,
        }
    }

    pub fn payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(self.name.len() + 1 + self.value.len());
        payload.extend_from_slice(self.name.as_bytes());
        payload.push(b'=');
        payload.extend_from_slice(&self.value);
        payload
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderBounds {
    /// Systemd-style mutable reader bounds.
    ///
    /// The reader keeps a cached file size and refreshes it only when a read
    /// would go beyond the cached end of file, matching libsystemd's active
    /// journal behavior without a metadata syscall on every object read.
    Live,
    /// Immutable reader bounds.
    ///
    /// The reader fixes the file size at open time, like
    /// `SD_JOURNAL_ASSUME_IMMUTABLE`, for polling/query consumers that do not
    /// need to observe appends during the current scan.
    Snapshot,
}

pub const DEFAULT_READER_WINDOW_SIZE: u64 = 32 * 1024 * 1024;

impl Default for ReaderBounds {
    fn default() -> Self {
        Self::Live
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderOptions {
    pub window_size: u64,
    pub bounds: ReaderBounds,
    pub mmap_strategy: ExperimentalMmapStrategy,
}

impl Default for ReaderOptions {
    fn default() -> Self {
        Self {
            window_size: DEFAULT_READER_WINDOW_SIZE,
            bounds: ReaderBounds::Live,
            mmap_strategy: ExperimentalMmapStrategy::Windowed,
        }
    }
}

impl ReaderOptions {
    pub fn live() -> Self {
        Self::default()
    }

    pub fn snapshot() -> Self {
        Self {
            bounds: ReaderBounds::Snapshot,
            ..Self::default()
        }
    }

    pub fn with_window_size(mut self, window_size: u64) -> Self {
        self.window_size = window_size;
        self
    }

    pub fn with_mmap_strategy(mut self, strategy: ExperimentalMmapStrategy) -> Self {
        self.mmap_strategy = strategy;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawField<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
}

impl RawField<'_> {
    pub fn payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(self.name.len() + 1 + self.value.len());
        payload.extend_from_slice(self.name);
        payload.push(b'=');
        payload.extend_from_slice(self.value);
        payload
    }

    pub fn name_str(&self) -> Option<&str> {
        std::str::from_utf8(self.name).ok()
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// Convenience map for UTF-8 field names. RAW-mode files may contain field
    /// names that are not valid UTF-8; use `raw_fields()` or `get_raw_values()`
    /// when byte-identical field-name identity matters.
    pub fields: HashMap<String, Vec<u8>>,
    /// Convenience repeated-value map for UTF-8 field names.
    pub field_values: HashMap<String, Vec<Vec<u8>>>,
    /// Full on-disk DATA payloads as `FIELD=value` bytes.
    pub payloads: Vec<Vec<u8>>,
    pub seqnum: u64,
    pub realtime: u64,
    pub monotonic: u64,
    pub boot_id: [u8; 16],
    pub cursor: String,
}

impl Entry {
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.fields.get(key).map(Vec::as_slice)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key)
            .and_then(|value| std::str::from_utf8(value).ok())
    }

    pub fn raw_fields(&self) -> impl Iterator<Item = RawField<'_>> {
        self.payloads
            .iter()
            .filter_map(|payload| split_raw_payload(payload))
    }

    pub fn get_raw(&self, key: &[u8]) -> Option<&[u8]> {
        self.raw_fields()
            .find(|field| field.name == key)
            .map(|field| field.value)
    }

    pub fn get_raw_values(&self, key: &[u8]) -> Vec<&[u8]> {
        self.raw_fields()
            .filter_map(|field| (field.name == key).then_some(field.value))
            .collect()
    }
}

fn split_raw_payload(payload: &[u8]) -> Option<RawField<'_>> {
    let eq = payload.iter().position(|byte| *byte == b'=')?;
    Some(RawField {
        name: &payload[..eq],
        value: &payload[eq + 1..],
    })
}

#[derive(Debug, Clone)]
pub struct BootInfo {
    pub index: i64,
    pub boot_id: String,
    pub first_entry: i64,
    pub last_entry: i64,
}

#[derive(Debug, Clone)]
pub struct FileHeader {
    pub signature: [u8; 8],
    pub compatible_flags: u32,
    pub incompatible_flags: u32,
    pub state: u8,
    pub header_size: u64,
    pub head_entry_realtime: u64,
    pub tail_entry_realtime: u64,
    pub head_entry_seqnum: u64,
    pub tail_entry_seqnum: u64,
    pub tail_entry_boot_id: [u8; 16],
    pub seqnum_id: [u8; 16],
}

#[self_referencing]
struct ReaderCell {
    file: JournalFile<Mmap>,
    #[borrows(file)]
    #[not_covariant]
    reader: JournalReader<'this, Mmap>,
}

pub struct FileReader {
    inner: ReaderCell,
    temp_path: Option<PathBuf>,
    borrow_uncompressed_row_payloads: bool,
    current_key: Option<DirectoryEntryKey>,
    data_offsets: Vec<NonZeroU64>,
    data_payload_context: Option<DataPayloadReadContext>,
    data_offsets_entry: Option<NonZeroU64>,
    data_index: usize,
    entry_data_state_active: bool,
    decompressed: Vec<u8>,
    row_payloads: Vec<RowPayload>,
    row_owned_payloads: Vec<Vec<u8>>,
}

#[derive(Clone, Copy)]
enum RowPayload {
    Borrowed { ptr: *const u8, len: usize },
    Owned { index: usize },
}

enum RowPayloadData {
    Borrowed { ptr: *const u8, len: usize },
    Owned(Vec<u8>),
}

impl RowPayload {
    fn as_slice<'a>(&self, owned: &'a [Vec<u8>]) -> &'a [u8] {
        match self {
            Self::Borrowed { ptr, len } => {
                // SAFETY: FileReader only creates borrowed row payloads for whole-file mmap.
                // Row storage is cleared before advancing/seeking/remapping, so the mapped
                // bytes stay valid for the documented current-row lifetime.
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                unsafe { std::slice::from_raw_parts(*ptr, *len) }
            }
            Self::Owned { index } => owned[*index].as_slice(),
        }
    }
}

enum StepStatus {
    Valid(DirectoryEntryKey, NonZeroU64),
    Skip,
    End,
}

impl Drop for FileReader {
    fn drop(&mut self) {
        if let Some(path) = &self.temp_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl FileReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_options(path, ReaderOptions::default())
    }

    pub fn open_with_options(path: impl AsRef<Path>, options: ReaderOptions) -> Result<Self> {
        let path = path.as_ref();
        if is_zst_file(path) {
            return Self::open_zst(path, options);
        }

        let file = open_journal_file(path, options)?;
        Ok(Self {
            inner: ReaderCellBuilder {
                file,
                reader_builder: |_file| JournalReader::default(),
            }
            .build(),
            temp_path: None,
            borrow_uncompressed_row_payloads: options.mmap_strategy
                == ExperimentalMmapStrategy::WholeFile,
            current_key: None,
            data_offsets: Vec::new(),
            data_payload_context: None,
            data_offsets_entry: None,
            data_index: 0,
            entry_data_state_active: false,
            decompressed: Vec::new(),
            row_payloads: Vec::new(),
            row_owned_payloads: Vec::new(),
        })
    }

    fn open_zst(path: &Path, options: ReaderOptions) -> Result<Self> {
        let temp_path = decompress_zst_to_temp(path, "rust-sdk-journal")?;
        let file = match open_journal_file(&temp_path, options) {
            Ok(file) => file,
            Err(err) => {
                let _ = std::fs::remove_file(&temp_path);
                return Err(err);
            }
        };
        Ok(Self {
            inner: ReaderCellBuilder {
                file,
                reader_builder: |_file| JournalReader::default(),
            }
            .build(),
            temp_path: Some(temp_path),
            borrow_uncompressed_row_payloads: options.mmap_strategy
                == ExperimentalMmapStrategy::WholeFile,
            current_key: None,
            data_offsets: Vec::new(),
            data_payload_context: None,
            data_offsets_entry: None,
            data_index: 0,
            entry_data_state_active: false,
            decompressed: Vec::new(),
            row_payloads: Vec::new(),
            row_owned_payloads: Vec::new(),
        })
    }

    pub fn header(&self) -> FileHeader {
        self.inner.with_file(|file| {
            let header = file.journal_header_ref();
            FileHeader {
                signature: header.signature,
                compatible_flags: header.compatible_flags,
                incompatible_flags: header.incompatible_flags,
                state: header.state,
                header_size: header.header_size,
                head_entry_realtime: header.head_entry_realtime,
                tail_entry_realtime: header.tail_entry_realtime,
                head_entry_seqnum: header.head_entry_seqnum,
                tail_entry_seqnum: header.tail_entry_seqnum,
                tail_entry_boot_id: header.tail_entry_boot_id,
                seqnum_id: header.seqnum_id,
            }
        })
    }

    pub fn bucket_utilization(&self) -> Option<BucketUtilization> {
        self.inner.with_file(JournalFile::bucket_utilization)
    }

    pub fn seek_head(&mut self) {
        self.reset_cached_entry_data_state();
        self.current_key = None;
        self.inner.with_reader_mut(|reader| {
            reader.set_location(Location::Head);
        });
    }

    pub fn seek_tail(&mut self) {
        self.reset_cached_entry_data_state();
        self.current_key = None;
        self.inner.with_reader_mut(|reader| {
            reader.set_location(Location::Tail);
        });
    }

    pub fn seek_realtime(&mut self, usec: u64) {
        self.reset_cached_entry_data_state();
        self.current_key = None;
        self.inner.with_reader_mut(|reader| {
            reader.set_location(Location::Realtime(usec));
        });
    }

    pub fn seek_cursor(&mut self, cursor: &str) -> Result<()> {
        let (seqnum_id, boot_id, realtime, seqnum) =
            parse_cursor(cursor).map_err(|err| SdkError::InvalidCursor(err.to_string()))?;
        self.seek_realtime(realtime);
        while self.next()? {
            let entry = self.get_entry()?;
            if entry.realtime > realtime {
                break;
            }
            if entry.realtime != realtime
                || entry.seqnum != seqnum
                || hex::encode(entry.boot_id) != boot_id
            {
                continue;
            }
            let current_cursor = self.get_cursor()?;
            let (current_seqnum_id, _, _, _) = parse_cursor(&current_cursor)
                .map_err(|err| SdkError::InvalidCursor(err.to_string()))?;
            if current_seqnum_id == seqnum_id {
                return Ok(());
            }
        }
        Ok(())
    }

    pub fn next(&mut self) -> Result<bool> {
        self.step_valid(Direction::Forward)
    }

    pub fn previous(&mut self) -> Result<bool> {
        self.step_valid(Direction::Backward)
    }

    fn step_valid(&mut self, direction: Direction) -> Result<bool> {
        loop {
            let data_offsets = &mut self.data_offsets;
            let status = self.inner.with_mut(|fields| {
                if !fields.reader.step(fields.file, direction)? {
                    return Ok(StepStatus::End);
                }

                match fields.reader.get_entry_offset().and_then(|offset| {
                    let entry = fields.file.entry_ref(offset)?;
                    let header = fields.file.journal_header_ref();
                    collect_offsets_from_entry_items(&entry.items, data_offsets);
                    Ok((
                        DirectoryEntryKey {
                            seqnum_id: header.seqnum_id,
                            seqnum: entry.header.seqnum,
                            boot_id: entry.header.boot_id,
                            monotonic: entry.header.monotonic,
                            realtime: entry.header.realtime,
                            xor_hash: entry.header.xor_hash,
                        },
                        offset,
                    ))
                }) {
                    Ok((key, offset)) => Ok(StepStatus::Valid(key, offset)),
                    Err(err) if recoverable_entry_error(&err) => Ok(StepStatus::Skip),
                    Err(err) => Err(err),
                }
            })?;

            match status {
                StepStatus::Valid(key, offset) => {
                    self.current_key = Some(key);
                    self.data_offsets_entry = Some(offset);
                    self.data_payload_context = Some(
                        self.inner
                            .with_file(|file| file.data_payload_read_context()),
                    );
                    self.data_index = 0;
                    self.entry_data_state_active = false;
                    self.row_payloads.clear();
                    self.row_owned_payloads.clear();
                    return Ok(true);
                }
                StepStatus::Skip => continue,
                StepStatus::End => {
                    self.current_key = None;
                    self.data_offsets.clear();
                    self.data_offsets_entry = None;
                    self.data_payload_context = None;
                    self.data_index = 0;
                    self.entry_data_state_active = false;
                    self.row_payloads.clear();
                    self.row_owned_payloads.clear();
                    return Ok(false);
                }
            }
        }
    }

    pub fn get_entry(&mut self) -> Result<Entry> {
        self.invalidate_entry_data_state();
        let inner = &mut self.inner;
        let data_offsets = &mut self.data_offsets;
        let data_offsets_entry = &mut self.data_offsets_entry;
        let decompressed = &mut self.decompressed;
        inner.with_mut(|fields| {
            let offset = fields.reader.get_entry_offset()?;
            *data_offsets_entry = Some(offset);
            read_entry_at(
                fields.file,
                fields.reader,
                offset,
                data_offsets,
                decompressed,
            )
        })
    }

    pub fn visit_entry_payloads<F>(&mut self, visitor: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        self.invalidate_entry_data_state();
        let inner = &mut self.inner;
        let data_offsets = &mut self.data_offsets;
        let data_offsets_entry = &mut self.data_offsets_entry;
        let decompressed = &mut self.decompressed;
        inner.with_mut(|fields| {
            let offset = fields.reader.get_entry_offset()?;
            if *data_offsets_entry != Some(offset) {
                collect_entry_data_offsets(fields.file, offset, data_offsets)?;
                *data_offsets_entry = Some(offset);
            }
            visit_entry_payload_offsets(fields.file, data_offsets, decompressed, visitor)
        })
    }

    pub fn clear_entry_data_state(&mut self) {
        self.reset_cached_entry_data_state();
        self.inner
            .with_reader_mut(|reader| reader.entry_data_restart());
    }

    fn reset_cached_entry_data_state(&mut self) {
        self.data_offsets.clear();
        self.data_offsets_entry = None;
        self.data_payload_context = None;
        self.data_index = 0;
        self.entry_data_state_active = false;
        self.row_payloads.clear();
        self.row_owned_payloads.clear();
    }

    fn invalidate_entry_data_state(&mut self) {
        if self.entry_data_state_active {
            self.clear_entry_data_state();
        }
    }

    pub fn entry_data_restart(&mut self) -> Result<()> {
        self.inner
            .with_reader_mut(|reader| reader.entry_data_restart());
        let inner = &mut self.inner;
        let data_offsets = &mut self.data_offsets;
        let data_offsets_entry = &mut self.data_offsets_entry;
        let data_payload_context = &mut self.data_payload_context;
        inner.with_mut(|fields| {
            let offset = fields.reader.get_entry_offset()?;
            if *data_offsets_entry != Some(offset) {
                collect_entry_data_offsets(fields.file, offset, data_offsets)?;
                *data_offsets_entry = Some(offset);
            }
            *data_payload_context = Some(fields.file.data_payload_read_context());
            Ok::<(), SdkError>(())
        })?;
        self.data_index = 0;
        self.entry_data_state_active = true;
        self.row_payloads.clear();
        self.row_owned_payloads.clear();
        Ok(())
    }

    pub fn enumerate_entry_payload(&mut self) -> Result<Option<&[u8]>> {
        let Some(data_offset) = self.data_offsets.get(self.data_index).copied() else {
            self.entry_data_state_active = true;
            return Ok(None);
        };
        let index = self.data_index;
        self.data_index += 1;
        self.entry_data_state_active = true;
        if self.row_payloads.len() == index {
            let payload = self.read_row_payload(data_offset)?;
            let payload = match payload {
                RowPayloadData::Borrowed { ptr, len } => RowPayload::Borrowed { ptr, len },
                RowPayloadData::Owned(bytes) => {
                    let owned_index = self.row_owned_payloads.len();
                    self.row_owned_payloads.push(bytes);
                    RowPayload::Owned { index: owned_index }
                }
            };
            self.row_payloads.push(payload);
        }
        let payload = self
            .row_payloads
            .get(index)
            .expect("payload should be stored before returning it");
        Ok(Some(payload.as_slice(&self.row_owned_payloads)))
    }

    fn read_row_payload(&mut self, data_offset: NonZeroU64) -> Result<RowPayloadData> {
        let borrow_uncompressed = self.borrow_uncompressed_row_payloads;
        let cached_context = self.data_payload_context;
        self.inner.with_mut(|fields| {
            let context = cached_context.unwrap_or_else(|| fields.file.data_payload_read_context());
            fields.reader.release_object_guards();
            let info = fields
                .file
                .data_payload_object_info_at(context, data_offset)?;
            if !info.is_compressed() {
                if borrow_uncompressed {
                    let (ptr, len) = fields.file.raw_data_payload_ptr_with_info_unguarded(
                        context,
                        data_offset,
                        info,
                    )?;
                    return Ok(RowPayloadData::Borrowed { ptr, len });
                }

                let payload =
                    fields
                        .reader
                        .raw_data_payload_at(fields.file, context, info, data_offset)?;
                let owned = RowPayloadData::Owned(payload.to_vec());
                fields.reader.release_object_guards();
                return Ok(owned);
            }

            let data_guard = fields.reader.data_object_at(fields.file, data_offset)?;
            let mut decompressed = Vec::new();
            let len = data_guard.decompress(&mut decompressed)?;
            decompressed.truncate(len);
            fields.reader.release_object_guards();
            Ok(RowPayloadData::Owned(decompressed))
        })
    }

    pub fn collect_entry_payloads(&mut self, payloads: &mut Vec<Vec<u8>>) -> Result<()> {
        payloads.clear();
        self.visit_entry_payloads(|payload| {
            payloads.push(payload.to_vec());
            Ok(())
        })
    }

    pub fn get_entry_payload(&mut self, field: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut found = None;
        self.visit_entry_payloads(|payload| {
            if found.is_none()
                && payload.len() > field.len()
                && payload.starts_with(field)
                && payload[field.len()] == b'='
            {
                found = Some(payload.to_vec());
            }
            Ok(())
        })?;
        Ok(found)
    }

    pub fn get_realtime_usec(&self) -> Result<u64> {
        if let Some(key) = self.current_key {
            return Ok(key.realtime);
        }
        self.inner
            .with(|fields| fields.reader.get_realtime_usec(fields.file))
            .map_err(Into::into)
    }

    pub fn get_cursor(&self) -> Result<String> {
        if let Some(key) = self.current_key {
            return Ok(format_cursor_from_key(key));
        }
        self.inner
            .with(|fields| build_cursor(fields.file, fields.reader))
    }

    fn current_directory_entry_key(&self) -> Result<DirectoryEntryKey> {
        if let Some(key) = self.current_key {
            return Ok(key);
        }
        self.inner.with(|fields| {
            let offset = fields.reader.get_entry_offset()?;
            let entry = fields.file.entry_ref(offset)?;
            let header = fields.file.journal_header_ref();
            Ok(DirectoryEntryKey {
                seqnum_id: header.seqnum_id,
                seqnum: entry.header.seqnum,
                boot_id: entry.header.boot_id,
                monotonic: entry.header.monotonic,
                realtime: entry.header.realtime,
                xor_hash: entry.header.xor_hash,
            })
        })
    }

    pub fn test_cursor(&self, cursor: &str) -> Result<bool> {
        Ok(self.get_cursor()? == cursor)
    }

    pub fn add_match(&mut self, data: &[u8]) {
        self.inner.with_reader_mut(|reader| reader.add_match(data));
    }

    pub fn add_conjunction(&mut self) -> Result<()> {
        self.inner
            .with_mut(|fields| fields.reader.add_conjunction(fields.file))
            .map_err(Into::into)
    }

    pub fn add_disjunction(&mut self) -> Result<()> {
        self.inner
            .with_mut(|fields| fields.reader.add_disjunction(fields.file))
            .map_err(Into::into)
    }

    pub fn flush_matches(&mut self) {
        self.inner.with_reader_mut(|reader| reader.flush_matches());
    }
}

/// Validate the structural integrity of a journal file.
///
/// Opens the file (decompressing `.zst` if needed), validates the header,
/// and walks all entries and their referenced data objects.
/// Any parse or decompression error is reported as an `SdkError` with
/// a message containing "corrupt" so callers can detect verification failures.
///
/// For sealed journals, this validates structure only; use `verify_file_with_key`
/// when TAG/HMAC verification is required.
pub fn verify_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let data = read_journal_file_for_verify(path)
        .map_err(|err| SdkError::VerificationError(format!("open/decompression failed: {err}")))?;
    verify_graph::verify_object_graph(&data)
        .map_err(|err| SdkError::VerificationError(format!("corrupt object graph: {err}")))?;

    let reader = FileReader::open(path)
        .map_err(|err| SdkError::VerificationError(format!("open/decompression failed: {err}")))?;
    reader.inner.with_file(verify_journal_file_strict)
}

/// Validate the integrity of a journal file with a verification key.
///
/// For sealed files, parses the key and validates TAG/HMAC chains.
/// For unsealed files, behaves like `verify_file`.
pub fn verify_file_with_key(path: impl AsRef<Path>, verification_key: &str) -> Result<()> {
    let path = path.as_ref();
    let data = read_journal_file_for_verify(path)
        .map_err(|err| SdkError::VerificationError(format!("open/decompression failed: {err}")))?;

    if data.len() < HEADER_MIN_SIZE as usize {
        return Err(SdkError::VerificationError("file too small".into()));
    }
    verify_graph::verify_object_graph(&data)
        .map_err(|err| SdkError::VerificationError(format!("corrupt object graph: {err}")))?;

    let compatible_flags = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let incompatible_flags = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let sealed = (compatible_flags & 1) != 0;

    if !sealed {
        return verify_file(path);
    }

    let (seed, start_usec, interval_usec) = parse_verification_key(verification_key)
        .map_err(|e| SdkError::VerificationError(format!("invalid verification key: {e}")))?;

    verify_sealed(
        &data,
        compatible_flags,
        incompatible_flags,
        seed,
        start_usec,
        interval_usec,
    )?;
    verify_file(path)
}

fn read_journal_file_for_verify(path: &Path) -> std::io::Result<Vec<u8>> {
    if is_zst_file(path) {
        let source = File::open(path)?;
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(source)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let mut data = Vec::new();
        decoder.read_to_end(&mut data)?;
        Ok(data)
    } else {
        std::fs::read(path)
    }
}

fn parse_verification_key(key: &str) -> std::result::Result<([u8; 12], u64, u64), String> {
    let bytes = key.as_bytes();
    let (seed, slash_offset) = parse_verification_seed(bytes)?;
    if slash_offset >= bytes.len() || bytes[slash_offset] != b'/' {
        return Err("missing / separator".into());
    }
    let (start_usec, dash_offset) = parse_verification_hex_value(bytes, slash_offset + 1, "start")?;
    if dash_offset >= bytes.len() || bytes[dash_offset] != b'-' {
        return Err("bad start hex".into());
    }
    let (interval_usec, end_offset) =
        parse_verification_hex_value(bytes, dash_offset + 1, "interval")?;
    if end_offset != bytes.len() {
        return Err("trailing data".into());
    }
    if interval_usec == 0 {
        return Err("zero interval".into());
    }
    Ok((seed, start_usec, interval_usec))
}

fn parse_verification_seed(bytes: &[u8]) -> std::result::Result<([u8; 12], usize), String> {
    let mut seed = [0u8; 12];
    let mut i = 0;
    for c in 0..12 {
        let (next, val) = parse_verification_seed_byte(bytes, i)?;
        seed[c] = val;
        i = next;
    }
    Ok((seed, i))
}

fn parse_verification_seed_byte(
    bytes: &[u8],
    start: usize,
) -> std::result::Result<(usize, u8), String> {
    let mut i = start;
    while i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    if i + 2 > bytes.len() {
        return Err("seed too short".into());
    }
    let val = u8::from_str_radix(std::str::from_utf8(&bytes[i..i + 2]).unwrap_or("xx"), 16)
        .map_err(|_| "bad seed hex".to_string())?;
    Ok((i + 2, val))
}

fn parse_verification_hex_value(
    bytes: &[u8],
    start: usize,
    label: &str,
) -> std::result::Result<(u64, usize), String> {
    let (next, ok) = consume_hex(bytes, start);
    if !ok {
        return Err(format!("bad {label} hex"));
    }
    let value = u64::from_str_radix(std::str::from_utf8(&bytes[start..next]).unwrap_or("0"), 16)
        .map_err(|_| format!("bad {label} hex"))?;
    Ok((value, next))
}

fn consume_hex(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
        i += 1;
    }
    (i, i > start)
}

fn align8(v: u64) -> u64 {
    v.checked_add(7).map(|value| value & !7).unwrap_or(0)
}

fn verify_slice<'a>(data: &'a [u8], offset: usize, len: usize, label: &str) -> Result<&'a [u8]> {
    let end = offset.checked_add(len).ok_or_else(|| {
        SdkError::VerificationError(format!("{} read at offset {} overflows", label, offset))
    })?;
    data.get(offset..end).ok_or_else(|| {
        SdkError::VerificationError(format!(
            "{} read at offset {} exceeds file bounds",
            label, offset
        ))
    })
}

fn read_u64_for_verify(data: &[u8], offset: usize, label: &str) -> Result<u64> {
    let bytes = verify_slice(data, offset, 8, label)?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
        SdkError::VerificationError(format!("{label} has invalid length"))
    })?))
}

const COMPATIBLE_SEALED_CONTINUOUS: u32 = 1 << 2;
const HEADER_MIN_SIZE: u64 = 208;
const OBJECT_TYPE_DATA: u8 = 1;
const OBJECT_TYPE_FIELD: u8 = 2;
const OBJECT_TYPE_ENTRY: u8 = 3;
const OBJECT_TYPE_DATA_HASH_TABLE: u8 = 4;
const OBJECT_TYPE_FIELD_HASH_TABLE: u8 = 5;
const OBJECT_TYPE_ENTRY_ARRAY: u8 = 6;
const OBJECT_TYPE_TAG: u8 = 7;
const OBJECT_HEADER_SIZE: u64 = 16;
const DATA_OBJECT_HEADER_SIZE: u64 = 64;
const COMPACT_DATA_OBJECT_HEADER_SIZE: u64 = 72;
const FIELD_OBJECT_HEADER_SIZE: u64 = 40;
const INCOMPATIBLE_COMPACT: u32 = 1 << 4;
const INCOMPATIBLE_COMPRESSED_XZ: u32 = 1 << 0;
const INCOMPATIBLE_COMPRESSED_LZ4: u32 = 1 << 1;
const INCOMPATIBLE_COMPRESSED_ZSTD: u32 = 1 << 3;
const OBJECT_COMPRESSED_XZ: u8 = 1 << 0;
const OBJECT_COMPRESSED_LZ4: u8 = 1 << 1;
const OBJECT_COMPRESSED_ZSTD: u8 = 1 << 2;

#[derive(Clone, Copy)]
struct SealedVerifyObject {
    offset: u64,
    typ: u8,
    flags: u8,
    size: u64,
    aligned_size: u64,
}

#[derive(Clone, Copy)]
struct SealedVerifyEntry {
    seqnum: u64,
    realtime: u64,
    monotonic: u64,
    boot_id: [u8; 16],
}

struct SealedVerifyState<'a> {
    data: &'a [u8],
    compatible_flags: u32,
    incompatible_flags: u32,
    seed: [u8; 12],
    msk: Vec<u8>,
    state0: Vec<u8>,
    start_epoch: u64,
    interval_usec: u64,
    is_compact: bool,
    header_size: u64,
    tail_object_offset: u64,
    file_size: u64,
    head_entry_seqnum: u64,
    head_entry_realtime: u64,
    n_objects_header: u64,
    n_entries_header: u64,
    n_tags_header: u64,
    n_objects: u64,
    n_entries: u64,
    n_tags: u64,
    last_tag_end: u64,
    last_epoch: u64,
    last_tag_realtime: u64,
    entry_seqnum: u64,
    entry_seqnum_set: bool,
    entry_monotonic: u64,
    entry_monotonic_set: bool,
    entry_boot_id: [u8; 16],
    entry_realtime: u64,
    entry_realtime_set: bool,
    max_entry_realtime: u64,
    min_entry_realtime: u64,
}

fn verify_sealed(
    data: &[u8],
    compatible_flags: u32,
    incompatible_flags: u32,
    seed: [u8; 12],
    start_epoch: u64,
    interval_usec: u64,
) -> Result<()> {
    SealedVerifyState::new(
        data,
        compatible_flags,
        incompatible_flags,
        seed,
        start_epoch,
        interval_usec,
    )?
    .run()
}

impl<'a> SealedVerifyState<'a> {
    fn new(
        data: &'a [u8],
        compatible_flags: u32,
        incompatible_flags: u32,
        seed: [u8; 12],
        start_epoch: u64,
        interval_usec: u64,
    ) -> Result<Self> {
        let header_size = read_u64_for_verify(data, 88, "header_size")?;
        let file_size = data.len() as u64;
        if header_size < HEADER_MIN_SIZE || header_size > file_size {
            return Err(SdkError::VerificationError(format!(
                "invalid header_size {header_size}"
            )));
        }
        let (msk, mpk) = gen_mk(&seed, RECOMMENDED_SECPAR);
        let n_tags_header = if header_size >= 232 && data.len() >= 232 {
            read_u64_for_verify(data, 224, "n_tags")?
        } else {
            0
        };
        Ok(Self {
            data,
            compatible_flags,
            incompatible_flags,
            seed,
            msk,
            state0: gen_state0(&mpk, &seed),
            start_epoch,
            interval_usec,
            is_compact: (incompatible_flags & INCOMPATIBLE_COMPACT) != 0,
            header_size,
            tail_object_offset: read_u64_for_verify(data, 136, "tail_object_offset")?,
            file_size,
            head_entry_seqnum: read_u64_for_verify(data, 168, "head_entry_seqnum")?,
            head_entry_realtime: read_u64_for_verify(data, 184, "head_entry_realtime")?,
            n_objects_header: read_u64_for_verify(data, 144, "n_objects")?,
            n_entries_header: read_u64_for_verify(data, 152, "n_entries")?,
            n_tags_header,
            n_objects: 0,
            n_entries: 0,
            n_tags: 0,
            last_tag_end: 0,
            last_epoch: 0,
            last_tag_realtime: 0,
            entry_seqnum: 0,
            entry_seqnum_set: false,
            entry_monotonic: 0,
            entry_monotonic_set: false,
            entry_boot_id: [0; 16],
            entry_realtime: 0,
            entry_realtime_set: false,
            max_entry_realtime: 0,
            min_entry_realtime: u64::MAX,
        })
    }

    fn run(mut self) -> Result<()> {
        let mut offset = self.header_size;
        while self.tail_object_offset != 0 {
            let obj = self.read_object(offset)?;
            self.verify_object(obj)?;
            if offset == self.tail_object_offset {
                break;
            }
            offset += obj.aligned_size;
        }
        self.verify_final_counts()
    }

    fn read_object(&self, offset: u64) -> Result<SealedVerifyObject> {
        if offset > self.tail_object_offset {
            return Err(SdkError::VerificationError(format!(
                "object offset {offset} exceeds tail_object_offset {}",
                self.tail_object_offset
            )));
        }
        if offset > self.file_size - OBJECT_HEADER_SIZE {
            return Err(SdkError::VerificationError(format!(
                "object header at offset {offset} exceeds file bounds"
            )));
        }
        let obj = SealedVerifyObject {
            offset,
            typ: self.data[offset as usize],
            flags: self.data[offset as usize + 1],
            size: read_u64_for_verify(self.data, offset as usize + 8, "object size")?,
            aligned_size: 0,
        };
        let obj = SealedVerifyObject {
            aligned_size: align8(obj.size),
            ..obj
        };
        self.verify_object_envelope(obj)?;
        self.verify_object_flags(obj)?;
        Ok(obj)
    }

    fn verify_object_envelope(&self, obj: SealedVerifyObject) -> Result<()> {
        if obj.size < OBJECT_HEADER_SIZE {
            return Err(SdkError::VerificationError(format!(
                "object size {} too small at offset {}",
                obj.size, obj.offset
            )));
        }
        if obj.aligned_size < obj.size || obj.aligned_size == 0 {
            return Err(SdkError::VerificationError(format!(
                "object size {} overflows alignment at offset {}",
                obj.size, obj.offset
            )));
        }
        if obj.aligned_size > self.file_size - obj.offset {
            return Err(SdkError::VerificationError(format!(
                "object at offset {} with aligned size {} exceeds file bounds",
                obj.offset, obj.aligned_size
            )));
        }
        Ok(())
    }

    fn verify_object_flags(&self, obj: SealedVerifyObject) -> Result<()> {
        if object_compression_flag_count(obj.flags) > 1 {
            return Err(SdkError::VerificationError(format!(
                "multiple compression flags at offset {}",
                obj.offset
            )));
        }
        self.verify_enabled_compression_flag(obj)?;
        if obj.flags & !(OBJECT_COMPRESSED_XZ | OBJECT_COMPRESSED_LZ4 | OBJECT_COMPRESSED_ZSTD) != 0
        {
            return Err(SdkError::VerificationError(format!(
                "unknown object flags 0x{:02x} at offset {}",
                obj.flags, obj.offset
            )));
        }
        if obj.typ != OBJECT_TYPE_DATA && obj.flags != 0 {
            return Err(SdkError::VerificationError(format!(
                "object type {} at offset {} has compression flags",
                obj.typ, obj.offset
            )));
        }
        Ok(())
    }

    fn verify_enabled_compression_flag(&self, obj: SealedVerifyObject) -> Result<()> {
        if obj.flags & OBJECT_COMPRESSED_XZ != 0
            && self.incompatible_flags & INCOMPATIBLE_COMPRESSED_XZ == 0
        {
            return Err(SdkError::VerificationError(format!(
                "XZ object in file without XZ support at offset {}",
                obj.offset
            )));
        }
        if obj.flags & OBJECT_COMPRESSED_LZ4 != 0
            && self.incompatible_flags & INCOMPATIBLE_COMPRESSED_LZ4 == 0
        {
            return Err(SdkError::VerificationError(format!(
                "LZ4 object in file without LZ4 support at offset {}",
                obj.offset
            )));
        }
        if obj.flags & OBJECT_COMPRESSED_ZSTD != 0
            && self.incompatible_flags & INCOMPATIBLE_COMPRESSED_ZSTD == 0
        {
            return Err(SdkError::VerificationError(format!(
                "ZSTD object in file without ZSTD support at offset {}",
                obj.offset
            )));
        }
        Ok(())
    }

    fn verify_object(&mut self, obj: SealedVerifyObject) -> Result<()> {
        self.n_objects += 1;
        match obj.typ {
            OBJECT_TYPE_DATA
            | OBJECT_TYPE_FIELD
            | OBJECT_TYPE_DATA_HASH_TABLE
            | OBJECT_TYPE_FIELD_HASH_TABLE
            | OBJECT_TYPE_ENTRY_ARRAY => Ok(()),
            OBJECT_TYPE_ENTRY => self.verify_entry_object(obj),
            OBJECT_TYPE_TAG => self.verify_tag_object(obj),
            _ => Err(SdkError::VerificationError(format!(
                "unknown object type {} at offset {}",
                obj.typ, obj.offset
            ))),
        }
    }

    fn verify_entry_object(&mut self, obj: SealedVerifyObject) -> Result<()> {
        if self.n_tags == 0 {
            return Err(SdkError::VerificationError(format!(
                "first entry before first tag at offset {}",
                obj.offset
            )));
        }
        let entry = self.read_entry(obj.offset)?;
        self.verify_entry_realtime_floor(obj, entry)?;
        self.verify_entry_seqnum(obj, entry.seqnum)?;
        self.verify_entry_monotonic(obj, entry)?;
        self.verify_entry_realtime_head(obj, entry.realtime)?;
        self.record_entry_realtime(entry.realtime);
        self.n_entries += 1;
        Ok(())
    }

    fn read_entry(&self, offset: u64) -> Result<SealedVerifyEntry> {
        let mut boot_id = [0u8; 16];
        let boot_id_bytes = verify_slice(self.data, offset as usize + 40, 16, "entry boot_id")?;
        boot_id.copy_from_slice(boot_id_bytes);
        Ok(SealedVerifyEntry {
            seqnum: read_u64_for_verify(self.data, offset as usize + 16, "entry seqnum")?,
            realtime: read_u64_for_verify(self.data, offset as usize + 24, "entry realtime")?,
            monotonic: read_u64_for_verify(self.data, offset as usize + 32, "entry monotonic")?,
            boot_id,
        })
    }

    fn verify_entry_realtime_floor(
        &self,
        obj: SealedVerifyObject,
        entry: SealedVerifyEntry,
    ) -> Result<()> {
        if self.entry_realtime_set && entry.realtime < self.last_tag_realtime {
            return Err(SdkError::VerificationError(format!(
                "older entry after newer tag at offset {}",
                obj.offset
            )));
        }
        Ok(())
    }

    fn verify_entry_seqnum(&mut self, obj: SealedVerifyObject, seqnum: u64) -> Result<()> {
        if !self.entry_seqnum_set && seqnum != self.head_entry_seqnum {
            return Err(SdkError::VerificationError(format!(
                "head entry seqnum mismatch at offset {}",
                obj.offset
            )));
        }
        if self.entry_seqnum_set && self.entry_seqnum >= seqnum {
            return Err(SdkError::VerificationError(format!(
                "entry seqnum out of sync at offset {}",
                obj.offset
            )));
        }
        self.entry_seqnum = seqnum;
        self.entry_seqnum_set = true;
        Ok(())
    }

    fn verify_entry_monotonic(
        &mut self,
        obj: SealedVerifyObject,
        entry: SealedVerifyEntry,
    ) -> Result<()> {
        if self.entry_monotonic_set
            && entry.boot_id == self.entry_boot_id
            && self.entry_monotonic > entry.monotonic
        {
            return Err(SdkError::VerificationError(format!(
                "entry monotonic out of sync at offset {}",
                obj.offset
            )));
        }
        self.entry_monotonic = entry.monotonic;
        self.entry_boot_id = entry.boot_id;
        self.entry_monotonic_set = true;
        Ok(())
    }

    fn verify_entry_realtime_head(&mut self, obj: SealedVerifyObject, realtime: u64) -> Result<()> {
        if !self.entry_realtime_set && realtime != self.head_entry_realtime {
            return Err(SdkError::VerificationError(format!(
                "head entry realtime mismatch at offset {}",
                obj.offset
            )));
        }
        self.entry_realtime = realtime;
        self.entry_realtime_set = true;
        Ok(())
    }

    fn record_entry_realtime(&mut self, realtime: u64) {
        self.max_entry_realtime = self.max_entry_realtime.max(realtime);
        self.min_entry_realtime = self.min_entry_realtime.min(realtime);
    }

    fn verify_tag_object(&mut self, obj: SealedVerifyObject) -> Result<()> {
        if obj.size != OBJECT_HEADER_SIZE + 8 + 8 + TAG_LENGTH as u64 {
            return Err(SdkError::VerificationError(format!(
                "invalid tag object size {} at offset {}",
                obj.size, obj.offset
            )));
        }
        let seqnum = read_u64_for_verify(self.data, obj.offset as usize + 16, "tag seqnum")?;
        let epoch = read_u64_for_verify(self.data, obj.offset as usize + 24, "tag epoch")?;
        self.verify_tag_seqnum(obj, seqnum)?;
        self.verify_tag_epoch(obj, epoch)?;
        let rt = self.verify_tag_realtime_window(obj, epoch)?;
        self.verify_tag_hmac(obj, epoch)?;
        self.record_tag(obj, epoch, rt);
        Ok(())
    }

    fn verify_tag_seqnum(&self, obj: SealedVerifyObject, seqnum: u64) -> Result<()> {
        if seqnum != self.n_tags + 1 {
            return Err(SdkError::VerificationError(format!(
                "tag seqnum mismatch: got {seqnum}, want {} at offset {}",
                self.n_tags + 1,
                obj.offset
            )));
        }
        Ok(())
    }

    fn verify_tag_epoch(&self, obj: SealedVerifyObject, epoch: u64) -> Result<()> {
        if self.compatible_flags & COMPATIBLE_SEALED_CONTINUOUS != 0 {
            return self.verify_continuous_tag_epoch(obj, epoch);
        }
        if epoch < self.last_epoch {
            return Err(SdkError::VerificationError(format!(
                "epoch out of sync: got {epoch}, last {} at offset {}",
                self.last_epoch, obj.offset
            )));
        }
        Ok(())
    }

    fn verify_continuous_tag_epoch(&self, obj: SealedVerifyObject, epoch: u64) -> Result<()> {
        let ok = self.n_tags == 0
            || (self.n_tags == 1 && epoch == self.last_epoch)
            || epoch == self.last_epoch + 1;
        if !ok {
            return Err(SdkError::VerificationError(format!(
                "epoch not continuous: got {epoch}, last {} at offset {}",
                self.last_epoch, obj.offset
            )));
        }
        Ok(())
    }

    fn verify_tag_realtime_window(&self, obj: SealedVerifyObject, epoch: u64) -> Result<u64> {
        let (rt, rt_end) = tag_realtime_range(self.start_epoch, epoch, self.interval_usec)?;
        if self.entry_realtime_set && self.entry_realtime >= rt_end {
            return Err(SdkError::VerificationError(format!(
                "entry realtime {} too late for tag end {rt_end} at offset {}",
                self.entry_realtime, obj.offset
            )));
        }
        if self.max_entry_realtime >= rt_end {
            return Err(SdkError::VerificationError(format!(
                "max entry realtime {} too late for tag end {rt_end} at offset {}",
                self.max_entry_realtime, obj.offset
            )));
        }
        if self.min_entry_realtime < rt {
            return Err(SdkError::VerificationError(format!(
                "entry realtime {} too early for tag start {rt} at offset {}",
                self.min_entry_realtime, obj.offset
            )));
        }
        Ok(rt)
    }

    fn verify_tag_hmac(&self, obj: SealedVerifyObject, epoch: u64) -> Result<()> {
        let mut hm = self.new_tag_hmac(epoch);
        if self.n_tags == 0 {
            self.write_first_tag_header_hmac(&mut hm);
        }
        self.write_tag_object_hmacs(&mut hm, obj.offset)?;
        let stored =
            &self.data[(obj.offset as usize + 32)..(obj.offset as usize + 32 + TAG_LENGTH)];
        if hm.verify_slice(stored).is_err() {
            return Err(SdkError::VerificationError(format!(
                "tag failed verification at offset {}",
                obj.offset
            )));
        }
        Ok(())
    }

    fn new_tag_hmac(&self, epoch: u64) -> Hmac<Sha256> {
        let state = seek(&self.state0, epoch, &self.msk, &self.seed);
        let key = get_key(&state, TAG_LENGTH, 0);
        Hmac::<Sha256>::new_from_slice(&key).expect("HMAC key length valid")
    }

    fn write_first_tag_header_hmac(&self, hm: &mut Hmac<Sha256>) {
        hm.update(&self.data[0..16]);
        hm.update(&self.data[24..56]);
        hm.update(&self.data[72..96]);
        hm.update(&self.data[104..136]);
    }

    fn write_tag_object_hmacs(&self, hm: &mut Hmac<Sha256>, tag_offset: u64) -> Result<()> {
        let mut offset = self.last_tag_end;
        if self.n_tags == 0 {
            offset = self.header_size;
        }
        while offset <= tag_offset {
            let obj = self.read_hmac_object(offset)?;
            hmac_object(hm, self.data, offset, obj.typ, obj.size, self.is_compact);
            offset += obj.aligned_size;
        }
        Ok(())
    }

    fn read_hmac_object(&self, offset: u64) -> Result<SealedVerifyObject> {
        if offset > self.file_size - OBJECT_HEADER_SIZE {
            return Err(SdkError::VerificationError(format!(
                "HMAC object header at offset {offset} exceeds file bounds"
            )));
        }
        let size = read_u64_for_verify(self.data, offset as usize + 8, "HMAC object size")?;
        let aligned_size = align8(size);
        if size < OBJECT_HEADER_SIZE {
            return Err(SdkError::VerificationError(format!(
                "HMAC object size {size} too small at offset {offset}"
            )));
        }
        if aligned_size < size || aligned_size == 0 {
            return Err(SdkError::VerificationError(format!(
                "HMAC object size {size} overflows alignment at offset {offset}"
            )));
        }
        if aligned_size > self.file_size - offset {
            return Err(SdkError::VerificationError(format!(
                "HMAC object at offset {offset} with aligned size {aligned_size} exceeds file bounds"
            )));
        }
        Ok(SealedVerifyObject {
            offset,
            typ: self.data[offset as usize],
            flags: 0,
            size,
            aligned_size,
        })
    }

    fn record_tag(&mut self, obj: SealedVerifyObject, epoch: u64, realtime: u64) {
        self.n_tags += 1;
        self.last_tag_end = obj.offset + obj.aligned_size;
        self.last_epoch = epoch;
        self.last_tag_realtime = realtime;
        self.min_entry_realtime = u64::MAX;
    }

    fn verify_final_counts(&self) -> Result<()> {
        if self.n_objects != self.n_objects_header {
            return Err(SdkError::VerificationError(format!(
                "object count mismatch: got {}, want {}",
                self.n_objects, self.n_objects_header
            )));
        }
        if self.n_entries != self.n_entries_header {
            return Err(SdkError::VerificationError(format!(
                "entry count mismatch: got {}, want {}",
                self.n_entries, self.n_entries_header
            )));
        }
        if self.n_tags != self.n_tags_header {
            return Err(SdkError::VerificationError(format!(
                "tag count mismatch: got {}, want {}",
                self.n_tags, self.n_tags_header
            )));
        }
        Ok(())
    }
}

fn object_compression_flag_count(flags: u8) -> u32 {
    [
        OBJECT_COMPRESSED_XZ,
        OBJECT_COMPRESSED_LZ4,
        OBJECT_COMPRESSED_ZSTD,
    ]
    .iter()
    .filter(|flag| flags & **flag != 0)
    .count() as u32
}

fn tag_realtime_range(start_epoch: u64, epoch: u64, interval_usec: u64) -> Result<(u64, u64)> {
    let absolute_epoch = start_epoch
        .checked_add(epoch)
        .ok_or_else(|| SdkError::VerificationError("tag realtime overflow".into()))?;
    let rt = absolute_epoch
        .checked_mul(interval_usec)
        .ok_or_else(|| SdkError::VerificationError("tag realtime overflow".into()))?;
    let rt_end = rt
        .checked_add(interval_usec)
        .ok_or_else(|| SdkError::VerificationError("tag realtime overflow".into()))?;
    Ok((rt, rt_end))
}

fn hmac_object(
    hm: &mut impl hmac::Mac,
    data: &[u8],
    offset: u64,
    typ: u8,
    size: u64,
    is_compact: bool,
) {
    hm.update(&data[offset as usize..(offset + OBJECT_HEADER_SIZE) as usize]);

    match typ {
        OBJECT_TYPE_DATA => {
            hm.update(&data[(offset + 16) as usize..(offset + 24) as usize]);
            let payload_offset = if is_compact {
                COMPACT_DATA_OBJECT_HEADER_SIZE
            } else {
                DATA_OBJECT_HEADER_SIZE
            };
            if size > payload_offset {
                hm.update(&data[(offset + payload_offset) as usize..(offset + size) as usize]);
            }
        }
        OBJECT_TYPE_FIELD => {
            hm.update(&data[(offset + 16) as usize..(offset + 24) as usize]);
            if size > FIELD_OBJECT_HEADER_SIZE {
                hm.update(
                    &data[(offset + FIELD_OBJECT_HEADER_SIZE) as usize..(offset + size) as usize],
                );
            }
        }
        OBJECT_TYPE_ENTRY => {
            if size > OBJECT_HEADER_SIZE {
                hm.update(&data[(offset + OBJECT_HEADER_SIZE) as usize..(offset + size) as usize]);
            }
        }
        OBJECT_TYPE_DATA_HASH_TABLE | OBJECT_TYPE_FIELD_HASH_TABLE | OBJECT_TYPE_ENTRY_ARRAY => {}
        OBJECT_TYPE_TAG => {
            hm.update(
                &data[(offset + OBJECT_HEADER_SIZE) as usize
                    ..(offset + OBJECT_HEADER_SIZE + 16) as usize],
            );
        }
        _ => {}
    }
}

pub struct DirectoryReader {
    files: Vec<FileReader>,
    index: usize,
    pending_realtime_seek: Option<u64>,
    realtime_seek_bound: Option<(u64, Direction)>,
    candidates: Vec<Option<DirectoryCandidate>>,
    current_key: Option<DirectoryEntryKey>,
    direction: Option<Direction>,
    boot_newest: HashMap<[u8; 16], DirectoryBootNewest>,
    non_overlapping: bool,
}

#[derive(Debug, Clone, Copy)]
struct DirectoryCandidate {
    reader_index: usize,
    key: DirectoryEntryKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectoryEntryKey {
    seqnum_id: [u8; 16],
    seqnum: u64,
    boot_id: [u8; 16],
    monotonic: u64,
    realtime: u64,
    xor_hash: u64,
}

#[derive(Debug, Clone, Copy)]
struct DirectoryBootNewest {
    machine_id: [u8; 16],
    monotonic: u64,
    realtime: u64,
}

impl DirectoryReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_options(path, ReaderOptions::default())
    }

    pub fn open_with_options(path: impl AsRef<Path>, options: ReaderOptions) -> Result<Self> {
        let path = path.as_ref();
        if !path.is_dir() {
            return Err(SdkError::InvalidPath(format!(
                "not a directory: {}",
                path.display()
            )));
        }

        let mut files = Vec::new();
        for file_path in collect_journal_files(path)? {
            if let Ok(reader) = FileReader::open_with_options(&file_path, options) {
                files.push(reader);
            }
        }

        Self::from_readers(files, true)
    }

    pub fn open_files<I, P>(paths: I) -> Result<Self>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        Self::open_files_with_options(paths, ReaderOptions::default())
    }

    pub fn open_files_with_options<I, P>(paths: I, options: ReaderOptions) -> Result<Self>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut files = Vec::new();
        for path in paths {
            let path = path.as_ref();
            if !path.is_file() || !is_journal_file_name(path) {
                return Err(SdkError::InvalidPath(format!(
                    "not a journal file: {}",
                    path.display()
                )));
            }
            files.push(FileReader::open_with_options(path, options)?);
        }

        Self::from_readers(files, false)
    }

    fn from_readers(mut files: Vec<FileReader>, allow_empty: bool) -> Result<Self> {
        if files.is_empty() && !allow_empty {
            return Err(SdkError::InvalidPath(
                "no readable journal files".to_string(),
            ));
        }

        files.sort_by_key(FileReader::header_realtime_start);
        let boot_newest = build_directory_boot_newest(&files);
        let non_overlapping = directory_files_non_overlapping(&files);
        let candidates = vec![None; files.len()];
        Ok(Self {
            files,
            index: usize::MAX,
            pending_realtime_seek: None,
            realtime_seek_bound: None,
            candidates,
            current_key: None,
            direction: None,
            boot_newest,
            non_overlapping,
        })
    }

    pub fn seek_head(&mut self) {
        self.pending_realtime_seek = None;
        self.realtime_seek_bound = None;
        self.index = usize::MAX;
        self.current_key = None;
        self.direction = None;
        self.reset_candidates();
        for reader in &mut self.files {
            reader.seek_head();
        }
    }

    pub fn seek_tail(&mut self) {
        self.pending_realtime_seek = None;
        self.realtime_seek_bound = None;
        self.index = usize::MAX;
        self.current_key = None;
        self.direction = None;
        self.reset_candidates();
        for reader in &mut self.files {
            reader.seek_tail();
        }
    }

    pub fn seek_realtime(&mut self, usec: u64) {
        self.pending_realtime_seek = Some(usec);
        self.realtime_seek_bound = None;
        self.index = usize::MAX;
        self.current_key = None;
        self.direction = None;
        self.reset_candidates();
    }

    pub fn next(&mut self) -> Result<bool> {
        self.step_merged(Direction::Forward)
    }

    pub fn previous(&mut self) -> Result<bool> {
        self.step_merged(Direction::Backward)
    }

    fn step_merged(&mut self, direction: Direction) -> Result<bool> {
        if self.can_step_sequential(direction) {
            return self.step_sequential(direction);
        }

        self.prepare_merge_direction(direction);

        let mut best: Option<DirectoryCandidate> = None;
        for idx in 0..self.files.len() {
            self.fill_candidate(idx, direction)?;
            let Some(candidate) = self.candidates[idx] else {
                continue;
            };
            let replace = match best {
                None => true,
                Some(current) => {
                    let cmp = self.compare_entry_keys(candidate.key, current.key);
                    (direction == Direction::Forward && cmp < 0)
                        || (direction == Direction::Backward && cmp > 0)
                }
            };
            if replace {
                best = Some(candidate);
            }
        }

        let Some(best) = best else {
            self.index = usize::MAX;
            self.realtime_seek_bound = None;
            return Ok(false);
        };

        self.index = best.reader_index;
        self.current_key = Some(best.key);
        self.candidates[best.reader_index] = None;
        self.realtime_seek_bound = None;
        Ok(true)
    }

    fn prepare_merge_direction(&mut self, direction: Direction) {
        if let Some(usec) = self.pending_realtime_seek.take() {
            for reader in &mut self.files {
                reader.seek_realtime(usec);
            }
            self.reset_candidates();
            self.realtime_seek_bound = Some((usec, direction));
            self.direction = Some(direction);
            return;
        }

        if self.direction == Some(direction) {
            return;
        }

        if let Some(current) = self.current_key {
            for reader in &mut self.files {
                reader.seek_realtime(current.realtime);
            }
        } else if direction == Direction::Forward {
            for reader in &mut self.files {
                reader.seek_head();
            }
        } else {
            for reader in &mut self.files {
                reader.seek_tail();
            }
        }

        self.reset_candidates();
        self.direction = Some(direction);
    }

    fn fill_candidate(&mut self, reader_index: usize, direction: Direction) -> Result<()> {
        if self.candidates[reader_index].is_some() {
            return Ok(());
        }

        loop {
            if !self.advance_candidate_reader(reader_index, direction)? {
                return Ok(());
            }
            let key = self.files[reader_index].current_directory_entry_key()?;
            if !self.candidate_matches_realtime_bound(key) {
                continue;
            }
            if !self.candidate_is_after_current(key, direction) {
                continue;
            }

            self.candidates[reader_index] = Some(DirectoryCandidate { reader_index, key });
            return Ok(());
        }
    }

    fn advance_candidate_reader(
        &mut self,
        reader_index: usize,
        direction: Direction,
    ) -> Result<bool> {
        match direction {
            Direction::Forward => self.files[reader_index].next(),
            Direction::Backward => self.files[reader_index].previous(),
        }
    }

    fn candidate_matches_realtime_bound(&self, key: DirectoryEntryKey) -> bool {
        let Some((usec, seek_direction)) = self.realtime_seek_bound else {
            return true;
        };
        match seek_direction {
            Direction::Forward => key.realtime >= usec,
            Direction::Backward => key.realtime <= usec,
        }
    }

    fn candidate_is_after_current(&self, key: DirectoryEntryKey, direction: Direction) -> bool {
        let Some(current) = self.current_key else {
            return true;
        };
        let cmp = self.compare_entry_keys(key, current);
        match direction {
            Direction::Forward => cmp > 0,
            Direction::Backward => cmp < 0,
        }
    }

    fn compare_entry_keys(&self, a: DirectoryEntryKey, b: DirectoryEntryKey) -> i8 {
        if a == b {
            return 0;
        }

        if a.seqnum_id == b.seqnum_id {
            let cmp = cmp_u64(a.seqnum, b.seqnum);
            if cmp != 0 {
                return cmp;
            }
        }

        if a.boot_id == b.boot_id {
            let cmp = cmp_u64(a.monotonic, b.monotonic);
            if cmp != 0 {
                return cmp;
            }
        } else {
            let cmp = self.compare_boot_ids(a.boot_id, b.boot_id);
            if cmp != 0 {
                return cmp;
            }
        }

        let cmp = cmp_u64(a.realtime, b.realtime);
        if cmp != 0 {
            return cmp;
        }
        cmp_u64(a.xor_hash, b.xor_hash)
    }

    fn compare_boot_ids(&self, a: [u8; 16], b: [u8; 16]) -> i8 {
        let Some(a_newest) = self.boot_newest.get(&a) else {
            return 0;
        };
        let Some(b_newest) = self.boot_newest.get(&b) else {
            return 0;
        };
        if a_newest.machine_id != b_newest.machine_id {
            return 0;
        }
        cmp_u64(a_newest.realtime, b_newest.realtime)
    }

    fn reset_candidates(&mut self) {
        if self.candidates.len() != self.files.len() {
            self.candidates = vec![None; self.files.len()];
            return;
        }
        for candidate in &mut self.candidates {
            *candidate = None;
        }
    }

    pub fn get_entry(&mut self) -> Result<Entry> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_entry()
    }

    pub fn visit_entry_payloads<F>(&mut self, visitor: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].visit_entry_payloads(visitor)
    }

    pub fn clear_entry_data_state(&mut self) {
        if self.index < self.files.len() {
            self.files[self.index].clear_entry_data_state();
        }
    }

    pub fn entry_data_restart(&mut self) -> Result<()> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].entry_data_restart()
    }

    pub fn enumerate_entry_payload(&mut self) -> Result<Option<&[u8]>> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].enumerate_entry_payload()
    }

    pub fn collect_entry_payloads(&mut self, payloads: &mut Vec<Vec<u8>>) -> Result<()> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].collect_entry_payloads(payloads)
    }

    pub fn get_entry_payload(&mut self, field: &[u8]) -> Result<Option<Vec<u8>>> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_entry_payload(field)
    }

    pub fn get_realtime_usec(&self) -> Result<u64> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_realtime_usec()
    }

    pub fn get_cursor(&self) -> Result<String> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_cursor()
    }

    pub fn test_cursor(&self, cursor: &str) -> Result<bool> {
        if self.index >= self.files.len() {
            return Ok(false);
        }
        self.files[self.index].test_cursor(cursor)
    }

    pub fn seek_cursor(&mut self, cursor: &str) -> Result<()> {
        let want = parse_cursor(cursor).map_err(|err| SdkError::InvalidCursor(err.to_string()))?;
        let realtime = want.2;
        self.seek_realtime(realtime);
        while self.next()? {
            let current_cursor = self.get_cursor()?;
            let got = parse_cursor(&current_cursor)
                .map_err(|err| SdkError::InvalidCursor(err.to_string()))?;
            if got.2 > realtime {
                return Ok(());
            }
            if got == want {
                return Ok(());
            }
        }
        Ok(())
    }

    pub fn enumerate_fields(&mut self) -> Result<Vec<String>> {
        let mut fields = HashSet::new();
        for reader in &mut self.files {
            for field in reader.enumerate_fields()? {
                fields.insert(field);
            }
        }
        let mut out: Vec<_> = fields.into_iter().collect();
        out.sort();
        Ok(out)
    }

    pub fn query_unique(&mut self, field_name: &str) -> Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        self.visit_unique_values(field_name, |value| {
            out.push(value.to_vec());
            Ok(())
        })?;
        Ok(out)
    }

    pub fn visit_unique_values<F>(&mut self, field_name: &str, mut visitor: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        if self.files.len() == 1 {
            return self.files[0].visit_unique_values(field_name, visitor);
        }

        let mut seen = HashSet::new();
        for reader in &mut self.files {
            reader.visit_unique_values(field_name, |value| {
                if seen.insert(value.to_vec()) {
                    visitor(value)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn list_boots(&self) -> Vec<BootInfo> {
        let mut boots: HashMap<String, (i64, i64)> = HashMap::new();
        for reader in &self.files {
            let header = reader.header();
            let boot_id = hex::encode(header.tail_entry_boot_id);
            let first = header.head_entry_realtime as i64;
            let last = header.tail_entry_realtime as i64;
            boots
                .entry(boot_id)
                .and_modify(|range| {
                    range.0 = range.0.min(first);
                    range.1 = range.1.max(last);
                })
                .or_insert((first, last));
        }

        let mut out: Vec<_> = boots
            .into_iter()
            .map(|(boot_id, (first_entry, last_entry))| BootInfo {
                index: 0,
                boot_id,
                first_entry,
                last_entry,
            })
            .collect();
        out.sort_by_key(|boot| boot.first_entry);
        let base = 1 - out.len() as i64;
        for (idx, boot) in out.iter_mut().enumerate() {
            boot.index = base + idx as i64;
        }
        out
    }

    pub fn add_match(&mut self, data: &[u8]) {
        for reader in &mut self.files {
            reader.add_match(data);
        }
        self.reset_merge_state();
    }

    pub fn add_conjunction(&mut self) -> Result<()> {
        for reader in &mut self.files {
            reader.add_conjunction()?;
        }
        self.reset_merge_state();
        Ok(())
    }

    pub fn add_disjunction(&mut self) -> Result<()> {
        for reader in &mut self.files {
            reader.add_disjunction()?;
        }
        self.reset_merge_state();
        Ok(())
    }

    pub fn flush_matches(&mut self) {
        for reader in &mut self.files {
            reader.flush_matches();
        }
        self.reset_merge_state();
    }

    fn reset_merge_state(&mut self) {
        self.index = usize::MAX;
        self.current_key = None;
        self.direction = None;
        self.realtime_seek_bound = None;
        self.reset_candidates();
    }

    fn can_step_sequential(&self, direction: Direction) -> bool {
        if !self.non_overlapping || self.pending_realtime_seek.is_some() {
            return false;
        }
        if self.direction.is_some_and(|current| current != direction) && self.current_key.is_some()
        {
            return false;
        }
        true
    }

    fn step_sequential(&mut self, direction: Direction) -> Result<bool> {
        if self.files.is_empty() {
            self.clear_current_directory_entry();
            return Ok(false);
        }

        if self.direction != Some(direction) {
            self.reset_sequential_direction(direction);
        }

        match direction {
            Direction::Forward => self.step_sequential_forward(),
            Direction::Backward => self.step_sequential_backward(),
        }
    }

    fn reset_sequential_direction(&mut self, direction: Direction) {
        match direction {
            Direction::Forward => {
                for reader in &mut self.files {
                    reader.seek_head();
                }
                self.index = 0;
            }
            Direction::Backward => {
                for reader in &mut self.files {
                    reader.seek_tail();
                }
                self.index = self.files.len() - 1;
            }
        }
        self.reset_candidates();
        self.current_key = None;
        self.realtime_seek_bound = None;
        self.direction = Some(direction);
    }

    fn step_sequential_forward(&mut self) -> Result<bool> {
        if self.index == usize::MAX {
            self.index = 0;
        }
        while self.index < self.files.len() {
            if self.files[self.index].next()? {
                self.current_key = Some(self.files[self.index].current_directory_entry_key()?);
                return Ok(true);
            }
            self.index += 1;
        }
        self.finish_sequential_end()
    }

    fn step_sequential_backward(&mut self) -> Result<bool> {
        if self.index >= self.files.len() {
            self.index = self.files.len() - 1;
        }
        loop {
            if self.files[self.index].previous()? {
                self.current_key = Some(self.files[self.index].current_directory_entry_key()?);
                return Ok(true);
            }
            if self.index == 0 {
                break;
            }
            self.index -= 1;
        }
        self.finish_sequential_end()
    }

    fn finish_sequential_end(&mut self) -> Result<bool> {
        self.clear_current_directory_entry();
        Ok(false)
    }

    fn clear_current_directory_entry(&mut self) {
        self.index = usize::MAX;
        self.current_key = None;
    }
}

impl FileReader {
    fn header_realtime_start(&self) -> u64 {
        self.header().head_entry_realtime
    }

    pub fn enumerate_fields(&mut self) -> Result<Vec<String>> {
        self.invalidate_entry_data_state();
        match self.inner.with_file(enumerate_file_fields_indexed) {
            Ok(fields) => Ok(fields),
            Err(_) => enumerate_file_fields_by_scan(self),
        }
    }

    pub fn query_unique(&mut self, field_name: &str) -> Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        self.visit_unique_values(field_name, |value| {
            out.push(value.to_vec());
            Ok(())
        })?;
        Ok(out)
    }

    pub fn visit_unique_values<F>(&mut self, field_name: &str, visitor: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        self.invalidate_entry_data_state();
        let decompressed = &mut self.decompressed;
        self.inner.with_file(|file| {
            visit_file_unique_values_indexed(file, field_name.as_bytes(), decompressed, visitor)
        })
    }
}

fn open_journal_file(path: &Path, options: ReaderOptions) -> Result<JournalFile<Mmap>> {
    let file = match options.bounds {
        ReaderBounds::Live => {
            JournalFile::open_path_with_strategy(path, options.window_size, options.mmap_strategy)
        }
        ReaderBounds::Snapshot => {
            JournalFile::open_path_snapshot(path, options.window_size, options.mmap_strategy)
        }
    };
    file.map_err(Into::into)
}

fn build_cursor(file: &JournalFile<Mmap>, reader: &JournalReader<'_, Mmap>) -> Result<String> {
    let (seqnum, seqnum_id) = reader.get_seqnum(file)?;
    let offset = reader.get_entry_offset()?;
    let entry = file.entry_ref(offset)?;
    Ok(format_cursor_from_key(DirectoryEntryKey {
        seqnum_id,
        seqnum,
        boot_id: entry.header.boot_id,
        monotonic: entry.header.monotonic,
        realtime: entry.header.realtime,
        xor_hash: entry.header.xor_hash,
    }))
}

fn format_cursor_from_key(key: DirectoryEntryKey) -> String {
    format!(
        "s={};j={};c={:016x};n={}",
        hex::encode(key.seqnum_id),
        hex::encode(key.boot_id),
        key.realtime,
        key.seqnum
    )
}

fn read_entry_at(
    file: &JournalFile<Mmap>,
    reader: &JournalReader<'_, Mmap>,
    entry_offset: NonZeroU64,
    data_offsets: &mut Vec<NonZeroU64>,
    decompressed: &mut Vec<u8>,
) -> Result<Entry> {
    let (seqnum, realtime, monotonic, boot_id) =
        collect_entry_metadata_and_data_offsets(file, entry_offset, data_offsets)?;

    let mut fields = HashMap::new();
    let mut field_values: HashMap<String, Vec<Vec<u8>>> = HashMap::new();
    let mut payloads = Vec::new();

    payloads.reserve(data_offsets.len());

    for data_offset in data_offsets.iter().copied() {
        let data = match file.data_ref(data_offset) {
            Ok(data) => data,
            Err(err) if recoverable_entry_data_error(&err) => continue,
            Err(err) => return Err(err.into()),
        };
        let payload = if data.is_compressed() {
            decompressed.clear();
            data.decompress(decompressed)?;
            decompressed.as_slice()
        } else {
            data.raw_payload()
        };

        payloads.push(payload.to_vec());
        if let Some(eq) = payload.iter().position(|byte| *byte == b'=') {
            let raw_name = &payload[..eq];
            let value = payload[eq + 1..].to_vec();
            if let Ok(name) = std::str::from_utf8(raw_name) {
                let name = name.to_string();
                fields.insert(name.clone(), value.clone());
                field_values.entry(name).or_default().push(value);
            }
        }
    }

    Ok(Entry {
        fields,
        field_values,
        payloads,
        seqnum,
        realtime,
        monotonic,
        boot_id,
        cursor: build_cursor(file, reader)?,
    })
}

fn visit_entry_payload_offsets<F>(
    file: &JournalFile<Mmap>,
    data_offsets: &[NonZeroU64],
    decompressed: &mut Vec<u8>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    let context = file.data_payload_read_context();
    for data_offset in data_offsets.iter().copied() {
        let mut visitor_result = Ok(());
        match file.visit_data_payload_at_with_context(
            context,
            data_offset,
            decompressed,
            |payload| {
                visitor_result = visitor(payload);
                Ok(())
            },
        ) {
            Ok(()) => {}
            Err(err) if recoverable_entry_data_error(&err) => continue,
            Err(err) => return Err(err.into()),
        }
        visitor_result?;
    }

    Ok(())
}

fn enumerate_file_fields_indexed(file: &JournalFile<Mmap>) -> Result<Vec<String>> {
    let mut fields = HashSet::new();

    for field in file.fields() {
        let field = field?;
        if let Ok(name) = std::str::from_utf8(field.payload.as_ref()) {
            fields.insert(name.to_string());
        }
    }

    let mut out: Vec<_> = fields.into_iter().collect();
    out.sort();
    Ok(out)
}

fn enumerate_file_fields_by_scan(reader: &mut FileReader) -> Result<Vec<String>> {
    let mut fields = HashSet::new();
    reader.seek_head();
    while reader.next()? {
        if let Ok(entry) = reader.get_entry() {
            fields.extend(entry.fields.into_keys());
        }
    }
    let mut out: Vec<_> = fields.into_iter().collect();
    out.sort();
    Ok(out)
}

fn visit_file_unique_values_indexed<F>(
    file: &JournalFile<Mmap>,
    field_name: &[u8],
    decompressed: &mut Vec<u8>,
    mut visitor: F,
) -> Result<()>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    for data in file.field_data_objects(field_name)? {
        let data = data?;
        let payload = if data.is_compressed() {
            decompressed.clear();
            let len = data.decompress(decompressed)?;
            &decompressed[..len]
        } else {
            data.raw_payload()
        };
        let Some(value) = payload
            .strip_prefix(field_name)
            .and_then(|rest| rest.strip_prefix(b"="))
        else {
            return Err(SdkError::VerificationError(
                "field DATA chain object does not match requested field".to_string(),
            ));
        };
        visitor(value)?;
    }

    Ok(())
}

fn collect_entry_metadata_and_data_offsets(
    file: &JournalFile<Mmap>,
    entry_offset: NonZeroU64,
    data_offsets: &mut Vec<NonZeroU64>,
) -> Result<(u64, u64, u64, [u8; 16])> {
    let entry = file.entry_ref(entry_offset)?;
    let metadata = (
        entry.header.seqnum,
        entry.header.realtime,
        entry.header.monotonic,
        entry.header.boot_id,
    );
    collect_offsets_from_entry_items(&entry.items, data_offsets);
    Ok(metadata)
}

fn collect_entry_data_offsets(
    file: &JournalFile<Mmap>,
    entry_offset: NonZeroU64,
    data_offsets: &mut Vec<NonZeroU64>,
) -> Result<()> {
    let entry = file.entry_ref(entry_offset)?;
    collect_offsets_from_entry_items(&entry.items, data_offsets);
    Ok(())
}

fn collect_offsets_from_entry_items(
    items: &EntryItemsType<&[u8]>,
    data_offsets: &mut Vec<NonZeroU64>,
) {
    data_offsets.clear();
    match items {
        EntryItemsType::Regular(items) => {
            data_offsets.reserve(items.len());
            data_offsets.extend(
                items
                    .iter()
                    .filter_map(|item| NonZeroU64::new(item.object_offset)),
            );
        }
        EntryItemsType::Compact(items) => {
            data_offsets.reserve(items.len());
            data_offsets.extend(
                items
                    .iter()
                    .filter_map(|item| NonZeroU64::new(item.object_offset as u64)),
            );
        }
    }
}

fn verify_journal_file_strict(file: &JournalFile<Mmap>) -> Result<()> {
    let mut entry_offsets = Vec::new();
    file.entry_offsets(&mut entry_offsets)
        .map_err(|err| SdkError::VerificationError(format!("entry array walk failed: {err}")))?;

    let mut decompressed = Vec::new();
    let mut last_monotonic = 0_u64;
    let mut last_boot_id = [0_u8; 16];
    let mut monotonic_set = false;
    for entry_offset in entry_offsets {
        let entry = file.entry_ref(entry_offset).map_err(|err| {
            SdkError::VerificationError(format!(
                "entry object at offset {entry_offset} failed: {err}"
            ))
        })?;
        if monotonic_set
            && entry.header.boot_id == last_boot_id
            && last_monotonic > entry.header.monotonic
        {
            return Err(SdkError::VerificationError(format!(
                "entry monotonic out of sync ({} > {})",
                last_monotonic, entry.header.monotonic
            )));
        }
        last_monotonic = entry.header.monotonic;
        last_boot_id = entry.header.boot_id;
        monotonic_set = true;
        drop(entry);

        verify_entry_at_strict(file, entry_offset, &mut decompressed)?;
    }

    Ok(())
}

fn verify_entry_at_strict(
    file: &JournalFile<Mmap>,
    entry_offset: NonZeroU64,
    decompressed: &mut Vec<u8>,
) -> Result<()> {
    file.entry_ref(entry_offset).map_err(|err| {
        SdkError::VerificationError(format!(
            "entry object at offset {entry_offset} failed: {err}"
        ))
    })?;

    let data_objects = file.entry_data_objects(entry_offset).map_err(|err| {
        SdkError::VerificationError(format!(
            "entry data list at offset {entry_offset} failed: {err}"
        ))
    })?;

    for data in data_objects {
        let data = data.map_err(|err| {
            SdkError::VerificationError(format!(
                "data object referenced by entry at offset {entry_offset} failed: {err}"
            ))
        })?;

        let flags = data.header.object_header.flags;
        let compression_flags = flags & 0x07;
        if flags & !0x07 != 0 || compression_flags.count_ones() > 1 {
            return Err(SdkError::VerificationError(format!(
                "data object referenced by entry at offset {entry_offset} has unsupported flags 0x{flags:02x}"
            )));
        }

        let payload = if data.is_compressed() {
            decompressed.clear();
            data.decompress(decompressed).map_err(|err| {
                SdkError::VerificationError(format!(
                    "compressed data object referenced by entry at offset {entry_offset} failed: {err}"
                ))
            })?;
            decompressed.as_slice()
        } else {
            data.raw_payload()
        };

        if !payload.contains(&b'=') {
            return Err(SdkError::VerificationError(format!(
                "data object referenced by entry at offset {entry_offset} is missing field separator"
            )));
        }
    }

    Ok(())
}

fn recoverable_entry_error(err: &JournalError) -> bool {
    matches!(
        err,
        JournalError::InvalidObjectSize(0) | JournalError::ObjectExceedsFileBounds
    )
}

fn recoverable_entry_data_error(err: &JournalError) -> bool {
    matches!(
        err,
        JournalError::InvalidOffset
            | JournalError::InvalidObjectSize(0)
            | JournalError::ObjectExceedsFileBounds
    )
}

fn is_journal_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".journal")
                || name.ends_with(".journal~")
                || name.ends_with(".journal.zst")
                || name.ends_with(".journal~.zst")
        })
}

fn collect_journal_files(path: &Path) -> Result<Vec<PathBuf>> {
    let entries: Vec<_> = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    let mut files = Vec::new();

    for entry in &entries {
        let file_path = entry.path();
        if file_path.is_file() && is_journal_file_name(&file_path) {
            files.push(file_path);
        }
    }

    for entry in &entries {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_journal_subdir_name(&name) {
            continue;
        }
        let child_path = entry.path();
        if !child_path.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&child_path) else {
            continue;
        };
        for child in children.flatten() {
            let file_path = child.path();
            if file_path.is_file() && is_journal_file_name(&file_path) {
                files.push(file_path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn is_journal_subdir_name(name: &str) -> bool {
    if name.contains('.') {
        return false;
    }
    id128_string_valid(name)
}

fn id128_string_valid(s: &str) -> bool {
    match s.len() {
        32 => s.bytes().all(|byte| byte.is_ascii_hexdigit()),
        36 => s.bytes().enumerate().all(|(idx, byte)| {
            if matches!(idx, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        }),
        _ => false,
    }
}

fn build_directory_boot_newest(files: &[FileReader]) -> HashMap<[u8; 16], DirectoryBootNewest> {
    let mut newest: HashMap<[u8; 16], DirectoryBootNewest> = HashMap::new();
    for reader in files {
        reader.inner.with_file(|file| {
            let header = file.journal_header_ref();
            if header.tail_entry_boot_id == [0; 16] {
                return;
            }
            let replace = match newest.get(&header.tail_entry_boot_id) {
                None => true,
                Some(current) => header.tail_entry_monotonic > current.monotonic,
            };
            if replace {
                newest.insert(
                    header.tail_entry_boot_id,
                    DirectoryBootNewest {
                        machine_id: header.machine_id,
                        monotonic: header.tail_entry_monotonic,
                        realtime: header.tail_entry_realtime,
                    },
                );
            }
        });
    }
    newest
}

fn directory_files_non_overlapping(files: &[FileReader]) -> bool {
    if files.is_empty() {
        return false;
    }

    for pair in files.windows(2) {
        let previous = pair[0].header();
        let next = pair[1].header();
        if previous.seqnum_id != next.seqnum_id
            || previous.tail_entry_seqnum == 0
            || next.head_entry_seqnum == 0
            || previous.tail_entry_seqnum >= next.head_entry_seqnum
            || previous.tail_entry_realtime == 0
            || next.head_entry_realtime == 0
            || previous.tail_entry_realtime >= next.head_entry_realtime
        {
            return false;
        }
    }

    true
}

fn cmp_u64(a: u64, b: u64) -> i8 {
    match a.cmp(&b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

fn is_zst_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".zst"))
}

fn decompress_zst_to_temp(path: &Path, prefix: &str) -> Result<PathBuf> {
    let source = File::open(path)?;
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(source)
        .map_err(|err| SdkError::DecompressionFailed(err.to_string()))?;
    let mut temp_file = tempfile::Builder::new()
        .prefix(prefix)
        .suffix(".journal")
        .tempfile()?;
    std::io::copy(&mut decoder, &mut temp_file)?;
    let (dest, temp_path) = temp_file.keep().map_err(|err| err.error)?;
    drop(dest);
    Ok(temp_path)
}

pub fn export_entry_bytes(entry: &Entry) -> Vec<u8> {
    let mut out = Vec::new();
    write_export_field(&mut out, "__CURSOR", entry.cursor.as_bytes());
    write_export_field(
        &mut out,
        "__REALTIME_TIMESTAMP",
        entry.realtime.to_string().as_bytes(),
    );
    write_export_field(
        &mut out,
        "__MONOTONIC_TIMESTAMP",
        entry.monotonic.to_string().as_bytes(),
    );
    write_export_field(&mut out, "_BOOT_ID", hex::encode(entry.boot_id).as_bytes());

    let mut keys: Vec<_> = entry.field_values.keys().collect();
    keys.sort();
    for key in keys {
        if key == "_BOOT_ID" {
            continue;
        }
        if let Some(values) = entry.field_values.get(key) {
            for value in values {
                write_export_field(&mut out, key, value);
            }
        }
    }
    let mut byte_name_fields: Vec<_> = entry
        .raw_fields()
        .filter(|field| std::str::from_utf8(field.name).is_err() && field.name != b"_BOOT_ID")
        .collect();
    byte_name_fields.sort_by(|left, right| {
        left.name
            .cmp(right.name)
            .then_with(|| left.value.cmp(right.value))
    });
    for field in byte_name_fields {
        write_export_field_bytes(&mut out, field.name, field.value);
    }
    out.push(b'\n');
    out
}

pub fn export_entry(entry: &Entry) -> String {
    String::from_utf8_lossy(&export_entry_bytes(entry)).into_owned()
}

fn write_export_field(out: &mut Vec<u8>, name: &str, value: &[u8]) {
    write_export_field_bytes(out, name.as_bytes(), value);
}

fn write_export_field_bytes(out: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    if value
        .iter()
        .all(|byte| *byte == b'\t' || (0x20..0x7f).contains(byte))
    {
        out.extend_from_slice(name);
        out.push(b'=');
        out.extend_from_slice(value);
        out.push(b'\n');
    } else {
        out.extend_from_slice(name);
        out.push(b'\n');
        out.extend_from_slice(&(value.len() as u64).to_le_bytes());
        out.extend_from_slice(value);
        out.push(b'\n');
    }
}

pub fn json_entry(entry: &Entry) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "__CURSOR".to_string(),
        serde_json::Value::String(entry.cursor.clone()),
    );
    map.insert(
        "__REALTIME_TIMESTAMP".to_string(),
        serde_json::Value::String(entry.realtime.to_string()),
    );
    map.insert(
        "__MONOTONIC_TIMESTAMP".to_string(),
        serde_json::Value::String(entry.monotonic.to_string()),
    );
    map.insert(
        "_BOOT_ID".to_string(),
        serde_json::Value::String(hex::encode(entry.boot_id)),
    );

    let mut keys: Vec<_> = entry.field_values.keys().collect();
    keys.sort();
    for key in keys {
        if key == "_BOOT_ID" {
            continue;
        }
        let values = &entry.field_values[key];
        let json_values: Vec<_> = values
            .iter()
            .map(|value| json_value_for_bytes(value))
            .collect();
        let value = if json_values.len() == 1 {
            json_values.into_iter().next().unwrap()
        } else {
            serde_json::Value::Array(json_values)
        };
        map.insert(key.clone(), value);
    }

    serde_json::Value::Object(map)
}

fn json_value_for_bytes(value: &[u8]) -> serde_json::Value {
    if json_bytes_printable(value) {
        serde_json::Value::String(String::from_utf8_lossy(value).into_owned())
    } else {
        serde_json::Value::Array(
            value
                .iter()
                .map(|byte| serde_json::Value::Number((*byte).into()))
                .collect(),
        )
    }
}

fn json_bytes_printable(value: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(value) else {
        return false;
    };
    for ch in text.chars() {
        let cp = ch as u32;
        if cp < 0x20 && ch != '\t' && ch != '\n' {
            return false;
        }
        if (0x7f..=0x9f).contains(&cp) {
            return false;
        }
    }
    true
}

pub fn format_entry_text(entry: &Entry) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(message) = entry.get("MESSAGE") {
        out.extend_from_slice(message);
    }
    out.push(b'\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use journal_core::file::{JournalFileOptions, JournalWriter, MmapMut};
    use journal_core::repository::File as RepoFile;
    use journal_core::seal::SealOptions;
    use serde_json::Value;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    struct TempPath(PathBuf);

    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn parse_match_bytes_accepts_binary_values() {
        let data = b"MESSAGE=\xff\x00binary";
        assert_eq!(parse_match_bytes(data).unwrap(), data);
    }

    #[test]
    fn parse_match_bytes_rejects_invalid_field_names() {
        assert!(parse_match_bytes(b"lower=value").is_err());
        assert!(parse_match_bytes(b"1FIELD=value").is_err());
        assert!(parse_match_bytes(b"=value").is_err());
    }

    #[test]
    fn json_entry_includes_monotonic_timestamp_and_preserves_utf8() {
        let mut fields = HashMap::new();
        fields.insert("MESSAGE".to_string(), "héllo".as_bytes().to_vec());

        let mut field_values = HashMap::new();
        field_values.insert("MESSAGE".to_string(), vec!["héllo".as_bytes().to_vec()]);
        field_values.insert("BINARY".to_string(), vec![vec![0xff, 0x00]]);
        field_values.insert("CONTROL".to_string(), vec![b"abc\x07def".to_vec()]);

        let entry = Entry {
            fields,
            field_values,
            payloads: Vec::new(),
            seqnum: 7,
            realtime: 100,
            monotonic: 42,
            boot_id: [1; 16],
            cursor: "s=1;j=1;c=64;n=7".to_string(),
        };

        let Value::Object(json) = json_entry(&entry) else {
            panic!("entry JSON should be an object");
        };

        assert_eq!(
            json.get("__MONOTONIC_TIMESTAMP"),
            Some(&Value::String("42".to_string()))
        );
        assert_eq!(
            json.get("MESSAGE"),
            Some(&Value::String("héllo".to_string()))
        );
        assert_eq!(
            json.get("BINARY"),
            Some(&Value::Array(vec![Value::from(255), Value::from(0)]))
        );
        assert_eq!(
            json.get("CONTROL"),
            Some(&Value::Array(vec![
                Value::from(97),
                Value::from(98),
                Value::from(99),
                Value::from(7),
                Value::from(100),
                Value::from(101),
                Value::from(102),
            ]))
        );
    }

    #[test]
    fn no_rtc_fixtures_drain_without_tail_object_errors() {
        let fixture_dir = repo_root().join("fixtures/systemd/test-data/no-rtc");
        let mut total_entries = 0usize;
        for entry in std::fs::read_dir(&fixture_dir).expect("fixture directory exists") {
            let path = entry.expect("fixture directory entry").path();
            if !is_journal_file_name(&path) {
                continue;
            }
            let mut reader = FileReader::open(&path).expect("open journal fixture");
            let mut file_entries = 0usize;
            while reader.next().expect("fixture drains cleanly") {
                reader.get_entry().expect("entry is readable");
                file_entries += 1;
            }
            assert!(
                file_entries > 0,
                "expected at least one readable entry in {}",
                path.display()
            );
            total_entries += file_entries;
        }
        assert_eq!(total_entries, 10757);
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("repo root")
    }

    fn test_uuid(n: u8) -> uuid::Uuid {
        let mut bytes = [0u8; 16];
        bytes[15] = n;
        uuid::Uuid::from_bytes(bytes)
    }

    fn test_seal_opts() -> SealOptions {
        SealOptions::new([0u8; 12], 1_000_000, 1_000_000)
    }

    fn create_facade_test_writer(path: &Path) -> (JournalFile<MmapMut>, JournalWriter) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create journal parent");
        }
        let repo_file = RepoFile::from_path(path)
            .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
        let mut journal_file = JournalFile::<MmapMut>::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let writer = JournalWriter::new(&mut journal_file, 1, test_uuid(4)).expect("create writer");
        (journal_file, writer)
    }

    fn create_facade_compressed_test_writer(path: &Path) -> (JournalFile<MmapMut>, JournalWriter) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create journal parent");
        }
        let repo_file = RepoFile::from_path(path)
            .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
        let mut journal_file = JournalFile::<MmapMut>::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                .with_compression(Compression::Zstd)
                .with_compress_threshold(8),
        )
        .expect("create compressed journal");
        let writer = JournalWriter::new_with_compression(
            &mut journal_file,
            1,
            test_uuid(4),
            Compression::Zstd,
            8,
        )
        .expect("create compressed writer");
        (journal_file, writer)
    }

    fn write_facade_test_journal(path: &Path) {
        let (mut journal_file, mut writer) = create_facade_test_writer(path);
        writer
            .add_entry(
                &mut journal_file,
                &[
                    b"MESSAGE=first".as_slice(),
                    b"REPEAT=one".as_slice(),
                    b"REPEAT=two".as_slice(),
                    b"BIN=\x00\xff".as_slice(),
                ],
                1000,
                11,
            )
            .expect("write first entry");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=second".as_slice(), b"REPEAT=three".as_slice()],
                1001,
                12,
            )
            .expect("write second entry");
        journal_file.sync().expect("sync journal");
    }

    fn write_single_entry_journal(
        path: &Path,
        seqnum: u64,
        realtime: u64,
        monotonic: u64,
        payload: &[u8],
    ) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create journal parent");
        }
        let repo_file = RepoFile::from_path(path)
            .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
        let mut journal_file = JournalFile::<MmapMut>::create(
            &repo_file,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)),
        )
        .expect("create journal");
        let mut writer =
            JournalWriter::new(&mut journal_file, seqnum, test_uuid(4)).expect("create writer");
        writer
            .add_entry(&mut journal_file, &[payload], realtime, monotonic)
            .expect("write entry");
        journal_file.sync().expect("sync journal");
    }

    fn write_facade_single_message_journal(path: &Path, message: &[u8], realtime: u64) {
        let (mut journal_file, mut writer) = create_facade_test_writer(path);
        let payload = [b"MESSAGE=".as_slice(), message].concat();
        writer
            .add_entry(&mut journal_file, &[payload.as_slice()], realtime, 21)
            .expect("write single message");
        journal_file.sync().expect("sync journal");
    }

    fn journalctl_verify_fails_if_available(path: &Path, expected_text: &str) {
        let available = std::process::Command::new("journalctl")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if !available {
            return;
        }

        let output = std::process::Command::new("journalctl")
            .arg("--verify")
            .arg("--file")
            .arg(path)
            .output()
            .expect("run journalctl --verify");
        assert!(
            !output.status.success(),
            "journalctl --verify unexpectedly passed for {}",
            path.display()
        );
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .to_lowercase();
        assert!(
            combined.contains(&expected_text.to_lowercase()),
            "journalctl --verify output missing {expected_text:?}: {combined}"
        );
    }

    #[test]
    fn raw_writer_backward_monotonic_pass_through_fails_verification() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/raw-backward-monotonic.journal");
        let (mut journal_file, mut writer) = create_facade_test_writer(&path);
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=raw monotonic first".as_slice()],
                1_700_003_000_000_000,
                10,
            )
            .expect("write first entry");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=raw monotonic second".as_slice()],
                1_700_003_000_000_001,
                5,
            )
            .expect("write second entry");
        journal_file.sync().expect("sync journal");

        let err = verify_file(&path)
            .expect_err("expected same-boot backward monotonic timestamps to fail verification");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("monotonic"),
            "expected monotonic verification failure, got: {err}"
        );
        journalctl_verify_fails_if_available(&path, "timestamp out of synchronization");
    }

    #[test]
    fn raw_writer_explicit_zero_monotonic_pass_through() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/raw-zero-monotonic.journal");
        let (mut journal_file, mut writer) = create_facade_test_writer(&path);
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=raw zero monotonic".as_slice()],
                1_700_003_000_100_000,
                0,
            )
            .expect("write entry");
        journal_file.sync().expect("sync journal");
        verify_file(&path).expect("zero monotonic first entry should verify");

        let mut journal =
            SdJournalOpenFiles(&[path.to_str().expect("utf8 path")], 0).expect("open files");
        assert_eq!(SdJournalNext(&mut journal).expect("next"), 1);
        let (monotonic, _boot_id) = SdJournalGetMonotonicUsec(&mut journal).expect("monotonic");
        assert_eq!(monotonic, 0);
    }

    #[test]
    fn snapshot_reader_handles_final_partial_mmap_window() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/system.journal");
        write_facade_test_journal(&path);

        let options = ReaderOptions::snapshot().with_window_size(32 * 1024 * 1024);
        let mut reader =
            FileReader::open_with_options(&path, options).expect("open snapshot reader");
        assert!(reader.next().expect("first entry"));

        let mut payloads = Vec::new();
        reader
            .visit_entry_payloads(|payload| {
                payloads.push(payload.to_vec());
                Ok(())
            })
            .expect("visit current entry payloads");
        assert!(payloads.iter().any(|payload| payload == b"MESSAGE=first"));
        assert!(payloads.iter().any(|payload| payload == b"BIN=\x00\xff"));
    }

    #[test]
    fn default_reader_options_use_production_window_size() {
        let options = ReaderOptions::default();
        assert_eq!(options.bounds, ReaderBounds::Live);
        assert_eq!(options.mmap_strategy, ExperimentalMmapStrategy::Windowed);
        assert_eq!(options.window_size, DEFAULT_READER_WINDOW_SIZE);
        assert_eq!(options.window_size, 32 * 1024 * 1024);
    }

    #[test]
    fn directory_reader_uses_sequential_path_for_non_overlapping_files() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let first_path = dir.path().join("journals/first.journal");
        let second_path = dir.path().join("journals/second.journal");

        write_single_entry_journal(&first_path, 1, 1_700_004_000_000_000, 10, b"MESSAGE=first");
        write_single_entry_journal(
            &second_path,
            2,
            1_700_004_000_000_001,
            20,
            b"MESSAGE=second",
        );

        let mut reader =
            DirectoryReader::open_files([&first_path, &second_path]).expect("open files");
        assert!(
            reader.non_overlapping,
            "test files should qualify for sequential directory reads"
        );

        reader.seek_head();
        assert!(reader.next().expect("first entry"));
        assert_eq!(
            reader.get_realtime_usec().expect("first realtime"),
            1_700_004_000_000_000
        );
        assert!(reader.next().expect("second entry"));
        assert_eq!(
            reader.get_realtime_usec().expect("second realtime"),
            1_700_004_000_000_001
        );
        assert!(!reader.next().expect("end"));

        reader.seek_tail();
        assert!(reader.previous().expect("tail entry"));
        assert_eq!(
            reader.get_realtime_usec().expect("tail realtime"),
            1_700_004_000_000_001
        );
        assert!(reader.previous().expect("previous entry"));
        assert_eq!(
            reader.get_realtime_usec().expect("previous realtime"),
            1_700_004_000_000_000
        );
        assert!(!reader.previous().expect("start"));
    }

    #[test]
    fn facade_uncompressed_data_uses_mmap_payload_for_whole_file_reader() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/system.journal");
        write_facade_test_journal(&path);

        let mut reader = FileReader::open_with_options(
            &path,
            ReaderOptions::snapshot().with_mmap_strategy(ExperimentalMmapStrategy::WholeFile),
        )
        .expect("open reader");
        assert!(reader.next().expect("first entry"));
        reader.entry_data_restart().expect("restart data");
        let first_offset = reader.data_offsets[0];
        let (returned_ptr, returned_len, returned_payload) = {
            let payload = reader
                .enumerate_entry_payload()
                .expect("enumerate data")
                .expect("first payload");
            (payload.as_ptr(), payload.len(), payload.to_vec())
        };
        reader.clear_entry_data_state();

        let (mmap_ptr, mmap_len, mmap_payload) = reader.inner.with(|fields| {
            let data = fields.file.data_ref(first_offset).expect("data ref");
            (
                data.raw_payload().as_ptr(),
                data.raw_payload().len(),
                data.raw_payload().to_vec(),
            )
        });

        assert_eq!(returned_payload, mmap_payload);
        assert_eq!(returned_len, mmap_len);
        assert_eq!(returned_ptr, mmap_ptr);
    }

    #[test]
    fn facade_uncompressed_windowed_data_remains_valid_for_current_row() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/system.journal");
        write_facade_test_journal(&path);

        let mut reader = FileReader::open(&path).expect("open reader");
        assert!(reader.next().expect("first entry"));
        reader.entry_data_restart().expect("restart data");
        let first_offset = reader.data_offsets[0];

        let (first_ptr, first_len) = {
            let payload = reader
                .enumerate_entry_payload()
                .expect("enumerate first data")
                .expect("first payload");
            assert_eq!(payload, b"MESSAGE=first");
            (payload.as_ptr(), payload.len())
        };

        let second = reader
            .enumerate_entry_payload()
            .expect("enumerate second data")
            .expect("second payload")
            .to_vec();
        assert_eq!(second, b"REPEAT=one");
        while reader
            .enumerate_entry_payload()
            .expect("enumerate rest")
            .is_some()
        {}

        // SAFETY: This test intentionally checks that the previously returned
        // row payload remains valid until the reader advances to another row.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let first_after_end = unsafe { std::slice::from_raw_parts(first_ptr, first_len) };
        assert_eq!(first_after_end, b"MESSAGE=first");

        let mmap_ptr = reader.inner.with(|fields| {
            let data = fields.file.data_ref(first_offset).expect("data ref");
            data.raw_payload().as_ptr()
        });
        assert_ne!(
            first_ptr, mmap_ptr,
            "windowed mmap facade payloads must use row-owned storage"
        );
    }

    #[test]
    fn facade_compressed_data_payloads_remain_valid_for_current_row() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/system.journal");
        let (mut journal_file, mut writer) = create_facade_compressed_test_writer(&path);
        let first_payload = format!("FIRST={}", "a".repeat(2048));
        let second_payload = format!("SECOND={}", "b".repeat(2047));
        assert_eq!(first_payload.len(), second_payload.len());
        writer
            .add_entry(
                &mut journal_file,
                &[first_payload.as_bytes(), second_payload.as_bytes()],
                1000,
                11,
            )
            .expect("write compressed entry");
        journal_file.sync().expect("sync compressed journal");

        let mut reader = FileReader::open(&path).expect("open reader");
        assert!(reader.next().expect("first entry"));
        reader.entry_data_restart().expect("restart data");

        let (first_ptr, first_len) = {
            let payload = reader
                .enumerate_entry_payload()
                .expect("enumerate first data")
                .expect("first payload");
            assert_eq!(payload, first_payload.as_bytes());
            (payload.as_ptr(), payload.len())
        };

        let second = reader
            .enumerate_entry_payload()
            .expect("enumerate second data")
            .expect("second payload")
            .to_vec();
        assert_eq!(second, second_payload.as_bytes());
        assert!(
            reader
                .enumerate_entry_payload()
                .expect("enumerate end")
                .is_none()
        );

        // SAFETY: This test intentionally checks that compressed row payload
        // storage remains valid while enumerating the current row.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let first_after_second = unsafe { std::slice::from_raw_parts(first_ptr, first_len) };
        assert_eq!(first_after_second, first_payload.as_bytes());
    }

    #[test]
    fn facade_whole_file_row_handles_mixed_compressed_and_uncompressed_payloads() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/system.journal");
        let (mut journal_file, mut writer) = create_facade_compressed_test_writer(&path);
        let small_payload = b"SMALL=x".to_vec();
        let large_payload = format!("LARGE={}", "mixed ".repeat(256)).into_bytes();
        writer
            .add_entry(
                &mut journal_file,
                &[small_payload.as_slice(), large_payload.as_slice()],
                1000,
                11,
            )
            .expect("write mixed compressed entry");
        journal_file.sync().expect("sync mixed compressed journal");

        let mut reader = FileReader::open_with_options(
            &path,
            ReaderOptions::snapshot().with_mmap_strategy(ExperimentalMmapStrategy::WholeFile),
        )
        .expect("open whole-file reader");
        assert!(reader.next().expect("first entry"));
        reader.entry_data_restart().expect("restart data");
        let small_offset = reader.data_offsets[0];

        let (small_ptr, small_len) = {
            let payload = reader
                .enumerate_entry_payload()
                .expect("enumerate small data")
                .expect("small payload");
            assert_eq!(payload, small_payload.as_slice());
            (payload.as_ptr(), payload.len())
        };
        let (large_ptr, large_len) = {
            let payload = reader
                .enumerate_entry_payload()
                .expect("enumerate large data")
                .expect("large payload");
            assert_eq!(payload, large_payload.as_slice());
            (payload.as_ptr(), payload.len())
        };
        assert!(
            reader
                .enumerate_entry_payload()
                .expect("enumerate end")
                .is_none()
        );

        // SAFETY: This test intentionally checks that both current-row payload
        // pointers remain valid after the row enumeration reaches EOF.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let small_after_end = unsafe { std::slice::from_raw_parts(small_ptr, small_len) };
        // SAFETY: Same current-row lifetime check as `small_after_end`.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let large_after_end = unsafe { std::slice::from_raw_parts(large_ptr, large_len) };
        assert_eq!(small_after_end, small_payload.as_slice());
        assert_eq!(large_after_end, large_payload.as_slice());

        let mmap_ptr = reader.inner.with(|fields| {
            let data = fields.file.data_ref(small_offset).expect("data ref");
            data.raw_payload().as_ptr()
        });
        assert_eq!(
            small_ptr, mmap_ptr,
            "small uncompressed payload should remain borrowed from whole-file mmap"
        );
    }

    #[test]
    fn file_reader_seek_clears_cached_entry_payload_offsets() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/system.journal");
        write_facade_test_journal(&path);

        let mut reader = FileReader::open(&path).expect("open reader");
        assert!(reader.next().expect("first entry"));
        assert!(
            reader
                .enumerate_entry_payload()
                .expect("enumerate first payload")
                .is_some()
        );

        reader.seek_tail();
        assert!(
            reader
                .enumerate_entry_payload()
                .expect("enumerate after seek")
                .is_none()
        );
    }

    #[test]
    fn file_reader_query_unique_uses_field_index() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/indexed-unique.journal");
        let (mut journal_file, mut writer) = create_facade_test_writer(&path);
        for (index, priority) in [b"0", b"3", b"6", b"7"].into_iter().enumerate() {
            let payload = [b"PRIORITY=".as_slice(), priority.as_slice()].concat();
            writer
                .add_entry(
                    &mut journal_file,
                    &[b"MESSAGE=irrelevant".as_slice(), payload.as_slice()],
                    2_000 + index as u64,
                    20 + index as u64,
                )
                .expect("write entry");
        }
        journal_file.sync().expect("sync journal");

        let mut reader = FileReader::open(&path).expect("open reader");
        let fields = reader.enumerate_fields().expect("enumerate fields");
        assert!(fields.iter().any(|field| field == "MESSAGE"));
        assert!(fields.iter().any(|field| field == "PRIORITY"));

        let values = reader.query_unique("PRIORITY").expect("query unique");
        let got: HashSet<Vec<u8>> = values.into_iter().collect();
        let want: HashSet<Vec<u8>> = [b"0", b"3", b"6", b"7"]
            .into_iter()
            .map(|value| value.to_vec())
            .collect();
        assert_eq!(got, want);

        let mut visited = Vec::new();
        reader
            .visit_unique_values("PRIORITY", |value| {
                visited.push(value.to_vec());
                Ok(())
            })
            .expect("visit unique");
        let got: HashSet<Vec<u8>> = visited.into_iter().collect();
        assert_eq!(got, want);
    }

    #[test]
    fn directory_reader_query_unique_deduplicates_indexed_values_across_files() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let first_path = dir.path().join("journals/unique-first.journal");
        let second_path = dir.path().join("journals/unique-second.journal");
        let (mut first_file, mut first_writer) = create_facade_test_writer(&first_path);
        let (mut second_file, mut second_writer) = create_facade_test_writer(&second_path);

        first_writer
            .add_entry(
                &mut first_file,
                &[b"MESSAGE=first".as_slice(), b"PRIORITY=6".as_slice()],
                2_100,
                21,
            )
            .expect("write first");
        second_writer
            .add_entry(
                &mut second_file,
                &[b"MESSAGE=second".as_slice(), b"PRIORITY=6".as_slice()],
                2_200,
                22,
            )
            .expect("write second");
        second_writer
            .add_entry(
                &mut second_file,
                &[b"MESSAGE=third".as_slice(), b"PRIORITY=3".as_slice()],
                2_300,
                23,
            )
            .expect("write third");
        first_file.sync().expect("sync first");
        second_file.sync().expect("sync second");

        let mut reader =
            DirectoryReader::open_files([&first_path, &second_path]).expect("open files");
        let values = reader.query_unique("PRIORITY").expect("query unique");
        let got: HashSet<Vec<u8>> = values.into_iter().collect();
        let want: HashSet<Vec<u8>> = [b"3", b"6"].into_iter().map(|v| v.to_vec()).collect();
        assert_eq!(got, want);

        let mut visited = Vec::new();
        reader
            .visit_unique_values("PRIORITY", |value| {
                visited.push(value.to_vec());
                Ok(())
            })
            .expect("visit unique");
        let got: HashSet<Vec<u8>> = visited.into_iter().collect();
        assert_eq!(got, want);
    }

    #[test]
    fn jf_facade_stateful_reader_operations() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/system.journal");
        write_facade_test_journal(&path);

        let mut journal =
            SdJournalOpenFiles(&[path.to_str().expect("utf8 path")], 0).expect("open files");
        assert_stateful_facade_current_entry(&mut journal);
        assert_stateful_facade_data_enumeration(&mut journal);
        assert_stateful_facade_unique_and_field_enumeration(&mut journal);
        assert_stateful_facade_cursor_navigation(&mut journal);
        assert_stateful_facade_multi_file_navigation(&dir, &path);
        assert_stateful_facade_match_cache_invalidation(&dir, &path);
    }

    fn assert_stateful_facade_current_entry(journal: &mut SdJournal) {
        assert_eq!(SdJournalNext(journal).expect("next"), 1);
        let (seqnum, seqnum_id) = SdJournalGetSeqnum(journal).expect("seqnum");
        assert_eq!(seqnum, 1);
        assert_ne!(seqnum_id, [0; 16]);
        let (monotonic, boot_id) = SdJournalGetMonotonicUsec(journal).expect("monotonic");
        assert_eq!(monotonic, 11);
        assert_ne!(boot_id, [0; 16]);

        SdJournalRestartData(journal).expect("restart data for interleaved calls");
        let first_payload = SdJournalEnumerateAvailableData(journal)
            .expect("enumerate first data")
            .expect("first data exists");
        assert!(!first_payload.is_empty());
        assert_eq!(
            SdJournalGetRealtimeUsec(journal).expect("interleaved realtime"),
            1000
        );
        assert!(
            !SdJournalGetCursor(journal)
                .expect("interleaved cursor")
                .is_empty()
        );
        assert_eq!(
            SdJournalGetData(journal, "REPEAT").expect("interleaved get data"),
            b"REPEAT=one"
        );
        assert_eq!(
            SdJournalGetEntry(journal)
                .expect("interleaved get entry")
                .get_str("MESSAGE"),
            Some("first")
        );
    }

    fn assert_stateful_facade_data_enumeration(journal: &mut SdJournal) {
        SdJournalRestartData(journal).expect("restart data");
        let mut payloads = Vec::new();
        while let Some(payload) = SdJournalEnumerateAvailableData(journal).expect("enumerate data")
        {
            payloads.push(payload.to_vec());
        }
        assert!(payloads.iter().any(|payload| payload == b"REPEAT=one"));
        assert!(payloads.iter().any(|payload| payload == b"REPEAT=two"));
        assert!(payloads.iter().any(|payload| payload == b"BIN=\x00\xff"));
        SdJournalRestartData(journal).expect("restart data again");
        let mut restarted_payloads = Vec::new();
        while let Some(payload) =
            SdJournalEnumerateAvailableData(journal).expect("enumerate restarted data")
        {
            restarted_payloads.push(payload.to_vec());
        }
        assert_eq!(payloads, restarted_payloads);
        assert_eq!(
            SdJournalGetData(journal, "REPEAT").expect("get data"),
            b"REPEAT=one"
        );
    }

    fn assert_stateful_facade_unique_and_field_enumeration(journal: &mut SdJournal) {
        let direct_unique = SdJournalQueryUnique(journal, "BIN").expect("query unique");
        assert_eq!(direct_unique.len(), 1);
        assert_eq!(direct_unique[0].0, "BIN");
        assert_eq!(direct_unique[0].1, b"\x00\xff");

        SdJournalQueryUniqueState(journal, "REPEAT").expect("query unique state");
        let mut unique = Vec::new();
        while let Some(payload) =
            SdJournalEnumerateAvailableUnique(journal).expect("enumerate unique")
        {
            unique.push(payload);
        }
        assert!(unique.iter().any(|payload| payload == b"REPEAT=one"));
        assert!(unique.iter().any(|payload| payload == b"REPEAT=two"));
        assert!(unique.iter().any(|payload| payload == b"REPEAT=three"));

        SdJournalRestartFields(journal).expect("restart fields");
        let mut fields = HashSet::new();
        while let Some(field) = SdJournalEnumerateField(journal).expect("enumerate field") {
            fields.insert(field);
        }
        assert!(fields.contains("MESSAGE"));
        assert!(fields.contains("REPEAT"));
        assert!(fields.contains("BIN"));
    }

    fn assert_stateful_facade_cursor_navigation(journal: &mut SdJournal) {
        SdJournalSeekRealtimeUsec(journal, 1001).expect("seek realtime forward");
        assert_eq!(SdJournalNext(journal).expect("next after realtime"), 1);
        let entry = SdJournalGetEntry(journal).expect("entry after realtime");
        assert_eq!(entry.get_str("MESSAGE"), Some("second"));

        SdJournalSeekRealtimeUsec(journal, 1001).expect("seek realtime backward");
        assert_eq!(
            SdJournalPrevious(journal).expect("previous after realtime"),
            1
        );
        let entry = SdJournalGetEntry(journal).expect("entry after reverse realtime");
        assert_eq!(entry.get_str("MESSAGE"), Some("second"));

        let cursor = SdJournalGetCursor(journal).expect("cursor");
        assert!(SdJournalTestCursor(journal, &cursor).expect("test current cursor"));
        assert!(!SdJournalTestCursor(journal, "invalid-cursor").expect("test invalid cursor"));
        assert!(matches!(
            SdJournalSeekCursor(journal, "invalid-cursor"),
            Err(FacadeError::InvalidCursor)
        ));
        SdJournalSeekRealtimeUsec(journal, 1000).expect("seek first by realtime");
        assert_eq!(SdJournalNext(journal).expect("next to first"), 1);
        let entry = SdJournalGetEntry(journal).expect("first entry");
        assert_eq!(entry.get_str("MESSAGE"), Some("first"));
        let first_cursor = SdJournalGetCursor(journal).expect("first cursor");
        let first_realtime =
            SdJournalGetRealtimeUsec(journal).expect("first realtime after cursor seek");
        SdJournalSeekCursor(journal, &cursor).expect("seek cursor back to second");
        let entry = SdJournalGetEntry(journal).expect("entry after cursor seek");
        assert_eq!(entry.get_str("MESSAGE"), Some("second"));
        let (cursor_prefix, _) = first_cursor.rsplit_once("n=").expect("cursor has seqnum");
        let missing_cursor = format!("{cursor_prefix}n=999999");
        SdJournalSeekCursor(journal, &missing_cursor)
            .expect("valid missing cursor is accepted as a seek location");
        assert!(
            !SdJournalTestCursor(journal, &first_cursor)
                .expect("missing cursor seek does not stay on original cursor")
        );
        assert!(
            SdJournalGetRealtimeUsec(journal).expect("realtime after missing cursor seek")
                >= first_realtime
        );
        let entry = SdJournalGetEntry(journal).expect("entry after missing cursor seek");
        assert_eq!(entry.get_str("MESSAGE"), Some("second"));
    }

    fn assert_stateful_facade_multi_file_navigation(dir: &tempfile::TempDir, path: &Path) {
        let path2 = facade_second_journal_path(dir);
        let mut multi = SdJournalOpenFiles(
            &[
                path2.to_str().expect("utf8 second path"),
                path.to_str().expect("utf8 first path"),
            ],
            0,
        )
        .expect("open multiple files");

        let mut messages = Vec::new();
        while SdJournalNext(&mut multi).expect("multi next") == 1 {
            let entry = SdJournalGetEntry(&mut multi).expect("multi entry");
            messages.push(entry.get_str("MESSAGE").unwrap_or("").to_string());
        }
        // systemd compares same-source seqnums before realtime when interleaving files.
        assert_eq!(messages, vec!["first", "third", "second"]);

        let mut cursor_multi = SdJournalOpenFiles(
            &[
                path2.to_str().expect("utf8 second path"),
                path.to_str().expect("utf8 first path"),
            ],
            0,
        )
        .expect("open cursor multiple files");
        assert_eq!(
            SdJournalNext(&mut cursor_multi).expect("cursor multi first"),
            1
        );
        let multi_first_cursor =
            SdJournalGetCursor(&cursor_multi).expect("cursor multi first cursor");
        let multi_first_realtime =
            SdJournalGetRealtimeUsec(&cursor_multi).expect("cursor multi first realtime");
        assert_eq!(
            SdJournalNext(&mut cursor_multi).expect("cursor multi second"),
            1
        );
        let multi_second_cursor =
            SdJournalGetCursor(&cursor_multi).expect("cursor multi second cursor");
        let entry = SdJournalGetEntry(&mut cursor_multi).expect("cursor multi second entry");
        assert_eq!(entry.get_str("MESSAGE"), Some("third"));
        SdJournalSeekCursor(&mut cursor_multi, &multi_second_cursor)
            .expect("directory cursor seek to found entry");
        assert!(
            SdJournalTestCursor(&cursor_multi, &multi_second_cursor)
                .expect("directory cursor seek landed on found cursor")
        );
        let entry =
            SdJournalGetEntry(&mut cursor_multi).expect("directory entry after cursor seek");
        assert_eq!(entry.get_str("MESSAGE"), Some("third"));
        let (cursor_prefix, _) = multi_first_cursor
            .rsplit_once("n=")
            .expect("multi cursor has seqnum");
        let missing_cursor = format!("{cursor_prefix}n=999999");
        SdJournalSeekCursor(&mut cursor_multi, &missing_cursor)
            .expect("directory valid missing cursor is accepted as a seek location");
        assert!(
            !SdJournalTestCursor(&cursor_multi, &multi_first_cursor)
                .expect("directory missing seek does not stay on original cursor")
        );
        assert!(
            SdJournalGetRealtimeUsec(&cursor_multi)
                .expect("directory realtime after missing cursor seek")
                >= multi_first_realtime
        );

        SdJournalSeekRealtimeUsec(&mut multi, 1002).expect("multi seek realtime backward");
        assert_eq!(SdJournalPrevious(&mut multi).expect("multi previous"), 1);
        let entry = SdJournalGetEntry(&mut multi).expect("multi entry after seek");
        assert_eq!(entry.get_str("MESSAGE"), Some("second"));

        SdJournalSeekRealtimeUsec(&mut multi, 999).expect("multi seek before range");
        assert_eq!(
            SdJournalPrevious(&mut multi).expect("multi previous before range"),
            0
        );
    }

    fn assert_stateful_facade_match_cache_invalidation(dir: &tempfile::TempDir, path: &Path) {
        let path2 = facade_second_journal_path(dir);
        let mut filtered_multi = SdJournalOpenFiles(
            &[
                path2.to_str().expect("utf8 second path"),
                path.to_str().expect("utf8 first path"),
            ],
            0,
        )
        .expect("open filtered multiple files");
        assert_eq!(
            SdJournalNext(&mut filtered_multi).expect("filtered first"),
            1
        );
        let entry = SdJournalGetEntry(&mut filtered_multi).expect("filtered first entry");
        assert_eq!(entry.get_str("MESSAGE"), Some("first"));
        // The first unfiltered step caches candidates from other files; match
        // mutation must discard those cached candidates before continuing.
        SdJournalAddMatch(&mut filtered_multi, b"MESSAGE=second").expect("filtered add match");
        assert_eq!(
            SdJournalNext(&mut filtered_multi).expect("filtered next"),
            1
        );
        let entry = SdJournalGetEntry(&mut filtered_multi).expect("filtered entry");
        assert_eq!(entry.get_str("MESSAGE"), Some("second"));
        assert_eq!(SdJournalNext(&mut filtered_multi).expect("filtered end"), 0);
    }

    fn facade_second_journal_path(dir: &tempfile::TempDir) -> PathBuf {
        let path = dir.path().join("journals/user.journal");
        if !path.exists() {
            write_facade_single_message_journal(&path, b"third", 1002);
        }
        path
    }

    #[test]
    fn jf_facade_data_enumeration_handles_compressed_payloads() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/system.journal");
        let (mut journal_file, mut writer) = create_facade_compressed_test_writer(&path);
        let compressed_payload = format!("MESSAGE={}", "compressed ".repeat(128));
        writer
            .add_entry(
                &mut journal_file,
                &[compressed_payload.as_bytes()],
                1000,
                11,
            )
            .expect("write compressed entry");
        journal_file.sync().expect("sync compressed journal");

        let mut journal =
            SdJournalOpenFiles(&[path.to_str().expect("utf8 path")], 0).expect("open files");
        assert_eq!(SdJournalNext(&mut journal).expect("next"), 1);
        SdJournalRestartData(&mut journal).expect("restart data");
        let mut payloads = Vec::new();
        while let Some(payload) =
            SdJournalEnumerateAvailableData(&mut journal).expect("enumerate data")
        {
            payloads.push(payload.to_vec());
        }

        assert_eq!(payloads, vec![compressed_payload.into_bytes()]);
    }

    #[test]
    fn reader_preserves_raw_byte_field_names() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("journals/raw-byte-names.journal");
        let invalid_utf8_name = invalid_utf8_raw_name();
        let nul_name = nul_raw_name();
        write_raw_byte_name_journal(&path, &invalid_utf8_name, nul_name);

        let mut reader = FileReader::open(&path).expect("open raw byte-name journal");
        assert!(reader.next().expect("read first entry"));
        let entry = reader.get_entry().expect("get raw byte-name entry");
        assert_raw_byte_name_accessors(&entry, &invalid_utf8_name, nul_name);
        assert_raw_byte_name_payload(&entry, &invalid_utf8_name);
        let lossy_name = String::from_utf8_lossy(&invalid_utf8_name).into_owned();
        assert_lossy_raw_name_is_not_invented(&entry, &lossy_name);
        assert_export_preserves_raw_byte_name(&entry, &invalid_utf8_name);
        assert_json_omits_lossy_raw_name(&entry, &lossy_name);
    }

    fn write_raw_byte_name_journal(path: &Path, invalid_utf8_name: &[u8], nul_name: &[u8]) {
        let (mut journal_file, mut writer) = create_facade_test_writer(path);
        let binary_value = [0x61_u8, 0, 0x3d, 0x62];
        writer
            .add_entry_fields_with_options(
                &mut journal_file,
                [
                    journal_core::file::EntryField::structured(b"MESSAGE", b"raw byte names"),
                    journal_core::file::EntryField::structured(invalid_utf8_name, b"invalid utf8"),
                    journal_core::file::EntryField::structured(nul_name, b"nul name"),
                    journal_core::file::EntryField::structured(b"field name", b"space"),
                    journal_core::file::EntryField::structured(b"BINARY", &binary_value),
                ],
                1_700_004_000_000_000,
                1,
                journal_core::file::EntryWriteOptions::default()
                    .field_name_policy(journal_core::file::FieldNamePolicy::Raw),
            )
            .expect("write raw byte-name entry");
        journal_file.sync().expect("sync raw byte-name journal");
    }

    fn invalid_utf8_raw_name() -> Vec<u8> {
        vec![0xff, 0x52, 0x41, 0x57]
    }

    fn nul_raw_name() -> &'static [u8] {
        &[0x52, 0x41, 0x57, 0, 0x4e, 0x41, 0x4d, 0x45]
    }

    fn binary_raw_value() -> &'static [u8] {
        &[0x61, 0, 0x3d, 0x62]
    }

    fn assert_raw_byte_name_accessors(entry: &Entry, invalid_utf8_name: &[u8], nul_name: &[u8]) {
        assert_eq!(entry.get("MESSAGE"), Some(b"raw byte names".as_slice()));
        assert_eq!(
            entry.get_raw(invalid_utf8_name),
            Some(b"invalid utf8".as_slice())
        );
        assert_eq!(entry.get_raw(nul_name), Some(b"nul name".as_slice()));
        assert_eq!(entry.get_raw(b"BINARY"), Some(binary_raw_value()));
        assert_eq!(
            entry.get_raw_values(b"field name"),
            vec![b"space".as_slice()]
        );
    }

    fn assert_raw_byte_name_payload(entry: &Entry, invalid_utf8_name: &[u8]) {
        assert!(
            entry
                .raw_fields()
                .any(|field| { field.name == invalid_utf8_name && field.value == b"invalid utf8" })
        );
        assert!(entry.payloads.iter().any(|payload| {
            let mut expected = invalid_utf8_name.to_vec();
            expected.push(b'=');
            expected.extend_from_slice(b"invalid utf8");
            payload == &expected
        }));
    }

    fn assert_lossy_raw_name_is_not_invented(entry: &Entry, lossy_name: &str) {
        assert!(
            !entry.fields.contains_key(lossy_name),
            "string convenience map must not invent lossy RAW field names"
        );
    }

    fn assert_export_preserves_raw_byte_name(entry: &Entry, invalid_utf8_name: &[u8]) {
        let export = export_entry_bytes(&entry);
        let mut expected_export = invalid_utf8_name.to_vec();
        expected_export.push(b'=');
        expected_export.extend_from_slice(b"invalid utf8\n");
        assert!(
            export
                .windows(expected_export.len())
                .any(|window| window == expected_export.as_slice()),
            "export output should preserve non-UTF8 RAW field names as bytes"
        );
    }

    fn assert_json_omits_lossy_raw_name(entry: &Entry, lossy_name: &str) {
        let serde_json::Value::Object(json) = json_entry(&entry) else {
            panic!("entry JSON should be an object");
        };
        assert!(
            !json.contains_key(lossy_name),
            "JSON output must not invent lossy RAW field names"
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

    fn write_sealed_verify_file(path: &Path) -> SealOptions {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create journal parent");
        }
        let repo_file = RepoFile::from_path(path)
            .unwrap_or_else(|| panic!("test journal path should parse: {}", path.display()));
        let seal = test_seal_opts();
        let mut journal_file = JournalFile::<MmapMut>::create(
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
                &[b"MESSAGE=sealed-covered".as_slice()],
                1_500_000,
                100,
            )
            .expect("write covered entry");
        writer
            .add_entry(
                &mut journal_file,
                &[b"MESSAGE=later-entry".as_slice()],
                2_500_000,
                200,
            )
            .expect("write later entry");
        journal_file.sync().expect("sync journal");
        seal
    }

    fn tamper_data_payload(path: &Path, payload: &[u8]) {
        let mut data = std::fs::read(path).expect("read journal bytes");
        let header_size = u64::from_le_bytes(data[88..96].try_into().unwrap());
        let tail_object_offset = u64::from_le_bytes(data[136..144].try_into().unwrap());
        let incompatible_flags = u32::from_le_bytes(data[12..16].try_into().unwrap());
        let payload_offset = if incompatible_flags & INCOMPATIBLE_COMPACT != 0 {
            COMPACT_DATA_OBJECT_HEADER_SIZE
        } else {
            DATA_OBJECT_HEADER_SIZE
        };

        let mut offset = header_size;
        let mut tag_count = 0usize;
        let mut second_tag_offset = 0u64;
        let mut target_payload_offset = 0u64;
        let mut target_object_offset = 0u64;

        loop {
            assert!(
                offset + OBJECT_HEADER_SIZE <= data.len() as u64,
                "object header at {offset} exceeds file"
            );
            let typ = data[offset as usize];
            let size = u64::from_le_bytes(
                data[(offset as usize + 8)..(offset as usize + 16)]
                    .try_into()
                    .unwrap(),
            );
            assert!(
                size >= OBJECT_HEADER_SIZE,
                "invalid object size {size} at {offset}"
            );
            let aligned_size = align8(size);
            assert!(
                offset + aligned_size <= data.len() as u64,
                "object at {offset} exceeds file"
            );

            if typ == OBJECT_TYPE_TAG {
                tag_count += 1;
                if tag_count == 2 {
                    second_tag_offset = offset;
                }
            } else if typ == OBJECT_TYPE_DATA && size > payload_offset {
                let start = (offset + payload_offset) as usize;
                let end = (offset + size) as usize;
                if &data[start..end] == payload {
                    target_payload_offset = start as u64;
                    target_object_offset = offset;
                }
            }

            if offset == tail_object_offset {
                break;
            }
            offset += aligned_size;
        }

        assert_ne!(target_payload_offset, 0, "payload not found");
        assert_ne!(second_tag_offset, 0, "second TAG not found");
        assert!(
            target_object_offset < second_tag_offset,
            "DATA object {target_object_offset} is not covered by second TAG {second_tag_offset}"
        );
        data[target_payload_offset as usize] ^= 0x01;
        std::fs::write(path, data).expect("write tampered journal bytes");
    }

    #[test]
    fn verify_file_detects_corruption() {
        let path =
            repo_root().join("fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst");
        let err =
            verify_file(&path).expect_err("expected verification error for truncated zstd frame");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("corrupt"),
            "expected error to contain 'corrupt', got: {msg}"
        );
    }

    #[test]
    fn verify_file_passes_on_valid_fixture() {
        let path = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        verify_file(&path).expect("expected verification to pass for valid fixture");
    }

    #[test]
    fn verify_file_with_key_validates_sealed_hmacs() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir
            .path()
            .join("00000000-0000-0000-0000-000000000001/system.journal");
        let seal = write_sealed_verify_file(&path);
        let key = verification_key(&seal);

        verify_file_with_key(&path, &key).expect("sealed verification should pass");
        let zst_path = dir.path().join("sealed.journal.zst");
        let source = std::fs::read(&path).expect("read sealed journal");
        let compressed = ruzstd::encoding::compress_to_vec(
            source.as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        std::fs::write(&zst_path, compressed).expect("write compressed sealed journal");
        verify_file_with_key(&zst_path, &key).expect("compressed sealed verification should pass");

        verify_file_with_key(&path, "000000000000000000000001/1-f4240")
            .expect_err("wrong key should fail");

        tamper_data_payload(&path, b"MESSAGE=sealed-covered");
        verify_file_with_key(&path, &key).expect_err("authenticated DATA tamper should fail");
    }

    #[test]
    fn verify_file_with_key_rejects_aligned_size_overflow() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir
            .path()
            .join("00000000-0000-0000-0000-000000000001/system.journal");
        let seal = write_sealed_verify_file(&path);
        let key = verification_key(&seal);

        let mut data = std::fs::read(&path).expect("read sealed journal");
        let header_size = u64::from_le_bytes(data[88..96].try_into().unwrap()) as usize;
        data[header_size + 8..header_size + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        std::fs::write(&path, data).expect("write malformed journal");

        let err = verify_file_with_key(&path, &key)
            .expect_err("aligned-size overflow should fail verification");
        let msg = err.to_string();
        assert!(
            msg.contains("overflows alignment"),
            "expected alignment overflow error, got: {msg}"
        );
    }

    #[test]
    fn verify_file_with_key_rejects_short_sealed_header_without_panic() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir
            .path()
            .join("00000000-0000-0000-0000-000000000001/system.journal");
        std::fs::create_dir_all(path.parent().expect("journal parent")).expect("create parent");

        let mut data = vec![0u8; HEADER_MIN_SIZE as usize];
        data[0..8].copy_from_slice(b"LPKSHHRH");
        data[8..12].copy_from_slice(&1u32.to_le_bytes());
        data[16] = 1;
        data[88..96].copy_from_slice(&HEADER_MIN_SIZE.to_le_bytes());
        std::fs::write(&path, data).expect("write short sealed header");

        let err = verify_file_with_key(&path, "000000000000000000000000/1-f4240")
            .expect_err("short sealed header should not verify");
        let msg = err.to_string();
        assert!(
            msg.contains("open/decompression failed")
                || msg.contains("corrupt")
                || msg.contains("verification"),
            "expected controlled verification error, got: {msg}"
        );
    }

    #[test]
    fn verify_file_rejects_referenced_zero_sized_data_object() {
        let fixture = repo_root().join("fixtures/systemd/test-data/no-rtc/system.journal.zst");
        let path = decompress_zst_to_temp(&fixture, "rust-sdk-verify-corrupt")
            .expect("decompress fixture to temporary journal");
        let _cleanup = TempPath(path.clone());

        let data_offset = {
            let reader = FileReader::open(&path).expect("open decompressed journal");
            reader.inner.with_file(|file| {
                let mut entry_offsets = Vec::new();
                file.entry_offsets(&mut entry_offsets)
                    .expect("collect entry offsets");
                let mut data_offsets = Vec::new();
                file.entry_data_object_offsets(entry_offsets[0], &mut data_offsets)
                    .expect("collect data offsets");
                data_offsets[0]
            })
        };

        let mut bytes = std::fs::read(&path).expect("read journal bytes");
        let size_start = data_offset.get() as usize + 8;
        bytes[size_start..size_start + 8].copy_from_slice(&0u64.to_le_bytes());
        std::fs::write(&path, bytes).expect("write corrupted journal bytes");

        let err = verify_file(&path)
            .expect_err("strict verification should reject referenced zero-sized data object");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("corrupt"),
            "expected error to contain 'corrupt', got: {msg}"
        );
        assert!(
            msg.contains("data object") || msg.contains("object size"),
            "expected strict data-object or object-size error, got: {msg}"
        );
    }
}
