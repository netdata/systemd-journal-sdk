// Source-byte restoration in this module is synthetic fault injection, not
// evidence of journald publication or a supported in-place repair lifecycle.
// Damaged-prefix, failed-position and bounds checks remain recovery invariants.

use super::*;

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn hardening_fixture(path: &Path, compact: bool) -> Vec<u8> {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let repo = RepoFile::from_path(path).unwrap();
    let mut file = JournalFile::<MmapMut>::create(
        &repo,
        JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(compact),
    )
    .unwrap();
    let mut writer = JournalWriter::new(&mut file, 1, test_uuid(4)).unwrap();
    for i in 0..1005 {
        let message = format!("MESSAGE=row-{i:04}");
        let mut fields = vec![message.as_bytes(), b"COMMON=all"];
        if i == 0 || i == 500 || i == 1004 {
            fields.push(b"SPARSE=some");
        }
        if i >= 1003 {
            fields.push(b"TAIL_PAIR=some");
        }
        writer
            .add_entry(&mut file, &fields, 1000 + i, 100 + i)
            .unwrap();
    }
    file.sync().unwrap();
    drop(writer);
    drop(file);
    std::fs::read(path).unwrap()
}

fn array_graph(bytes: &[u8], compact: bool) -> Vec<(usize, Vec<u64>)> {
    let item = if compact { 4 } else { 8 };
    let mut remaining = u64_at(bytes, 152) as usize;
    let mut at = u64_at(bytes, 176) as usize;
    let mut arrays = Vec::new();
    while remaining > 0 {
        let count = remaining.min((u64_at(bytes, at + 8) as usize - 24) / item);
        let offsets = (0..count)
            .map(|i| {
                if compact {
                    u32::from_le_bytes(
                        bytes[at + 24 + i * item..at + 28 + i * item]
                            .try_into()
                            .unwrap(),
                    ) as u64
                } else {
                    u64_at(bytes, at + 24 + i * item)
                }
            })
            .collect();
        arrays.push((at, offsets));
        remaining -= count;
        at = u64_at(bytes, at + 16) as usize;
    }
    arrays
}

fn short_array_cycle_fixture(
    path: &Path,
    compact: bool,
    postings: bool,
    descending: bool,
) -> (u64, usize) {
    let mut source = hardening_fixture(path, compact);
    let arrays = array_graph(&source, compact);
    let (mut head, mut next, count, root_slot) = if postings {
        let payload_start = if compact { 72 } else { 64 };
        let mut data = u64_at(&source, 88) as usize;
        loop {
            let size = u64_at(&source, data + 8) as usize;
            if source[data] == 1 && &source[data + payload_start..data + size] == b"COMMON=all" {
                break;
            }
            data += (size + 7) & !7;
        }
        let head = u64_at(&source, data + 48) as usize;
        let next = u64_at(&source, head + 16) as usize;
        let width = if compact { 4 } else { 8 };
        let count = 2
            + (u64_at(&source, head + 8) as usize - 24) / width
            + (u64_at(&source, next + 8) as usize - 24) / width;
        source[data + 56..data + 64].copy_from_slice(&(count as u64).to_le_bytes());
        (head, next, count - 1, data + 48)
    } else {
        let count = arrays[0].1.len() + arrays[1].1.len() + 1;
        source[152..160].copy_from_slice(&(count as u64).to_le_bytes());
        (arrays[0].0, arrays[1].0, count, 176)
    };
    if descending {
        std::mem::swap(&mut head, &mut next);
        source[root_slot..root_slot + 8].copy_from_slice(&(head as u64).to_le_bytes());
    }
    source[head + 16..head + 24].copy_from_slice(&(next as u64).to_le_bytes());
    source[next + 16..next + 24].copy_from_slice(&(head as u64).to_le_bytes());
    std::fs::write(path, source).unwrap();
    (head as u64, count)
}

#[test]
fn counted_array_cycles_reject_required_revisit() {
    for compact in [false, true] {
        for postings in [false, true] {
            for descending in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().join("system.journal");
                let (_, count) = short_array_cycle_fixture(&path, compact, postings, descending);
                let mut reader =
                    FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
                if postings {
                    reader.add_match(b"COMMON=all");
                }
                let mut failed = false;
                for _ in 0..=count + 1 {
                    match reader.next() {
                        Err(SdkError::Journal(JournalError::CyclicObjectChain)) => {
                            failed = true;
                            break;
                        }
                        Err(error) => panic!("unexpected cycle error: {error:?}"),
                        Ok(false) => panic!("required revisit returned successful EOF"),
                        Ok(true) => {}
                    }
                }
                assert!(failed, "required revisit returned duplicate rows");
            }
        }
    }
}

#[test]
fn counted_array_nodes_reject_required_revisit() {
    use journal_core::file::offset_array::{Cursor, List, Node};
    use std::num::{NonZeroU64, NonZeroUsize};
    assert_eq!(
        std::mem::size_of::<Node>(),
        2 * std::mem::size_of::<u64>() + 2 * std::mem::size_of::<usize>()
    );
    for postings in [false, true] {
        for descending in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("system.journal");
            let (head, count) = short_array_cycle_fixture(&path, false, postings, descending);
            let file = JournalFile::<Mmap>::open_path(&path, 64 * 1024).unwrap();
            let list = List::new(
                NonZeroU64::new(head).unwrap(),
                NonZeroUsize::new(count).unwrap(),
            );
            let head = list.head(&file).unwrap();
            let second = head.next(&file).unwrap().unwrap();
            assert!(matches!(
                second.next(&file),
                Err(JournalError::CyclicObjectChain)
            ));
            let cursor = Cursor::at_position(
                &file,
                list,
                second.offset(),
                0,
                NonZeroUsize::new(count - head.len().get()).unwrap(),
            )
            .unwrap();
            assert!(matches!(
                cursor.node(&file).unwrap().next(&file),
                Err(JournalError::CyclicObjectChain)
            ));
        }
    }
}

#[test]
fn counted_array_accepts_acyclic_backward_link() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let mut source = hardening_fixture(&path, compact);
        let arrays = array_graph(&source, compact);
        let (a, b) = (&arrays[0], &arrays[1]);
        let entries: Vec<_> = a.1.iter().chain(&b.1).copied().collect();
        let width = if compact { 4 } else { 8 };
        for (index, entry) in entries.iter().enumerate() {
            let (array, slot) = if index < b.1.len() {
                (b.0, index)
            } else {
                (a.0, index - b.1.len())
            };
            let start = array + 24 + slot * width;
            source[start..start + width].copy_from_slice(&entry.to_le_bytes()[..width]);
        }
        source[152..160].copy_from_slice(&(entries.len() as u64).to_le_bytes());
        source[176..184].copy_from_slice(&(b.0 as u64).to_le_bytes());
        source[b.0 + 16..b.0 + 24].copy_from_slice(&(a.0 as u64).to_le_bytes());
        source[a.0 + 16..a.0 + 24].fill(0);
        std::fs::write(&path, source).unwrap();
        let mut reader = FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
        for index in 0..entries.len() {
            assert!(reader.next().unwrap());
            assert_eq!(reader.get_realtime_usec().unwrap(), 1000 + index as u64);
        }
        assert!(!reader.next().unwrap());
    }
}

#[test]
fn counted_array_ignores_unused_terminal_link() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("system.journal");
    let mut source = hardening_fixture(&path, false);
    let arrays = array_graph(&source, false);
    let count = arrays[0].1.len() + arrays[1].1.len();
    source[152..160].copy_from_slice(&(count as u64).to_le_bytes());
    let next = arrays[1].0;
    source[next + 16..next + 24].copy_from_slice(&(next as u64).to_le_bytes());
    std::fs::write(&path, source).unwrap();
    let mut reader = FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
    for _ in 0..count {
        assert!(reader.next().unwrap());
    }
    assert!(!reader.next().unwrap());
}

