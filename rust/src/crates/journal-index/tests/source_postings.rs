//! Source-timestamp postings must describe the same captured ENTRY population
//! as the global entry array (SOW-0161).

use journal_common::Seconds;
use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter, MmapMut};
use journal_core::repository::File;
use journal_index::{FieldName, FileIndexer};
use uuid::Uuid;

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

const SOURCE: &[u8] = b"SOURCE_TIMESTAMP=2000000";

struct Fixture {
    _temp: tempfile::TempDir,
    path: std::path::PathBuf,
    file: File,
    journal: Option<JournalFile<MmapMut>>,
    writer: Option<JournalWriter>,
}

impl Fixture {
    fn new(compact: bool, rows: u64) -> Self {
        Self::with_name(compact, rows, "system.journal")
    }

    /// An archived name with an offline header makes the file stable.
    fn archived(compact: bool, rows: u64) -> Self {
        let name = format!(
            "system@{}-0000000000000001-00000000000f4240.journal",
            Uuid::from_u128(3).simple()
        );
        let fixture = Self::with_name(compact, rows, &name);
        let mut bytes = fixture.bytes();
        bytes[16] = 0;
        std::fs::write(&fixture.path, bytes).unwrap();
        fixture
    }

    fn with_name(compact: bool, rows: u64, name: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join(Uuid::from_u128(1).simple().to_string());
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(name);
        let file = File::from_path(&path).unwrap();
        let mut journal = JournalFile::<MmapMut>::create(
            &file,
            JournalFileOptions::new(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3))
                .with_compact(compact),
        )
        .unwrap();
        let writer = JournalWriter::new(&mut journal, 1, Uuid::from_u128(2)).unwrap();
        let mut fixture = Self {
            _temp: temp,
            path,
            file,
            journal: Some(journal),
            writer: Some(writer),
        };
        fixture.append(1..=rows, &[b"MESSAGE=row", SOURCE]);
        fixture
    }

    fn append(&mut self, rows: std::ops::RangeInclusive<u64>, payloads: &[&[u8]]) {
        let journal = self.journal.as_mut().unwrap();
        let writer = self.writer.as_mut().unwrap();
        for index in rows {
            writer
                .add_entry(journal, payloads, index * 1_000_000, index)
                .unwrap();
        }
        journal.sync().unwrap();
    }

    fn bytes(&self) -> Vec<u8> {
        std::fs::read(&self.path).unwrap()
    }

    fn index(&self) -> journal_index::Result<usize> {
        let field = FieldName::new_unchecked("SOURCE_TIMESTAMP".to_owned());
        FileIndexer::default()
            .index(&self.file, Some(&field), &[], Seconds(1))
            .map(|index| index.total_entries())
    }

    fn index_message_bitmap(&self) -> journal_index::Result<usize> {
        let field = FieldName::new_unchecked("MESSAGE".to_owned());
        FileIndexer::default()
            .index(&self.file, None, std::slice::from_ref(&field), Seconds(1))
            .map(|index| index.total_entries())
    }

    fn patch(&self, offset: usize, value: u64) {
        let mut bytes = self.bytes();
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        std::fs::write(&self.path, bytes).unwrap();
    }

    /// Drops the writer so the file can be patched like a closed file.
    fn close(mut self) -> Self {
        self.writer = None;
        self.journal = None;
        self
    }
}

fn find_data(bytes: &[u8], compact: bool, payload: &[u8]) -> usize {
    let payload_start = if compact { 72 } else { 64 };
    let mut data = read_u64(bytes, 88) as usize;
    loop {
        let size = read_u64(bytes, data + 8) as usize;
        if bytes[data] == 1 && &bytes[data + payload_start..data + size] == payload {
            return data;
        }
        data += (size + 7) & !7;
        assert!(data < bytes.len(), "missing DATA");
    }
}

#[test]
fn source_postings_beyond_the_captured_tail_are_ignored() {
    for compact in [false, true] {
        let mut fixture = Fixture::new(compact, 14);
        let captured_tail = read_u64(&fixture.bytes(), 136);
        // Rows published after the reader captured its tail.
        fixture.append(15..=16, &[b"MESSAGE=row", SOURCE]);
        fixture.patch(136, captured_tail);

        assert_eq!(fixture.index().unwrap(), 14, "compact={compact}");
    }
}

/// The inline ENTRY target of the source DATA names the DATA object itself.
fn point_source_inline_at_itself(fixture: &Fixture, compact: bool) {
    let data = find_data(&fixture.bytes(), compact, SOURCE);
    fixture.patch(data + 40, data as u64);
}

#[test]
fn stable_source_posting_outside_the_entry_population_is_rejected() {
    for compact in [false, true] {
        let fixture = Fixture::archived(compact, 14).close();
        assert_eq!(fixture.index().unwrap(), 14, "compact={compact}");
        point_source_inline_at_itself(&fixture, compact);

        let result = fixture.index();
        assert!(
            matches!(result, Err(journal_index::IndexError::MissingOffset)),
            "compact={compact}: {result:?}"
        );
    }
}

#[test]
fn live_source_posting_outside_the_entry_population_is_dropped() {
    for compact in [false, true] {
        let fixture = Fixture::new(compact, 14).close();
        point_source_inline_at_itself(&fixture, compact);
        // Entry 1 falls back to its ENTRY realtime; the bogus posting is gone.
        assert_eq!(fixture.index().unwrap(), 14, "compact={compact}");
    }
}

#[test]
fn live_entry_linked_before_it_is_counted_is_not_an_error() {
    for compact in [false, true] {
        let fixture = Fixture::new(compact, 14).close();
        // A reader that captured the tail after the last entry was linked into
        // its DATA arrays, but read n_entries before the writer counted it.
        let n_entries = read_u64(&fixture.bytes(), 152);
        fixture.patch(152, n_entries - 1);

        assert_eq!(fixture.index().unwrap(), 13, "compact={compact}");
        assert_eq!(
            fixture.index_message_bitmap().unwrap(),
            13,
            "compact={compact}"
        );
    }
}

#[test]
fn entry_with_repeated_source_field_counts_once() {
    for compact in [false, true] {
        let mut fixture = Fixture::new(compact, 3);
        fixture.append(
            4..=4,
            &[b"MESSAGE=row", SOURCE, b"SOURCE_TIMESTAMP=3000000"],
        );
        assert_eq!(fixture.index().unwrap(), 4, "compact={compact}");
    }
}
