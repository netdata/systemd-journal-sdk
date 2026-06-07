# Codacy Rust/Go Metrics Audit

## Scope

- Codacy branch: `master`.
- Codacy fetched at: `2026-06-07T13:32:26.458Z`.
- Files analyzed in this report: `217`.
- Raw Codacy API responses stay under `.local/` and are not committed.

## Interpretation

- Codacy file complexity is the sum of method/function cyclomatic complexity in a file.
- Local `lizard` max CCN is the highest single-function CCN found in the same tracked file set.
- A high Codacy complexity with max CCN <= 12 means file-size/ownership pressure, not a single dangerous function.
- Test and harness paths are classified separately because they should not drive production coverage decisions.
- Coverage values are Codacy file metrics at fetch time; coverage-report exclusions are validated separately by the coverage scripts and remote Codacy run.
- This is a point-in-time audit snapshot. Regenerate it after substantial Rust/Go refactors or Codacy metric changes.

## Regeneration

```bash
tests/code_scanning/export_codacy_file_metrics.js \
  --output .local/codacy/file-metrics-rust-go.json \
  --search go/ --search rust/
git ls-files 'go/**/*.go' 'rust/**/*.rs' > .local/codacy/rust-go-source-files.txt
lizard -C 12 --csv -f .local/codacy/rust-go-source-files.txt > .local/codacy/lizard-rust-go.csv
python3 - <<'PY' > .agents/sow/specs/codacy-rust-go-metrics-audit.md
import json
from pathlib import Path
from tests.code_scanning.summarize_codacy_file_metrics import (
    load_lizard_max_ccn, metric_row, render_markdown, source_field,
)
source = json.loads(Path('.local/codacy/file-metrics-rust-go.json').read_text(encoding='utf-8'))
max_ccn = load_lizard_max_ccn(Path('.local/codacy/lizard-rust-go.csv'))
rows = [
    metric_row(file_metric, max_ccn.get(str(file_metric['path']), 0))
    for file_metric in source.get('files', [])
    if isinstance(file_metric, dict) and isinstance(file_metric.get('path'), str)
]
print(render_markdown({
    'branch': source_field(source, 'branch'),
    'fetched_at': source_field(source, 'fetchedAt'),
}, rows), end='')
PY
```

## Surface Summary

| Surface | Files | Complex Files | Duplicated Files | Complexity Sum | Duplication Sum |
|---|---:|---:|---:|---:|---:|
| `adapter` | 4 | 4 | 3 | 376 | 187 |
| `cli` | 2 | 2 | 2 | 333 | 203 |
| `go_sdk` | 43 | 24 | 19 | 4208 | 677 |
| `legacy_jf` | 18 | 8 | 15 | 946 | 3022 |
| `other` | 20 | 5 | 4 | 289 | 102 |
| `rust_core` | 28 | 20 | 22 | 1700 | 3375 |
| `rust_engine` | 11 | 6 | 1 | 279 | 6 |
| `rust_index` | 8 | 4 | 3 | 273 | 70 |
| `rust_log_writer` | 7 | 4 | 3 | 410 | 376 |
| `rust_public` | 13 | 10 | 8 | 1158 | 284 |
| `test_or_harness` | 63 | 52 | 48 | 4122 | 10353 |

## Top Complexity