fn readable_prefix(bytes: &[u8], arrays: &[(usize, Vec<u64>)], cut: usize, compact: bool) -> usize {
    let mut count = 0;
    for (array, entries) in arrays {
        for (index, &offset) in entries.iter().enumerate() {
            let slot_end = array + 24 + (index + 1) * if compact { 4 } else { 8 };
            if slot_end > cut || offset as usize + u64_at(bytes, offset as usize + 8) as usize > cut
            {
                return count;
            }
            count += 1;
        }
    }
    count
}

#[test]
fn recovered_prefix_keeps_reverse_time_and_exact_filter_reads() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("system.journal");
        let bytes = hardening_fixture(&source, compact);
        let arrays = array_graph(&bytes, compact);
        let last_entry = *arrays.last().unwrap().1.last().unwrap() as usize;
        let last_array = arrays.last().unwrap().0;
        for (name, cut) in [
            ("entry-header", last_entry),
            ("entry-body", last_entry + 32),
            ("array-header", last_array),
            (
                "array-slots",
                last_array + 24 + 3 * if compact { 4 } else { 8 },
            ),
        ] {
            let path = temp.path().join(format!("{name}.journal"));
            std::fs::write(&path, &bytes[..cut]).unwrap();
            let expected = readable_prefix(&bytes, &arrays, cut, compact);
            let mut reader =
                FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
            let mut forward = Vec::new();
            while reader.next().unwrap() {
                forward.push(reader.get_realtime_usec().unwrap());
            }
            assert_eq!(forward.len(), expected, "{compact}/{name}: forward");
            let status = reader.recovery_status();
            assert!(status.incomplete(), "{compact}/{name}: status");
            assert_eq!(status.recovered_entries, expected as u64);
            assert_eq!(reader.header().n_entries, 1005);
            reader.seek_tail();
            let mut reverse = Vec::new();
            while reader.previous().unwrap() {
                reverse.push(reader.get_realtime_usec().unwrap());
            }
            assert_eq!(
                reverse,
                forward.iter().rev().copied().collect::<Vec<_>>(),
                "{compact}/{name}: reverse"
            );
            reader.seek_realtime(1000 + (expected / 2) as u64);
            assert!(reader.next().unwrap());
            assert_eq!(
                reader.get_realtime_usec().unwrap(),
                1000 + (expected / 2) as u64
            );
            reader.flush_matches();
            reader.add_match(b"COMMON=all");
            reader.seek_head();
            let mut filtered = Vec::new();
            while reader.next().unwrap() {
                filtered.push(reader.get_realtime_usec().unwrap());
            }
            assert_eq!(filtered, forward, "{compact}/{name}: filter");
            assert!(verify_file(&path).is_err());
            let control = SnapshotControl::default();
            assert!(verify_index(&path, &control).is_err());
            assert!(
                IndexedSnapshot::open(&path, IndexedSnapshotOptions::default(), &control).is_err()
            );
            let before = std::fs::read(&path).unwrap();
            assert!(
                JournalFile::<MmapMut>::open_for_append(
                    &RepoFile::from_path(&path).unwrap(),
                    8 * 1024 * 1024
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
}

#[test]
fn recovery_sparse_posting_boundary() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let source = hardening_fixture(&path, compact);
        let arrays = array_graph(&source, compact);
        let cut = *arrays.last().unwrap().1.last().unwrap() as usize;
        let payload_start = if compact { 72 } else { 64 };
        let mut at = u64_at(&source, 88) as usize;
        let sparse = loop {
            let size = u64_at(&source, at + 8) as usize;
            if source[at] == 1
                && size >= payload_start
                && &source[at + payload_start..at + size] == b"SPARSE=some"
            {
                break at;
            }
            assert!(size >= 16 && at + size < source.len());
            at += (size + 7) & !7;
        };
        for missing_array in [false, true] {
            let mut bytes = source[..cut].to_vec();
            if missing_array {
                bytes[sparse + 48..sparse + 56].fill(0);
            }
            std::fs::write(&path, bytes).unwrap();
            let mut reader =
                FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
            let mut population = 0;
            while reader.next().unwrap() {
                population += 1;
            }
            assert_eq!(population, 1004);
            reader.add_match(b"SPARSE=some");
            reader.seek_head();
            if missing_array {
                assert!(reader.next().is_err(), "{compact}: unknown posting targets");
            } else {
                let mut times = Vec::new();
                while reader.next().unwrap() {
                    times.push(reader.get_realtime_usec().unwrap());
                }
                assert_eq!(times, [1000, 1500], "{compact}: sparse prefix");
            }
            let query = ExplorerQuery {
                filters: vec![ExplorerFilter::new(b"SPARSE", [b"some".as_slice()])],
                field_mode: ExplorerFieldMode::AllValues,
                use_source_realtime: false,
                ..Default::default()
            };
            let result = reader.explore_with_strategy(&query, ExplorerStrategy::Index);
            assert_eq!(
                result.is_err(),
                missing_array,
                "{compact}: Explorer boundary"
            );
        }
    }
}

#[test]
fn indexed_postings_reject_missing_pointers_without_prior_traversal() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let source = hardening_fixture(&path, compact);
        let payload_start = if compact { 72 } else { 64 };
        let mut common = u64_at(&source, 88) as usize;
        loop {
            let size = u64_at(&source, common + 8) as usize;
            if source[common] == 1
                && size >= payload_start
                && &source[common + payload_start..common + size] == b"COMMON=all"
            {
                break;
            }
            assert!(size >= 16 && common + size < source.len());
            common += (size + 7) & !7;
        }
        let array = u64_at(&source, common + 48) as usize;
        for pointer in [common + 40, common + 48, array + 16] {
            let mut bytes = source.clone();
            bytes[pointer..pointer + 8].fill(0);
            std::fs::write(&path, bytes).unwrap();
            for backward in [false, true] {
                let mut reader =
                    FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
                reader.add_match(b"COMMON=all");
                if backward {
                    reader.seek_tail();
                }
                let mut failed = false;
                for _ in 0..1006 {
                    match if backward {
                        reader.previous()
                    } else {
                        reader.next()
                    } {
                        Err(_) => {
                            failed = true;
                            break;
                        }
                        Ok(false) => break,
                        Ok(true) => {}
                    }
                }
                assert!(failed, "{compact}/{pointer}: incomplete exact result");
                assert!(!reader.recovery_status().incomplete());
            }
            for mode in ["filter", "facet", "histogram"] {
                let mut reader =
                    FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
                let mut query = ExplorerQuery {
                    limit: 0,
                    field_mode: ExplorerFieldMode::AllValues,
                    use_source_realtime: false,
                    ..Default::default()
                };
                match mode {
                    "filter" => {
                        query.limit = 1;
                        query
                            .filters
                            .push(ExplorerFilter::new(b"COMMON", [b"all".as_slice()]));
                    }
                    "facet" => query.facets.push(b"COMMON".to_vec()),
                    _ => query.histogram = Some(b"COMMON".to_vec()),
                }
                assert!(
                    reader
                        .explore_with_strategy(&query, ExplorerStrategy::Index)
                        .is_err(),
                    "{compact}/{pointer}/{mode}: incomplete indexed result"
                );
                assert!(!reader.recovery_status().incomplete());
            }
        }
    }
}

