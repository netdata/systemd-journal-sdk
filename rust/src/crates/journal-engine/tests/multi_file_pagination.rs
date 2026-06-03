//! Integration tests for multi-file pagination with PaginationState.

use journal_common::Seconds;
use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter};
use journal_core::repository::File;
use journal_engine::logs::query::{LogEntryData, LogQuery, PaginationState};
use journal_index::{
    Anchor, Direction, FieldName, FieldValuePair, FileIndex, FileIndexer, Filter, Microseconds,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

/// Test journal entry specification
struct TestEntry {
    timestamp: Microseconds,
    fields: Vec<(String, String)>,
}

#[derive(Clone)]
struct PageSpec {
    anchor: Anchor,
    direction: Direction,
    limit: usize,
    after_usec: Option<u64>,
    before_usec: Option<u64>,
    filter: Option<Filter>,
}

impl TestEntry {
    fn new(timestamp: Microseconds) -> Self {
        Self {
            timestamp,
            fields: Vec::new(),
        }
    }

    fn with_field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push((name.into(), value.into()));
        self
    }
}

/// Create a test journal file path with a specific name
fn create_test_journal_path(temp_dir: &TempDir, filename: &str) -> PathBuf {
    let machine_id = Uuid::from_u128(0x12345678_1234_1234_1234_123456789abc);
    let machine_dir = temp_dir.path().join(machine_id.to_string());
    fs::create_dir_all(&machine_dir).expect("create machine dir");
    machine_dir.join(filename)
}

/// Helper to create a test journal file with specified entries
fn create_test_journal(
    temp_dir: &TempDir,
    filename: &str,
    entries: Vec<TestEntry>,
) -> Result<File, Box<dyn std::error::Error>> {
    let journal_path = create_test_journal_path(temp_dir, filename);

    let file =
        File::from_path(&journal_path).ok_or("Failed to create repository File from path")?;

    let machine_id = Uuid::from_u128(0x12345678_1234_1234_1234_123456789abc);
    let boot_id = Uuid::from_u128(0x11111111_1111_1111_1111_111111111111);
    let seqnum_id = Uuid::from_u128(0x22222222_2222_2222_2222_222222222222);

    let options = JournalFileOptions::new(machine_id, boot_id, seqnum_id);

    let mut journal_file = JournalFile::create(&file, options)?;
    let mut writer = JournalWriter::new(&mut journal_file, 1, boot_id)?;

    for entry in entries {
        let mut entry_data = Vec::new();

        // Add _SOURCE_REALTIME_TIMESTAMP first
        entry_data.push(format!("_SOURCE_REALTIME_TIMESTAMP={}", entry.timestamp.0).into_bytes());

        // Add all other fields
        for (field, value) in entry.fields {
            entry_data.push(format!("{}={}", field, value).into_bytes());
        }

        let entry_refs: Vec<&[u8]> = entry_data.iter().map(|v| v.as_slice()).collect();

        writer.add_entry(
            &mut journal_file,
            &entry_refs,
            entry.timestamp.0,
            entry.timestamp.0,
        )?;
    }

    Ok(file)
}

fn timestamp_range_entries(
    file_label: &str,
    start: u64,
    end: u64,
    with_file_field: bool,
    with_entry_id: bool,
) -> Vec<TestEntry> {
    (start..end)
        .map(|i| {
            let mut entry = TestEntry::new(Microseconds(i))
                .with_field("MESSAGE", format!("File{} Entry {}", file_label, i));
            if with_entry_id {
                entry = entry.with_field("ENTRY_ID", format!("file{}_{}", file_label, i));
            }
            if with_file_field {
                entry = entry.with_field("FILE", file_label);
            }
            entry
        })
        .collect()
}

fn same_timestamp_entries(
    file_label: &str,
    count: usize,
    timestamp: u64,
    with_file_field: bool,
) -> Vec<TestEntry> {
    (0..count)
        .map(|i| {
            let mut entry = TestEntry::new(Microseconds(timestamp))
                .with_field("MESSAGE", format!("File{} Entry {}", file_label, i))
                .with_field("ENTRY_ID", format!("file{}_{}", file_label, i));
            if with_file_field {
                entry = entry.with_field("FILE", file_label);
            }
            entry
        })
        .collect()
}

