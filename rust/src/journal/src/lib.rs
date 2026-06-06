//! Pure-Rust systemd journal reader and writer SDK.
//!
//! This crate provides a public Rust layer over the imported Netdata journal
//! reader/writer crates. It intentionally keeps the low-level file parsing in
//! the imported implementation and adds byte-safe entries, directory reading,
//! export/JSON formatting, and a libsystemd-style facade.

mod directory;
mod explorer;
mod export;
mod facade;
pub mod netdata;
mod parse;
mod reader_helpers;
mod sealed_verify;
mod verify_graph;

pub use directory::DirectoryReader;
pub use explorer::{
    ExplorerAnchor, ExplorerComparison, ExplorerControl, ExplorerFieldMode, ExplorerFilter,
    ExplorerFtsPattern, ExplorerHistogram, ExplorerHistogramBucket, ExplorerProgress,
    ExplorerQuery, ExplorerResult, ExplorerRow, ExplorerSampling, ExplorerStats,
    ExplorerStopReason, ExplorerStrategy,
};
pub use export::{export_entry, export_entry_bytes, format_entry_text, json_entry};
pub use parse::{ParseError, ParsedCursor, parse_cursor, parse_match_bytes, parse_match_string};
pub use sealed_verify::{verify_file, verify_file_with_key};

use ouroboros::self_referencing;
use std::collections::HashMap;
use std::fmt;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use directory::DirectoryEntryKey;
#[cfg(test)]
use directory::is_journal_file_name;
use reader_helpers::*;
#[cfg(test)]
use sealed_verify::{
    COMPACT_DATA_OBJECT_HEADER_SIZE, DATA_OBJECT_HEADER_SIZE, HEADER_MIN_SIZE,
    INCOMPATIBLE_COMPACT, OBJECT_HEADER_SIZE, OBJECT_TYPE_DATA, OBJECT_TYPE_TAG, align8,
};

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
pub use journal_core::file::{
    BucketUtilization, Compression, Direction, EntryItemsType, ExperimentalMmapStrategy,
    FieldNamePolicy, HashableObject, JournalFile, JournalReader, Location, Mmap,
    WindowManagerStats,
};
use journal_core::file::{CurrentRowMetadata, CurrentRowView};
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