#[test]
fn first_exact_filter_recovers_after_its_data_guard_drops() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let mut bytes = hardening_fixture(&path, compact);
        let arrays = array_graph(&bytes, compact);
        let last = *arrays.last().unwrap().1.last().unwrap() as usize;
        bytes[last..last + 16].fill(0);
        let payload_start = if compact { 72 } else { 64 };
        let mut pair = u64_at(&bytes, 88) as usize;
        loop {
            let size = u64_at(&bytes, pair + 8) as usize;
            if bytes[pair] == 1
                && size >= payload_start
                && &bytes[pair + payload_start..pair + size] == b"TAIL_PAIR=some"
            {
                break;
            }
            assert!(size >= 16 && pair + size < bytes.len());
            pair += (size + 7) & !7;
        }
        bytes[pair + 48..pair + 56].fill(0);
        std::fs::write(&path, bytes).unwrap();
        for combined in [false, true] {
            for backward in [false, true] {
                let mut reader =
                    FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
                assert!(!reader.recovery_status().incomplete());
                if combined {
                    reader.add_match(b"COMMON=all");
                }
                reader.add_match(b"TAIL_PAIR=some");
                if backward {
                    reader.seek_tail();
                }
                assert!(
                    if backward {
                        reader.previous()
                    } else {
                        reader.next()
                    }
                    .unwrap()
                );
                assert_eq!(reader.get_realtime_usec().unwrap(), 2003);
                assert!(
                    !if backward {
                        reader.previous()
                    } else {
                        reader.next()
                    }
                    .unwrap()
                );
                assert_eq!(reader.recovery_status().recovered_entries, 1004);
            }
        }
    }
}

#[test]
fn recovered_sparse_eof_preserves_matching_position_and_repair() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let original = hardening_fixture(&path, compact);
        let last = *array_graph(&original, compact)
            .last()
            .unwrap()
            .1
            .last()
            .unwrap() as usize;
        let mut damaged = original.clone();
        damaged[last..last + 16].fill(0);
        std::fs::write(&path, damaged).unwrap();
        let mut reader = FileReader::open(&path).unwrap();
        reader.add_match(b"SPARSE=some");
        for time in [1000, 1500] {
            assert!(reader.next().unwrap());
            assert_eq!(reader.get_realtime_usec().unwrap(), time);
        }
        assert!(!reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 1500);
        assert!(reader.previous().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 1000);
        assert!(reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 1500);
        assert!(!reader.next().unwrap());

        use std::io::{Seek, SeekFrom, Write};
        let mut repair = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        repair.seek(SeekFrom::Start(last as u64)).unwrap();
        repair.write_all(&original[last..last + 16]).unwrap();
        assert!(!reader.next().unwrap());
        reader.seek_realtime(1501);
        assert!(reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 2004);
        assert!(!reader.next().unwrap());
        assert!(!reader.recovery_status().incomplete());
    }
}

#[test]
fn wrong_entry_type_is_an_error_without_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("system.journal");
    let mut bytes = hardening_fixture(&path, false);
    let entry = array_graph(&bytes, false)[0].1[0] as usize;
    bytes[entry] = 2;
    std::fs::write(&path, &bytes).unwrap();
    let mut reader = FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
    assert!(reader.next().is_err());
    assert!(!reader.recovery_status().incomplete());
}

#[test]
fn recovered_eof_rechecks_changed_count_and_direction() {
    for filtered in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let source = hardening_fixture(&path, false);
        let last = *array_graph(&source, false)
            .last()
            .unwrap()
            .1
            .last()
            .unwrap() as usize;
        let mut damaged = source.clone();
        damaged[last..last + 16].fill(0);
        std::fs::write(&path, &damaged).unwrap();
        let mut reader = FileReader::open(&path).unwrap();
        if filtered {
            reader.add_match(b"COMMON=all");
        }
        let mut count = 0;
        while reader.next().unwrap() {
            count += 1;
        }
        assert_eq!(count, 1004);
        for _ in 0..10 {
            assert!(!reader.next().unwrap());
        }
        assert_eq!(std::fs::read(&path).unwrap(), damaged);
        assert!(reader.previous().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 2002);
        assert!(reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 2003);
        assert!(!reader.next().unwrap());

        // Synthetic restoration alone must not restart unchanged EOF polling.
        // The actual append below publishes a new declared count.
        std::fs::write(&path, &source).unwrap();
        assert!(!reader.next().unwrap());
        let repo = RepoFile::from_path(&path).unwrap();
        let mut file = JournalFile::<MmapMut>::open_for_append(&repo, 8 * 1024 * 1024).unwrap();
        let mut writer = JournalWriter::new(&mut file, 1006, test_uuid(4)).unwrap();
        writer
            .add_entry(&mut file, &[b"COMMON=all"], 3000, 3000)
            .unwrap();
        file.sync().unwrap();
        for expected in [2004, 3000] {
            assert!(reader.next().unwrap());
            assert_eq!(reader.get_realtime_usec().unwrap(), expected);
        }
        assert!(!reader.next().unwrap());
        assert!(!reader.recovery_status().incomplete());
    }
}

#[test]
fn recovery_rejects_interior_holes_and_out_of_arena_pointers() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let source = hardening_fixture(&path, compact);
        let arrays = array_graph(&source, compact);
        let last = *arrays.last().unwrap().1.last().unwrap() as usize;
        for corruption in ["zero-entry", "zero-slot", "arena-pointer", "nonzero-header"] {
            let mut bytes = source[..last].to_vec();
            let first = arrays[0].1[0] as usize;
            match corruption {
                "zero-entry" => bytes[first..first + 16].fill(0),
                "nonzero-header" => {
                    bytes[first..first + 16].fill(0);
                    bytes[first + 1] = 1;
                }
                "zero-slot" => {
                    bytes[arrays[0].0 + 24..arrays[0].0 + 24 + if compact { 4 } else { 8 }].fill(0)
                }
                _ => {
                    let bad = u64_at(&source, 88) + u64_at(&source, 96) + 8;
                    let slot = arrays[0].0 + 24;
                    if compact {
                        bytes[slot..slot + 4].copy_from_slice(&(bad as u32).to_le_bytes());
                    } else {
                        bytes[slot..slot + 8].copy_from_slice(&bad.to_le_bytes());
                    }
                }
            }
            std::fs::write(&path, bytes).unwrap();
            match FileReader::open_with_options(&path, ReaderOptions::snapshot()) {
                Err(_) => (),
                Ok(mut reader) => assert!(reader.next().is_err(), "{compact}/{corruption}"),
            }
        }
    }
}

#[test]
fn same_size_repair_restores_empty_exact_match_and_tail_once() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let bytes = hardening_fixture(&path, compact);
        let arrays = array_graph(&bytes, compact);
        let last = *arrays.last().unwrap().1.last().unwrap() as usize;
        for zero_slot in [false, true] {
            let mut damaged = bytes.clone();
            if zero_slot {
                let (array, entries) = arrays.last().unwrap();
                let item = if compact { 4 } else { 8 };
                let slot = array + 24 + (entries.len() - 1) * item;
                damaged[slot..slot + item].fill(0);
            } else {
                damaged[last..last + 16].fill(0);
            }
            std::fs::write(&path, damaged).unwrap();
            let mut reader = FileReader::open(&path).unwrap();
            let mut rows = 0;
            while reader.next().unwrap() {
                rows += 1;
            }
            assert_eq!(rows, 1004);
            reader.add_match(b"MESSAGE=row-1004");
            reader.seek_head();
            assert!(!reader.next().unwrap(), "{compact}/{zero_slot}");
            assert!(reader.recovery_status().incomplete());
            assert!(!reader.next().unwrap());
            std::fs::write(&path, &bytes).unwrap();
            assert!(!reader.next().unwrap(), "unchanged terminal count");
            reader.seek_head();
            assert!(
                reader.next().unwrap(),
                "{compact}/{zero_slot}: explicit seek"
            );
            assert_eq!(reader.get_realtime_usec().unwrap(), 2004);
            assert!(!reader.recovery_status().incomplete());
            assert!(!reader.next().unwrap());
        }
    }
}

