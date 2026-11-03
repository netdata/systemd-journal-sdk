use super::*;

const PAST: u64 = 1_600_000_000_000_000;

fn write_at(log: &mut Log, message: &[u8], realtime: u64) {
    log.write_entry_with_timestamps(
        &[message],
        next_test_timestamps().with_entry_realtime_usec(realtime),
    )
    .unwrap();
}

/// Returns `(n_entries, head_realtime, tail_realtime)` for every journal file.
fn realtime_bounds(dir: &TempDir) -> Vec<(u64, u64, u64)> {
    journal_file_paths(dir)
        .iter()
        .map(|path| {
            let file = File::from_path(path).expect("journal path should parse");
            let journal = JournalFile::<Mmap>::open(&file, 4096).expect("open journal");
            let header = journal.journal_header_ref();
            (
                header.n_entries,
                header.head_entry_realtime,
                header.tail_entry_realtime,
            )
        })
        .collect()
}

#[test]
fn fresh_seed_zero_keeps_past_event_times() {
    let dir = TempDir::new().unwrap();
    let mut log = Log::new(dir.path(), test_config().with_fresh_realtime_seed_usec(0)).unwrap();
    write_at(&mut log, b"MESSAGE=event 1", PAST);
    write_at(&mut log, b"MESSAGE=event 2", PAST + 5);
    log.close().unwrap();

    assert_eq!(realtime_bounds(&dir), vec![(2, PAST, PAST + 5)]);
}

#[test]
fn fresh_default_clamps_past_event_time_to_wall_clock() {
    let dir = TempDir::new().unwrap();
    let before = Microseconds::now().get();
    let mut log = Log::new(dir.path(), test_config()).unwrap();
    write_at(&mut log, b"MESSAGE=event", PAST);
    log.close().unwrap();

    let bounds = realtime_bounds(&dir);
    assert_eq!(bounds.len(), 1);
    assert!(
        bounds[0].1 > before,
        "an unset seed keeps the wall-clock floor"
    );
}

#[test]
fn fresh_seed_is_a_floor() {
    let dir = TempDir::new().unwrap();
    let mut log = Log::new(
        dir.path(),
        test_config().with_fresh_realtime_seed_usec(PAST),
    )
    .unwrap();
    write_at(&mut log, b"MESSAGE=below the seed", PAST - 10);
    write_at(&mut log, b"MESSAGE=above the seed", PAST + 100);
    log.close().unwrap();

    assert_eq!(realtime_bounds(&dir), vec![(2, PAST + 1, PAST + 100)]);
}

#[test]
fn existing_tail_wins_over_fresh_seed() {
    let dir = TempDir::new().unwrap();
    let config = test_config().with_fresh_realtime_seed_usec(0);
    let mut log = Log::new(dir.path(), config.clone()).unwrap();
    write_at(&mut log, b"MESSAGE=first run", PAST);
    log.close().unwrap();

    let mut log = Log::new(dir.path(), config).unwrap();
    write_at(&mut log, b"MESSAGE=second run, older", PAST - 10);
    log.close().unwrap();

    let tails: Vec<u64> = realtime_bounds(&dir).iter().map(|b| b.2).collect();
    assert_eq!(tails.iter().max(), Some(&(PAST + 1)));
}

#[test]
fn fresh_seed_keeps_duplicate_and_out_of_order_clamping() {
    let dir = TempDir::new().unwrap();
    let mut log = Log::new(dir.path(), test_config().with_fresh_realtime_seed_usec(0)).unwrap();
    write_at(&mut log, b"MESSAGE=a", PAST);
    write_at(&mut log, b"MESSAGE=b duplicate", PAST);
    write_at(&mut log, b"MESSAGE=c older", PAST - 50);
    log.close().unwrap();

    assert_eq!(realtime_bounds(&dir), vec![(3, PAST, PAST + 2)]);
}

#[test]
fn fresh_seed_zero_keeps_wall_clock_for_omitted_realtime() {
    let dir = TempDir::new().unwrap();
    let before = Microseconds::now().get();
    let mut log = Log::new(dir.path(), test_config().with_fresh_realtime_seed_usec(0)).unwrap();
    write_test_entry(&mut log, &[b"MESSAGE=no realtime"]).unwrap();
    log.close().unwrap();

    let bounds = realtime_bounds(&dir);
    assert_eq!(bounds.len(), 1);
    assert!(bounds[0].1 >= before);
}

#[test]
fn fresh_seed_outside_systemd_realtime_range_is_rejected() {
    for seed in [u64::MAX, (1 << 55) - 1] {
        let dir = TempDir::new().unwrap();
        let result = Log::new(
            dir.path(),
            test_config().with_fresh_realtime_seed_usec(seed),
        );
        let Err(error) = result else {
            panic!("seed {seed} must be rejected");
        };
        assert!(matches!(error, WriterError::InvalidConfig(_)), "{error:?}");
    }
    let dir = TempDir::new().unwrap();
    let mut log = Log::new(
        dir.path(),
        test_config().with_fresh_realtime_seed_usec((1 << 55) - 2),
    )
    .unwrap();
    write_at(&mut log, b"MESSAGE=at the limit", 1);
    log.close().unwrap();
    let bounds = realtime_bounds(&dir);
    assert_eq!(bounds, vec![(1, (1 << 55) - 1, (1 << 55) - 1)]);
}
