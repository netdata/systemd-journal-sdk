//! Integration tests for query pagination.
//!
//! These tests verify that pagination works correctly when querying log entries,
//! especially in the edge case where many entries share the same timestamp.

use journal_common::Seconds;
use journal_core::file::{JournalFile, JournalFileOptions, JournalWriter};
use journal_core::repository::File;
use journal_index::{
    Anchor, Direction, FieldName, FileIndex, FileIndexer, LogEntryId, LogQueryParamsBuilder,
    Microseconds,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

// Helper constants
const JAN_1_2024_MIDNIGHT: Microseconds = Microseconds(1704067200_000_000);

/// Test journal entry specification
struct TestEntry {
    timestamp: Microseconds,
    fields: Vec<(String, String)>,
}

struct IndexedJournal {
    _temp_dir: TempDir,
    file: File,
    index: FileIndex,
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

/// Create a test journal file path that conforms to the expected format
fn create_test_journal_path(temp_dir: &TempDir) -> PathBuf {
    let machine_id = Uuid::from_u128(0x12345678_1234_1234_1234_123456789abc);
    let machine_dir = temp_dir.path().join(machine_id.to_string());
    fs::create_dir_all(&machine_dir).expect("create machine dir");
    machine_dir.join("system.journal")
}

/// Helper to create a test journal file with specified entries
fn create_test_journal(
    entries: Vec<TestEntry>,
) -> Result<(TempDir, File), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let journal_path = create_test_journal_path(&temp_dir);

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

    Ok((temp_dir, file))
}

fn create_indexed_journal(
    entries: Vec<TestEntry>,
    timestamp_field: Option<&str>,
    indexed_fields: &[&str],
) -> IndexedJournal {
    let (_temp_dir, file) = create_test_journal(entries).unwrap();
    let mut indexer = FileIndexer::default();
    let timestamp_field = timestamp_field.map(|field| FieldName::new(field).unwrap());
    let indexed_fields: Vec<_> = indexed_fields
        .iter()
        .map(|field| FieldName::new(*field).unwrap())
        .collect();
    let index = indexer
        .index(
            &file,
            timestamp_field.as_ref(),
            &indexed_fields,
            Seconds(3600),
        )
        .unwrap();

    IndexedJournal {
        _temp_dir,
        file,
        index,
    }
}

fn same_timestamp_entries(total: usize, timestamp: Microseconds) -> Vec<TestEntry> {
    (0..total)
        .map(|i| {
            TestEntry::new(timestamp)
                .with_field("MESSAGE", format!("Entry {}", i))
                .with_field("ENTRY_ID", i.to_string())
        })
        .collect()
}

fn hourly_entries(total: usize, base_timestamp: Microseconds) -> Vec<TestEntry> {
    (0..total)
        .map(|i| {
            TestEntry::new(Microseconds(base_timestamp.0 + i as u64 * 3600_000_000))
                .with_field("ENTRY_ID", i.to_string())
        })
        .collect()
}

fn read_page(
    fixture: &IndexedJournal,
    anchor: Anchor,
    direction: Direction,
    limit: usize,
    resume_position: Option<usize>,
) -> Vec<LogEntryId> {
    let mut params = LogQueryParamsBuilder::new(anchor, direction).with_limit(limit);
    if let Some(position) = resume_position {
        params = params.with_resume_position(position);
    }
    fixture
        .index
        .find_log_entries(&fixture.file, &params.build().unwrap())
        .unwrap()
}

fn read_bounded_page(
    fixture: &IndexedJournal,
    anchor: Anchor,
    direction: Direction,
    after: Microseconds,
    before: Microseconds,
    limit: usize,
    resume_position: Option<usize>,
) -> Vec<LogEntryId> {
    let mut params = LogQueryParamsBuilder::new(anchor, direction)
        .with_after(after)
        .with_before(before)
        .with_limit(limit);
    if let Some(position) = resume_position {
        params = params.with_resume_position(position);
    }
    fixture
        .index
        .find_log_entries(&fixture.file, &params.build().unwrap())
        .unwrap()
}

fn record_same_timestamp_page(
    results: &[LogEntryId],
    timestamp: Microseconds,
    all_offsets: &mut Vec<u64>,
    all_positions: &mut HashSet<usize>,
) {
    for entry in results {
        assert_eq!(entry.timestamp, timestamp);
        all_offsets.push(entry.offset);
        assert!(
            all_positions.insert(entry.position),
            "Position {} appeared twice",
            entry.position
        );
    }
}

fn assert_all_positions_seen(
    all_offsets: &[u64],
    all_positions: &HashSet<usize>,
    total_entries: usize,
) {
    assert_eq!(
        all_offsets.len(),
        total_entries,
        "Should have retrieved all entries"
    );
    let unique_offsets: HashSet<_> = all_offsets.iter().collect();
    assert_eq!(
        unique_offsets.len(),
        total_entries,
        "All offsets should be unique"
    );
    assert_eq!(
        all_positions.len(),
        total_entries,
        "Should have unique positions"
    );
    for i in 0..total_entries {
        assert!(all_positions.contains(&i), "Position {} missing", i);
    }
}

fn assert_resume_returns_empty(
    fixture: &IndexedJournal,
    anchor: Anchor,
    direction: Direction,
    resume_position: usize,
    message: &str,
) {
    let results = read_page(fixture, anchor, direction, 10, Some(resume_position));
    assert_eq!(results.len(), 0, "{message}");
}

#[test]
fn test_pagination_forward_with_same_timestamps() {
    assert_same_timestamp_pagination(Anchor::Head, Direction::Forward);
}

fn assert_same_timestamp_pagination(anchor: Anchor, direction: Direction) {
    const TOTAL_ENTRIES: usize = 300;
    const PAGE_SIZE: usize = 200;
    let same_timestamp = JAN_1_2024_MIDNIGHT;
    let fixture = create_indexed_journal(
        same_timestamp_entries(TOTAL_ENTRIES, same_timestamp),
        Some("_SOURCE_REALTIME_TIMESTAMP"),
        &["ENTRY_ID"],
    );
    let mut all_offsets = Vec::new();
    let mut all_positions = HashSet::new();

    let results = read_page(&fixture, anchor, direction, PAGE_SIZE, None);
    println!("First page: {} entries", results.len());
    assert_eq!(
        results.len(),
        PAGE_SIZE,
        "First page should return PAGE_SIZE entries"
    );
    record_same_timestamp_page(
        &results,
        same_timestamp,
        &mut all_offsets,
        &mut all_positions,
    );
    let resume_position = results.last().expect("first page has entries").position;

    let results = read_page(
        &fixture,
        anchor,
        direction,
        PAGE_SIZE,
        Some(resume_position),
    );
    println!("Second page: {} entries", results.len());
    assert_eq!(
        results.len(),
        TOTAL_ENTRIES - PAGE_SIZE,
        "Second page should return remaining entries"
    );
    record_same_timestamp_page(
        &results,
        same_timestamp,
        &mut all_offsets,
        &mut all_positions,
    );
    let resume_position = results.last().expect("second page has entries").position;

    let results = read_page(
        &fixture,
        anchor,
        direction,
        PAGE_SIZE,
        Some(resume_position),
    );
    println!("Third page: {} entries", results.len());
    assert_eq!(results.len(), 0, "Third page should be empty");
    assert_all_positions_seen(&all_offsets, &all_positions, TOTAL_ENTRIES);
}

#[test]
fn test_pagination_backward_with_same_timestamps() {
    assert_same_timestamp_pagination(Anchor::Tail, Direction::Backward);
}

#[test]
fn test_pagination_forward_with_mixed_timestamps() {
    // Create entries with varying timestamps to ensure pagination works across different timestamps too
    const ENTRIES_PER_TIMESTAMP: usize = 150;
    const PAGE_SIZE: usize = 200;

    let mut entries = Vec::new();

    // First 150 entries at timestamp T
    let timestamp1 = JAN_1_2024_MIDNIGHT;
    for i in 0..ENTRIES_PER_TIMESTAMP {
        entries.push(
            TestEntry::new(timestamp1)
                .with_field("MESSAGE", format!("Batch 1 Entry {}", i))
                .with_field("ENTRY_ID", format!("1-{}", i)),
        );
    }

    // Next 150 entries at timestamp T+1
    let timestamp2 = Microseconds(timestamp1.0 + 1_000_000);
    for i in 0..ENTRIES_PER_TIMESTAMP {
        entries.push(
            TestEntry::new(timestamp2)
                .with_field("MESSAGE", format!("Batch 2 Entry {}", i))
                .with_field("ENTRY_ID", format!("2-{}", i)),
        );
    }

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let entry_id_field = FieldName::new("ENTRY_ID").unwrap();
    let source_timestamp_field = FieldName::new("_SOURCE_REALTIME_TIMESTAMP").unwrap();
    let file_index = indexer
        .index(
            &file,
            Some(&source_timestamp_field),
            &[entry_id_field],
            Seconds(3600),
        )
        .unwrap();

    let mut all_offsets = Vec::new();
    let mut all_positions = HashSet::new();

    // First page - should get 200 entries (all 150 from timestamp1 + 50 from timestamp2)
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(PAGE_SIZE)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    println!("First page: {} entries", results.len());
    assert_eq!(results.len(), PAGE_SIZE);

    for entry in &results {
        all_offsets.push(entry.offset);
        assert!(all_positions.insert(entry.position));
    }

    let resume_position = results.last().unwrap().position;

    // Second page - should get remaining 100 entries (all from timestamp2)
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(PAGE_SIZE)
        .with_resume_position(resume_position)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    println!("Second page: {} entries", results.len());
    assert_eq!(results.len(), 100);

    // All entries in second page should have timestamp2
    for entry in &results {
        assert_eq!(entry.timestamp, timestamp2);
        all_offsets.push(entry.offset);
        assert!(all_positions.insert(entry.position));
    }

    // Verify we got all entries without duplicates
    assert_eq!(all_offsets.len(), 300);
    let unique_offsets: HashSet<_> = all_offsets.iter().collect();
    assert_eq!(unique_offsets.len(), 300);
}

#[test]
fn test_pagination_empty_journal() {
    // Create an empty journal file
    let entries = Vec::new();

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();

    // Indexing an empty journal should fail with EmptyHistogramInput
    let result = indexer.index(&file, None, &[], Seconds(3600));
    assert!(result.is_err(), "Empty journal should fail to index");
}

#[test]
fn test_pagination_single_entry() {
    let timestamp = JAN_1_2024_MIDNIGHT;
    let entries = vec![TestEntry::new(timestamp).with_field("MESSAGE", "Single entry")];

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let file_index = indexer.index(&file, None, &[], Seconds(3600)).unwrap();

    // Query forward with large limit
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(100)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 1, "Should return single entry");
    assert_eq!(results[0].timestamp, timestamp);
    assert_eq!(results[0].position, 0);

    // Try to paginate from that position (should return empty)
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(100)
        .with_resume_position(results[0].position)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 0, "No more entries after single entry");

    // Query backward
    let params = LogQueryParamsBuilder::new(Anchor::Tail, Direction::Backward)
        .with_limit(100)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 1, "Should return single entry");
    assert_eq!(results[0].timestamp, timestamp);
    assert_eq!(results[0].position, 0);

    // Try to paginate backward from position 0 (should return empty)
    let params = LogQueryParamsBuilder::new(Anchor::Tail, Direction::Backward)
        .with_limit(100)
        .with_resume_position(0)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        0,
        "Backward from position 0 should return empty"
    );
}