#[test]
fn realtime_seek_uses_requested_direction_and_range_with_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("system.journal");
    let mut bytes = hardening_fixture(&path, false);
    let arrays = array_graph(&bytes, false);
    for (index, &entry) in arrays.iter().flat_map(|(_, rows)| rows).enumerate() {
        let at = entry as usize + 24;
        bytes[at..at + 8].copy_from_slice(&(1000 + 2 * index as u64).to_le_bytes());
    }
    for damaged in [false, true] {
        let cut = if damaged {
            *arrays.last().unwrap().1.last().unwrap() as usize
        } else {
            bytes.len()
        };
        std::fs::write(&path, &bytes[..cut]).unwrap();
        let mut reader = FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
        for filtered in [false, true] {
            if filtered {
                reader.add_match(b"COMMON=all");
            }
            for (time, forward, expected) in [
                (1001, true, Some(1002)),
                (1001, false, Some(1000)),
                (999, false, None),
                (999, true, Some(1000)),
                (4000, true, None),
                (4000, false, Some(if damaged { 3006 } else { 3008 })),
            ] {
                reader.seek_realtime(time);
                let found = if forward {
                    reader.next().unwrap()
                } else {
                    reader.previous().unwrap()
                };
                assert_eq!(
                    found,
                    expected.is_some(),
                    "{damaged}/{filtered}/{time}/{forward}"
                );
                if let Some(time) = expected {
                    assert_eq!(reader.get_realtime_usec().unwrap(), time);
                }
            }
        }
    }
}

#[test]
fn directory_repair_preserves_pending_merge_rows() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a.journal");
        let b = temp.path().join("b.journal");
        let repo = RepoFile::from_path(&a).unwrap();
        let mut file = JournalFile::<MmapMut>::create(
            &repo,
            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3)).with_compact(compact),
        )
        .unwrap();
        let mut writer = JournalWriter::new(&mut file, 1, test_uuid(4)).unwrap();
        writer
            .add_entry(&mut file, &[b"MESSAGE=1"], 1000, 1)
            .unwrap();
        writer
            .add_entry(&mut file, &[b"MESSAGE=3"], 3000, 3)
            .unwrap();
        file.sync().unwrap();
        drop(writer);
        drop(file);
        let mut bytes = std::fs::read(&a).unwrap();
        let last = array_graph(&bytes, compact)[0].1[1] as usize;
        bytes[last + 16..last + 24].copy_from_slice(&3u64.to_le_bytes());
        bytes[160..168].copy_from_slice(&3u64.to_le_bytes());
        std::fs::write(&a, &bytes[..last]).unwrap();
        write_single_entry_journal(&b, 2, 2000, 2, b"MESSAGE=2");
        let ar = FileReader::open(&a).unwrap();
        let br = FileReader::open(&b).unwrap();
        let mut directory = DirectoryReader::from_readers(vec![br, ar], false).unwrap();
        assert!(!directory.non_overlapping);
        assert!(directory.next().unwrap());
        assert_eq!(directory.get_realtime_usec().unwrap(), 1000);
        std::fs::write(&a, bytes).unwrap();
        let mut times = Vec::new();
        while directory.next().unwrap() {
            times.push(directory.get_realtime_usec().unwrap());
        }
        assert_eq!(times, vec![2000, 3000]);
        assert!(directory.recovery_statuses().is_empty());
        let boots = directory.list_boots();
        assert_eq!(boots.len(), 1);
        assert_eq!(boots[0].last_entry, 3000);
        directory.seek_tail();
        times.clear();
        while directory.previous().unwrap() {
            times.push(directory.get_realtime_usec().unwrap());
        }
        assert_eq!(times, vec![3000, 2000, 1000]);
    }
}

#[test]
fn recovery_preserves_historical_no_rtc_population() {
    let mut count = 0;
    for entry in std::fs::read_dir(repo_root().join("fixtures/systemd/test-data/no-rtc")).unwrap() {
        let path = entry.unwrap().path();
        if !is_journal_file_name(&path) {
            continue;
        }
        let mut reader = FileReader::open_with_options(path, ReaderOptions::snapshot()).unwrap();
        while reader.next().unwrap() {
            reader.get_entry().unwrap();
            count += 1;
        }
    }
    assert_eq!(count, 10757);
}

#[test]
fn snapshot_observed_shrink_and_repair_preserve_original_extent() {
    use std::io::{Seek, SeekFrom, Write};
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let mut bytes = hardening_fixture(&path, compact);
        let last = *array_graph(&bytes, compact)
            .last()
            .unwrap()
            .1
            .last()
            .unwrap();
        let mut reader = FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
        reader.add_match(b"SPARSE=some");
        reader.seek_tail();
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(last + 8).unwrap();
        assert!(reader.previous().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 1500);
        assert_eq!(reader.recovery_status().recovered_entries, 1004);
        let original_size = bytes.len() as u64;
        bytes.extend_from_slice(&[0; 4096]);
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&bytes).unwrap();
        assert!(reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 2004);
        assert!(!reader.recovery_status().incomplete());
        assert_eq!(
            reader
                .inner
                .with_file(|file| file.reader_file_size().unwrap()),
            original_size
        );
    }
}

#[test]
fn snapshot_cold_recovery_bounds_retained_entry_headers_by_physical_extent() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let bytes = hardening_fixture(&path, compact);
        let last = *array_graph(&bytes, compact)
            .last()
            .unwrap()
            .1
            .last()
            .unwrap();
        let mut reader = FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(last + 64)
            .unwrap();
        assert!(reader.inner.with_file(|file| {
            file.recover_reader_tail(&JournalError::ObjectExceedsFileBounds)
                .unwrap()
        }));
        assert_eq!(reader.recovery_status().recovered_entries, 1004);
        reader.add_match(b"SPARSE=some");
        reader.seek_tail();
        assert!(reader.previous().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 1500);
    }
}

enum PositionTestReader {
    Sdk(FileReader),
    Directory(DirectoryReader),
    Facade(SdJournal),
}

impl PositionTestReader {
    fn open(path: &Path, facade: bool) -> Self {
        Self::open_bounds(path, facade, false)
    }

    fn open_bounds(path: &Path, facade: bool, live: bool) -> Self {
        let options = if live {
            ReaderOptions::default()
        } else {
            ReaderOptions::snapshot()
        };
        if path.is_dir() {
            return if facade {
                Self::Facade(
                    SdJournalOpenDirectoryWithOptions(path.to_str().unwrap(), 0, options).unwrap(),
                )
            } else {
                Self::Directory(DirectoryReader::open_with_options(path, options).unwrap())
            };
        }
        if facade {
            Self::Facade(SdJournalOpenFileWithOptions(path.to_str().unwrap(), 0, options).unwrap())
        } else {
            Self::Sdk(FileReader::open_with_options(path, options).unwrap())
        }
    }

    fn seqnum(&mut self) -> u64 {
        match self {
            Self::Sdk(reader) => reader.get_entry().unwrap().seqnum,
            Self::Directory(reader) => reader.get_seqnum().unwrap().0,
            Self::Facade(reader) => SdJournalGetSeqnum(reader).unwrap().0,
        }
    }

    fn seek_head(&mut self) {
        match self {
            Self::Sdk(reader) => reader.seek_head(),
            Self::Directory(reader) => reader.seek_head(),
            Self::Facade(reader) => SdJournalSeekHead(reader).unwrap(),
        }
    }

    fn step(&mut self, backward: bool) -> std::result::Result<bool, String> {
        match self {
            Self::Directory(reader) => {
                let result = if backward {
                    reader.previous()
                } else {
                    reader.next()
                };
                result.map_err(|err| format!("{err:?}"))
            }
            Self::Sdk(reader) => {
                let result = if backward {
                    reader.previous()
                } else {
                    reader.next()
                };
                result.map_err(|err| format!("{err:?}"))
            }
            Self::Facade(reader) => {
                let result = if backward {
                    SdJournalPrevious(reader)
                } else {
                    SdJournalNext(reader)
                };
                result
                    .map(|count| count != 0)
                    .map_err(|err| format!("{err:?}"))
            }
        }
    }

    fn time(&self) -> u64 {
        match self {
            Self::Sdk(reader) => reader.get_realtime_usec().unwrap(),
            Self::Directory(reader) => reader.get_realtime_usec().unwrap(),
            Self::Facade(reader) => SdJournalGetRealtimeUsec(reader).unwrap(),
        }
    }

    fn want(&mut self, backward: bool, time: u64) {
        assert!(
            self.step(backward).unwrap(),
            "backward={backward}, want {time}"
        );
        assert_eq!(self.time(), time, "backward={backward}");
    }