| Path | Surface | Complexity | Max CCN | Duplication | Coverage | Classification |
|---|---|---:|---:|---:|---:|---|
| `go/journal/netdata.go` | `go_sdk` | 870 | 12 | 0 | 72.32 | real file-size/ownership pressure; functions stay below CCN gate |
| `go/journal/explorer.go` | `go_sdk` | 763 | 12 | 111 | 78.46 | real file-size/ownership pressure; functions stay below CCN gate |
| `go/cmd/journalctl/main.go` | `cli` | 304 | 12 | 71 | 40.52 | real file-size/ownership pressure; functions stay below CCN gate |
| `go/journal/verify_graph.go` | `go_sdk` | 276 | 12 | 16 | 65.74 | real file-size/ownership pressure; functions stay below CCN gate |
| `go/journal/directory_reader.go` | `go_sdk` | 263 | 12 | 101 | 63.11 | real file-size/ownership pressure; functions stay below CCN gate |
| `go/journal/log.go` | `go_sdk` | 251 | 11 | 50 | 80.29 | real file-size/ownership pressure; functions stay below CCN gate |
| `go/journal/reader_test.go` | `test_or_harness` | 244 | 12 | 1215 | - | test/harness metric; not production coverage signal |
| `go/journal/reader.go` | `go_sdk` | 231 | 10 | 63 | 74.29 | real file-size/ownership pressure; functions stay below CCN gate |
| `go/journal/writer_test.go` | `test_or_harness` | 209 | 12 | 426 | - | test/harness metric; not production coverage signal |
| `go/journal/explorer_test.go` | `test_or_harness` | 208 | 8 | 353 | - | test/harness metric; not production coverage signal |
| `rust/src/internal/testcmd/corpus_experiment/src/main.rs` | `test_or_harness` | 201 | 11 | 115 | 0 | test/harness metric; not production coverage signal |
| `go/internal/testcmd/corpus_experiment/main.go` | `test_or_harness` | 193 | 12 | 135 | 0 | test/harness metric; not production coverage signal |
| `rust/src/journal/src/directory.rs` | `rust_public` | 193 | 12 | 122 | 63.04 | moderate file-size pressure; functions stay below CCN gate |
| `rust/src/internal/testcmd/reader_core_bench/src/main.rs` | `test_or_harness` | 191 | 12 | 0 | 0 | test/harness metric; not production coverage signal |
| `rust/src/crates/journal-core/src/file/mmap.rs` | `rust_core` | 189 | 11 | 0 | 96.11 | moderate file-size pressure; functions stay below CCN gate |
| `go/journal/verify.go` | `go_sdk` | 185 | 12 | 0 | 64.50 | moderate file-size pressure; functions stay below CCN gate |
| `rust/src/journal/src/verify_graph/walk.rs` | `rust_public` | 182 | 12 | 0 | 83.39 | moderate file-size pressure; functions stay below CCN gate |
| `rust/src/journal/src/sealed_verify.rs` | `rust_public` | 180 | 12 | 19 | 74.15 | moderate file-size pressure; functions stay below CCN gate |
| `go/journal/netdata_test.go` | `test_or_harness` | 177 | 12 | 52 | - | test/harness metric; not production coverage signal |
| `rust/src/crates/jf/journal_file/src/object.rs` | `legacy_jf` | 176 | 8 | 0 | 70.10 | moderate file-size pressure; functions stay below CCN gate |

## Top Duplication

| Path | Surface | Duplication | Clones | Complexity | Coverage | Classification |
|---|---|---:|---:|---:|---:|---|
| `go/journal/reader_test.go` | `test_or_harness` | 1215 | 107 | 244 | - | test/harness repetition; not production coverage signal |
| `go/journal/seal_test.go` | `test_or_harness` | 786 | 60 | 60 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/jf/journal_file/src/file.rs` | `legacy_jf` | 686 | 24 | 126 | 77.98 | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/offset_array.rs` | `rust_core` | 662 | 23 | 163 | 63.01 | real legacy/core overlap; architecture debt, not scanner noise |
| `go/journal/verify_test.go` | `test_or_harness` | 629 | 52 | 83 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/file/writer_tests.rs` | `test_or_harness` | 618 | 52 | 32 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/jf/journal_file/src/offset_array.rs` | `legacy_jf` | 600 | 17 | 131 | 31.06 | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-index/tests/pagination.rs` | `test_or_harness` | 529 | 53 | 48 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/file/file.rs` | `rust_core` | 491 | 20 | 162 | 82.65 | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/jf/journal_file/src/file/tests.rs` | `test_or_harness` | 456 | 22 | 27 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/file/writer_seal_tests.rs` | `test_or_harness` | 432 | 31 | 21 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/jf/journal_file/src/journal_file.rs` | `legacy_jf` | 427 | 28 | 108 | - | real legacy/core overlap; architecture debt, not scanner noise |
| `go/journal/writer_test.go` | `test_or_harness` | 426 | 50 | 209 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/file/file_tests.rs` | `test_or_harness` | 405 | 18 | 25 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-log-writer/tests/log_writer/rotation_retention.rs` | `test_or_harness` | 383 | 45 | 44 | - | test/harness repetition; not production coverage signal |
| `go/journal/explorer_test.go` | `test_or_harness` | 353 | 46 | 208 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-index/tests/filter_evaluation.rs` | `test_or_harness` | 353 | 29 | 0 | - | test/harness repetition; not production coverage signal |
| `go/journal/log_test.go` | `test_or_harness` | 342 | 38 | 172 | - | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/file/cursor.rs` | `rust_core` | 300 | 5 | 60 | 81.40 | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/filter.rs` | `rust_core` | 299 | 11 | 62 | 76.59 | real legacy/core overlap; architecture debt, not scanner noise |

## File By File