#[test]
fn test_pagination_two_entries() {
    let timestamp = JAN_1_2024_MIDNIGHT;
    let entries = vec![
        TestEntry::new(timestamp).with_field("MESSAGE", "Entry 1"),
        TestEntry::new(timestamp).with_field("MESSAGE", "Entry 2"),
    ];

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let file_index = indexer.index(&file, None, &[], Seconds(3600)).unwrap();

    // Forward: Get both entries at once with limit 10
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(10)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 2, "Should return both entries");
    assert_eq!(results[0].position, 0);
    assert_eq!(results[1].position, 1);

    // Forward: Get first entry with limit 1, then paginate
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(1)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 1, "Should return first entry");
    assert_eq!(results[0].position, 0);

    let first_position = results[0].position;

    // Get second entry
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(1)
        .with_resume_position(first_position)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 1, "Should return second entry");
    assert_eq!(results[0].position, 1);

    // Try to get third entry (should be empty)
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(1)
        .with_resume_position(1)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 0, "No third entry");

    // Backward: Get both entries at once
    let params = LogQueryParamsBuilder::new(Anchor::Tail, Direction::Backward)
        .with_limit(10)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 2, "Should return both entries backward");

    // Backward: Get last entry with limit 1, then paginate
    let params = LogQueryParamsBuilder::new(Anchor::Tail, Direction::Backward)
        .with_limit(1)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 1, "Should return last entry");
    assert_eq!(results[0].position, 1);

    // Get first entry going backward
    let params = LogQueryParamsBuilder::new(Anchor::Tail, Direction::Backward)
        .with_limit(1)
        .with_resume_position(1)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 1, "Should return first entry");
    assert_eq!(results[0].position, 0);
}

