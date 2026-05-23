#![allow(non_snake_case)]

use crate::{
    BootInfo, DirectoryReader, Entry, FileReader, SdkError, export_entry_bytes, format_entry_text,
    json_entry,
};
use std::fmt;
use std::path::Path;

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
}

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

    Ok(SdJournal {
        reader,
        output_mode: OutputMode::Default,
    })
}

impl SdJournal {
    pub fn add_match(&mut self, data: &[u8]) {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.add_match(data),
            ReaderKind::Directory(reader) => reader.add_match(data),
        }
    }

    pub fn add_conjunction(&mut self) -> std::result::Result<(), Error> {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.add_conjunction(),
            ReaderKind::Directory(reader) => reader.add_conjunction(),
        }
        .map_err(map_error)
    }

    pub fn add_disjunction(&mut self) -> std::result::Result<(), Error> {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.add_disjunction(),
            ReaderKind::Directory(reader) => reader.add_disjunction(),
        }
        .map_err(map_error)
    }

    pub fn flush_matches(&mut self) {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.flush_matches(),
            ReaderKind::Directory(reader) => reader.flush_matches(),
        }
    }

    pub fn next(&mut self) -> std::result::Result<i32, Error> {
        let advanced = match &mut self.reader {
            ReaderKind::File(reader) => reader.next(),
            ReaderKind::Directory(reader) => reader.next(),
        }
        .map_err(map_error)?;
        Ok(i32::from(advanced))
    }

    pub fn previous(&mut self) -> std::result::Result<i32, Error> {
        let advanced = match &mut self.reader {
            ReaderKind::File(reader) => reader.previous(),
            ReaderKind::Directory(reader) => reader.previous(),
        }
        .map_err(map_error)?;
        Ok(i32::from(advanced))
    }

    pub fn seek_head(&mut self) {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.seek_head(),
            ReaderKind::Directory(reader) => reader.seek_head(),
        }
    }

    pub fn seek_tail(&mut self) {
        match &mut self.reader {
            ReaderKind::File(reader) => reader.seek_tail(),
            ReaderKind::Directory(reader) => reader.seek_tail(),
        }
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

    pub fn test_cursor(&self, cursor: &str) -> std::result::Result<bool, Error> {
        match &self.reader {
            ReaderKind::File(reader) => reader.test_cursor(cursor),
            ReaderKind::Directory(reader) => reader.test_cursor(cursor),
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

    pub fn query_unique(&mut self, field: &str) -> std::result::Result<Vec<Vec<u8>>, Error> {
        match &mut self.reader {
            ReaderKind::File(reader) => query_file_unique(reader, field),
            ReaderKind::Directory(reader) => reader.query_unique(field),
        }
        .map_err(map_error)
    }

    pub fn list_boots(&self) -> Vec<BootInfo> {
        match &self.reader {
            ReaderKind::File(reader) => {
                let header = reader.header();
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

pub fn SdJournalPrevious(j: &mut SdJournal) -> std::result::Result<i32, Error> {
    j.previous()
}

pub fn SdJournalSeekHead(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.seek_head();
    Ok(())
}

pub fn SdJournalSeekTail(j: &mut SdJournal) -> std::result::Result<(), Error> {
    j.seek_tail();
    Ok(())
}

pub fn SdJournalGetRealtimeUsec(j: &SdJournal) -> std::result::Result<u64, Error> {
    j.get_realtime_usec()
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

pub fn SdJournalEnumerateFields(j: &mut SdJournal) -> std::result::Result<Vec<String>, Error> {
    j.enumerate_fields()
}

pub fn SdJournalListBoots(j: &mut SdJournal) -> std::result::Result<Vec<BootInfo>, Error> {
    Ok(j.list_boots())
}

pub fn SdJournalQueryUnique(
    j: &mut SdJournal,
    field: &str,
) -> std::result::Result<Vec<Vec<u8>>, Error> {
    j.query_unique(field)
}

pub fn SdJournalSetOutputMode(j: &mut SdJournal, mode: OutputMode) {
    j.set_output_mode(mode);
}

pub fn SdJournalProcessOutput(j: &SdJournal, entry: &Entry) -> std::result::Result<Vec<u8>, Error> {
    j.process_output(entry)
}

fn enumerate_file_fields(reader: &mut FileReader) -> crate::Result<Vec<String>> {
    let mut fields = std::collections::HashSet::new();
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

fn query_file_unique(reader: &mut FileReader, field: &str) -> crate::Result<Vec<Vec<u8>>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    reader.seek_head();
    while reader.next()? {
        if let Ok(entry) = reader.get_entry() {
            if let Some(values) = entry.field_values.get(field) {
                for value in values {
                    if seen.insert(value.clone()) {
                        out.push(value.clone());
                    }
                }
            }
        }
    }
    Ok(out)
}

fn map_error(err: SdkError) -> Error {
    match err {
        SdkError::NoEntry => Error::NoEntry,
        SdkError::InvalidCursor(_) => Error::InvalidCursor,
        SdkError::Unsupported(_) => Error::Unsupported,
        SdkError::DecompressionFailed(msg) => Error::Other(msg),
        SdkError::InvalidPath(msg) => Error::Other(msg),
        SdkError::Journal(err) => Error::Other(err.to_string()),
    }
}
