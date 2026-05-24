//! Pure-Rust systemd journal reader and writer SDK.
//!
//! This crate provides a public Rust layer over the imported Netdata journal
//! reader/writer crates. It intentionally keeps the low-level file parsing in
//! the imported implementation and adds byte-safe entries, directory reading,
//! export/JSON formatting, and a libsystemd-style facade.

mod facade;

use ouroboros::self_referencing;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

pub use facade::{
    ERR_END_OF_ENTRIES, ERR_INVALID_CURSOR, ERR_NO_ENTRY, ERR_UNSUPPORTED, Error as FacadeError,
    OutputMode, SdJournal, SdJournalAddConjunction, SdJournalAddDisjunction, SdJournalAddMatch,
    SdJournalEnumerateFields, SdJournalFlushMatches, SdJournalGetCursor, SdJournalGetEntry,
    SdJournalGetRealtimeUsec, SdJournalListBoots, SdJournalNext, SdJournalOpen, SdJournalPrevious,
    SdJournalProcessOutput, SdJournalQueryUnique, SdJournalSeekHead, SdJournalSeekTail,
    SdJournalSetOutputMode, SdJournalTestCursor,
};
pub use journal_core::error::JournalError;
pub use journal_core::file::{
    BucketUtilization, Direction, HashableObject, JournalFile, JournalReader, Location, Mmap,
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

#[derive(Debug, Clone)]
pub struct Entry {
    pub fields: HashMap<String, Vec<u8>>,
    pub field_values: HashMap<String, Vec<Vec<u8>>>,
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
}

enum StepStatus {
    Valid,
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
        let path = path.as_ref();
        if is_zst_file(path) {
            return Self::open_zst(path);
        }

        let file = open_journal_file(path)?;
        Ok(Self {
            inner: ReaderCellBuilder {
                file,
                reader_builder: |_file| JournalReader::default(),
            }
            .build(),
            temp_path: None,
        })
    }

    fn open_zst(path: &Path) -> Result<Self> {
        let temp_path = decompress_zst_to_temp(path, "rust-sdk-journal")?;
        let file = open_journal_file(&temp_path)?;
        Ok(Self {
            inner: ReaderCellBuilder {
                file,
                reader_builder: |_file| JournalReader::default(),
            }
            .build(),
            temp_path: Some(temp_path),
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
        self.inner.with_reader_mut(|reader| {
            reader.set_location(Location::Head);
        });
    }

    pub fn seek_tail(&mut self) {
        self.inner.with_reader_mut(|reader| {
            reader.set_location(Location::Tail);
        });
    }

    pub fn seek_realtime(&mut self, usec: u64) {
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
        Err(SdkError::InvalidCursor(cursor.to_string()))
    }

    pub fn next(&mut self) -> Result<bool> {
        self.step_valid(Direction::Forward)
    }

    pub fn previous(&mut self) -> Result<bool> {
        self.step_valid(Direction::Backward)
    }

    fn step_valid(&mut self, direction: Direction) -> Result<bool> {
        loop {
            let status = self.inner.with_mut(|fields| {
                if !fields.reader.step(fields.file, direction)? {
                    return Ok(StepStatus::End);
                }

                match fields
                    .reader
                    .get_entry_offset()
                    .and_then(|offset| fields.file.entry_ref(offset).map(|_| ()))
                {
                    Ok(()) => Ok(StepStatus::Valid),
                    Err(err) if recoverable_entry_error(&err) => Ok(StepStatus::Skip),
                    Err(err) => Err(err),
                }
            })?;

            match status {
                StepStatus::Valid => return Ok(true),
                StepStatus::Skip => continue,
                StepStatus::End => return Ok(false),
            }
        }
    }

    pub fn get_entry(&mut self) -> Result<Entry> {
        self.inner.with_mut(|fields| {
            let offset = fields.reader.get_entry_offset()?;
            read_entry_at(fields.file, fields.reader, offset)
        })
    }

    pub fn get_realtime_usec(&self) -> Result<u64> {
        self.inner
            .with(|fields| fields.reader.get_realtime_usec(fields.file))
            .map_err(Into::into)
    }

    pub fn get_cursor(&self) -> Result<String> {
        self.inner
            .with(|fields| build_cursor(fields.file, fields.reader))
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

pub struct DirectoryReader {
    files: Vec<FileReader>,
    index: usize,
}

impl DirectoryReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.is_dir() {
            return Err(SdkError::InvalidPath(format!(
                "not a directory: {}",
                path.display()
            )));
        }

        let mut files = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();
            if !file_path.is_file() || !is_journal_file_name(&file_path) {
                continue;
            }
            if let Ok(reader) = FileReader::open(&file_path) {
                files.push(reader);
            }
        }

        if files.is_empty() {
            return Err(SdkError::InvalidPath(format!(
                "no readable journal files in {}",
                path.display()
            )));
        }

        files.sort_by_key(FileReader::header_realtime_start);
        Ok(Self { files, index: 0 })
    }

    pub fn seek_head(&mut self) {
        self.index = 0;
        if let Some(reader) = self.files.first_mut() {
            reader.seek_head();
        }
    }

    pub fn seek_tail(&mut self) {
        self.index = self.files.len().saturating_sub(1);
        if let Some(reader) = self.files.last_mut() {
            reader.seek_tail();
        }
    }

    pub fn next(&mut self) -> Result<bool> {
        while self.index < self.files.len() {
            if self.files[self.index].next()? {
                return Ok(true);
            }
            self.index += 1;
            if self.index < self.files.len() {
                self.files[self.index].seek_head();
            }
        }
        Ok(false)
    }

    pub fn previous(&mut self) -> Result<bool> {
        loop {
            if self.index >= self.files.len() {
                return Ok(false);
            }
            if self.files[self.index].previous()? {
                return Ok(true);
            }
            if self.index == 0 {
                return Ok(false);
            }
            self.index -= 1;
            self.files[self.index].seek_tail();
        }
    }

    pub fn get_entry(&mut self) -> Result<Entry> {
        if self.index >= self.files.len() {
            return Err(SdkError::NoEntry);
        }
        self.files[self.index].get_entry()
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
        for idx in 0..self.files.len() {
            if self.files[idx].seek_cursor(cursor).is_ok() {
                self.index = idx;
                return Ok(());
            }
        }
        Err(SdkError::InvalidCursor(cursor.to_string()))
    }

    pub fn enumerate_fields(&mut self) -> Result<Vec<String>> {
        let mut fields = HashSet::new();
        for reader in &mut self.files {
            reader.seek_head();
            while reader.next()? {
                if let Ok(entry) = reader.get_entry() {
                    fields.extend(entry.fields.into_keys());
                }
            }
        }
        let mut out: Vec<_> = fields.into_iter().collect();
        out.sort();
        Ok(out)
    }

    pub fn query_unique(&mut self, field_name: &str) -> Result<Vec<Vec<u8>>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for reader in &mut self.files {
            reader.seek_head();
            while reader.next()? {
                if let Ok(entry) = reader.get_entry() {
                    if let Some(values) = entry.field_values.get(field_name) {
                        for value in values {
                            if seen.insert(value.clone()) {
                                out.push(value.clone());
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
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
    }

    pub fn add_conjunction(&mut self) -> Result<()> {
        for reader in &mut self.files {
            reader.add_conjunction()?;
        }
        Ok(())
    }

    pub fn add_disjunction(&mut self) -> Result<()> {
        for reader in &mut self.files {
            reader.add_disjunction()?;
        }
        Ok(())
    }

    pub fn flush_matches(&mut self) {
        for reader in &mut self.files {
            reader.flush_matches();
        }
    }
}

impl FileReader {
    fn header_realtime_start(&self) -> u64 {
        self.header().head_entry_realtime
    }
}

fn open_journal_file(path: &Path) -> Result<JournalFile<Mmap>> {
    let repo_file = journal_core::repository::File::from_path(path)
        .ok_or_else(|| SdkError::InvalidPath(path.display().to_string()))?;
    JournalFile::open(&repo_file, 4096).map_err(Into::into)
}

fn build_cursor(file: &JournalFile<Mmap>, reader: &JournalReader<'_, Mmap>) -> Result<String> {
    let (seqnum, seqnum_id) = reader.get_seqnum(file)?;
    let offset = reader.get_entry_offset()?;
    let entry = file.entry_ref(offset)?;
    Ok(format!(
        "s={};j={};c={:016x};n={}",
        hex::encode(seqnum_id),
        hex::encode(entry.header.boot_id),
        entry.header.realtime,
        seqnum
    ))
}

fn read_entry_at(
    file: &JournalFile<Mmap>,
    reader: &JournalReader<'_, Mmap>,
    entry_offset: NonZeroU64,
) -> Result<Entry> {
    let (seqnum, realtime, monotonic, boot_id) = {
        let entry = file.entry_ref(entry_offset)?;
        (
            entry.header.seqnum,
            entry.header.realtime,
            entry.header.monotonic,
            entry.header.boot_id,
        )
    };

    let mut fields = HashMap::new();
    let mut field_values: HashMap<String, Vec<Vec<u8>>> = HashMap::new();
    let mut decompressed = Vec::new();

    for data in file.entry_data_objects(entry_offset)? {
        let data = match data {
            Ok(data) => data,
            Err(err) if recoverable_entry_data_error(&err) => continue,
            Err(err) => return Err(err.into()),
        };
        let payload = if data.is_compressed() {
            decompressed.clear();
            data.decompress(&mut decompressed)?;
            decompressed.as_slice()
        } else {
            data.raw_payload()
        };

        if let Some(eq) = payload.iter().position(|byte| *byte == b'=') {
            let name = String::from_utf8_lossy(&payload[..eq]).into_owned();
            let value = payload[eq + 1..].to_vec();
            fields.insert(name.clone(), value.clone());
            field_values.entry(name).or_default().push(value);
        }
    }

    Ok(Entry {
        fields,
        field_values,
        seqnum,
        realtime,
        monotonic,
        boot_id,
        cursor: build_cursor(file, reader)?,
    })
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

fn is_zst_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".zst"))
}

fn decompress_zst_to_temp(path: &Path, prefix: &str) -> Result<PathBuf> {
    let source = File::open(path)?;
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(source)
        .map_err(|err| SdkError::DecompressionFailed(err.to_string()))?;
    let temp_path = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}.journal",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut dest = File::create(&temp_path)?;
    std::io::copy(&mut decoder, &mut dest)?;
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
    out.push(b'\n');
    out
}

pub fn export_entry(entry: &Entry) -> String {
    String::from_utf8_lossy(&export_entry_bytes(entry)).into_owned()
}

fn write_export_field(out: &mut Vec<u8>, name: &str, value: &[u8]) {
    if value
        .iter()
        .all(|byte| *byte == b'\t' || (0x20..0x7f).contains(byte))
    {
        out.extend_from_slice(name.as_bytes());
        out.push(b'=');
        out.extend_from_slice(value);
        out.push(b'\n');
    } else {
        out.extend_from_slice(name.as_bytes());
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

pub fn parse_match_string(s: &str) -> std::result::Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    parse_match_bytes(s.as_bytes())
}

pub fn parse_match_bytes(
    data: &[u8],
) -> std::result::Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let Some(eq) = data.iter().position(|byte| *byte == b'=') else {
        return Err("EINVAL: missing '=' separator".into());
    };
    let key = &data[..eq];
    if key.is_empty()
        || key[0].is_ascii_digit()
        || !key
            .iter()
            .copied()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err("EINVAL: invalid field name".into());
    }
    Ok(data.to_vec())
}

pub fn parse_cursor(
    cursor: &str,
) -> std::result::Result<(String, String, u64, u64), Box<dyn Error + Send + Sync>> {
    let mut seqnum_id = String::new();
    let mut boot_id = String::new();
    let mut realtime = None;
    let mut seqnum = None;

    for part in cursor.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key {
            "s" => seqnum_id = value.to_string(),
            "j" => boot_id = value.to_string(),
            "c" => realtime = Some(u64::from_str_radix(value, 16)?),
            "n" => seqnum = Some(value.parse()?),
            _ => {}
        }
    }

    if seqnum_id.is_empty() || boot_id.is_empty() {
        return Err("invalid cursor: missing id".into());
    }

    Ok((
        seqnum_id,
        boot_id,
        realtime.ok_or("invalid cursor: missing realtime")?,
        seqnum.ok_or("invalid cursor: missing seqnum")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::path::PathBuf;

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
}