| Path | Surface | Grade | Complexity | Max CCN | Duplication | Clones | Coverage | LOC | Complexity Classification | Duplication Classification |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| `go/API.md` | `other` | A | 0 | 0 | 0 | 0 | - | 254 | low; reasonable | low; reasonable |
| `go/README.md` | `other` | A | 0 | 0 | 0 | 0 | - | 328 | low; reasonable | low; reasonable |
| `go/adapter/complex_match.go` | `adapter` | B | 22 | 5 | 34 | 2 | 0 | 136 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/adapter/main.go` | `adapter` | C | 163 | 12 | 135 | 11 | 14.99 | 876 | moderate file-size pressure; functions stay below CCN gate | high production duplication; follow-up refactor candidate |
| `go/adapter/main_test.go` | `test_or_harness` | A | 19 | 9 | 0 | 0 | - | 66 | low; reasonable | low; reasonable |
| `go/cmd/journalctl/main.go` | `cli` | B | 304 | 12 | 71 | 4 | 40.52 | 945 | real file-size/ownership pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/cmd/journalctl/main_test.go` | `test_or_harness` | E | 96 | 9 | 154 | 20 | - | 378 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/go.mod` | `other` | A | 0 | 0 | 0 | 0 | - | 6 | low; reasonable | low; reasonable |
| `go/internal/testcmd/corpus_digest/main.go` | `test_or_harness` | B | 40 | 10 | 35 | 3 | 0 | 215 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/internal/testcmd/corpus_experiment/main.go` | `test_or_harness` | C | 193 | 12 | 135 | 10 | 0 | 825 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/internal/testcmd/corpus_regenerate/main.go` | `test_or_harness` | D | 56 | 8 | 160 | 12 | 0 | 316 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/internal/testcmd/dataset_ingester/main.go` | `test_or_harness` | C | 86 | 11 | 56 | 8 | 0 | 379 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/internal/testcmd/livewriter/main.go` | `test_or_harness` | B | 47 | 9 | 14 | 1 | 0 | 246 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/internal/testcmd/netdata_function_wrapper/main.go` | `test_or_harness` | A | 27 | 11 | 0 | 0 | 0 | 119 | test/harness metric; not production coverage signal | low; reasonable |
| `go/internal/testcmd/reader_core_bench/main.go` | `test_or_harness` | B | 159 | 9 | 11 | 1 | 0 | 759 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/internal/testcmd/writer_core_bench/main.go` | `test_or_harness` | C | 67 | 10 | 81 | 7 | 0 | 440 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/boot_id_linux.go` | `go_sdk` | A | 0 | 0 | 0 | 0 | - | 1 | low; reasonable | low; reasonable |
| `go/journal/boot_id_other.go` | `go_sdk` | A | 0 | 0 | 0 | 0 | - | 1 | low; reasonable | low; reasonable |
| `go/journal/directory_reader.go` | `go_sdk` | C | 263 | 12 | 101 | 8 | 63.11 | 865 | real file-size/ownership pressure; functions stay below CCN gate | high production duplication; follow-up refactor candidate |
| `go/journal/doc.go` | `go_sdk` | A | 0 | 0 | 0 | 0 | - | 1 | low; reasonable | low; reasonable |
| `go/journal/example_test.go` | `test_or_harness` | A | 3 | 3 | 0 | 0 | - | 22 | low; reasonable | low; reasonable |
| `go/journal/explorer.go` | `go_sdk` | B | 763 | 12 | 111 | 12 | 78.46 | 2675 | real file-size/ownership pressure; functions stay below CCN gate | high production duplication; follow-up refactor candidate |
| `go/journal/explorer_test.go` | `test_or_harness` | D | 208 | 8 | 353 | 46 | - | 1119 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/facade.go` | `go_sdk` | B | 145 | 11 | 15 | 1 | 58.48 | 562 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/facade_test.go` | `test_or_harness` | C | 138 | 12 | 76 | 6 | - | 510 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/field_policy.go` | `go_sdk` | B | 59 | 10 | 0 | 0 | 79.26 | 166 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/file_lock.go` | `go_sdk` | A | 0 | 0 | 0 | 0 | - | 5 | low; reasonable | low; reasonable |
| `go/journal/file_lock_other.go` | `go_sdk` | A | 2 | 1 | 0 | 0 | - | 8 | low; reasonable | low; reasonable |
| `go/journal/file_lock_unix.go` | `go_sdk` | A | 2 | 1 | 0 | 0 | 0 | 11 | low; reasonable | low; reasonable |
| `go/journal/file_lock_windows.go` | `go_sdk` | C | 7 | 3 | 28 | 2 | - | 56 | low; reasonable | small repeated blocks; monitor |
| `go/journal/file_open_other.go` | `go_sdk` | E | 3 | 2 | 15 | 1 | - | 12 | low; reasonable | small repeated blocks; monitor |
| `go/journal/file_open_unix.go` | `go_sdk` | E | 3 | 2 | 15 | 1 | 100 | 12 | low; reasonable | small repeated blocks; monitor |
| `go/journal/file_open_windows.go` | `go_sdk` | A | 7 | 4 | 0 | 0 | - | 40 | low; reasonable | low; reasonable |
| `go/journal/format.go` | `go_sdk` | B | 53 | 10 | 10 | 2 | 88.53 | 403 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/format_test.go` | `test_or_harness` | A | 23 | 11 | 0 | 0 | - | 113 | test/harness metric; not production coverage signal | low; reasonable |
| `go/journal/fss.go` | `go_sdk` | B | 50 | 6 | 0 | 0 | 88.94 | 247 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/fss_test.go` | `test_or_harness` | B | 22 | 4 | 11 | 1 | - | 119 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/hash.go` | `go_sdk` | A | 14 | 4 | 0 | 0 | 100 | 122 | low; reasonable | low; reasonable |
| `go/journal/hash_test.go` | `test_or_harness` | A | 10 | 4 | 0 | 0 | - | 52 | low; reasonable | low; reasonable |
| `go/journal/live_concurrency_test.go` | `test_or_harness` | B | 49 | 8 | 16 | 2 | - | 327 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/live_reader_test.go` | `test_or_harness` | E | 70 | 11 | 202 | 19 | - | 332 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/lock.go` | `go_sdk` | B | 48 | 12 | 0 | 0 | 62.70 | 170 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/lock_owner_bsd.go` | `go_sdk` | C | 8 | 5 | 17 | 1 | - | 30 | low; reasonable | small repeated blocks; monitor |
| `go/journal/lock_owner_linux.go` | `go_sdk` | A | 7 | 5 | 0 | 0 | 71.43 | 29 | low; reasonable | low; reasonable |
| `go/journal/lock_owner_other.go` | `go_sdk` | A | 3 | 2 | 0 | 0 | - | 11 | low; reasonable | low; reasonable |
| `go/journal/lock_owner_unix_other.go` | `go_sdk` | C | 6 | 5 | 17 | 1 | - | 25 | low; reasonable | small repeated blocks; monitor |
| `go/journal/lock_owner_windows.go` | `go_sdk` | A | 5 | 4 | 0 | 0 | - | 27 | low; reasonable | low; reasonable |
| `go/journal/lock_test.go` | `test_or_harness` | A | 13 | 4 | 0 | 0 | - | 59 | low; reasonable | low; reasonable |
| `go/journal/log.go` | `go_sdk` | B | 251 | 11 | 50 | 4 | 80.29 | 936 | real file-size/ownership pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/log_field_policy_test.go` | `test_or_harness` | E | 90 | 11 | 271 | 31 | - | 463 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/log_helpers_test.go` | `test_or_harness` | D | 90 | 10 | 78 | 9 | - | 301 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/log_retention.go` | `go_sdk` | B | 71 | 12 | 0 | 0 | 75.58 | 213 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/log_retention_policy_test.go` | `test_or_harness` | E | 61 | 11 | 184 | 23 | - | 341 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/log_rotation_policy_test.go` | `test_or_harness` | D | 34 | 8 | 86 | 9 | - | 154 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/log_test.go` | `test_or_harness` | E | 172 | 11 | 342 | 38 | - | 819 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/mmap_other.go` | `go_sdk` | C | 35 | 8 | 63 | 3 | - | 120 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/mmap_unix.go` | `go_sdk` | B | 49 | 11 | 63 | 3 | 62.81 | 200 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/netdata.go` | `go_sdk` | B | 870 | 12 | 0 | 0 | 72.32 | 3581 | real file-size/ownership pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/netdata_test.go` | `test_or_harness` | C | 177 | 12 | 52 | 4 | - | 686 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/reader.go` | `go_sdk` | B | 231 | 10 | 63 | 4 | 74.29 | 891 | real file-size/ownership pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/reader_directory_test.go` | `test_or_harness` | B | 60 | 11 | 30 | 3 | - | 237 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/reader_entry.go` | `go_sdk` | B | 72 | 9 | 8 | 1 | 68.72 | 268 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/reader_filter.go` | `go_sdk` | A | 36 | 8 | 0 | 0 | 85.98 | 141 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/reader_output.go` | `go_sdk` | B | 83 | 12 | 0 | 0 | 90.60 | 268 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/reader_parse_test.go` | `test_or_harness` | F | 24 | 9 | 133 | 12 | - | 110 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/reader_test.go` | `test_or_harness` | F | 244 | 12 | 1215 | 107 | - | 1030 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/reader_unique.go` | `go_sdk` | B | 14 | 6 | 9 | 1 | 61.29 | 76 | low; reasonable | small repeated blocks; monitor |
| `go/journal/reader_zstd_test.go` | `test_or_harness` | A | 13 | 7 | 0 | 0 | - | 51 | low; reasonable | low; reasonable |
| `go/journal/seal.go` | `go_sdk` | B | 66 | 10 | 0 | 0 | 76.40 | 263 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/seal_test.go` | `test_or_harness` | F | 60 | 7 | 786 | 60 | - | 304 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/sync_dir_other.go` | `go_sdk` | A | 1 | 1 | 0 | 0 | - | 4 | low; reasonable | low; reasonable |
| `go/journal/sync_dir_unix.go` | `go_sdk` | A | 4 | 4 | 0 | 0 | 83.33 | 19 | low; reasonable | low; reasonable |
| `go/journal/verify.go` | `go_sdk` | B | 185 | 12 | 0 | 0 | 64.50 | 623 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/verify_graph.go` | `go_sdk` | B | 276 | 12 | 16 | 2 | 65.74 | 913 | real file-size/ownership pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/verify_test.go` | `test_or_harness` | F | 83 | 10 | 629 | 52 | - | 352 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/writer.go` | `go_sdk` | B | 112 | 11 | 20 | 2 | 76.39 | 487 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/writer_arrays.go` | `go_sdk` | C | 115 | 11 | 23 | 3 | 58.99 | 360 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/writer_compression.go` | `go_sdk` | B | 53 | 9 | 0 | 0 | 70 | 207 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/writer_compression_test.go` | `test_or_harness` | B | 44 | 9 | 16 | 2 | - | 221 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/writer_dedup_test.go` | `test_or_harness` | E | 23 | 8 | 45 | 6 | - | 92 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/writer_file_mode_test.go` | `test_or_harness` | A | 7 | 4 | 0 | 0 | - | 37 | low; reasonable | low; reasonable |
| `go/journal/writer_init.go` | `go_sdk` | B | 116 | 12 | 0 | 0 | 74.93 | 423 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `go/journal/writer_objects.go` | `go_sdk` | C | 120 | 11 | 33 | 4 | 60.87 | 388 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `go/journal/writer_snapshot_test.go` | `test_or_harness` | C | 88 | 11 | 63 | 8 | - | 398 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `go/journal/writer_test.go` | `test_or_harness` | E | 209 | 12 | 426 | 50 | - | 889 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/README.md` | `other` | A | 0 | 0 | 0 | 0 | - | 414 | low; reasonable | low; reasonable |
| `rust/src/adapter/helpers.rs` | `adapter` | B | 36 | 7 | 18 | 2 | 0 | 187 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/adapter/main.rs` | `adapter` | B | 155 | 11 | 0 | 0 | 0 | 943 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/cmd/journalctl/main.rs` | `cli` | B | 29 | 9 | 132 | 8 | 30.88 | 897 | moderate file-size pressure; functions stay below CCN gate | high production duplication; follow-up refactor candidate |
| `rust/src/crates/jf/error/src/lib.rs` | `legacy_jf` | B | 3 | 2 | 61 | 2 | 0 | 116 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/jf/journal_file/src/cursor.rs` | `legacy_jf` | B | 60 | 6 | 286 | 3 | 50.94 | 259 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/jf/journal_file/src/file.rs` | `legacy_jf` | D | 126 | 10 | 686 | 24 | 77.98 | 733 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/jf/journal_file/src/file/iterators.rs` | `legacy_jf` | E | 19 | 9 | 219 | 9 | 63.64 | 109 | low; reasonable | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/jf/journal_file/src/file/matchers.rs` | `legacy_jf` | B | 14 | 4 | 23 | 2 | 89.19 | 77 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/jf/journal_file/src/file/tests.rs` | `test_or_harness` | D | 27 | 9 | 456 | 22 | - | 365 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/jf/journal_file/src/filter.rs` | `legacy_jf` | D | 80 | 12 | 283 | 11 | 29.04 | 361 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/jf/journal_file/src/hash.rs` | `legacy_jf` | D | 0 | 3 | 29 | 2 | 97.22 | 0 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/jf/journal_file/src/journal_file.rs` | `legacy_jf` | E | 108 | 11 | 427 | 28 | - | 598 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/jf/journal_file/src/lib.rs` | `legacy_jf` | C | 0 | 0 | 12 | 1 | - | 20 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/jf/journal_file/src/object.rs` | `legacy_jf` | B | 176 | 8 | 0 | 0 | 70.10 | 979 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/jf/journal_file/src/offset_array.rs` | `legacy_jf` | D | 131 | 11 | 600 | 17 | 31.06 | 573 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/jf/journal_file/src/reader.rs` | `legacy_jf` | F | 0 | 6 | 184 | 5 | 58.92 | 0 | low; reasonable | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/jf/journal_file/src/value_guard.rs` | `legacy_jf` | E | 0 | 1 | 93 | 3 | 91.89 | 0 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/jf/journal_file/src/writer.rs` | `legacy_jf` | B | 159 | 11 | 0 | 0 | 93.51 | 840 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/jf/journal_reader_ffi/build.rs` | `legacy_jf` | A | 0 | 1 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/jf/journal_reader_ffi/src/lib.rs` | `legacy_jf` | F | 0 | 5 | 66 | 6 | 0 | 0 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/jf/sigbus/src/lib.rs` | `legacy_jf` | D | 5 | 3 | 37 | 2 | 0 | 40 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/jf/window_manager/src/lib.rs` | `legacy_jf` | B | 65 | 10 | 16 | 2 | 92.05 | 226 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-common/src/collections.rs` | `other` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-common/src/compat.rs` | `other` | A | 0 | 2 | 0 | 0 | 100 | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-common/src/lib.rs` | `other` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-common/src/system.rs` | `other` | B | 23 | 6 | 20 | 1 | 0 | 120 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-common/src/time.rs` | `other` | B | 78 | 8 | 12 | 2 | 93.59 | 466 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/collections.rs` | `rust_core` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-core/src/error.rs` | `rust_core` | B | 1 | 1 | 61 | 2 | 0 | 78 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/cursor.rs` | `rust_core` | C | 60 | 6 | 300 | 5 | 81.40 | 262 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/file.rs` | `rust_core` | D | 162 | 11 | 491 | 20 | 82.65 | 783 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/file_iterators.rs` | `rust_core` | C | 22 | 9 | 161 | 5 | 77.78 | 143 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/file_mut.rs` | `rust_core` | D | 84 | 12 | 246 | 15 | 92.93 | 499 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/file_payload.rs` | `rust_core` | C | 99 | 8 | 105 | 9 | 56.37 | 478 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/file_tests.rs` | `test_or_harness` | C | 25 | 2 | 405 | 18 | - | 587 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/file/filter.rs` | `rust_core` | D | 62 | 10 | 299 | 11 | 76.59 | 269 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/guarded_cell.rs` | `rust_core` | B | 20 | 4 | 20 | 2 | 97.90 | 134 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/hash.rs` | `rust_core` | B | 38 | 5 | 65 | 6 | 98.44 | 370 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/index_filter.rs` | `rust_core` | A | 0 | 10 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-core/src/file/lock.rs` | `rust_core` | B | 78 | 11 | 20 | 1 | 75.30 | 352 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/mmap.rs` | `rust_core` | B | 189 | 11 | 0 | 0 | 96.11 | 1084 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-core/src/file/mod.rs` | `rust_core` | B | 0 | 0 | 12 | 1 | - | 42 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/object.rs` | `rust_core` | C | 122 | 6 | 290 | 18 | 75.52 | 774 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/object_compression.rs` | `rust_core` | C | 45 | 7 | 28 | 4 | 83.55 | 185 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/object_hash.rs` | `rust_core` | E | 41 | 4 | 118 | 12 | 66.67 | 164 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/offset_array.rs` | `rust_core` | D | 163 | 11 | 662 | 23 | 63.01 | 744 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/reader.rs` | `rust_core` | C | 58 | 8 | 200 | 6 | 43.35 | 289 | moderate file-size pressure; functions stay below CCN gate | real legacy/core overlap; architecture debt, not scanner noise |
| `rust/src/crates/journal-core/src/file/row_view.rs` | `rust_core` | C | 44 | 6 | 73 | 8 | 91.71 | 256 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/sigbus.rs` | `rust_core` | C | 9 | 3 | 37 | 2 | 0 | 66 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/value_guard.rs` | `rust_core` | E | 0 | 1 | 93 | 3 | 86.05 | 0 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/writer.rs` | `rust_core` | B | 160 | 11 | 0 | 0 | 89.61 | 839 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-core/src/file/writer_entry_arrays.rs` | `rust_core` | C | 122 | 12 | 60 | 6 | 70.69 | 578 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/file/writer_seal.rs` | `rust_core` | B | 43 | 6 | 0 | 0 | 94 | 174 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-core/src/file/writer_seal_tests.rs` | `test_or_harness` | D | 21 | 3 | 432 | 31 | - | 477 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/file/writer_structured_tests.rs` | `test_or_harness` | C | 31 | 5 | 149 | 14 | - | 505 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/file/writer_tests.rs` | `test_or_harness` | F | 32 | 3 | 618 | 52 | - | 575 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-core/src/fss.rs` | `rust_core` | B | 49 | 6 | 14 | 2 | 95.17 | 336 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-core/src/lib.rs` | `rust_core` | A | 0 | 0 | 0 | 0 | - | 14 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-core/src/seal.rs` | `rust_core` | B | 29 | 9 | 20 | 2 | 94.82 | 168 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-engine/examples/cgroup-run.sh` | `test_or_harness` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-engine/examples/index.rs` | `test_or_harness` | A | 11 | 11 | 0 | 0 | - | 79 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-engine/src/cache.rs` | `rust_engine` | A | 1 | 1 | 0 | 0 | 0 | 24 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-engine/src/error.rs` | `rust_engine` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-engine/src/facets.rs` | `rust_engine` | A | 21 | 5 | 0 | 0 | 0 | 119 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-engine/src/histogram.rs` | `rust_engine` | B | 45 | 4 | 0 | 0 | 2.82 | 357 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-engine/src/indexing.rs` | `rust_engine` | B | 48 | 7 | 0 | 0 | 18.82 | 390 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-engine/src/lib.rs` | `rust_engine` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-engine/src/logs/mod.rs` | `rust_engine` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-engine/src/logs/query.rs` | `rust_engine` | B | 94 | 9 | 6 | 1 | 85.50 | 465 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-engine/src/logs/table.rs` | `rust_engine` | B | 41 | 7 | 0 | 0 | 0 | 159 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-engine/src/logs/transformations.rs` | `rust_engine` | A | 29 | 5 | 0 | 0 | - | 518 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-engine/src/query_time_range.rs` | `rust_engine` | A | 0 | 3 | 0 | 0 | 100 | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-engine/tests/multi_file_pagination.rs` | `test_or_harness` | B | 109 | 7 | 0 | 0 | - | 939 | test/harness metric; not production coverage signal | low; reasonable |
| `rust/src/crates/journal-index/src/bitmap.rs` | `rust_index` | A | 0 | 2 | 0 | 0 | 72.73 | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-index/src/error.rs` | `rust_index` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-index/src/field_types.rs` | `rust_index` | B | 50 | 4 | 0 | 0 | 84.98 | 264 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-index/src/file_index.rs` | `rust_index` | C | 122 | 11 | 26 | 3 | 71.69 | 565 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-index/src/file_indexer.rs` | `rust_index` | B | 52 | 9 | 0 | 0 | 82 | 397 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-index/src/filter.rs` | `rust_index` | B | 49 | 11 | 32 | 4 | 64.50 | 236 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-index/src/histogram.rs` | `rust_index` | D | 0 | 9 | 12 | 2 | 97.55 | 0 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/journal-index/src/lib.rs` | `rust_index` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-index/tests/filter_evaluation.rs` | `test_or_harness` | F | 0 | 8 | 353 | 29 | - | 0 | low; reasonable | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-index/tests/pagination.rs` | `test_or_harness` | E | 48 | 8 | 529 | 53 | - | 791 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-log-writer/src/error.rs` | `rust_log_writer` | A | 0 | 0 | 0 | 0 | - | 25 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-log-writer/src/lib.rs` | `rust_log_writer` | A | 0 | 0 | 0 | 0 | - | 10 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-log-writer/src/log/chain.rs` | `rust_log_writer` | C | 130 | 10 | 166 | 14 | 86.38 | 709 | moderate file-size pressure; functions stay below CCN gate | high production duplication; follow-up refactor candidate |
| `rust/src/crates/journal-log-writer/src/log/config.rs` | `rust_log_writer` | A | 19 | 1 | 0 | 0 | 33.68 | 150 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-log-writer/src/log/helpers.rs` | `rust_log_writer` | B | 50 | 8 | 0 | 0 | 83.66 | 229 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-log-writer/src/log/mod.rs` | `rust_log_writer` | C | 137 | 9 | 150 | 13 | 88.38 | 693 | moderate file-size pressure; functions stay below CCN gate | high production duplication; follow-up refactor candidate |
| `rust/src/crates/journal-log-writer/src/log/serde_api_tests.rs` | `test_or_harness` | A | 9 | 1 | 0 | 0 | - | 230 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-log-writer/src/log/startup.rs` | `rust_log_writer` | C | 74 | 7 | 60 | 6 | 97.13 | 370 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-log-writer/tests/log_writer.rs` | `test_or_harness` | B | 26 | 3 | 57 | 5 | - | 302 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-log-writer/tests/log_writer/entries_policy.rs` | `test_or_harness` | C | 34 | 6 | 255 | 26 | - | 635 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-log-writer/tests/log_writer/lifecycle.rs` | `test_or_harness` | C | 6 | 2 | 70 | 5 | - | 141 | low; reasonable | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-log-writer/tests/log_writer/naming.rs` | `test_or_harness` | C | 22 | 3 | 159 | 20 | - | 522 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-log-writer/tests/log_writer/rotation_retention.rs` | `test_or_harness` | E | 44 | 6 | 383 | 45 | - | 739 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/crates/journal-registry/README.md` | `other` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-registry/src/lib.rs` | `other` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-registry/src/registry/error.rs` | `other` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-registry/src/registry/mod.rs` | `other` | B | 40 | 6 | 12 | 2 | 0 | 251 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/crates/journal-registry/src/registry/monitor.rs` | `other` | A | 0 | 4 | 0 | 0 | 0 | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-registry/src/repository/collection.rs` | `other` | B | 48 | 9 | 0 | 0 | 61.94 | 197 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-registry/src/repository/error.rs` | `other` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-registry/src/repository/file.rs` | `other` | B | 86 | 8 | 0 | 0 | 60.51 | 392 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/crates/journal-registry/src/repository/metadata.rs` | `other` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/crates/journal-registry/src/repository/mod.rs` | `other` | B | 14 | 1 | 58 | 8 | 100 | 311 | low; reasonable | small repeated blocks; monitor |
| `rust/src/crates/journal-registry/src/time_range.rs` | `other` | A | 0 | 0 | 0 | 0 | - | 0 | low; reasonable | low; reasonable |
| `rust/src/internal/testcmd/corpus_digest/src/main.rs` | `test_or_harness` | A | 23 | 10 | 0 | 0 | 0 | 214 | test/harness metric; not production coverage signal | low; reasonable |
| `rust/src/internal/testcmd/corpus_experiment/src/main.rs` | `test_or_harness` | C | 201 | 11 | 115 | 9 | 0 | 907 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/internal/testcmd/corpus_regenerate/src/main.rs` | `test_or_harness` | C | 70 | 12 | 131 | 9 | 0 | 359 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/internal/testcmd/dataset_ingester/src/main.rs` | `test_or_harness` | C | 89 | 12 | 80 | 7 | 0 | 468 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/internal/testcmd/livewriter/src/main.rs` | `test_or_harness` | C | 58 | 11 | 58 | 6 | 0 | 343 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/internal/testcmd/netdata_function_wrapper/src/main.rs` | `test_or_harness` | A | 32 | 12 | 0 | 0 | 0 | 151 | test/harness metric; not production coverage signal | low; reasonable |
| `rust/src/internal/testcmd/reader_core_bench/src/main.rs` | `test_or_harness` | B | 191 | 12 | 0 | 0 | 0 | 936 | test/harness metric; not production coverage signal | low; reasonable |
| `rust/src/internal/testcmd/writer_core_bench/src/main.rs` | `test_or_harness` | C | 113 | 12 | 132 | 13 | 0 | 664 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/journal/src/directory.rs` | `rust_public` | C | 193 | 12 | 122 | 8 | 63.04 | 687 | moderate file-size pressure; functions stay below CCN gate | high production duplication; follow-up refactor candidate |
| `rust/src/journal/src/export.rs` | `rust_public` | B | 27 | 8 | 18 | 2 | 83.47 | 138 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/journal/src/facade.rs` | `rust_public` | C | 139 | 5 | 34 | 4 | 59.70 | 547 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/journal/src/lib.rs` | `rust_public` | C | 122 | 12 | 47 | 5 | 78.90 | 665 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/journal/src/parse.rs` | `rust_public` | A | 20 | 11 | 0 | 0 | 85.11 | 55 | low; reasonable | low; reasonable |
| `rust/src/journal/src/reader_helpers.rs` | `rust_public` | B | 57 | 10 | 25 | 3 | 65.57 | 247 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/journal/src/sealed_verify.rs` | `rust_public` | B | 180 | 12 | 19 | 3 | 74.15 | 744 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/journal/src/tests.rs` | `test_or_harness` | C | 30 | 4 | 92 | 10 | - | 395 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/journal/src/tests/facade.rs` | `test_or_harness` | C | 44 | 4 | 150 | 17 | - | 780 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/journal/src/tests/verification.rs` | `test_or_harness` | C | 21 | 8 | 69 | 7 | - | 221 | test/harness metric; not production coverage signal | test/harness repetition; not production coverage signal |
| `rust/src/journal/src/verify_graph.rs` | `rust_public` | A | 10 | 7 | 7 | 1 | 100 | 183 | low; reasonable | small repeated blocks; monitor |
| `rust/src/journal/src/verify_graph/hash.rs` | `rust_public` | A | 2 | 2 | 0 | 0 | 88.89 | 15 | low; reasonable | low; reasonable |
| `rust/src/journal/src/verify_graph/header.rs` | `rust_public` | B | 61 | 11 | 0 | 0 | 85.59 | 123 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/journal/src/verify_graph/io.rs` | `rust_public` | B | 42 | 11 | 12 | 2 | 64 | 107 | moderate file-size pressure; functions stay below CCN gate | small repeated blocks; monitor |
| `rust/src/journal/src/verify_graph/validation.rs` | `rust_public` | B | 123 | 10 | 0 | 0 | 88.84 | 430 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
| `rust/src/journal/src/verify_graph/walk.rs` | `rust_public` | B | 182 | 12 | 0 | 0 | 83.39 | 585 | moderate file-size pressure; functions stay below CCN gate | low; reasonable |