fn level_entries(file_label: &str, level: &str, start: u64, end: u64) -> Vec<TestEntry> {
    let file_label = file_label.to_string();
    let level_lower = level.to_ascii_lowercase();
    (start..end)
        .map(|i| {
            TestEntry::new(Microseconds(i))
                .with_field("MESSAGE", format!("File{} {} {}", file_label, level, i))
                .with_field(
                    "ENTRY_ID",
                    format!("file{}_{}_{}", file_label, level_lower, i),
                )
                .with_field("LEVEL", level)
        })
        .collect()
}

fn create_indexed_files(
    temp_dir: &TempDir,
    files: Vec<(&str, Vec<TestEntry>)>,
    indexed_fields: &[&str],
) -> Vec<FileIndex> {
    let mut indexer = FileIndexer::default();
    let source_timestamp_field = FieldName::new("_SOURCE_REALTIME_TIMESTAMP").unwrap();
    let indexed_fields: Vec<_> = indexed_fields
        .iter()
        .map(|field| FieldName::new(*field).unwrap())
        .collect();

    files
        .into_iter()
        .map(|(filename, entries)| {
            let file = create_test_journal(temp_dir, filename, entries).unwrap();
            indexer
                .index(
                    &file,
                    Some(&source_timestamp_field),
                    &indexed_fields,
                    Seconds(3600),
                )
                .unwrap()
        })
        .collect()
}

fn run_page(
    indexes: &[FileIndex],
    spec: &PageSpec,
    state: Option<&PaginationState>,
) -> (Vec<LogEntryData>, PaginationState) {
    let mut query = LogQuery::new(indexes, spec.anchor, spec.direction).with_limit(spec.limit);
    if let Some(after_usec) = spec.after_usec {
        query = query.with_after_usec(after_usec);
    }
    if let Some(before_usec) = spec.before_usec {
        query = query.with_before_usec(before_usec);
    }
    if let Some(filter) = &spec.filter {
        query = query.with_filter(filter.clone());
    }
    query.execute_page(state).unwrap()
}

fn page_spec(anchor: Anchor, direction: Direction, limit: usize) -> PageSpec {
    PageSpec {
        anchor,
        direction,
        limit,
        after_usec: None,
        before_usec: None,
        filter: None,
    }
}

fn bounded_page_spec(
    anchor: Anchor,
    direction: Direction,
    limit: usize,
    after_usec: u64,
    before_usec: u64,
) -> PageSpec {
    PageSpec {
        anchor,
        direction,
        limit,
        after_usec: Some(after_usec),
        before_usec: Some(before_usec),
        filter: None,
    }
}

fn filtered_page_spec(
    anchor: Anchor,
    direction: Direction,
    limit: usize,
    filter: Filter,
) -> PageSpec {
    PageSpec {
        anchor,
        direction,
        limit,
        after_usec: None,
        before_usec: None,
        filter: Some(filter),
    }
}

fn assert_ordered(page: &[LogEntryData], direction: Direction) {
    for pair in page.windows(2) {
        match direction {
            Direction::Forward => assert!(
                pair[0].timestamp <= pair[1].timestamp,
                "Entries should be in ascending timestamp order"
            ),
            Direction::Backward => assert!(
                pair[0].timestamp >= pair[1].timestamp,
                "Entries should be in descending timestamp order"
            ),
        }
    }
}

fn assert_page_bounds(page: &[LogEntryData], len: usize, first_ts: u64, last_ts: u64) {
    assert_eq!(page.len(), len, "page length should match");
    assert_eq!(page.first().unwrap().timestamp, first_ts);
    assert_eq!(page.last().unwrap().timestamp, last_ts);
}

fn assert_empty_page(indexes: &[FileIndex], spec: &PageSpec, state: &PaginationState) {
    let (page, _) = run_page(indexes, spec, Some(state));
    assert_eq!(page.len(), 0, "next page should be empty");
}

fn assert_all_timestamps(page: &[LogEntryData], timestamp: u64) {
    for entry in page {
        assert_eq!(
            entry.timestamp, timestamp,
            "All entries should have timestamp {timestamp}"
        );
    }
}

