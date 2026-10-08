use journal_core::file::{
    Direction, JournalFile, JournalFileOptions, JournalReader, JournalState, JournalWriter, Mmap,
    MmapMut,
};
use journal_log_writer::{
    Config, EntryTimestamps, Log, RetentionPolicy, RotationPolicy, inspect_root_retention,
};
use journal_registry::{Origin, Source, repository::File};
use std::{
    fs,
    path::Path,
    time::{Duration, UNIX_EPOCH},
};
use uuid::Uuid;
const DAY: u64 = 86_400_000_000;
fn id(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}
fn source() -> Source {
    Source::Unknown("history".into())
}
fn config() -> Config {
    Config::new(
        Origin {
            machine_id: Some(id(1)),
            namespace: None,
            source: source(),
        },
        RotationPolicy::default(),
        RetentionPolicy::default(),
    )
    .with_boot_id(id(2))
    .with_strict_systemd_naming(true)
    .with_root_retention(true)
}
fn inventory(root: &Path) {
    for f in inspect_root_retention(root, &source()).unwrap().files {
        println!(
            "{} {} {} {} {} {} {} {}",
            f.machine_id.simple(),
            f.bytes,
            f.entries,
            f.head_seqnum,
            f.tail_seqnum,
            f.head_realtime,
            f.tail_realtime,
            f.active
        )
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let root = Path::new(&args[2]);
    let now: u64 = args[3].parse().unwrap();
    match args[1].as_str() {
        "fixture" => {
            let count: usize = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(0);
            for n in 21..=23 {
                if count > 0 && n != 21 {
                    continue;
                }
                let times = if count > 0 {
                    (0..count).map(|i| now + i as u64).collect::<Vec<_>>()
                } else if n == 22 {
                    vec![now - 40 * DAY, now - 20 * DAY]
                } else {
                    vec![now - 40 * DAY]
                };
                let dir = root.join(id(n).simple().to_string());
                fs::create_dir_all(&dir).unwrap();
                let path = dir.join("history.journal");
                let file = File::from_path(&path).unwrap();
                let opts = JournalFileOptions::new(id(n), id(2), id(3))
                    .with_data_hash_table_buckets(32)
                    .with_field_hash_table_buckets(16);
                let mut journal = JournalFile::<MmapMut>::create(&file, opts).unwrap();
                let mut writer = JournalWriter::new(&mut journal, 1, id(2)).unwrap();
                for (i, t) in times.iter().enumerate() {
                    writer
                        .add_entry(&mut journal, &[b"MESSAGE=fixture"], *t, i as u64 + 1)
                        .unwrap();
                }
                journal.journal_header_mut().state = if n == 23 {
                    JournalState::Offline
                } else {
                    JournalState::Archived
                } as u8;
                journal.sync().unwrap();
                drop(writer);
                drop(journal);
                if n != 23 {
                    fs::rename(
                        &path,
                        dir.join(format!(
                            "history@{}-{:016x}-{:016x}.journal",
                            id(3).simple(),
                            1,
                            times[0]
                        )),
                    )
                    .unwrap();
                }
            }
        }
        "benchmark" => {
            let start = std::time::Instant::now();
            for _ in 0..1000 {
                assert_eq!(
                    inspect_root_retention(root, &source()).unwrap().files.len(),
                    1
                );
            }
            println!("{}", start.elapsed().as_nanos() / 1000);
        }
        "inspect" => inventory(root),
        "maintain" => {
            let mut log = Log::new(root, config()).unwrap();
            log.set_root_retention_policy(
                RetentionPolicy::default()
                    .with_duration_of_journal_files(Duration::from_micros(30 * DAY)),
            )
            .unwrap();
            let r = log
                .maintain_root_retention(UNIX_EPOCH + Duration::from_micros(now))
                .unwrap();
            println!("deleted {}", r.deleted_files);
            log.close_without_retention().unwrap();
            inventory(root)
        }
        "reject" => {
            assert!(inspect_root_retention(root, &source()).is_err());
            assert!(
                Log::new(
                    root,
                    config().with_retention_policy(
                        RetentionPolicy::default().with_size_of_journal_files(1)
                    )
                )
                .is_err()
            );
            println!("rejected")
        }
        "live" => {
            let mut log = Log::new(root, config()).unwrap();
            let append = |log: &mut Log, t| {
                log.write_entry_with_timestamps(
                    &[b"MESSAGE=live"],
                    EntryTimestamps::default()
                        .with_entry_realtime_usec(t)
                        .with_entry_monotonic_usec(1),
                )
                .unwrap();
            };
            append(&mut log, now);
            log.set_root_retention_policy(
                RetentionPolicy::default()
                    .with_duration_of_journal_files(Duration::from_micros(30 * DAY)),
            )
            .unwrap();
            let r = log
                .maintain_root_retention(UNIX_EPOCH + Duration::from_micros(now + 31 * DAY))
                .unwrap();
            assert!(r.inventory.files.is_empty());
            assert!(log.active_path().is_none());
            append(&mut log, now + 32 * DAY);
            log.close_without_retention().unwrap();
            inventory(root)
        }
        "read" => {
            for f in inspect_root_retention(root, &source()).unwrap().files {
                let file = JournalFile::<Mmap>::open_path(&f.path, f.bytes).unwrap();
                let mut reader = JournalReader::default();
                while reader.step(&file, Direction::Forward).unwrap() {
                    println!("{}", reader.get_realtime_usec(&file).unwrap());
                }
            }
        }
        _ => panic!("unknown mode"),
    }
}
