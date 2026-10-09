use super::*;
use crate::file::{JournalFile, JournalFileOptions, JournalWriter};

fn stable_header_fixture(compact: bool) -> (JournalHeader, u64) {
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
    writer
        .add_entry(&mut file, &[b"MESSAGE=header"], 1_000_000, 1)
        .unwrap();
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
        ("aggregate entries exceed objects", |h| h.n_entries = h.n_objects),
        ("aggregate data exceed objects", |h| h.n_data = h.n_objects),
        ("aggregate fields exceed objects", |h| h.n_fields = h.n_objects),
        ("aggregate tags exceed objects", |h| h.n_tags = h.n_objects),
        ("aggregate arrays exceed objects", |h| h.n_entry_arrays = h.n_objects),
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
        let (original, size) = stable_header_fixture(compact);
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
    let (original, size) = stable_header_fixture(false);
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
    let (mut header, size) = stable_header_fixture(false);
    header.n_objects = header.arena_size / 16;
    header.n_data = header.n_objects
        - header.n_entries
        - header.n_fields
        - header.n_tags
        - header.n_entry_arrays;
    header.validated_arena_end(size).unwrap();
    header.entry_array_offset = None;
    header.tail_entry_array_offset = 0;
    header.tail_entry_array_n_entries = 0;
    header.n_entries = 0;
    header.validated_arena_end(size).unwrap();
}
