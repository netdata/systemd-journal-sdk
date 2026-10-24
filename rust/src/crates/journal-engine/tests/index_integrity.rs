use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter, MmapMut};
use journal_engine::{
    Facets, FileIndexCacheBuilder, FileIndexKey, IndexingLimits, QueryTimeRange,
    batch_compute_file_indexes,
};
use journal_index::FieldName;
use journal_registry::{File, Monitor, Registry};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[tokio::test(flavor = "current_thread")]
async fn failed_native_field_indexes_are_not_cached() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let machine = Uuid::from_u128(1);
        let directory = temp.path().join(machine.simple().to_string());
        std::fs::create_dir_all(&directory).unwrap();
        let mut files = Vec::new();
        for index in 0..2 {
            let sequence = Uuid::from_u128(3 + index);
            let path = directory.join(format!(
                "system@{}-0000000000000001-00000000000f4240.journal",
                sequence.simple()
            ));
            let file = File::from_path(&path).unwrap();
            let mut journal = JournalFile::<MmapMut>::create(
                &file,
                JournalFileOptions::new(machine, Uuid::from_u128(2), sequence)
                    .with_compact(compact),
            )
            .unwrap();
            let mut writer = JournalWriter::new(&mut journal, 1, Uuid::from_u128(2)).unwrap();
            for row in 1..=14 {
                writer
                    .add_entry(
                        &mut journal,
                        &[b"MESSAGE=row", b"SOURCE_TIMESTAMP=2000000"],
                        row * 1_000_000,
                        row,
                    )
                    .unwrap();
            }
            journal.sync().unwrap();
            drop(writer);
            drop(journal);
            let mut source = std::fs::read(&path).unwrap();
            source[16] = 0;
            std::fs::write(&path, &source).unwrap();
            files.push((file, path, source));
        }
        for source_timestamp in [false, true] {
            let payload: &[u8] = if source_timestamp {
                b"SOURCE_TIMESTAMP=2000000"
            } else {
                b"MESSAGE=row"
            };
            let mut damaged = files[1].2.clone();
            let mut data = read_u64(&damaged, 88) as usize;
            let payload_start = if compact { 72 } else { 64 };
            loop {
                let size = read_u64(&damaged, data + 8) as usize;
                if damaged[data] == 1 && &damaged[data + payload_start..data + size] == payload {
                    break;
                }
                data += (size + 7) & !7;
                assert!(data < damaged.len());
            }
            if source_timestamp {
                let head = read_u64(&damaged, data + 48) as usize;
                damaged[head + 16..head + 24].fill(0);
            } else {
                damaged[data + 32..data + 40].copy_from_slice(&(data as u64).to_le_bytes());
            }
            std::fs::write(&files[1].1, damaged).unwrap();
            let facets = Facets::new(&["MESSAGE".to_owned()]);
            let source_field =
                source_timestamp.then(|| FieldName::new_unchecked("SOURCE_TIMESTAMP"));
            let good_key = FileIndexKey::new(&files[0].0, &facets, source_field.clone());
            let bad_key = FileIndexKey::new(&files[1].0, &facets, source_field);
            let cache = FileIndexCacheBuilder::new()
                .with_memory_capacity(4)
                .without_disk_cache()
                .build()
                .await
                .unwrap();
            let (monitor, _receiver) = Monitor::new().unwrap();
            let registry = Registry::new(monitor);
            let responses = batch_compute_file_indexes(
                &cache,
                &registry,
                vec![good_key.clone(), bad_key.clone()],
                &QueryTimeRange::new(0, 20).unwrap(),
                CancellationToken::new(),
                IndexingLimits::default(),
                None,
            )
            .await
            .unwrap();
            assert_eq!(responses.len(), 1, "{compact}/{source_timestamp}");
            assert_eq!(responses[0].0, good_key);
            assert_eq!(responses[0].1.total_entries(), 14);
            assert!(cache.get(&good_key).await.unwrap().is_some());
            assert!(cache.get(&bad_key).await.unwrap().is_none());
            cache.close().await.unwrap();
        }
    }
}
