#![allow(non_snake_case)]

use crate::{
    BootInfo, DirectoryReader, Entry, FileReader, ReaderOptions, SdkError, export_entry_bytes,
    format_entry_text, json_entry,
};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Default,
    Json,
    Export,
}

impl Default for OutputMode {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    Unsupported,
    NoEntry,
    InvalidCursor,
    EndOfEntries,
    CorruptFile,
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => write!(f, "operation not supported"),
            Self::NoEntry => write!(f, "no matching entry"),
            Self::InvalidCursor => write!(f, "invalid cursor"),
            Self::EndOfEntries => write!(f, "end of entries"),
            Self::CorruptFile => write!(f, "corrupt journal file"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}

pub const ERR_UNSUPPORTED: i32 = 1;
pub const ERR_NO_ENTRY: i32 = 2;
pub const ERR_INVALID_CURSOR: i32 = 3;
pub const ERR_END_OF_ENTRIES: i32 = 4;

enum ReaderKind {
    File(FileReader),
    Directory(DirectoryReader),
}

pub struct SdJournal {
    reader: ReaderKind,
    output_mode: OutputMode,
    field_items: Vec<String>,
    field_index: usize,
    unique_field: Option<String>,
}

pub type UniqueValue = (String, Vec<u8>);

pub fn SdJournalOpen(path: &str, flags: u32) -> std::result::Result<SdJournal, Error> {
    if flags != 0 {
        return Err(Error::Unsupported);
    }

    let path = Path::new(path);
    let reader = if path.is_dir() {
        ReaderKind::Directory(DirectoryReader::open(path).map_err(map_error)?)
    } else {
        ReaderKind::File(FileReader::open(path).map_err(map_error)?)
    };

    Ok(SdJournal::new(reader))
}

pub fn SdJournalOpenFile(path: &str, flags: u32) -> std::result::Result<SdJournal, Error> {
    SdJournalOpenFileWithOptions(path, flags, ReaderOptions::default())
}

pub fn SdJournalOpenFileWithOptions(
    path: &str,
    flags: u32,
    options: ReaderOptions,
) -> std::result::Result<SdJournal, Error> {
    if flags != 0 {
        return Err(Error::Unsupported);
    }
    Ok(SdJournal::new(ReaderKind::File(
        FileReader::open_with_options(path, options).map_err(map_error)?,
    )))
}

pub fn SdJournalOpenDirectory(path: &str, flags: u32) -> std::result::Result<SdJournal, Error> {
    SdJournalOpenDirectoryWithOptions(path, flags, ReaderOptions::default())
}

pub fn SdJournalOpenDirectoryWithOptions(
    path: &str,
    flags: u32,
    options: ReaderOptions,
) -> std::result::Result<SdJournal, Error> {
    if flags != 0 {
        return Err(Error::Unsupported);
    }
    Ok(SdJournal::new(ReaderKind::Directory(
        DirectoryReader::open_with_options(path, options).map_err(map_error)?,
    )))
}

pub fn SdJournalOpenFiles(paths: &[&str], flags: u32) -> std::result::Result<SdJournal, Error> {
    SdJournalOpenFilesWithOptions(paths, flags, ReaderOptions::default())
}

pub fn SdJournalOpenFilesWithOptions(
    paths: &[&str],
    flags: u32,
    options: ReaderOptions,
) -> std::result::Result<SdJournal, Error> {
    if flags != 0 {
        return Err(Error::Unsupported);
    }
    let paths = dedupe_open_file_paths(paths);
    if paths.len() == 1 {
        return SdJournalOpenFileWithOptions(&paths[0], flags, options);
    }
    Ok(SdJournal::new(ReaderKind::Directory(
        DirectoryReader::open_files_with_options(&paths, options).map_err(map_error)?,
    )))
}

fn dedupe_open_file_paths(paths: &[&str]) -> Vec<String> {
    let mut out = Vec::with_capacity(paths.len());
    let mut seen = HashSet::with_capacity(paths.len());
    for path in paths {
        let key = canonical_open_file_key(Path::new(path));
        if seen.insert(key) {
            out.push((*path).to_string());
        }
    }
    out
}

fn canonical_open_file_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn SdJournalClose(j: SdJournal) {
    drop(j);
}

impl SdJournal {
    fn new(reader: ReaderKind) -> Self {
        Self {
            reader,
            output_mode: OutputMode::Default,
            field_items: Vec::new(),
            field_index: 0,
            unique_field: None,
        }
    }