fn assert_timestamps_in_range(page: &[LogEntryData], after_usec: u64, before_usec: u64) {
    for entry in page {
        assert!(
            entry.timestamp >= after_usec && entry.timestamp < before_usec,
            "Entry timestamp {} should be within [{}, {})",
            entry.timestamp,
            after_usec,
            before_usec
        );
    }
}

fn collect_entry_ids<'a>(pages: impl IntoIterator<Item = &'a [LogEntryData]>) -> HashSet<String> {
    let mut ids = HashSet::new();
    for page in pages {
        for entry in page {
            for field in &entry.fields {
                if field.field() == "ENTRY_ID" {
                    assert!(
                        ids.insert(field.value().to_string()),
                        "Found duplicate ENTRY_ID: {}",
                        field.value()
                    );
                }
            }
        }
    }
    ids
}

fn assert_id_prefix_count(ids: &HashSet<String>, prefix: &str, expected: usize) {
    let count = ids.iter().filter(|id| id.starts_with(prefix)).count();
    assert_eq!(
        count, expected,
        "prefix {prefix} should have {expected} entries"
    );
}

fn assert_timestamps_present<'a>(
    pages: impl IntoIterator<Item = &'a [LogEntryData]>,
    range: std::ops::Range<u64>,
) {
    let mut timestamps = HashSet::new();
    for page in pages {
        for entry in page {
            timestamps.insert(entry.timestamp);
        }
    }
    for ts in range {
        assert!(timestamps.contains(&ts), "Missing timestamp: {}", ts);
    }
}

fn assert_unique_timestamp_count<'a>(
    pages: impl IntoIterator<Item = &'a [LogEntryData]>,
    expected: usize,
) {
    let mut timestamps = HashSet::new();
    for page in pages {
        for entry in page {
            assert!(
                timestamps.insert(entry.timestamp),
                "Found duplicate timestamp: {}",
                entry.timestamp
            );
        }
    }
    assert_eq!(timestamps.len(), expected, "unique timestamp count");
}

#[test]
fn test_multi_file_pagination_forward_non_overlapping() {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = create_indexed_files(
        &temp_dir,
        vec![
            (
                "file1.journal",
                timestamp_range_entries("1", 100, 200, true, false),
            ),
            (
                "file2.journal",
                timestamp_range_entries("2", 200, 300, true, false),
            ),
        ],
        &["FILE"],
    );
    let spec = page_spec(Anchor::Head, Direction::Forward, 150);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_page_bounds(&first_page, 150, 100, 249);
    assert_ordered(&first_page, Direction::Forward);
    assert!(
        !state1.file_positions.is_empty(),
        "State should track positions"
    );

    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_page_bounds(&second_page, 50, 250, 299);
    assert_unique_timestamp_count([first_page.as_slice(), second_page.as_slice()], 200);
    assert_empty_page(&file_indexes, &spec, &state2);
}

#[test]
fn test_multi_file_pagination_same_timestamps() {
    assert_same_timestamp_pages(Anchor::Head, Direction::Forward);
}

fn assert_same_timestamp_pages(anchor: Anchor, direction: Direction) {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = create_indexed_files(
        &temp_dir,
        vec![
            (
                "file1.journal",
                same_timestamp_entries("1", 150, 1000, true),
            ),
            (
                "file2.journal",
                same_timestamp_entries("2", 150, 1000, true),
            ),
        ],
        &["FILE", "ENTRY_ID"],
    );
    let spec = page_spec(anchor, direction, 200);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_eq!(first_page.len(), 200);
    assert_all_timestamps(&first_page, 1000);
    assert_eq!(
        state1.file_positions.len(),
        2,
        "State should track positions for both files"
    );

    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_eq!(second_page.len(), 100);
    assert_all_timestamps(&second_page, 1000);

    let ids = collect_entry_ids([first_page.as_slice(), second_page.as_slice()]);
    assert_eq!(ids.len(), 300, "Should have retrieved all entries");
    assert_id_prefix_count(&ids, "file1_", 150);
    assert_id_prefix_count(&ids, "file2_", 150);
    assert_empty_page(&file_indexes, &spec, &state2);
}

