use journal_common::Seconds;
use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter, MmapMut};
use journal_core::repository::File;
use journal_index::{FieldName, FileIndexer};
use uuid::Uuid;

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[test]
fn archived_indexes_reject_required_missing_or_revisited_arrays() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let machine = Uuid::from_u128(1);
        let boot = Uuid::from_u128(2);
        let sequence = Uuid::from_u128(3);
        let directory = temp.path().join(machine.simple().to_string());
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!(
            "system@{}-0000000000000001-00000000000f4240.journal",
            sequence.simple()
        ));
        let file = File::from_path(&path).unwrap();
        assert!(!file.is_active());
        let mut journal = JournalFile::<MmapMut>::create(
            &file,
            JournalFileOptions::new(machine, boot, sequence).with_compact(compact),
        )
        .unwrap();
        let mut writer = JournalWriter::new(&mut journal, 1, boot).unwrap();
        for index in 1..=13 {
            writer
                .add_entry(&mut journal, &[b"MESSAGE=row"], index * 1_000_000, index)
                .unwrap();
        }
        journal.sync().unwrap();
        drop(writer);
        drop(journal);
        let mut source = std::fs::read(&path).unwrap();
        source[16] = 0;
        std::fs::write(&path, &source).unwrap();
        let healthy = FileIndexer::default()
            .index(&file, None, &[], Seconds(1))
            .unwrap();
        assert_eq!(healthy.total_entries(), 13);
        assert!(!healthy.online());
        assert!(healthy.is_fresh());

        let head = read_u64(&source, 176) as usize;
        let next = read_u64(&source, head + 16) as usize;
        assert_ne!(next, 0);
        for cycle in [false, true] {
            let mut damaged = source.clone();
            if cycle {
                damaged[next + 16..next + 24].copy_from_slice(&(head as u64).to_le_bytes());
            } else {
                damaged[head + 16..head + 24].fill(0);
            }
            std::fs::write(&path, damaged).unwrap();
            let result = FileIndexer::default().index(&file, None, &[], Seconds(1));
            assert!(
                result.is_err(),
                "compact={compact}/cycle={cycle}: incomplete archived index succeeded: {:?}",
                result.as_ref().map(|index| index.total_entries())
            );
        }
    }
}

#[test]
fn archived_indexes_reject_incomplete_requested_field_indexes() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join(Uuid::from_u128(1).simple().to_string());
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!(
            "system@{}-0000000000000001-00000000000f4240.journal",
            Uuid::from_u128(3).simple()
        ));
        let file = File::from_path(&path).unwrap();
        let mut journal = JournalFile::<MmapMut>::create(
            &file,
            JournalFileOptions::new(Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3))
                .with_compact(compact),
        )
        .unwrap();
        let mut writer = JournalWriter::new(&mut journal, 1, Uuid::from_u128(2)).unwrap();
        for index in 1..=14 {
            writer
                .add_entry(
                    &mut journal,
                    &[b"MESSAGE=row", b"SOURCE_TIMESTAMP=2000000"],
                    index * 1_000_000,
                    index,
                )
                .unwrap();
        }
        journal.sync().unwrap();
        drop(writer);
        drop(journal);
        let mut source = std::fs::read(&path).unwrap();
        source[16] = 0;
        for source_timestamp in [false, true] {
            let (name, payload): (&str, &[u8]) = if source_timestamp {
                ("SOURCE_TIMESTAMP", b"SOURCE_TIMESTAMP=2000000")
            } else {
                ("MESSAGE", b"MESSAGE=row")
            };
            let field = FieldName::new_unchecked(name.to_owned());
            let fields = if source_timestamp {
                Vec::new()
            } else {
                vec![field.clone()]
            };
            let source_field = source_timestamp.then_some(&field);
            std::fs::write(&path, &source).unwrap();
            assert_eq!(
                FileIndexer::default()
                    .index(&file, source_field, &fields, Seconds(1))
                    .unwrap()
                    .total_entries(),
                14
            );
            let payload_start = if compact { 72 } else { 64 };
            let mut data = read_u64(&source, 88) as usize;
            loop {
                let size = read_u64(&source, data + 8) as usize;
                if source[data] == 1 && &source[data + payload_start..data + size] == payload {
                    break;
                }
                data += (size + 7) & !7;
                assert!(data < source.len(), "missing requested DATA");
            }
            let head = read_u64(&source, data + 48) as usize;
            let next = read_u64(&source, head + 16) as usize;
            assert_ne!(next, 0);
            for mutation in [
                "field-cycle",
                "missing-link",
                "posting-cycle",
                "missing-inline",
                "missing-head",
                "unindexed-inline",
            ] {
                if source_timestamp && mutation == "unindexed-inline" {
                    continue;
                }
                let mut damaged = source.clone();
                match mutation {
                    "field-cycle" => {
                        damaged[data + 32..data + 40].copy_from_slice(&(data as u64).to_le_bytes())
                    }
                    "missing-link" => damaged[head + 16..head + 24].fill(0),
                    "posting-cycle" => {
                        damaged[next + 16..next + 24].copy_from_slice(&(head as u64).to_le_bytes())
                    }
                    "missing-inline" => damaged[data + 40..data + 48].fill(0),
                    "missing-head" => damaged[data + 48..data + 56].fill(0),
                    "unindexed-inline" => {
                        damaged[data + 40..data + 48].copy_from_slice(&(data as u64).to_le_bytes())
                    }
                    _ => unreachable!(),
                }
                std::fs::write(&path, damaged).unwrap();
                let result = FileIndexer::default().index(&file, source_field, &fields, Seconds(1));
                assert!(
                    result.is_err(),
                    "compact={compact}/source={source_timestamp}/{mutation}: incomplete index succeeded: {:?}",
                    result.as_ref().map(|index| index.total_entries())
                );
            }
        }
    }
}