    fn reset_iterators(&mut self) {
        match &mut self.reader {
            ReaderKind::File(reader) => {
                reader.clear_entry_data_state();
                reader.clear_unique_state();
            }
            ReaderKind::Directory(reader) => {
                reader.clear_entry_data_state();
                reader.clear_unique_state();
            }
        }
        self.field_items.clear();
        self.field_index = 0;
        self.unique_field = None;
    }

    pub fn add_match(&mut self, data: &[u8]) {
        self.reset_iterators();
        match &mut self.reader {
            ReaderKind::File(reader) => reader.add_match(data),
            ReaderKind::Directory(reader) => reader.add_match(data),
        }
    }

    pub fn add_conjunction(&mut self) -> std::result::Result<(), Error> {
        self.reset_iterators();
        match &mut self.reader {
            ReaderKind::File(reader) => reader.add_conjunction(),
            ReaderKind::Directory(reader) => reader.add_conjunction(),
        }
        .map_err(map_error)
    }

    pub fn add_disjunction(&mut self) -> std::result::Result<(), Error> {
        self.reset_iterators();
        match &mut self.reader {
            ReaderKind::File(reader) => reader.add_disjunction(),
            ReaderKind::Directory(reader) => reader.add_disjunction(),
        }
        .map_err(map_error)
    }

    pub fn flush_matches(&mut self) {
        self.reset_iterators();
        match &mut self.reader {
            ReaderKind::File(reader) => reader.flush_matches(),
            ReaderKind::Directory(reader) => reader.flush_matches(),
        }
    }

    pub fn next(&mut self) -> std::result::Result<i32, Error> {
        self.reset_iterators();
        let advanced = match &mut self.reader {
            ReaderKind::File(reader) => reader.next(),
            ReaderKind::Directory(reader) => reader.next(),
        }
        .map_err(map_error)?;
        Ok(i32::from(advanced))
    }

    pub fn previous(&mut self) -> std::result::Result<i32, Error> {
        self.reset_iterators();
        let advanced = match &mut self.reader {
            ReaderKind::File(reader) => reader.previous(),
            ReaderKind::Directory(reader) => reader.previous(),
        }
        .map_err(map_error)?;
        Ok(i32::from(advanced))
    }

    pub fn seek_head(&mut self) {
        self.reset_iterators();
        match &mut self.reader {
            ReaderKind::File(reader) => reader.seek_head(),
            ReaderKind::Directory(reader) => reader.seek_head(),
        }
    }

    pub fn seek_tail(&mut self) {
        self.reset_iterators();
        match &mut self.reader {
            ReaderKind::File(reader) => reader.seek_tail(),
            ReaderKind::Directory(reader) => reader.seek_tail(),
        }
    }

    pub fn seek_realtime_usec(&mut self, usec: u64) {
        self.reset_iterators();
        match &mut self.reader {
            ReaderKind::File(reader) => reader.seek_realtime(usec),
            ReaderKind::Directory(reader) => reader.seek_realtime(usec),
        }
    }

    pub fn seek_cursor(&mut self, cursor: &str) -> std::result::Result<(), Error> {
        self.reset_iterators();
        match &mut self.reader {
            ReaderKind::File(reader) => reader.seek_cursor(cursor),
            ReaderKind::Directory(reader) => reader.seek_cursor(cursor),
        }
        .map_err(map_error)
    }

    pub fn get_entry(&mut self) -> std::result::Result<Entry, Error> {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.get_entry(),
            ReaderKind::Directory(reader) => reader.get_entry(),
        }
        .map_err(map_error)
    }