#[test]
fn test_pagination_limit_zero() {
    let timestamp = JAN_1_2024_MIDNIGHT;
    let entries = vec![
        TestEntry::new(timestamp).with_field("MESSAGE", "Entry 1"),
        TestEntry::new(timestamp).with_field("MESSAGE", "Entry 2"),
        TestEntry::new(timestamp).with_field("MESSAGE", "Entry 3"),
    ];

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let file_index = indexer.index(&file, None, &[], Seconds(3600)).unwrap();

    // Query with limit 0 should return empty results
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(0)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 0, "Limit 0 should return no results");

    // Same for backward
    let params = LogQueryParamsBuilder::new(Anchor::Tail, Direction::Backward)
        .with_limit(0)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 0, "Limit 0 should return no results");
}

#[test]
fn test_pagination_limit_exact_match() {
    // Create exactly 50 entries
    const TOTAL_ENTRIES: usize = 50;
    let timestamp = JAN_1_2024_MIDNIGHT;

    let entries: Vec<TestEntry> = (0..TOTAL_ENTRIES)
        .map(|i| TestEntry::new(timestamp).with_field("ENTRY_ID", i.to_string()))
        .collect();

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let file_index = indexer.index(&file, None, &[], Seconds(3600)).unwrap();

    // Query with limit exactly equal to total entries
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(TOTAL_ENTRIES)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), TOTAL_ENTRIES, "Should return all entries");

    // Try to paginate from last position (should return empty)
    let last_position = results.last().unwrap().position;
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(TOTAL_ENTRIES)
        .with_resume_position(last_position)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 0, "No more entries after exact match");
}