fn assert_limit_one_pages(anchor: Anchor, direction: Direction) {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = create_indexed_files(
        &temp_dir,
        vec![
            (
                "file1.journal",
                timestamp_range_entries("1", 100, 110, false, true),
            ),
            (
                "file2.journal",
                timestamp_range_entries("2", 110, 120, false, true),
            ),
        ],
        &["ENTRY_ID"],
    );
    let spec = page_spec(anchor, direction, 1);
    let mut all_ids = HashSet::new();
    let mut all_timestamps = Vec::new();
    let mut state = None;
    let mut page_count = 0;

    loop {
        let (page, new_state) = run_page(&file_indexes, &spec, state.as_ref());
        if page.is_empty() {
            break;
        }
        page_count += 1;
        assert_eq!(page.len(), 1, "Each page should have exactly 1 entry");
        all_timestamps.push(page[0].timestamp);
        for id in collect_entry_ids([page.as_slice()]) {
            assert!(all_ids.insert(id), "Found duplicate ENTRY_ID across pages");
        }
        state = Some(new_state);
    }

    assert_eq!(page_count, 20, "Should need exactly 20 pages");
    assert_eq!(all_ids.len(), 20, "Should have retrieved all entries");
    assert_timestamp_order(&all_timestamps, direction);
    let unique_timestamps: HashSet<_> = all_timestamps.into_iter().collect();
    assert_eq!(
        unique_timestamps.len(),
        20,
        "Should have 20 unique timestamps"
    );
    for ts in 100..120 {
        assert!(unique_timestamps.contains(&ts), "Missing timestamp: {}", ts);
    }
}

fn assert_timestamp_order(timestamps: &[u64], direction: Direction) {
    for pair in timestamps.windows(2) {
        match direction {
            Direction::Forward => assert!(
                pair[0] <= pair[1],
                "Timestamps should be in ascending order"
            ),
            Direction::Backward => assert!(
                pair[0] >= pair[1],
                "Timestamps should be in descending order"
            ),
        }
    }
}

fn two_file_non_overlapping_indexes(temp_dir: &TempDir, with_entry_id: bool) -> Vec<FileIndex> {
    create_indexed_files(
        temp_dir,
        vec![
            (
                "file1.journal",
                timestamp_range_entries("1", 100, 200, true, with_entry_id),
            ),
            (
                "file2.journal",
                timestamp_range_entries("2", 200, 300, true, with_entry_id),
            ),
        ],
        if with_entry_id {
            &["FILE", "ENTRY_ID"]
        } else {
            &["FILE"]
        },
    )
}

fn three_file_contiguous_indexes(temp_dir: &TempDir, with_file_field: bool) -> Vec<FileIndex> {
    create_indexed_files(
        temp_dir,
        vec![
            (
                "file1.journal",
                timestamp_range_entries("1", 100, 200, with_file_field, true),
            ),
            (
                "file2.journal",
                timestamp_range_entries("2", 200, 300, with_file_field, true),
            ),
            (
                "file3.journal",
                timestamp_range_entries("3", 300, 400, with_file_field, true),
            ),
        ],
        if with_file_field {
            &["FILE", "ENTRY_ID"]
        } else {
            &["ENTRY_ID"]
        },
    )
}

fn two_file_overlapping_indexes(temp_dir: &TempDir) -> Vec<FileIndex> {
    create_indexed_files(
        temp_dir,
        vec![
            (
                "file1.journal",
                timestamp_range_entries("1", 100, 200, true, true),
            ),
            (
                "file2.journal",
                timestamp_range_entries("2", 150, 250, true, true),
            ),
        ],
        &["FILE", "ENTRY_ID"],
    )
}

fn assert_non_overlapping_backward_pages() {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = two_file_non_overlapping_indexes(&temp_dir, false);
    let spec = page_spec(Anchor::Tail, Direction::Backward, 150);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_page_bounds(&first_page, 150, 299, 150);
    assert_ordered(&first_page, Direction::Backward);
    assert!(
        !state1.file_positions.is_empty(),
        "State should track positions"
    );

    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_page_bounds(&second_page, 50, 149, 100);
    assert_unique_timestamp_count([first_page.as_slice(), second_page.as_slice()], 200);
    assert_empty_page(&file_indexes, &spec, &state2);
}

