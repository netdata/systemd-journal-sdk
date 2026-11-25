//! A DATA object that cannot be decompressed costs its field in queries, not
//! the query (SOW-0178 regression 2026-11-25).

use journal_common::{Microseconds, Seconds};
use journal_core::file::{Compression, JournalFile, JournalFileOptions, JournalWriter, MmapMut};
use journal_core::repository::File;
use journal_index::{Anchor, Direction, FieldName, FileIndexer, LogQueryParamsBuilder};
use uuid::Uuid;

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

/// Writes two zstd rows and corrupts the first row's compressed MESSAGE, whose
/// DATA offset is lower than that row's source timestamp.
fn damaged_journal() -> (tempfile::TempDir, File) {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join(Uuid::from_u128(1).simple().to_string());
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("system.journal");
    let file = File::from_path(&path).unwrap();
    {
        let mut journal = JournalFile::<MmapMut>::create(
            &file,
            JournalFileOptions::new(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3))
                .with_compression(Compression::Zstd),
        )
        .unwrap();
        let mut writer = JournalWriter::new(&mut journal, 1, Uuid::from_u128(2)).unwrap();
        // The writer compresses payloads of 512 bytes or more.
        let first = format!("MESSAGE=first {}", "y".repeat(600));
        let second = format!("MESSAGE=second {}", "z".repeat(600));
        writer
            .add_entry(
                &mut journal,
                &[first.as_bytes(), b"SOURCE_TIMESTAMP=1500000"],
                1_000_000,
                1,
            )
            .unwrap();
        writer
            .add_entry(
                &mut journal,
                &[second.as_bytes(), b"SOURCE_TIMESTAMP=2500000"],
                2_000_000,
                2,
            )
            .unwrap();
        journal.sync().unwrap();
    }

    // Zero the tail of the zstd frame of the compressed DATA object that the
    // first ENTRY references: the first row's MESSAGE.
    let mut bytes = std::fs::read(&path).unwrap();
    let mut compressed = Vec::new();
    let mut first_entry = None;
    let mut object = read_u64(&bytes, 88) as usize;
    while object + 16 <= bytes.len() {
        let size = read_u64(&bytes, object + 8) as usize;
        if size == 0 {
            break;
        }
        match bytes[object] {
            1 if bytes[object + 1] != 0 => compressed.push((object, size)),
            3 if first_entry.is_none() => first_entry = Some((object, size)),
            _ => {}
        }
        object += (size + 7) & !7;
    }
    let (entry, entry_size) = first_entry.expect("missing ENTRY");
    let items = &bytes[entry + 64..entry + entry_size];
    let (data, size) = compressed
        .into_iter()
        .find(|(data, _)| {
            let offset = *data as u64;
            items
                .windows(8)
                .any(|window| window == offset.to_le_bytes())
                || items
                    .windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes())
        })
        .expect("first row has no compressed DATA");
    bytes[data + size - 8..data + size].fill(0);
    std::fs::write(&path, bytes).unwrap();
    (temp, file)
}

fn query_times(file: &File, source: bool, regex: Option<&str>) -> Vec<u64> {
    let field = FieldName::new_unchecked("SOURCE_TIMESTAMP".to_owned());
    let index = FileIndexer::default()
        .index(file, source.then_some(&field), &[], Seconds(1))
        .unwrap();
    let mut params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_source_timestamp_field(source.then_some(field))
        .with_after(Microseconds(0))
        .with_before(Microseconds(10_000_000))
        .with_limit(10);
    if let Some(regex) = regex {
        params = params.with_regex(regex);
    }
    let params = params.build().unwrap();
    let rows = index.find_log_entries(file, &params).unwrap();
    rows.iter().map(|row| row.timestamp.0).collect()
}

#[test]
fn damaged_data_does_not_fail_a_source_timestamp_query() {
    let (_temp, file) = damaged_journal();
    assert_eq!(query_times(&file, true, None), vec![1_500_000, 2_500_000]);
}

#[test]
fn damaged_data_matches_no_regex() {
    let (_temp, file) = damaged_journal();
    assert_eq!(query_times(&file, false, Some("second")), vec![2_000_000]);
    assert_eq!(query_times(&file, false, Some("first")), Vec::<u64>::new());
    assert_eq!(
        query_times(&file, false, Some("SOURCE_TIMESTAMP=1500000")),
        vec![1_000_000]
    );
}