    fn add_match(&mut self, value: &[u8]) {
        match self {
            Self::Sdk(reader) => reader.add_match(value),
            Self::Directory(reader) => reader.add_match(value),
            Self::Facade(reader) => SdJournalAddMatch(reader, value).unwrap(),
        }
    }

    fn add_disjunction(&mut self) {
        match self {
            Self::Sdk(reader) => reader.add_disjunction().unwrap(),
            Self::Directory(reader) => reader.add_disjunction().unwrap(),
            Self::Facade(reader) => SdJournalAddDisjunction(reader).unwrap(),
        }
    }

    fn seek_tail(&mut self) {
        match self {
            Self::Sdk(reader) => reader.seek_tail(),
            Self::Directory(reader) => reader.seek_tail(),
            Self::Facade(reader) => SdJournalSeekTail(reader).unwrap(),
        }
    }

    fn flush_matches(&mut self) {
        match self {
            Self::Sdk(reader) => reader.flush_matches(),
            Self::Directory(reader) => reader.flush_matches(),
            Self::Facade(reader) => SdJournalFlushMatches(reader).unwrap(),
        }
    }

    fn seek_realtime(&mut self, time: u64) {
        match self {
            Self::Sdk(reader) => reader.seek_realtime(time),
            Self::Directory(reader) => reader.seek_realtime(time),
            Self::Facade(reader) => SdJournalSeekRealtimeUsec(reader, time).unwrap(),
        }
    }

    fn cursor(&self) -> String {
        match self {
            Self::Sdk(reader) => reader.get_cursor().unwrap(),
            Self::Directory(reader) => reader.get_cursor().unwrap(),
            Self::Facade(reader) => SdJournalGetCursor(reader).unwrap(),
        }
    }

    fn message(&mut self) -> Vec<u8> {
        match self {
            Self::Sdk(reader) => reader.get_entry_payload(b"MESSAGE").unwrap().unwrap(),
            Self::Directory(reader) => reader.get_entry_payload(b"MESSAGE").unwrap().unwrap(),
            Self::Facade(reader) => SdJournalGetData(reader, "MESSAGE").unwrap(),
        }
    }
}

#[test]
fn directory_filter_replacement_keeps_sequential_discovery() {
    use std::io::{Seek, SeekFrom, Write};

    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..3 {
            let path = temp.path().join(format!("{index}.journal"));
            let repo = RepoFile::from_path(&path).unwrap();
            let mut file = JournalFile::<MmapMut>::create(
                &repo,
                JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                    .with_compact(compact),
            )
            .unwrap();
            let mut writer = JournalWriter::new(&mut file, 3 * index + 1, test_uuid(4)).unwrap();
            for row in 1..=3 {
                let time = (3 * index + row) * 1000;
                writer
                    .add_entry(&mut file, &[b"COMMON=all"], time, time)
                    .unwrap();
            }
            file.sync().unwrap();
        }
        let mut reader = DirectoryReader::open(temp.path()).unwrap();
        assert!(reader.non_overlapping);
        reader.add_match(b"COMMON=all");
        for time in [1000, 2000, 3000, 4000] {
            assert!(reader.next().unwrap());
            assert_eq!(reader.get_realtime_usec().unwrap(), time);
        }
        let path = temp.path().join("2.journal");
        let source = std::fs::read(&path).unwrap();
        let offset = array_graph(&source, compact)[0].1[0];
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(&[1]).unwrap();
        reader.add_match(b"COMMON=all");
        for time in [5000, 6000] {
            assert!(reader.next().unwrap(), "nearer healthy step {time}");
            assert_eq!(reader.get_realtime_usec().unwrap(), time);
        }
        assert!(reader.next().is_err());
        assert_eq!(reader.get_realtime_usec().unwrap(), 6000);
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(&[3]).unwrap();
        assert!(reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 7000);
    }
}

fn directory_filter_boundary_fixture(path: &Path, compact: bool, merged: bool) {
    let times = if merged {
        [[1000, 3000, 5000], [2000, 4000, 6000]]
    } else {
        [[1000, 2000, 3000], [4000, 5000, 6000]]
    };
    for (index, rows) in times.iter().enumerate() {
        let path = path.join(format!("{index}.journal"));
        let repo = RepoFile::from_path(&path).unwrap();
        let mut file = JournalFile::<MmapMut>::create(
            &repo,
            JournalFileOptions::new(
                test_uuid(1),
                test_uuid(2),
                test_uuid(if merged { 3 + index as u8 } else { 3 }),
            )
            .with_compact(compact),
        )
        .unwrap();
        let mut writer = JournalWriter::new(
            &mut file,
            if merged { 1 } else { 3 * index as u64 + 1 },
            test_uuid(4),
        )
        .unwrap();
        for &time in rows {
            let message = format!("MESSAGE=row-{time}");
            writer
                .add_entry(&mut file, &[message.as_bytes(), b"COMMON=all"], time, time)
                .unwrap();
        }
        file.sync().unwrap();
    }
}

#[test]
fn directory_filter_mutation_keeps_emitted_anchor() {
    for compact in [false, true] {
        for merged in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            directory_filter_boundary_fixture(temp.path(), compact, merged);
            for facade in [false, true] {
                for backward in [false, true] {
                    for mutation in ["install", "replace", "and", "or"] {
                        let mut reader = PositionTestReader::open(temp.path(), facade);
                        if mutation == "replace" {
                            reader.add_match(b"COMMON=all");
                        }
                        for time in [1000, 2000, 3000] {
                            reader.want(false, time);
                        }
                        if mutation == "replace" {
                            reader.add_match(b"MESSAGE=row-1000");
                            reader.add_match(b"MESSAGE=row-5000");
                        } else if mutation == "or" {
                            reader.add_match(b"MESSAGE=row-1000");
                        } else {
                            reader.add_match(b"COMMON=all");
                        }
                        if mutation == "and" {
                            match &mut reader {
                                PositionTestReader::Directory(directory) => {
                                    directory.add_conjunction().unwrap();
                                }
                                PositionTestReader::Facade(facade) => {
                                    SdJournalAddConjunction(facade).unwrap();
                                }
                                PositionTestReader::Sdk(_) => unreachable!(),
                            }
                            reader.add_match(b"MESSAGE=row-1000");
                            reader.add_match(b"MESSAGE=row-5000");
                        } else if mutation == "or" {
                            reader.add_disjunction();
                            reader.add_match(b"MESSAGE=row-5000");
                        }
                        assert_eq!(reader.time(), 3000, "mutation changed selected row");
                        let times: &[u64] = if mutation != "install" {
                            if backward { &[1000] } else { &[5000] }
                        } else if backward {
                            &[2000, 1000]
                        } else {
                            &[4000, 5000, 6000]
                        };
                        for &time in times {
                            assert!(
                                reader.step(backward).unwrap(),
                                "compact={compact}/merge={merged}/facade={facade}/back={backward}/{mutation}"
                            );
                            assert_eq!(
                                reader.time(),
                                time,
                                "compact={compact}/merge={merged}/facade={facade}/back={backward}/{mutation}"
                            );
                        }
                        assert!(!reader.step(backward).unwrap());
                    }
                }
            }
        }
    }
}

#[test]
fn directory_filter_mutation_keeps_failed_realtime_seek() {
    use std::io::{Seek, SeekFrom, Write};
    for compact in [false, true] {
        for merged in [false, true] {
            for facade in [false, true] {
                for backward in [false, true] {
                    let temp = tempfile::tempdir().unwrap();
                    directory_filter_boundary_fixture(temp.path(), compact, merged);
                    let mut reader = PositionTestReader::open(temp.path(), facade);
                    reader.add_match(b"COMMON=all");
                    reader.seek_realtime(3500);
                    let path = temp
                        .path()
                        .join(if backward { "0.journal" } else { "1.journal" });
                    let want = if backward { 3000 } else { 4000 };
                    let source = std::fs::read(&path).unwrap();
                    let offset = array_graph(&source, compact)
                        .into_iter()
                        .flat_map(|(_, entries)| entries)
                        .find(|&offset| u64_at(&source, offset as usize + 24) == want)
                        .unwrap();
                    let patch = |typ| {
                        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
                        file.seek(SeekFrom::Start(offset)).unwrap();
                        file.write_all(&[typ]).unwrap();
                    };
                    patch(1);
                    assert!(reader.step(backward).is_err());
                    reader.add_match(b"COMMON=all");
                    patch(3);
                    assert!(reader.step(backward).unwrap());
                    assert_eq!(
                        reader.time(),
                        want,
                        "{compact}/{merged}/{facade}/{backward}"
                    );
                }
            }
        }
    }
}