fn assert_three_file_pages(direction: Direction) {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = three_file_contiguous_indexes(&temp_dir, true);
    let anchor = match direction {
        Direction::Forward => Anchor::Head,
        Direction::Backward => Anchor::Tail,
    };
    let spec = page_spec(anchor, direction, 125);
    let expected = match direction {
        Direction::Forward => [(125, 100, 224), (125, 225, 349), (50, 350, 399)],
        Direction::Backward => [(125, 399, 275), (125, 274, 150), (50, 149, 100)],
    };

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_page_bounds(&first_page, expected[0].0, expected[0].1, expected[0].2);
    assert_ordered(&first_page, direction);
    assert_eq!(state1.file_positions.len(), 2);

    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_page_bounds(&second_page, expected[1].0, expected[1].1, expected[1].2);
    assert_ordered(&second_page, direction);
    assert_eq!(state2.file_positions.len(), 3);

    let (third_page, state3) = run_page(&file_indexes, &spec, Some(&state2));
    assert_page_bounds(&third_page, expected[2].0, expected[2].1, expected[2].2);
    assert_ordered(&third_page, direction);

    let ids = collect_entry_ids([
        first_page.as_slice(),
        second_page.as_slice(),
        third_page.as_slice(),
    ]);
    assert_eq!(ids.len(), 300);
    assert_id_prefix_count(&ids, "file1_", 100);
    assert_id_prefix_count(&ids, "file2_", 100);
    assert_id_prefix_count(&ids, "file3_", 100);
    assert_empty_page(&file_indexes, &spec, &state3);
}

fn assert_overlapping_pages(direction: Direction) {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = two_file_overlapping_indexes(&temp_dir);
    let anchor = match direction {
        Direction::Forward => Anchor::Head,
        Direction::Backward => Anchor::Tail,
    };
    let spec = page_spec(anchor, direction, 120);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_eq!(first_page.len(), 120);
    assert_ordered(&first_page, direction);
    match direction {
        Direction::Forward => assert_eq!(first_page.first().unwrap().timestamp, 100),
        Direction::Backward => assert_eq!(first_page.first().unwrap().timestamp, 249),
    }
    assert!(
        !state1.file_positions.is_empty(),
        "State should track positions"
    );

    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_eq!(second_page.len(), 80);
    assert_ordered(&second_page, direction);
    match direction {
        Direction::Forward => {
            assert!(first_page.last().unwrap().timestamp <= second_page.first().unwrap().timestamp);
            assert_eq!(second_page.last().unwrap().timestamp, 249);
        }
        Direction::Backward => {
            assert!(first_page.last().unwrap().timestamp >= second_page.first().unwrap().timestamp);
            assert_eq!(second_page.last().unwrap().timestamp, 100);
        }
    }

    let ids = collect_entry_ids([first_page.as_slice(), second_page.as_slice()]);
    assert_eq!(ids.len(), 200);
    assert_id_prefix_count(&ids, "file1_", 100);
    assert_id_prefix_count(&ids, "file2_", 100);
    assert_timestamps_present([first_page.as_slice(), second_page.as_slice()], 100..250);
    assert_empty_page(&file_indexes, &spec, &state2);
}

fn paginate_until_empty(indexes: &[FileIndex], spec: &PageSpec) -> Vec<Vec<LogEntryData>> {
    let mut pages = Vec::new();
    let mut state = None;
    loop {
        let (page, new_state) = run_page(indexes, spec, state.as_ref());
        if page.is_empty() {
            break;
        }
        pages.push(page);
        state = Some(new_state);
    }
    pages
}

fn assert_pages_ordered(pages: &[Vec<LogEntryData>], direction: Direction) {
    for page in pages {
        assert_ordered(page, direction);
    }
    for pair in pages.windows(2) {
        let previous = pair[0].last().unwrap().timestamp;
        let next = pair[1].first().unwrap().timestamp;
        match direction {
            Direction::Forward => assert!(previous <= next),
            Direction::Backward => assert!(previous >= next),
        }
    }
}

fn page_slices(pages: &[Vec<LogEntryData>]) -> Vec<&[LogEntryData]> {
    pages.iter().map(Vec::as_slice).collect()
}