#[derive(Debug, Clone, Copy)]
pub struct FileHeader {
    pub signature: [u8; 8],
    pub compatible_flags: u32,
    pub incompatible_flags: u32,
    pub state: u8,
    pub header_size: u64,
    pub n_entries: u64,
    pub head_entry_realtime: u64,
    pub tail_entry_realtime: u64,
    pub head_entry_seqnum: u64,
    pub tail_entry_seqnum: u64,
    pub tail_entry_boot_id: [u8; 16],
    pub seqnum_id: [u8; 16],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FileHeaderSnapshot {
    pub(crate) header: FileHeader,
    pub(crate) machine_id: [u8; 16],
    pub(crate) tail_entry_monotonic: u64,
}

impl FileHeaderSnapshot {
    fn from_file(file: &JournalFile<Mmap>) -> Self {
        let header = file.journal_header_ref();
        Self {
            header: FileHeader {
                signature: header.signature,
                compatible_flags: header.compatible_flags,
                incompatible_flags: header.incompatible_flags,
                state: header.state,
                header_size: header.header_size,
                n_entries: header.n_entries,
                head_entry_realtime: header.head_entry_realtime,
                tail_entry_realtime: header.tail_entry_realtime,
                head_entry_seqnum: header.head_entry_seqnum,
                tail_entry_seqnum: header.tail_entry_seqnum,
                tail_entry_boot_id: header.tail_entry_boot_id,
                seqnum_id: header.seqnum_id,
            },
            machine_id: header.machine_id,
            tail_entry_monotonic: header.tail_entry_monotonic,
        }
    }
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
    row: CurrentRowView,
    header_snapshot: FileHeaderSnapshot,
    bounds: ReaderBounds,
}

fn key_from_metadata(metadata: CurrentRowMetadata) -> DirectoryEntryKey {
    DirectoryEntryKey {
        seqnum_id: metadata.seqnum_id,
        seqnum: metadata.seqnum,
        boot_id: metadata.boot_id,
        monotonic: metadata.monotonic,
        realtime: metadata.realtime,
        xor_hash: metadata.xor_hash,
    }
}

enum StepStatus {
    Valid,
    Skip,
    End,
}

impl Drop for FileReader {
    fn drop(&mut self) {
        self.inner
            .with_file(|file| self.row.clear_current_best_effort(file));
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
        let header_snapshot = FileHeaderSnapshot::from_file(&file);
        Ok(Self {
            inner: ReaderCellBuilder {
                file,
                reader_builder: |_file| JournalReader::default(),
            }
            .build(),
            temp_path: None,
            row: CurrentRowView::default(),
            header_snapshot,
            bounds: options.bounds,
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
        let header_snapshot = FileHeaderSnapshot::from_file(&file);
        Ok(Self {
            inner: ReaderCellBuilder {
                file,
                reader_builder: |_file| JournalReader::default(),
            }
            .build(),
            temp_path: Some(temp_path),
            row: CurrentRowView::default(),
            header_snapshot,
            bounds: options.bounds,
        })
    }

    pub fn header(&self) -> FileHeader {
        if self.bounds == ReaderBounds::Snapshot {
            return self.header_snapshot.header;
        }
        self.live_header()
    }

    pub(crate) fn cached_header(&self) -> FileHeaderSnapshot {
        self.header_snapshot
    }

    fn live_header(&self) -> FileHeader {
        self.inner
            .with_file(|file| FileHeaderSnapshot::from_file(file).header)
    }

    pub fn bucket_utilization(&self) -> Option<BucketUtilization> {
        self.inner.with_file(JournalFile::bucket_utilization)
    }

    #[doc(hidden)]
    pub fn mmap_stats(&self) -> Result<WindowManagerStats> {
        self.inner
            .with_file(|file| file.mmap_stats())
            .map_err(Into::into)
    }

    pub fn seek_head(&mut self) {
        self.inner
            .with_file(|file| self.row.clear_current_best_effort(file));
        self.inner.with_reader_mut(|reader| {
            reader.set_location(Location::Head);
        });
    }

    pub fn seek_tail(&mut self) {
        self.inner
            .with_file(|file| self.row.clear_current_best_effort(file));
        self.inner.with_reader_mut(|reader| {
            reader.set_location(Location::Tail);
        });
    }

    pub fn seek_realtime(&mut self, usec: u64) {
        self.inner
            .with_file(|file| self.row.clear_current_best_effort(file));
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
        self.inner
            .with_file(|file| self.row.clear_current(file))
            .map_err(SdkError::from)?;
        loop {
            let row = &mut self.row;
            let status = self.inner.with_mut(|fields| {
                if !fields.reader.step(fields.file, direction)? {
                    return Ok(StepStatus::End);
                }

                match fields
                    .reader
                    .get_entry_offset()
                    .and_then(|offset| row.load_entry(fields.file, offset))
                {
                    Ok(_) => Ok(StepStatus::Valid),
                    Err(err) if recoverable_entry_error(&err) => Ok(StepStatus::Skip),
                    Err(err) => Err(err),
                }
            })?;

            match status {
                StepStatus::Valid => {
                    return Ok(true);
                }
                StepStatus::Skip => continue,
                StepStatus::End => {
                    self.inner
                        .with_file(|file| self.row.clear_current(file))
                        .map_err(SdkError::from)?;
                    return Ok(false);
                }
            }
        }
    }

    pub fn get_entry(&mut self) -> Result<Entry> {
        self.invalidate_entry_data_state();
        let inner = &mut self.inner;
        let row = &mut self.row;
        inner.with_mut(|fields| {
            if row.entry_offset().is_none() {
                let offset = fields.reader.get_entry_offset()?;
                row.load_entry(fields.file, offset)?;
            }
            read_current_row_entry(fields.file, row)
        })
    }

    pub fn visit_entry_payloads<F>(&mut self, mut visitor: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        self.invalidate_entry_data_state();
        let inner = &mut self.inner;
        let row = &mut self.row;
        inner.with_mut(|fields| {
            fields.reader.release_object_guards();
            if row.entry_offset().is_none() {
                let offset = fields.reader.get_entry_offset()?;
                row.load_entry(fields.file, offset)?;
            }
            row.restart_data()?;
            loop {
                let payload = match row.read_next_payload(fields.file) {
                    Ok(Some(payload)) => payload,
                    Ok(None) => break,
                    Err(err) if recoverable_entry_data_error(&err) => continue,
                    Err(err) => {
                        let _ = row.reset_data_state(fields.file);
                        return Err(err.into());
                    }
                };
                let payload = row.payload_slice(payload);
                if let Err(err) = visitor(payload) {
                    let _ = row.reset_data_state(fields.file);
                    return Err(err);
                }
            }
            row.reset_data_state(fields.file)?;
            Ok(())
        })
    }

    pub fn clear_entry_data_state(&mut self) {
        self.inner
            .with_file(|file| self.row.reset_data_state_best_effort(file));
        self.inner
            .with_reader_mut(|reader| reader.entry_data_restart());
    }

    fn invalidate_entry_data_state(&mut self) {
        if self.row.data_state_active() {
            self.clear_entry_data_state();
        }
    }

    pub fn entry_data_restart(&mut self) -> Result<()> {
        self.inner
            .with_file(|file| self.row.clear_pins(file))
            .map_err(SdkError::from)?;
        self.inner
            .with_reader_mut(|reader| reader.entry_data_restart());
        if self.row.entry_offset().is_none() {
            let row = &mut self.row;
            self.inner.with_mut(|fields| {
                let offset = fields.reader.get_entry_offset()?;
                row.load_entry(fields.file, offset).map(|_| ())
            })?;
        }
        self.row.restart_data().map_err(Into::into)
    }

    pub fn enumerate_entry_payload(&mut self) -> Result<Option<&[u8]>> {
        let row = &mut self.row;
        let payload = self.inner.with_mut(|fields| {
            fields.reader.release_object_guards();
            row.read_next_payload(fields.file)
        })?;
        Ok(payload.map(|payload| self.row.payload_slice(payload)))
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
        if let Some(metadata) = self.row.metadata() {
            return Ok(metadata.realtime);
        }
        self.inner
            .with(|fields| fields.reader.get_realtime_usec(fields.file))
            .map_err(Into::into)
    }

    pub fn get_seqnum(&self) -> Result<(u64, [u8; 16])> {
        let key = self.current_directory_entry_key()?;
        Ok((key.seqnum, key.seqnum_id))
    }

    pub fn get_monotonic_usec(&self) -> Result<(u64, [u8; 16])> {
        let key = self.current_directory_entry_key()?;
        Ok((key.monotonic, key.boot_id))
    }

    pub fn get_cursor(&self) -> Result<String> {
        if let Some(metadata) = self.row.metadata() {
            return Ok(format_cursor_from_key(key_from_metadata(metadata)));
        }
        let seqnum_id = self.header_snapshot.header.seqnum_id;
        self.inner
            .with(|fields| build_cursor(fields.file, fields.reader, seqnum_id))
    }

    fn current_directory_entry_key(&self) -> Result<DirectoryEntryKey> {
        if let Some(metadata) = self.row.metadata() {
            return Ok(key_from_metadata(metadata));
        }
        self.inner.with(|fields| {
            let offset = fields.reader.get_entry_offset()?;
            let entry = fields.file.entry_ref(offset)?;
            Ok(DirectoryEntryKey {
                seqnum_id: self.header_snapshot.header.seqnum_id,
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

impl FileReader {
    fn header_realtime_start(&self) -> u64 {
        self.header_snapshot.header.head_entry_realtime
    }

    pub fn enumerate_fields(&mut self) -> Result<Vec<String>> {
        self.invalidate_entry_data_state();
        match self.enumerate_fields_indexed() {
            Ok(fields) => Ok(fields),
            Err(_) => enumerate_file_fields_by_scan(self),
        }
    }

    pub(crate) fn enumerate_fields_indexed(&mut self) -> Result<Vec<String>> {
        self.invalidate_entry_data_state();
        self.inner.with_file(enumerate_file_fields_indexed)
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
        let decompressed = self.row.decompressed_mut();
        self.inner.with_file(|file| {
            visit_file_unique_values_indexed(file, field_name.as_bytes(), decompressed, visitor)
        })
    }
}

#[cfg(test)]
mod tests;