#[test]
fn test_pagination_limit_exceeds_total() {
    // Create 10 entries but query with limit 1000
    const TOTAL_ENTRIES: usize = 10;
    const LARGE_LIMIT: usize = 1000;
    let timestamp = JAN_1_2024_MIDNIGHT;

    let entries: Vec<TestEntry> = (0..TOTAL_ENTRIES)
        .map(|i| TestEntry::new(timestamp).with_field("ENTRY_ID", i.to_string()))
        .collect();

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let file_index = indexer.index(&file, None, &[], Seconds(3600)).unwrap();

    // Query with limit much larger than total entries
    let params = LogQueryParamsBuilder::new(Anchor::Head, Direction::Forward)
        .with_limit(LARGE_LIMIT)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        TOTAL_ENTRIES,
        "Should return all entries (not more than available)"
    );

    // Verify all positions are present
    for (i, entry) in results.iter().enumerate() {
        assert_eq!(entry.position, i);
    }

    // Same for backward
    let params = LogQueryParamsBuilder::new(Anchor::Tail, Direction::Backward)
        .with_limit(LARGE_LIMIT)
        .build()
        .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        TOTAL_ENTRIES,
        "Should return all entries backward"
    );
}

#[test]
fn test_pagination_resume_out_of_bounds() {
    const TOTAL_ENTRIES: usize = 10;
    let timestamp = JAN_1_2024_MIDNIGHT;
    let entries: Vec<TestEntry> = (0..TOTAL_ENTRIES)
        .map(|i| TestEntry::new(timestamp).with_field("ENTRY_ID", i.to_string()))
        .collect();
    let fixture = create_indexed_journal(entries, None, &[]);

    assert_resume_returns_empty(
        &fixture,
        Anchor::Head,
        Direction::Forward,
        TOTAL_ENTRIES - 1,
        "Resume from last position should return empty",
    );
    assert_resume_returns_empty(
        &fixture,
        Anchor::Head,
        Direction::Forward,
        TOTAL_ENTRIES,
        "Resume from beyond last position should return empty",
    );
    assert_resume_returns_empty(
        &fixture,
        Anchor::Head,
        Direction::Forward,
        999,
        "Resume from way beyond should return empty (not panic)",
    );
    assert_resume_returns_empty(
        &fixture,
        Anchor::Tail,
        Direction::Backward,
        0,
        "Backward from position 0 should return empty",
    );
    assert_resume_returns_empty(
        &fixture,
        Anchor::Tail,
        Direction::Backward,
        TOTAL_ENTRIES,
        "Backward from position equal to total should return empty (not panic)",
    );
    assert_resume_returns_empty(
        &fixture,
        Anchor::Tail,
        Direction::Backward,
        TOTAL_ENTRIES + 5,
        "Backward from beyond total should return empty (not panic)",
    );
    assert_resume_returns_empty(
        &fixture,
        Anchor::Tail,
        Direction::Backward,
        999,
        "Backward from way beyond should return empty (not panic)",
    );
}