fn assert_small_limit_pages() {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = create_indexed_files(
        &temp_dir,
        vec![
            (
                "file1.journal",
                timestamp_range_entries("1", 100, 200, false, true),
            ),
            (
                "file2.journal",
                timestamp_range_entries("2", 200, 300, false, true),
            ),
        ],
        &["ENTRY_ID"],
    );
    let spec = page_spec(Anchor::Head, Direction::Forward, 30);
    let pages = paginate_until_empty(&file_indexes, &spec);
    assert_eq!(pages.len(), 7, "Should need exactly 7 pages");
    assert_pages_ordered(&pages, Direction::Forward);
    let slices = page_slices(&pages);
    let ids = collect_entry_ids(slices.iter().copied());
    assert_eq!(ids.len(), 200, "Should have retrieved all entries");
    assert_id_prefix_count(&ids, "file1_", 100);
    assert_id_prefix_count(&ids, "file2_", 100);
}

fn assert_empty_file_pages() {
    let temp_dir = TempDir::new().unwrap();
    create_test_journal(&temp_dir, "file2.journal", Vec::new()).unwrap();
    let file_indexes = create_indexed_files(
        &temp_dir,
        vec![
            (
                "file1.journal",
                timestamp_range_entries("1", 100, 150, false, true),
            ),
            (
                "file3.journal",
                timestamp_range_entries("3", 150, 200, false, true),
            ),
        ],
        &["ENTRY_ID"],
    );
    let spec = page_spec(Anchor::Head, Direction::Forward, 60);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_page_bounds(&first_page, 60, 100, 159);
    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_page_bounds(&second_page, 40, 160, 199);

    let ids = collect_entry_ids([first_page.as_slice(), second_page.as_slice()]);
    assert_eq!(
        ids.len(),
        100,
        "Should have retrieved all non-empty entries"
    );
    assert_id_prefix_count(&ids, "file1_", 50);
    assert_id_prefix_count(&ids, "file3_", 50);
    assert_empty_page(&file_indexes, &spec, &state2);
}

fn assert_reverse_file_order_pages() {
    let temp_dir = TempDir::new().unwrap();
    let mut file_indexes = three_file_contiguous_indexes(&temp_dir, false);
    file_indexes.reverse();
    let spec = page_spec(Anchor::Head, Direction::Forward, 150);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_eq!(first_page.len(), 150);
    assert_eq!(first_page.first().unwrap().timestamp, 100);
    assert_ordered(&first_page, Direction::Forward);

    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_eq!(second_page.len(), 150);
    assert_ordered(&second_page, Direction::Forward);
    assert!(first_page.last().unwrap().timestamp <= second_page.first().unwrap().timestamp);

    let ids = collect_entry_ids([first_page.as_slice(), second_page.as_slice()]);
    assert_eq!(ids.len(), 300, "Should have retrieved all entries");
    assert_empty_page(&file_indexes, &spec, &state2);
}

fn assert_anchor_timestamp_pages(direction: Direction) {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = two_file_non_overlapping_indexes(&temp_dir, true);
    let (anchor_ts, first, second, total) = match direction {
        Direction::Forward => (150, (80, 150, 229), (70, 230, 299), 150),
        Direction::Backward => (250, (80, 250, 171), (71, 170, 100), 151),
    };
    let spec = page_spec(Anchor::Timestamp(Microseconds(anchor_ts)), direction, 80);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_page_bounds(&first_page, first.0, first.1, first.2);
    assert_ordered(&first_page, direction);

    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_page_bounds(&second_page, second.0, second.1, second.2);

    let ids = collect_entry_ids([first_page.as_slice(), second_page.as_slice()]);
    assert_eq!(
        ids.len(),
        total,
        "anchor query should return unique entries"
    );
    assert_empty_page(&file_indexes, &spec, &state2);
}

fn assert_anchor_same_timestamp_pages() {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = create_indexed_files(
        &temp_dir,
        vec![
            (
                "file1.journal",
                same_timestamp_entries("1", 100, 150, false),
            ),
            (
                "file2.journal",
                same_timestamp_entries("2", 100, 150, false),
            ),
        ],
        &["ENTRY_ID"],
    );
    let spec = page_spec(Anchor::Timestamp(Microseconds(150)), Direction::Forward, 80);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_eq!(first_page.len(), 80);
    assert_all_timestamps(&first_page, 150);
    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_eq!(second_page.len(), 80);
    assert_all_timestamps(&second_page, 150);
    let (third_page, state3) = run_page(&file_indexes, &spec, Some(&state2));
    assert_eq!(third_page.len(), 40);
    assert_all_timestamps(&third_page, 150);

    let ids = collect_entry_ids([
        first_page.as_slice(),
        second_page.as_slice(),
        third_page.as_slice(),
    ]);
    assert_eq!(ids.len(), 200, "Should have retrieved all entries");
    assert_empty_page(&file_indexes, &spec, &state3);
}