#[test]
fn directory_filter_flush_discards_pending_realtime_seek() {
    for compact in [false, true] {
        for merged in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            directory_filter_boundary_fixture(temp.path(), compact, merged);
            for facade in [false, true] {
                let mut reader = PositionTestReader::open(temp.path(), facade);
                if let PositionTestReader::Directory(directory) = &reader {
                    assert_eq!(directory.non_overlapping, !merged);
                }
                reader.add_match(b"COMMON=all");
                reader.seek_realtime(5500);
                reader.flush_matches();
                reader.want(false, 1000);
            }
        }
    }
}

#[test]
fn directory_filter_eof_reversal_keeps_nearest_match() {
    for compact in [false, true] {
        for merged in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            directory_filter_boundary_fixture(temp.path(), compact, merged);
            for facade in [false, true] {
                for backward in [false, true] {
                    let mut reader = PositionTestReader::open(temp.path(), facade);
                    if let PositionTestReader::Directory(directory) = &reader {
                        assert_eq!(directory.non_overlapping, !merged);
                    }
                    reader.add_match(b"COMMON=all");
                    if backward {
                        reader.seek_tail();
                    }
                    for i in 1..=6 {
                        reader.want(backward, if backward { 7000 - 1000 * i } else { 1000 * i });
                    }
                    assert!(!reader.step(backward).unwrap());
                    assert!(!reader.step(backward).unwrap());
                    reader.want(!backward, if backward { 2000 } else { 5000 });
                }
            }
        }
    }
}

#[test]
fn flush_matches_clears_selected_row() {
    let temp = tempfile::tempdir().unwrap();
    directory_filter_boundary_fixture(temp.path(), false, false);
    for directory in [false, true] {
        for facade in [false, true] {
            let file_path = temp.path().join("0.journal");
            let path = if directory { temp.path() } else { &file_path };
            let mut reader = PositionTestReader::open(path, facade);
            reader.add_match(b"COMMON=all");
            reader.want(false, 1000);
            reader.message();
            reader.flush_matches();
            match &mut reader {
                PositionTestReader::Sdk(reader) => {
                    assert!(reader.get_realtime_usec().is_err());
                    assert!(reader.get_cursor().is_err());
                    assert!(reader.get_entry().is_err());
                }
                PositionTestReader::Directory(reader) => {
                    assert!(reader.get_realtime_usec().is_err());
                    assert!(reader.get_cursor().is_err());
                    assert!(reader.get_entry().is_err());
                }
                PositionTestReader::Facade(reader) => {
                    assert!(SdJournalGetRealtimeUsec(reader).is_err());
                    assert!(SdJournalGetCursor(reader).is_err());
                    assert!(SdJournalGetData(reader, "MESSAGE").is_err());
                }
            }
            reader.want(false, 1000);
        }
    }
}

#[test]
fn snapshot_secondary_posting_shrink_returns_prefix_eof() {
    use std::io::{Seek, SeekFrom, Write};
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let source = hardening_fixture(&path, compact);
        let payload_start = if compact { 72 } else { 64 };
        let mut common = u64_at(&source, 88) as usize;
        loop {
            let size = u64_at(&source, common + 8) as usize;
            if source[common] == 1
                && size >= payload_start
                && &source[common + payload_start..common + size] == b"COMMON=all"
            {
                break;
            }
            assert!(size >= 16 && common + size < source.len());
            common += (size + 7) & !7;
        }
        let array = u64_at(&source, common + 48) as usize;
        let cut = array + 24 + if compact { 4 } else { 8 };
        assert_eq!(
            readable_prefix(&source, &array_graph(&source, compact), cut, compact),
            2
        );
        let mut reader = FileReader::open_with_options(&path, ReaderOptions::snapshot()).unwrap();
        reader.add_match(b"COMMON=all");
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(cut as u64).unwrap();
        assert!(reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 1000);
        assert!(reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 1001);
        assert!(!reader.next().unwrap());
        assert_eq!(reader.recovery_status().recovered_entries, 2);
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&source).unwrap();
        assert!(!reader.next().unwrap());
        reader.seek_realtime(1002);
        assert!(reader.next().unwrap());
        assert_eq!(reader.get_realtime_usec().unwrap(), 1002);
        assert!(!reader.recovery_status().incomplete());
    }
}

#[test]
fn directory_recovery_merge_error_retains_prefetched_row() {
    use std::io::{Seek, SeekFrom, Write};

    for compact in [false, true] {
        for facade in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let mut entries = Vec::new();
            for index in 0..3 {
                let path = temp.path().join(format!("{index}.journal"));
                let repo = RepoFile::from_path(&path).unwrap();
                let mut file = JournalFile::<MmapMut>::create(
                    &repo,
                    JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                        .with_compact(compact),
                )
                .unwrap();
                let mut writer =
                    JournalWriter::new(&mut file, 3 * index + 1, test_uuid(4)).unwrap();
                for row in 1..=3 {
                    let time = (3 * index + row) * 1000;
                    let message = format!("MESSAGE=row-{time}");
                    writer
                        .add_entry(&mut file, &[message.as_bytes(), b"COMMON=all"], time, time)
                        .unwrap();
                }
                file.sync().unwrap();
                drop(writer);
                drop(file);
                entries.push(
                    array_graph(&std::fs::read(path).unwrap(), compact)
                        .into_iter()
                        .flat_map(|(_, rows)| rows)
                        .collect::<Vec<_>>(),
                );
            }
            let mut reader = PositionTestReader::open(temp.path(), facade);
            if let PositionTestReader::Directory(directory) = &reader {
                assert!(directory.non_overlapping);
            }
            reader.add_match(b"COMMON=all");
            reader.seek_tail();
            for time in (4..=9).rev() {
                reader.want(true, time * 1000);
            }
            let cursor = reader.cursor();
            for (index, row, damage) in [(0, 2, vec![0; 16]), (2, 1, vec![2])] {
                let path = temp.path().join(format!("{index}.journal"));
                let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
                file.seek(SeekFrom::Start(entries[index][row])).unwrap();
                file.write_all(&damage).unwrap();
            }
            assert!(reader.step(true).is_err(), "merge probe must fail");
            assert_eq!(reader.time(), 4000, "selected row survives error");
            assert_eq!(reader.cursor(), cursor);
            let path = temp.path().join("2.journal");
            let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            file.seek(SeekFrom::Start(entries[2][1])).unwrap();
            file.write_all(&[3]).unwrap();
            reader.want(true, 2000);
            reader.want(true, 1000);
            assert!(!reader.step(true).unwrap());
        }
    }
}