    pub fn get_realtime_usec(&self) -> std::result::Result<u64, Error> {
        match &self.reader {
            ReaderKind::File(reader) => reader.get_realtime_usec(),
            ReaderKind::Directory(reader) => reader.get_realtime_usec(),
        }
        .map_err(map_error)
    }

    pub fn get_cursor(&self) -> std::result::Result<String, Error> {
        match &self.reader {
            ReaderKind::File(reader) => reader.get_cursor(),
            ReaderKind::Directory(reader) => reader.get_cursor(),
        }
        .map_err(map_error)
    }

    pub fn get_seqnum(&self) -> std::result::Result<(u64, [u8; 16]), Error> {
        match &self.reader {
            ReaderKind::File(reader) => reader.get_seqnum(),
            ReaderKind::Directory(reader) => reader.get_seqnum(),
        }
        .map_err(map_error)
    }

    pub fn get_monotonic_usec(&self) -> std::result::Result<(u64, [u8; 16]), Error> {
        match &self.reader {
            ReaderKind::File(reader) => reader.get_monotonic_usec(),
            ReaderKind::Directory(reader) => reader.get_monotonic_usec(),
        }
        .map_err(map_error)
    }

    pub fn test_cursor(&self, cursor: &str) -> std::result::Result<bool, Error> {
        match &self.reader {
            ReaderKind::File(reader) => reader.test_cursor(cursor),
            ReaderKind::Directory(reader) => reader.test_cursor(cursor),
        }
        .map_err(map_error)
    }

    pub fn restart_data(&mut self) -> std::result::Result<(), Error> {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.entry_data_restart(),
            ReaderKind::Directory(reader) => reader.entry_data_restart(),
        }
        .map_err(map_error)
    }

    pub fn enumerate_available_data(&mut self) -> std::result::Result<Option<&[u8]>, Error> {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.enumerate_entry_payload(),
            ReaderKind::Directory(reader) => reader.enumerate_entry_payload(),
        }
        .map_err(map_error)
    }

    pub fn enumerate_fields(&mut self) -> std::result::Result<Vec<String>, Error> {
        match &mut self.reader {
            ReaderKind::File(reader) => enumerate_file_fields(reader),
            ReaderKind::Directory(reader) => reader.enumerate_fields(),
        }
        .map_err(map_error)
    }

    pub fn restart_fields(&mut self) -> std::result::Result<(), Error> {
        self.field_items = self.enumerate_fields()?;
        self.field_index = 0;
        Ok(())
    }

    pub fn enumerate_field(&mut self) -> std::result::Result<Option<String>, Error> {
        if self.field_index >= self.field_items.len() {
            return Ok(None);
        }
        let item = self.field_items[self.field_index].clone();
        self.field_index += 1;
        Ok(Some(item))
    }

    fn query_unique_values(&mut self, field: &str) -> std::result::Result<Vec<Vec<u8>>, Error> {
        let mut values = Vec::new();
        self.visit_unique_values(field, |value| {
            values.push(value.to_vec());
            Ok(())
        })?;
        Ok(values)
    }

    pub fn visit_unique_values<F>(
        &mut self,
        field: &str,
        visitor: F,
    ) -> std::result::Result<(), Error>
    where
        F: FnMut(&[u8]) -> std::result::Result<(), Error>,
    {
        let mut visitor = visitor;
        let mut visitor_error = None;
        let result = {
            let mut sdk_visitor = |value: &[u8]| match visitor(value) {
                Ok(()) => Ok(()),
                Err(err) => {
                    visitor_error = Some(err);
                    Err(crate::SdkError::VerificationError(
                        "unique value visitor failed".to_string(),
                    ))
                }
            };
            match &mut self.reader {
                ReaderKind::File(reader) => reader.visit_unique_values(field, &mut sdk_visitor),
                ReaderKind::Directory(reader) => {
                    reader.visit_unique_values(field, &mut sdk_visitor)
                }
            }
        };
        result.map_err(|err| visitor_error.take().unwrap_or_else(|| map_error(err)))
    }

    pub fn query_unique(&mut self, field: &str) -> std::result::Result<Vec<UniqueValue>, Error> {
        Ok(self
            .query_unique_values(field)?
            .into_iter()
            .map(|value| (field.to_string(), value))
            .collect())
    }

    pub fn query_unique_state(&mut self, field: &str) -> std::result::Result<(), Error> {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.query_unique_state(field),
            ReaderKind::Directory(reader) => reader.query_unique_state(field),
        }
        .map_err(map_error)?;
        self.unique_field = Some(field.to_string());
        Ok(())
    }

    pub fn restart_unique(&mut self) {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.restart_unique_state(),
            ReaderKind::Directory(reader) => reader.restart_unique_state(),
        }
    }

    pub fn enumerate_available_unique(&mut self) -> std::result::Result<Option<Vec<u8>>, Error> {
        let Some(field) = &self.unique_field else {
            return Ok(None);
        };
        match &mut self.reader {
            ReaderKind::File(reader) => reader.enumerate_unique_payload(field),
            ReaderKind::Directory(reader) => reader.enumerate_unique_payload(),
        }
        .map_err(map_error)
    }

    pub fn list_boots(&self) -> Vec<BootInfo> {
        match &self.reader {
            ReaderKind::File(reader) => {
                let header = reader.cached_header().header;
                vec![BootInfo {
                    index: 0,
                    boot_id: hex::encode(header.tail_entry_boot_id),
                    first_entry: header.head_entry_realtime as i64,
                    last_entry: header.tail_entry_realtime as i64,
                }]
            }
            ReaderKind::Directory(reader) => reader.list_boots(),
        }
    }

    pub fn set_output_mode(&mut self, mode: OutputMode) {
        self.output_mode = mode;
    }

    pub fn process_output(&self, entry: &Entry) -> std::result::Result<Vec<u8>, Error> {
        match self.output_mode {
            OutputMode::Default => Ok(format_entry_text(entry)),
            OutputMode::Export => Ok(export_entry_bytes(entry)),
            OutputMode::Json => {
                let mut out = serde_json::to_vec(&json_entry(entry))
                    .map_err(|err| Error::Other(err.to_string()))?;
                out.push(b'\n');
                Ok(out)
            }
        }
    }
}