#[test]
fn test_pagination_anchor_before_all_entries() {
    // Create entries at timestamps 10:00, 11:00, 12:00
    let base_timestamp = JAN_1_2024_MIDNIGHT;
    let entries = vec![
        TestEntry::new(Microseconds(base_timestamp.0 + 10 * 3600_000_000))
            .with_field("ENTRY_ID", "0"),
        TestEntry::new(Microseconds(base_timestamp.0 + 11 * 3600_000_000))
            .with_field("ENTRY_ID", "1"),
        TestEntry::new(Microseconds(base_timestamp.0 + 12 * 3600_000_000))
            .with_field("ENTRY_ID", "2"),
    ];

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let file_index = indexer.index(&file, None, &[], Seconds(3600)).unwrap();

    // Anchor at 09:00 (before all entries), going forward
    let anchor_timestamp = Microseconds(base_timestamp.0 + 9 * 3600_000_000);
    let params =
        LogQueryParamsBuilder::new(Anchor::Timestamp(anchor_timestamp), Direction::Forward)
            .with_limit(10)
            .build()
            .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        3,
        "Forward from before all entries should return all entries"
    );

    // Anchor at 09:00, going backward
    let params =
        LogQueryParamsBuilder::new(Anchor::Timestamp(anchor_timestamp), Direction::Backward)
            .with_limit(10)
            .build()
            .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        0,
        "Backward from before all entries should return no entries"
    );
}

#[test]
fn test_pagination_anchor_after_all_entries() {
    // Create entries at timestamps 10:00, 11:00, 12:00
    let base_timestamp = JAN_1_2024_MIDNIGHT;
    let entries = vec![
        TestEntry::new(Microseconds(base_timestamp.0 + 10 * 3600_000_000))
            .with_field("ENTRY_ID", "0"),
        TestEntry::new(Microseconds(base_timestamp.0 + 11 * 3600_000_000))
            .with_field("ENTRY_ID", "1"),
        TestEntry::new(Microseconds(base_timestamp.0 + 12 * 3600_000_000))
            .with_field("ENTRY_ID", "2"),
    ];

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let file_index = indexer.index(&file, None, &[], Seconds(3600)).unwrap();

    // Anchor at 13:00 (after all entries), going forward
    let anchor_timestamp = Microseconds(base_timestamp.0 + 13 * 3600_000_000);
    let params =
        LogQueryParamsBuilder::new(Anchor::Timestamp(anchor_timestamp), Direction::Forward)
            .with_limit(10)
            .build()
            .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        0,
        "Forward from after all entries should return no entries"
    );

    // Anchor at 13:00, going backward
    let params =
        LogQueryParamsBuilder::new(Anchor::Timestamp(anchor_timestamp), Direction::Backward)
            .with_limit(10)
            .build()
            .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        3,
        "Backward from after all entries should return all entries"
    );
}