fn assert_forward_time_boundary_pages() {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = three_file_contiguous_indexes(&temp_dir, false);
    let spec = bounded_page_spec(Anchor::Head, Direction::Forward, 80, 150, 350);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_page_bounds(&first_page, 80, 150, 229);
    assert_timestamps_in_range(&first_page, 150, 350);
    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_page_bounds(&second_page, 80, 230, 309);
    assert_timestamps_in_range(&second_page, 150, 350);
    let (third_page, state3) = run_page(&file_indexes, &spec, Some(&state2));
    assert_page_bounds(&third_page, 40, 310, 349);
    assert_timestamps_in_range(&third_page, 150, 350);

    let ids = collect_entry_ids([
        first_page.as_slice(),
        second_page.as_slice(),
        third_page.as_slice(),
    ]);
    assert_eq!(ids.len(), 200, "Should have retrieved bounded entries");
    assert_empty_page(&file_indexes, &spec, &state3);
}

fn filter_level_indexes(temp_dir: &TempDir) -> Vec<FileIndex> {
    let mut file1_entries = level_entries("1", "ERROR", 100, 150);
    file1_entries.extend(level_entries("1", "INFO", 150, 200));
    let mut file2_entries = level_entries("2", "ERROR", 200, 250);
    file2_entries.extend(level_entries("2", "INFO", 250, 300));
    let mut file3_entries = level_entries("3", "ERROR", 300, 350);
    file3_entries.extend(level_entries("3", "INFO", 350, 400));
    create_indexed_files(
        temp_dir,
        vec![
            ("file1.journal", file1_entries),
            ("file2.journal", file2_entries),
            ("file3.journal", file3_entries),
        ],
        &["ENTRY_ID", "LEVEL"],
    )
}

fn assert_all_level(page: &[LogEntryData], level: &str) {
    for entry in page {
        let level_values: Vec<_> = entry
            .fields
            .iter()
            .filter(|field| field.field() == "LEVEL")
            .map(|field| field.value())
            .collect();
        assert_eq!(level_values, vec![level], "All entries should match level");
    }
}

fn assert_filtered_error_pages() {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = filter_level_indexes(&temp_dir);
    let filter = Filter::match_field_value_pair(FieldValuePair::parse("LEVEL=ERROR").unwrap());
    let spec = filtered_page_spec(Anchor::Head, Direction::Forward, 80, filter);

    let (first_page, state1) = run_page(&file_indexes, &spec, None);
    assert_page_bounds(&first_page, 80, 100, 229);
    assert_all_level(&first_page, "ERROR");
    let (second_page, state2) = run_page(&file_indexes, &spec, Some(&state1));
    assert_page_bounds(&second_page, 70, 230, 349);
    assert_all_level(&second_page, "ERROR");

    let ids = collect_entry_ids([first_page.as_slice(), second_page.as_slice()]);
    assert_eq!(ids.len(), 150, "Should have all ERROR entries");
    assert_eq!(ids.iter().filter(|id| id.contains("_error_")).count(), 150);
    assert_id_prefix_count(&ids, "file1_", 50);
    assert_id_prefix_count(&ids, "file2_", 50);
    assert_id_prefix_count(&ids, "file3_", 50);
    assert_empty_page(&file_indexes, &spec, &state2);
}