pub fn SdJournalAddMatch(j: &mut SdJournal, data: &[u8]) -> std::result::Result<(), Error> {
    crate::parse_match_bytes(data).map_err(|_| Error::Other("EINVAL".to_string()))?;
    j.add_match(data);
    Ok(())
}

pub fn SdJournalAddDisjunction(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.add_disjunction()
}

pub fn SdJournalAddConjunction(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.add_conjunction()
}

pub fn SdJournalFlushMatches(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.flush_matches();
    Ok(())
}

pub fn SdJournalNext(j: &mut SdJournal) -> std::result::Result<i32, Error> {
    j.next()
}

pub fn SdJournalNextSkip(j: &mut SdJournal, skip: u64) -> std::result::Result<i32, Error> {
    let mut advanced = 0;
    for _ in 0..skip {
        if j.next()? == 0 {
            break;
        }
        advanced += 1;
    }
    Ok(advanced)
}

pub fn SdJournalPrevious(j: &mut SdJournal) -> std::result::Result<i32, Error> {
    j.previous()
}

pub fn SdJournalPreviousSkip(j: &mut SdJournal, skip: u64) -> std::result::Result<i32, Error> {
    let mut advanced = 0;
    for _ in 0..skip {
        if j.previous()? == 0 {
            break;
        }
        advanced += 1;
    }
    Ok(advanced)
}

pub fn SdJournalSeekHead(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.seek_head();
    Ok(())
}

pub fn SdJournalSeekTail(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.seek_tail();
    Ok(())
}

pub fn SdJournalSeekRealtimeUsec(j: &mut SdJournal, usec: u64) -> std::result::Result<(), Error> {
    j.seek_realtime_usec(usec);
    Ok(())
}

pub fn SdJournalSeekCursor(j: &mut SdJournal, cursor: &str) -> std::result::Result<(), Error> {
    j.seek_cursor(cursor)
}