#[test]
fn directory_position_after_entry_load_error_retries_in_order() {
    use std::io::{Seek, SeekFrom, Write};

    for compact in [false, true] {
        for facade in [false, true] {
            for filtered in [false, true] {
                for backward in [false, true] {
                    for merged in [false, true] {
                        let temp = tempfile::tempdir().unwrap();
                        let times = if merged {
                            [[1000, 3000, 5000], [2000, 4000, 6000]]
                        } else {
                            [[1000, 2000, 3000], [4000, 5000, 6000]]
                        };
                        let mut entries = Vec::new();
                        for (index, rows) in times.iter().enumerate() {
                            let path = temp.path().join(format!("{index}.journal"));
                            let repo = RepoFile::from_path(&path).unwrap();
                            let mut file = JournalFile::<MmapMut>::create(
                                &repo,
                                JournalFileOptions::new(
                                    test_uuid(1),
                                    test_uuid(2),
                                    test_uuid(if merged { 3 + index as u8 } else { 3 }),
                                )
                                .with_compact(compact),
                            )
                            .unwrap();
                            let mut writer = JournalWriter::new(
                                &mut file,
                                if merged { 1 } else { 3 * index as u64 + 1 },
                                test_uuid(4),
                            )
                            .unwrap();
                            for &time in rows {
                                let message = format!("MESSAGE=row-{time}");
                                writer
                                    .add_entry(
                                        &mut file,
                                        &[message.as_bytes(), b"COMMON=all"],
                                        time,
                                        time,
                                    )
                                    .unwrap();
                            }
                            file.sync().unwrap();
                            drop(writer);
                            drop(file);
                            entries.push(
                                array_graph(&std::fs::read(path).unwrap(), compact)
                                    .into_iter()
                                    .flat_map(|(_, rows)| rows)
                                    .collect::<Vec<_>>(),
                            );
                        }
                        let mut reader = PositionTestReader::open(temp.path(), facade);
                        if let PositionTestReader::Directory(directory) = &reader {
                            assert_eq!(directory.non_overlapping, !merged);
                        }
                        if filtered {
                            reader.add_match(b"COMMON=all");
                        }
                        let (before, target_file, target_row) = if merged {
                            (3000, 1, if backward { 0 } else { 1 })
                        } else if backward {
                            (4000, 0, 2)
                        } else {
                            (3000, 1, 0)
                        };
                        let initial_backward = if merged { !backward } else { backward };
                        if initial_backward {
                            reader.seek_tail();
                        }
                        loop {
                            assert!(reader.step(initial_backward).unwrap());
                            if reader.time() == before {
                                break;
                            }
                        }
                        let cursor = reader.cursor();
                        let message = format!("MESSAGE=row-{before}").into_bytes();
                        assert_eq!(reader.message(), message);
                        let path = temp.path().join(format!("{target_file}.journal"));
                        let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
                        let offset = entries[target_file][target_row];
                        file.seek(SeekFrom::Start(offset)).unwrap();
                        file.write_all(&[2]).unwrap();
                        assert!(
                            reader.step(backward).is_err(),
                            "{compact}/{facade}/{filtered}/{backward}/{merged}"
                        );
                        assert_eq!(reader.time(), before, "position while damaged");
                        assert_eq!(reader.cursor(), cursor);
                        assert_eq!(reader.message(), message);
                        file.seek(SeekFrom::Start(offset)).unwrap();
                        file.write_all(&[3]).unwrap();
                        assert_eq!(reader.time(), before, "position after repair");
                        assert_eq!(reader.cursor(), cursor);
                        assert_eq!(reader.message(), message);
                        for delta in [1000, 2000] {
                            reader.want(
                                backward,
                                if backward {
                                    before - delta
                                } else {
                                    before + delta
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn native_filter_reverse_anchor_keeps_nearest_match() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        hardening_fixture(&path, compact);
        for facade in [false, true] {
            for conjunction in [false, true] {
                for scenario in ["install", "install-eof", "filtered-eof"] {
                    let mut reader = PositionTestReader::open(&path, facade);
                    if scenario != "filtered-eof" {
                        for i in 0..1005 {
                            reader.want(false, 1000 + i);
                        }
                    }
                    if conjunction {
                        reader.add_match(b"COMMON=all");
                    }
                    reader.add_match(b"SPARSE=some");
                    if scenario == "filtered-eof" {
                        for time in [1000, 1500, 2004] {
                            reader.want(false, time);
                        }
                    }
                    if scenario != "install" {
                        assert!(
                            !reader.step(false).unwrap(),
                            "{compact}/{facade}/{conjunction}/{scenario}"
                        );
                    }
                    reader.want(true, 1500);
                    reader.want(false, 2004);
                    reader.want(true, 1500);
                    reader.want(true, 1000);
                    assert!(!reader.step(true).unwrap());
                    reader.want(false, 1500);
                }
            }
        }
    }
}

#[test]
fn directory_initial_entry_error_retries_pending_time_and_boundary() {
    use std::io::{Seek, SeekFrom, Write};

    for compact in [false, true] {
        for facade in [false, true] {
            for filtered in [false, true] {
                for scenario in ["head", "tail", "time-forward", "time-backward"] {
                    if !filtered && scenario.starts_with("time-") {
                        continue;
                    }
                    let temp = tempfile::tempdir().unwrap();
                    let mut entries = Vec::new();
                    for index in 0..2 {
                        let path = temp.path().join(format!("{index}.journal"));
                        let repo = RepoFile::from_path(&path).unwrap();
                        let mut file = JournalFile::<MmapMut>::create(
                            &repo,
                            JournalFileOptions::new(test_uuid(1), test_uuid(2), test_uuid(3))
                                .with_compact(compact),
                        )
                        .unwrap();
                        let mut writer =
                            JournalWriter::new(&mut file, 3 * index + 1, test_uuid(4)).unwrap();
                        for row in 0..3 {
                            let time = 1000 * (3 * index + row + 1);
                            writer
                                .add_entry(&mut file, &[b"COMMON=all"], time, time)
                                .unwrap();
                        }
                        file.sync().unwrap();
                        drop(writer);
                        drop(file);
                        entries.push(
                            array_graph(&std::fs::read(path).unwrap(), compact)
                                .into_iter()
                                .flat_map(|(_, rows)| rows)
                                .collect::<Vec<_>>(),
                        );
                    }
                    let mut reader = PositionTestReader::open(temp.path(), facade);
                    if filtered {
                        reader.add_match(b"COMMON=all");
                    }
                    let backward = matches!(scenario, "tail" | "time-backward");
                    let (file_index, row_index, want): (usize, usize, Vec<u64>) = match scenario {
                        "tail" => {
                            reader.seek_tail();
                            (1, 2, vec![6000, 5000, 4000, 3000, 2000, 1000])
                        }
                        "time-forward" => {
                            reader.seek_realtime(1500);
                            (1, 0, vec![2000, 3000, 4000, 5000, 6000])
                        }
                        "time-backward" => {
                            reader.seek_realtime(5500);
                            (1, 1, vec![5000, 4000, 3000, 2000, 1000])
                        }
                        _ => (0, 0, vec![1000, 2000, 3000, 4000, 5000, 6000]),
                    };
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .open(temp.path().join(format!("{file_index}.journal")))
                        .unwrap();
                    let offset = entries[file_index][row_index];
                    file.seek(SeekFrom::Start(offset)).unwrap();
                    file.write_all(&[2]).unwrap();
                    assert!(
                        reader.step(backward).is_err(),
                        "{compact}/{facade}/{filtered}/{scenario}"
                    );
                    assert!(match &reader {
                        PositionTestReader::Directory(reader) => reader.get_cursor().is_err(),
                        PositionTestReader::Facade(reader) => SdJournalGetCursor(reader).is_err(),
                        _ => unreachable!(),
                    });
                    file.seek(SeekFrom::Start(offset)).unwrap();
                    file.write_all(&[3]).unwrap();
                    for time in want {
                        reader.want(backward, time);
                    }
                    assert!(!reader.step(backward).unwrap());
                }
            }
        }
    }
}

#[test]
fn reader_position_after_entry_load_error_retries_same_candidate() {
    use std::io::{Seek, SeekFrom, Write};

    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("source.journal");
        let source = hardening_fixture(&source_path, compact);
        let offset = array_graph(&source, compact)
            .iter()
            .flat_map(|(_, entries)| entries.iter())
            .nth(500)
            .copied()
            .unwrap() as usize;
        for facade in [false, true] {
            for filtered in [false, true] {
                for backward in [false, true] {
                    for damage in ["type", "size"] {
                        let path = temp
                            .path()
                            .join(format!("{facade}-{filtered}-{backward}-{damage}.journal"));
                        std::fs::write(&path, &source).unwrap();
                        let mut reader = PositionTestReader::open(&path, facade);
                        let before = if filtered {
                            reader.add_match(b"SPARSE=some");
                            if backward {
                                reader.seek_tail();
                                2004
                            } else {
                                1000
                            }
                        } else {
                            let time = if backward { 1501 } else { 1499 };
                            reader.seek_realtime(time);
                            time
                        };
                        reader.want(backward, before);
                        let (at, damaged) = if damage == "type" {
                            (offset, vec![2])
                        } else {
                            (offset + 8, 8u64.to_le_bytes().to_vec())
                        };
                        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
                        file.seek(SeekFrom::Start(at as u64)).unwrap();
                        file.write_all(&damaged).unwrap();
                        assert!(
                            reader.step(backward).is_err(),
                            "{compact}/{facade}/{filtered}/{backward}/{damage}"
                        );
                        assert_eq!(reader.time(), before, "position while damaged");
                        file.seek(SeekFrom::Start(at as u64)).unwrap();
                        file.write_all(&source[at..at + damaged.len()]).unwrap();
                        assert_eq!(
                            reader.time(),
                            before,
                            "{compact}/{facade}/{filtered}/{backward}/{damage}"
                        );
                        reader.want(backward, 1500);
                    }
                }
            }
        }
    }
}

#[test]
fn native_filter_nested_direction_change_keeps_unvisited_branches() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        hardening_fixture(&path, compact);
        for facade in [false, true] {
            let mut reader = PositionTestReader::open(&path, facade);
            reader.seek_realtime(1500);
            reader.want(false, 1500);
            reader.add_match(b"MESSAGE=row-0500");
            reader.add_match(b"SPARSE=some");
            reader.add_disjunction();
            reader.add_match(b"TAIL_PAIR=some");
            reader.want(false, 2003);
            reader.want(true, 1500);
            reader.want(false, 2003);
            reader.want(false, 2004);
            reader.want(true, 2003);
            reader.want(true, 1500);
        }
    }
}

#[test]
fn resolved_backward_filter_skips_unvisited_leaf() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("system.journal");
    let mut source = hardening_fixture(&path, false);
    let mut common = u64_at(&source, 88) as usize;
    loop {
        let size = u64_at(&source, common + 8) as usize;
        if source[common] == 1 && &source[common + 64..common + size] == b"COMMON=all" {
            break;
        }
        common += (size + 7) & !7;
    }
    let array = u64_at(&source, common + 48) as usize;
    source[array] = 1;
    std::fs::write(&path, source).unwrap();
    for facade in [false, true] {
        let mut reader = PositionTestReader::open(&path, facade);
        reader.seek_realtime(1500);
        reader.want(false, 1500);
        reader.add_match(b"COMMON=all");
        reader.add_match(b"ZZ_ABSENT=x");
        assert!(!reader.step(true).unwrap());
        reader.seek_tail();
        assert!(reader.step(true).is_err());
    }
}

#[test]
fn explicit_filtered_time_seek_checks_only_visited_slots() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("source.journal");
        let source = hardening_fixture(&source_path, compact);
        let payload_start = if compact { 72 } else { 64 };
        let mut data = u64_at(&source, 88) as usize;
        loop {
            let size = u64_at(&source, data + 8) as usize;
            if source[data] == 1 && &source[data + payload_start..data + size] == b"COMMON=all" {
                break;
            }
            data += (size + 7) & !7;
        }
        let width = if compact { 4 } else { 8 };
        let mut array = u64_at(&source, data + 48) as usize;
        let first_slot = array + 24;
        let middle_slot =
            first_slot + ((u64_at(&source, array + 8) as usize - 24) / width / 2) * width;
        let mut remaining = u64_at(&source, data + 56) as usize - 1;
        let last_slot = loop {
            let count = remaining.min((u64_at(&source, array + 8) as usize - 24) / width);
            remaining -= count;
            if remaining == 0 {
                break array + 24 + (count - 1) * width;
            }
            array = u64_at(&source, array + 16) as usize;
        };
        for facade in [false, true] {
            for backward in [false, true] {
                for probed in [false, true] {
                    let path = temp
                        .path()
                        .join(format!("{facade}-{backward}-{probed}.journal"));
                    let mut damaged = source.clone();
                    let at = if probed {
                        middle_slot
                    } else if backward {
                        last_slot
                    } else {
                        first_slot
                    };
                    damaged[at..at + width].fill(0);
                    std::fs::write(&path, damaged).unwrap();
                    let mut reader = PositionTestReader::open(&path, facade);
                    reader.add_match(b"COMMON=all");
                    reader.seek_realtime(1500);
                    if probed {
                        use std::io::{Seek, SeekFrom, Write};
                        assert!(reader.step(backward).is_err(), "visited zero slot");
                        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
                        file.seek(SeekFrom::Start(at as u64)).unwrap();
                        file.write_all(&source[at..at + width]).unwrap();
                    }
                    reader.want(backward, 1500);
                    if !probed {
                        if backward {
                            reader.seek_tail();
                        } else {
                            reader.seek_head();
                            reader.want(false, 1000);
                        }
                        assert!(
                            reader.step(backward).is_err(),
                            "ordinary traversal must visit the damaged slot"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn native_filter_realtime_duplicate_bounds_choose_exact_entry() {
    for compact in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("system.journal");
        let mut source = hardening_fixture(&path, compact);
        let entries: Vec<_> = array_graph(&source, compact)
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .collect();
        for (index, offset) in entries.iter().copied().enumerate() {
            let at = offset as usize + 24;
            source[at..at + 8].copy_from_slice(&(1000 + 2 * (index as u64 / 7)).to_le_bytes());
        }
        source[192..200].copy_from_slice(&1286u64.to_le_bytes());
        std::fs::write(&path, source).unwrap();
        for live in [false, true] {
            for facade in [false, true] {
                let mut reader = PositionTestReader::open_bounds(&path, facade, live);
                reader.add_match(b"COMMON=all");
                for (time, backward, seqnum) in [
                    (0, false, 1),
                    (0, true, 0),
                    (999, true, 0),
                    (1000, false, 1),
                    (1000, true, 7),
                    (1001, false, 8),
                    (1001, true, 7),
                    (1142, false, 498),
                    (1142, true, 504),
                    (1143, false, 505),
                    (1143, true, 504),
                    (1286, false, 1002),
                    (1286, true, 1005),
                    (1287, false, 0),
                    (u64::MAX, false, 0),
                    (u64::MAX, true, 1005),
                ] {
                    reader.seek_realtime(time);
                    assert_eq!(
                        reader.step(backward).unwrap(),
                        seqnum != 0,
                        "{compact}/{live}/{facade}/{time}/{backward}"
                    );
                    if seqnum != 0 {
                        assert_eq!(
                            reader.seqnum(),
                            seqnum,
                            "{compact}/{live}/{facade}/{time}/{backward}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn directional_cursor_uses_recovered_prefix_without_mutation() {
    for compact in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("source.journal");
        let source = hardening_fixture(&source_path, compact);
        let arrays = array_graph(&source, compact);
        let cut = *arrays.last().unwrap().1.last().unwrap() as usize;
        let path = directory.path().join("cut.journal");
        std::fs::write(&path, &source[..cut]).unwrap();
        let mut reader = FileReader::open(&path).unwrap();
        reader.add_match(b"COMMON=all");
        reader
            .seek_cursor_direction("t=ffffffffffffffff", Direction::Backward)
            .unwrap();
        assert_eq!(reader.get_realtime_usec().unwrap(), 2003);
        assert!(reader.recovery_status().incomplete());
        assert_eq!(std::fs::read(&path).unwrap(), source[..cut]);
        let current = reader.get_cursor().unwrap();
        reader
            .seek_cursor_direction(&current, Direction::Forward)
            .unwrap();
        assert_eq!(reader.get_realtime_usec().unwrap(), 2003);
        assert!(!reader.next().unwrap());
    }
}