#[test]
fn test_pagination_anchor_in_middle_with_pagination() {
    // Create entries at timestamps 10:00, 11:00, 12:00, 13:00, 14:00
    let base_timestamp = JAN_1_2024_MIDNIGHT;
    let entries = vec![
        TestEntry::new(Microseconds(base_timestamp.0 + 10 * 3600_000_000))
            .with_field("ENTRY_ID", "0"),
        TestEntry::new(Microseconds(base_timestamp.0 + 11 * 3600_000_000))
            .with_field("ENTRY_ID", "1"),
        TestEntry::new(Microseconds(base_timestamp.0 + 12 * 3600_000_000))
            .with_field("ENTRY_ID", "2"),
        TestEntry::new(Microseconds(base_timestamp.0 + 13 * 3600_000_000))
            .with_field("ENTRY_ID", "3"),
        TestEntry::new(Microseconds(base_timestamp.0 + 14 * 3600_000_000))
            .with_field("ENTRY_ID", "4"),
    ];

    let (_temp_dir, file) = create_test_journal(entries).unwrap();

    let mut indexer = FileIndexer::default();
    let file_index = indexer.index(&file, None, &[], Seconds(3600)).unwrap();

    // Anchor at 12:00 (middle), going forward with limit 2
    let anchor_timestamp = Microseconds(base_timestamp.0 + 12 * 3600_000_000);
    let params =
        LogQueryParamsBuilder::new(Anchor::Timestamp(anchor_timestamp), Direction::Forward)
            .with_limit(2)
            .build()
            .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        2,
        "Should return 2 entries starting from anchor"
    );
    // Should get entries at 12:00 and 13:00 (positions 2 and 3)
    assert_eq!(results[0].position, 2);
    assert_eq!(results[1].position, 3);

    // Paginate forward to get the rest
    let params =
        LogQueryParamsBuilder::new(Anchor::Timestamp(anchor_timestamp), Direction::Forward)
            .with_limit(2)
            .with_resume_position(results[1].position)
            .build()
            .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 1, "Should return remaining 1 entry");
    assert_eq!(results[0].position, 4);

    // Anchor at 12:00, going backward with limit 2
    let params =
        LogQueryParamsBuilder::new(Anchor::Timestamp(anchor_timestamp), Direction::Backward)
            .with_limit(2)
            .build()
            .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(
        results.len(),
        2,
        "Should return 2 entries backward from anchor"
    );
    // Should get entries at 12:00 and 11:00 (positions 2 and 1)
    assert_eq!(results[0].position, 2);
    assert_eq!(results[1].position, 1);

    // Paginate backward to get the rest
    let params =
        LogQueryParamsBuilder::new(Anchor::Timestamp(anchor_timestamp), Direction::Backward)
            .with_limit(2)
            .with_resume_position(results[1].position)
            .build()
            .unwrap();

    let results = file_index.find_log_entries(&file, &params).unwrap();
    assert_eq!(results.len(), 1, "Should return remaining 1 entry");
    assert_eq!(results[0].position, 0);
}

#[test]
fn test_pagination_with_time_boundaries() {
    assert_time_boundary_pages(
        Direction::Forward,
        &[&[5, 6, 7, 8], &[9, 10, 11, 12], &[13, 14]],
    );
}

#[test]
fn test_pagination_backward_with_time_boundaries() {
    assert_time_boundary_pages(
        Direction::Backward,
        &[&[14, 13, 12, 11], &[10, 9, 8, 7], &[6, 5]],
    );
}

fn assert_time_boundary_pages(direction: Direction, expected_pages: &[&[usize]]) {
    let base_timestamp = JAN_1_2024_MIDNIGHT;
    let fixture = create_indexed_journal(hourly_entries(20, base_timestamp), None, &[]);
    let after = Microseconds(base_timestamp.0 + 5 * 3600_000_000);
    let before = Microseconds(base_timestamp.0 + 15 * 3600_000_000);
    let anchor = match direction {
        Direction::Forward => Anchor::Head,
        Direction::Backward => Anchor::Tail,
    };

    let mut all_results = Vec::new();
    let mut resume_position = None;
    for (page_idx, expected_positions) in expected_pages.iter().enumerate() {
        let results = read_bounded_page(
            &fixture,
            anchor,
            direction,
            after,
            before,
            4,
            resume_position,
        );
        assert_eq!(
            positions(&results),
            *expected_positions,
            "page {page_idx} positions should match"
        );
        resume_position = results.last().map(|entry| entry.position);
        all_results.extend(results);
    }

    let results = read_bounded_page(
        &fixture,
        anchor,
        direction,
        after,
        before,
        4,
        resume_position,
    );
    assert_eq!(results.len(), 0, "Fourth page should be empty");
    assert_eq!(all_results.len(), 10, "should collect ten bounded entries");
    for entry in &all_results {
        assert!(
            entry.timestamp.0 >= after.0,
            "Entry timestamp should be >= after boundary"
        );
        assert!(
            entry.timestamp.0 < before.0,
            "Entry timestamp should be < before boundary"
        );
    }
}

fn positions(results: &[LogEntryId]) -> Vec<usize> {
    results.iter().map(|entry| entry.position).collect()
}