pub fn SdJournalGetRealtimeUsec(j: &SdJournal) -> std::result::Result<u64, Error> {
    j.get_realtime_usec()
}

pub fn SdJournalGetSeqnum(j: &SdJournal) -> std::result::Result<(u64, [u8; 16]), Error> {
    j.get_seqnum()
}

pub fn SdJournalGetMonotonicUsec(j: &SdJournal) -> std::result::Result<(u64, [u8; 16]), Error> {
    j.get_monotonic_usec()
}

pub fn SdJournalGetCursor(j: &SdJournal) -> std::result::Result<String, Error> {
    j.get_cursor()
}

pub fn SdJournalTestCursor(j: &SdJournal, cursor: &str) -> std::result::Result<bool, Error> {
    j.test_cursor(cursor)
}

pub fn SdJournalGetEntry(j: &mut SdJournal) -> std::result::Result<Entry, Error> {
    j.get_entry()
}

pub fn SdJournalGetData(j: &mut SdJournal, field: &str) -> std::result::Result<Vec<u8>, Error> {
    let found = match &mut j.reader {
        ReaderKind::File(reader) => reader.get_entry_payload(field.as_bytes()),
        ReaderKind::Directory(reader) => reader.get_entry_payload(field.as_bytes()),
    }
    .map_err(map_error)?;
    found.ok_or(Error::NoEntry)
}

pub fn SdJournalRestartData(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.restart_data()
}

pub fn SdJournalEnumerateAvailableData(
    j: &mut SdJournal,
) -> std::result::Result<Option<&[u8]>, Error> {
    j.enumerate_available_data()
}

pub fn SdJournalEnumerateFields(j: &mut SdJournal) -> std::result::Result<Vec<String>, Error> {
    j.enumerate_fields()
}

pub fn SdJournalRestartFields(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.restart_fields()
}

pub fn SdJournalEnumerateField(j: &mut SdJournal) -> std::result::Result<Option<String>, Error> {
    j.enumerate_field()
}

pub fn SdJournalListBoots(j: &mut SdJournal) -> std::result::Result<Vec<BootInfo>, Error> {
    Ok(j.list_boots())
}

pub fn SdJournalQueryUnique(
    j: &mut SdJournal,
    field: &str,
) -> std::result::Result<Vec<UniqueValue>, Error> {
    j.query_unique(field)
}

pub fn SdJournalVisitUniqueValues<F>(
    j: &mut SdJournal,
    field: &str,
    visitor: F,
) -> std::result::Result<(), Error>
where
    F: FnMut(&[u8]) -> std::result::Result<(), Error>,
{
    j.visit_unique_values(field, visitor)
}

pub fn SdJournalQueryUniqueState(j: &mut SdJournal, field: &str) -> std::result::Result<(), Error> {
    j.query_unique_state(field)
}

pub fn SdJournalRestartUnique(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.restart_unique();
    Ok(())
}

pub fn SdJournalEnumerateAvailableUnique(
    j: &mut SdJournal,
) -> std::result::Result<Option<Vec<u8>>, Error> {
    j.enumerate_available_unique()
}

pub fn SdJournalSetOutputMode(j: &mut SdJournal, mode: OutputMode) {
    j.set_output_mode(mode);
}

pub fn SdJournalProcessOutput(j: &SdJournal, entry: &Entry) -> std::result::Result<Vec<u8>, Error> {
    j.process_output(entry)
}

fn enumerate_file_fields(reader: &mut FileReader) -> crate::Result<Vec<String>> {
    reader.enumerate_fields()
}

fn map_error(err: SdkError) -> Error {
    match err {
        SdkError::NoEntry => Error::NoEntry,
        SdkError::InvalidCursor(_) => Error::InvalidCursor,
        SdkError::Unsupported(_) => Error::Unsupported,
        SdkError::DecompressionFailed(msg) => Error::Other(msg),
        SdkError::InvalidPath(msg) => Error::Other(msg),
        SdkError::Journal(err) => Error::Other(err.to_string()),
        SdkError::VerificationError(msg) => {
            Error::Other(format!("journal verification failed: corrupt file: {msg}"))
        }
    }
}