fn assert_file_boundary_anchor_pages() {
    let temp_dir = TempDir::new().unwrap();
    let file_indexes = three_file_contiguous_indexes(&temp_dir, false);
    let anchor_200 = Anchor::Timestamp(Microseconds(200));
    let forward_200 = page_spec(anchor_200, Direction::Forward, 80);
    let backward_200 = page_spec(anchor_200, Direction::Backward, 80);

    let (first_fwd, state_fwd) = run_page(&file_indexes, &forward_200, None);
    assert_page_bounds(&first_fwd, 80, 200, 279);
    let (second_fwd, _) = run_page(&file_indexes, &forward_200, Some(&state_fwd));
    assert_page_bounds(&second_fwd, 80, 280, 359);

    let (first_bwd, state_bwd) = run_page(&file_indexes, &backward_200, None);
    assert_page_bounds(&first_bwd, 80, 200, 121);
    let (second_bwd, _) = run_page(&file_indexes, &backward_200, Some(&state_bwd));
    assert_page_bounds(&second_bwd, 21, 120, 100);

    assert_boundary_300_pages(&file_indexes);
    assert_boundary_200_ids(&first_fwd, &second_fwd, &first_bwd, &second_bwd);
}

fn assert_boundary_300_pages(file_indexes: &[FileIndex]) {
    let anchor = Anchor::Timestamp(Microseconds(300));
    let (page_fwd, _) = run_page(
        file_indexes,
        &page_spec(anchor, Direction::Forward, 50),
        None,
    );
    assert_page_bounds(&page_fwd, 50, 300, 349);
    let (page_bwd, _) = run_page(
        file_indexes,
        &page_spec(anchor, Direction::Backward, 50),
        None,
    );
    assert_page_bounds(&page_bwd, 50, 300, 251);
}

fn assert_boundary_200_ids(
    first_fwd: &[LogEntryData],
    second_fwd: &[LogEntryData],
    first_bwd: &[LogEntryData],
    second_bwd: &[LogEntryData],
) {
    let fwd_ids = collect_entry_ids([first_fwd, second_fwd]);
    let bwd_ids = collect_entry_ids([first_bwd, second_bwd]);
    assert_eq!(
        fwd_ids.len(),
        160,
        "Forward from boundary 200 should return two pages of 80"
    );
    assert_eq!(
        bwd_ids.len(),
        101,
        "Backward from boundary 200 should return 100-200 inclusive"
    );
    assert!(
        fwd_ids.contains("file2_200"),
        "Forward should include boundary entry 200"
    );
    assert!(
        bwd_ids.contains("file2_200"),
        "Backward should include boundary entry 200"
    );
}

#[test]
fn test_multi_file_pagination_overlapping_timestamps() {
    assert_overlapping_pages(Direction::Forward);
}

#[test]
fn test_multi_file_pagination_three_files() {
    assert_three_file_pages(Direction::Forward);
}

#[test]
fn test_multi_file_pagination_small_limit() {
    assert_small_limit_pages();
}

#[test]
fn test_multi_file_pagination_limit_one() {
    assert_limit_one_pages(Anchor::Head, Direction::Forward);
}

#[test]
fn test_multi_file_pagination_with_empty_file() {
    assert_empty_file_pages();
}

#[test]
fn test_multi_file_pagination_reverse_file_order() {
    assert_reverse_file_order_pages();
}

#[test]
fn test_multi_file_pagination_backward_non_overlapping() {
    assert_non_overlapping_backward_pages();
}

#[test]
fn test_multi_file_pagination_backward_same_timestamps() {
    assert_same_timestamp_pages(Anchor::Tail, Direction::Backward);
}

#[test]
fn test_multi_file_pagination_backward_limit_one() {
    assert_limit_one_pages(Anchor::Tail, Direction::Backward);
}

#[test]
fn test_multi_file_pagination_anchor_timestamp_forward() {
    assert_anchor_timestamp_pages(Direction::Forward);
}

#[test]
fn test_multi_file_pagination_anchor_timestamp_backward() {
    assert_anchor_timestamp_pages(Direction::Backward);
}

#[test]
fn test_multi_file_pagination_anchor_timestamp_same_timestamps() {
    assert_anchor_same_timestamp_pages();
}

#[test]
fn test_multi_file_pagination_forward_with_time_boundaries() {
    assert_forward_time_boundary_pages();
}

#[test]
fn test_multi_file_pagination_backward_overlapping_timestamps() {
    assert_overlapping_pages(Direction::Backward);
}

#[test]
fn test_multi_file_pagination_backward_three_files() {
    assert_three_file_pages(Direction::Backward);
}

#[test]
fn test_multi_file_pagination_with_filter() {
    assert_filtered_error_pages();
}

#[test]
fn test_multi_file_pagination_anchor_at_file_boundary() {
    assert_file_boundary_anchor_pages();
}
