use super::*;
use crate::file::{JournalFile, JournalFileOptions, JournalWriter};

fn stable_header_fixture(compact: bool, entries: u64) -> (JournalHeader, u64) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("header.journal");
    let repository = crate::repository::File::from_path(&path).unwrap();
    let identity = uuid::Uuid::from_bytes([1; 16]);
    let mut file = JournalFile::create(
        &repository,
        JournalFileOptions::new(identity, identity, identity).with_compact(compact),
    )
    .unwrap();
    let mut writer = JournalWriter::new(&mut file, 1, identity).unwrap();
    for i in 0..entries {
        writer
            .add_entry(&mut file, &[b"MESSAGE=header"], 1_000_000 + i, i + 1)
            .unwrap();
    }
    file.sync().unwrap();
    (
        *file.journal_header_ref(),
        std::fs::metadata(path).unwrap().len(),
    )
}

type HeaderEdit = fn(&mut JournalHeader);

#[test]
fn stable_header_rejects_population_contradictions() {
    let cases: &[(&str, HeaderEdit)] = &[
        ("objects exceed arena", |h| {
            h.n_objects = h.arena_size / 16 + 1
        }),
        ("entries exceed objects", |h| h.n_entries = h.n_objects + 1),
        ("data exceed objects", |h| h.n_data = h.n_objects + 1),
        ("fields exceed objects", |h| h.n_fields = h.n_objects + 1),
        ("tags exceed objects", |h| h.n_tags = h.n_objects + 1),
        ("arrays exceed objects", |h| {
            h.n_entry_arrays = h.n_objects + 1
        }),
        ("aggregate entries exceed objects", |h| {
            h.n_entries = h.n_objects
        }),
        ("aggregate data exceed objects", |h| h.n_data = h.n_objects),
        ("aggregate fields exceed objects", |h| {
            h.n_fields = h.n_objects
        }),
        ("aggregate tags exceed objects", |h| h.n_tags = h.n_objects),
        ("aggregate arrays exceed objects", |h| {
            h.n_entry_arrays = h.n_objects
        }),
        ("declared hash tables exceed objects", |h| h.n_data += 1),
        ("cached count exceeds entries", |h| {
            h.tail_entry_array_n_entries = h.n_entries as u32 + 1
        }),
        ("missing cached offset", |h| h.tail_entry_array_offset = 0),
        ("cached count without arrays", |h| {
            h.entry_array_offset = None;
            h.tail_entry_array_offset = 0;
        }),
        ("missing cached count", |h| h.tail_entry_array_n_entries = 0),
        ("missing first array", |h| h.entry_array_offset = None),
        ("tail array before first", |h| {
            h.tail_entry_array_offset = h.entry_array_offset.unwrap().get() as u32 - 8
        }),
    ];
    for compact in [false, true] {
        let (original, size) = stable_header_fixture(compact, 1);
        original.validated_arena_end(size).unwrap();
        for (name, edit) in cases {
            let mut header = original;
            edit(&mut header);
            assert!(
                header.validated_arena_end(size).is_err(),
                "accepted {name}, compact={compact}"
            );
            // Live-reader mapping checks must not impose stable population rules.
            header.validate_reader_mappings(size).unwrap();
        }
    }
}

#[test]
fn stable_header_historical_population_gates() {
    let (original, size) = stable_header_fixture(false, 1);
    let cases: &[(u64, HeaderEdit)] = &[
        (216, |h| h.n_data = h.n_objects),
        (224, |h| h.n_fields = h.n_objects),
        (232, |h| h.n_tags = h.n_objects),
        (240, |h| h.n_entry_arrays = h.n_objects),
        (264, |h| {
            h.tail_entry_array_n_entries = h.n_entries as u32 + 1
        }),
    ];
    for (end, edit) in cases {
        let mut header = original;
        edit(&mut header);
        header.header_size = end - 8;
        assert!(
            header.validated_arena_end(size).is_ok(),
            "checked absent field ending at {end}"
        );
        header.header_size = *end;
        assert!(
            header.validated_arena_end(size).is_err(),
            "ignored present field ending at {end}"
        );
    }
}

#[test]
fn stable_header_accepts_population_boundaries() {
    let (mut header, size) = stable_header_fixture(false, 1);
    header.n_objects = header.arena_size / 16;
    header.n_data = header.n_objects
        - header.n_entries
        - header.n_fields
        - header.n_tags
        - header.n_entry_arrays
        - 2;
    header.validated_arena_end(size).unwrap();
    header.entry_array_offset = None;
    header.tail_entry_array_offset = 0;
    header.tail_entry_array_n_entries = 0;
    header.n_entries = 0;
    header.n_entry_arrays = 0;
    header.validated_arena_end(size).unwrap();
}

// Both tables predate the optional category counters in historical headers.
#[test]
fn stable_header_hash_table_population_boundaries() {
    for compact in [false, true] {
        let (original, size) = stable_header_fixture(compact, 1);
        for header_size in [208, original.header_size] {
            for tables in 0..4 {
                let mut header = original;
                header.header_size = header_size;
                if header_size == 208 {
                    header.n_objects = 3; // One entry plus the two declared tables.
                }
                if tables & 1 == 0 {
                    header.data_hash_table_offset = None;
                    header.data_hash_table_size = None;
                    header.n_objects -= 1;
                }
                if tables & 2 == 0 {
                    header.field_hash_table_offset = None;
                    header.field_hash_table_size = None;
                    header.n_objects -= 1;
                }
                assert!(
                    header.validated_arena_end(size).is_ok(),
                    "rejected exact budget: compact={compact} header={header_size} tables={tables}"
                );
                header.n_objects -= 1;
                assert!(
                    header.validated_arena_end(size).is_err(),
                    "accepted insufficient budget: compact={compact} header={header_size} tables={tables}"
                );
            }
        }
    }
}

#[test]
fn stable_header_array_reference_population() {
    for compact in [false, true] {
        for entries in [1, 32] {
            let (original, size) = stable_header_fixture(compact, entries);
            assert_eq!(
                original.entry_array_offset.unwrap().get()
                    == u64::from(original.tail_entry_array_offset),
                entries == 1
            );
            for (header_size, single, multiple) in [
                (232, 0, 0),
                (240, 1, 1),
                (256, 1, 1),
                (264, 1, 2),
                (272, 1, 2),
            ] {
                let minimum = if entries == 1 { single } else { multiple };
                for count in 0..=original.n_entry_arrays {
                    let mut header = original;
                    header.header_size = header_size;
                    header.n_entry_arrays = count;
                    assert_eq!(
                        header.validated_arena_end(size).is_ok(),
                        count >= minimum,
                        "compact={compact} entries={entries} header={header_size} arrays={count} minimum={minimum}"
                    );
                    header.validate_reader_mappings(size).unwrap();
                }
            }
        }
    }
}
