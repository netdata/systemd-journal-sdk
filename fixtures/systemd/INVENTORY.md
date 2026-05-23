# systemd v260.1 Test and Fixture Inventory

**Source:** `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
**Baseline:** tag `v260.1`

## Inventory Methodology

All journal-related test sources in systemd v260.1 were surveyed.
Files were classified as:

- **include**: file-backed behavior testable without daemon/system integration
- **exclude**: daemon-only behavior requiring running systemd-journald
- **defer**: large/archived/generated; recorded for future regeneration

## Source File Inventory

### `src/libsystemd/sd-journal/test-journal*.c`

| File | Category | Decision | Reason | Extractable Behavior |
|------|----------|----------|--------|---------------------|
| `test-journal-file.c` | file-format/parsing | **include** | Tests `journal_file_parse_uid_from_filename` - pure filename parsing, no daemon needed | User journal filename conventions, UID extraction, error cases (`-EISDIR`, `-EREMOTE`, `-EINVAL`, `-ENXIO`) |
| `test-journal-verify.c` | verification/sealing | **include** | Tests `journal_file_verify` with FSS key - generates test journal, seals it, flips bits to test detection | Sealing semantics, FSS key verification, bit-toggling detection, compression toggle |
| `test-journal-match.c` | matching/queries | **include** | Tests `sd_journal_add_match/disjunction/conjunction` - match string construction and boolean logic | AND/OR match semantics, empty-field handling, cursor test, `journal_make_match_string` output format |
| `test-journal-stream.c` | stream/interleaving | **include** | Tests multi-journal directory with `sd_journal_open_directory`, backward/forward iteration, unique values | Directory-based journal opening, `SD_JOURNAL_FOREACH`, `sd_journal_query_unique`, `sd_journal_test_cursor` |
| `test-journal-append.c` | corruption/resilience | **include** | Bit-flip corruption during write - tests resilience | Corruption recovery, write-after-corrupt behavior, error handling on corrupted journals |
| `test-journal-send.c` | entry-send | **defer** | Uses `sd_journal_send` which requires daemon socket | Runtime journald socket required - cannot test file-backed without daemon |
| `test-journal-init.c` | initialization | **defer** | Uses `sd_journal_init_namespace` and daemon queries | Requires running systemd-journald |
| `test-journal-dump.c` | dump/print | **include** | Tests `journal_file_dump` - header and entry printing | Header structure, entry dump format |
| `test-journal-enum.c` | enumeration | **include** | Tests field enumeration with `sd_journal_query_unique` | `SD_JOURNAL_FOREACH_UNIQUE`, field listing |
| `test-journal-flush.c` | flush/sync | **defer** | Uses `sd_journal_flush_matches` with runtime daemon | Daemon-coupled behavior |
| `test-journal.c` | integration | **defer** | Full `sd_journal` integration - most calls require daemon or live journal | Requires running journald + live journal socket |
| `test-journal-interleaving.c` | stream/cursor/seek | **include** | 1341-line test: multi-file directory reading, skip/seek, cursor validation, boot IDs, sequence numbers, match filtering, realtime/monotonic seek. Generates journals dynamically; extractable behaviors are directory open, forward/backward iteration, `sd_journal_next_skip`/`sd_journal_previous_skip`, `sd_journal_seek_head`/`sd_journal_seek_tail`, `sd_journal_test_cursor`, `sd_journal_seek_monotonic_usec`, `sd_journal_seek_realtime_usec`, boot ID enumeration, sequence number continuity, match filtering with `sd_journal_add_match` | Directory-based multi-file journal reading, skip/seek operations, cursor validity testing, boot ID handling, sequence number tracking, realtime/monotonic timestamp seeking, match filtering during iteration |

### `src/test/test-journal-importer.c`

| File | Category | Decision | Reason | Extractable Behavior |
|------|----------|----------|--------|---------------------|
| `test-journal-importer.c` | import/parse | **include** | Tests `journal_importer_process_data` - parses text exports into iovec entries | Entry field count, field ordering (`_BOOT_ID`, `_TRANSPORT`, `COREDUMP_*`, `_SOURCE_REALTIME_TIMESTAMP`), EOF behavior, `journal_importer_eof` |

### Ancillary sd-journal tests

| File | Category | Decision | Reason | Extractable Behavior |
|------|----------|----------|--------|---------------------|
| `test-catalog.c` | catalog parsing | **exclude** | Tests `catalog_import_file` for systemd message catalog parsing - not journal file read/write behavior | Catalog file format parsing, not relevant to journal SDK |
| `test-mmap-cache.c` | memory mapping | **exclude** | Tests `MMapCache` internal memory mapping cache - internal infrastructure, not journal file format | mmap cache behavior, not relevant to journal SDK |
| `test-audit-type.c` | audit labels | **exclude** | Tests `audit_type_name_alloca` for Linux audit type labels - not journal file behavior | Audit type string mapping, not relevant to journal SDK |

### `test/journal-data/`

| File | Size | Decision | Reason | Extractable Behavior |
|------|------|----------|--------|---------------------|
| `journal-1.txt` | 586 bytes | **include** | Source fixture for `test-journal-importer.c`; copied into repo | Text-export format test data used by importer tests |
| `journal-2.txt` | 513 bytes | **include** | Source fixture for `test-journal-importer.c`; copied into repo | Text-export format test data |

### `test/test-journals/`

| File/Dir | Decision | Reason | Extractable Behavior |
|------|----------|--------|---------------------|
| `no-rtc/` (7 `.zst` files) | **include** | Portable compressed test journals without RTC - all 7 files copied into repo | Boot-id/time handling without RTC, archive journal files, multi-file directory iteration |
| `corrupted/` (3 `.zst` files) | **include** | AFL-generated corrupted journals for resilience testing - all 3 files copied into repo | Corruption resilience, truncated zstd frames, AFL fuzz corpus |
| `afl-corrupted-journals.tar.zst` (94KB) | **defer** | Large tarball; individual components already in `corrupted/` | Regenerate by extracting tarball if needed |

### `test/units/TEST-04-JOURNAL*.sh`

All 16 TEST-04-JOURNAL shell tests are daemon/integration tests requiring live systemd-journald.
Individual disposition below:

| File | Decision | Reason | Extractable File-Backed Behavior |
|------|----------|--------|---------------------|
| `TEST-04-JOURNAL.sh` | **exclude** | Main test orchestrator; requires live journald, systemctl, varlink | None extractable |
| `TEST-04-JOURNAL.journal.sh` | **exclude** | Daemon-dependent throughout; uses `systemctl`, `journalctl --sync`, `journalctl --rotate`, live cursors | None extractable |
| `TEST-04-JOURNAL.corrupted-journals.sh` | **include** | File-backed only: extracts `.zst` archives, runs `journalctl --file`, `--verify`, `--output=export`, `--grep`, `--fields`, `--list-boots` against file paths | `journalctl --file` with corrupted journals, `--verify` detection, `--output=export` format, `--grep` pattern matching, `--fields` enumeration, `--list-boots` output |
| `TEST-04-JOURNAL.fss.sh` | **defer** | FSS (Forward Secure Sealing) - requires daemon key generation; extractable as separate fixture if key material is provided | FSS sealing and verification with pre-generated keys |
| `TEST-04-JOURNAL.journal-append.sh` | **include** | Tests journal file append behavior with `journalctl --file`; file-backed append and rotation | Journal append semantics, file rotation, `--file` reading of appended journals |
| `TEST-04-JOURNAL.journal-corrupt.sh` | **include** | File-backed corruption: writes journal, corrupts it, verifies with `journalctl --verify --file` | Corruption detection via `--verify`, file-backed corruption resilience |
| `TEST-04-JOURNAL.journal-remote.sh` | **exclude** | Tests `systemd-journal-remote` daemon; requires network sockets and remote journal protocol | Daemon-coupled; remote journal protocol not file-backed |
| `TEST-04-JOURNAL.journal-gatewayd.sh` | **exclude** | Tests `systemd-journal-gatewayd` HTTP daemon; requires running HTTP service | Daemon-coupled; HTTP gateway not file-backed |
| `TEST-04-JOURNAL.journalctl-varlink.sh` | **exclude** | Tests journalctl varlink interface; requires varlink socket | Daemon-coupled; varlink protocol not file-backed |
| `TEST-04-JOURNAL.LogFilterPatterns.sh` | **include** | Tests `journalctl --file` with log filter patterns (`--grep`, `--priority`, `--unit`); file-backed | `--grep` pattern matching, `--priority` filtering, `--unit` filtering on file-backed journals |
| `TEST-04-JOURNAL.SYSTEMD_JOURNAL_COMPRESS.sh` | **include** | Tests `SYSTEMD_JOURNAL_COMPRESS` environment variable with `journalctl --file`; file-backed compression | Compression flag handling, compressed journal reading via `--file` |
| `TEST-04-JOURNAL.bsod.sh` | **exclude** | Tests BSOD (blue screen of death) journal entries; requires live system crash simulation | Daemon-coupled; crash simulation not file-backed |
| `TEST-04-JOURNAL.cat.sh` | **include** | Tests `journalctl --file --output=cat` for raw message output; file-backed | `--output=cat` raw message extraction from file-backed journals |
| `TEST-04-JOURNAL.invocation.sh` | **include** | Tests `journalctl --file` with `_SYSTEMD_INVOCATION_ID` field; file-backed | Invocation ID field parsing from file-backed journals |
| `TEST-04-JOURNAL.reload.sh` | **exclude** | Tests journal reload behavior with live daemon; requires `systemctl reload` | Daemon-coupled; reload requires running journald |
| `TEST-04-JOURNAL.stopped-socket-activation.sh` | **exclude** | Tests socket activation behavior with stopped journald; requires systemd socket activation | Daemon-coupled; socket activation not file-backed |

## Fixture Copy Policy

- **Copied into repo**: All 7 compressed journals from `no-rtc/` (total ~1.66 MiB), all 3 corrupted journals from `corrupted/` (total ~4.6 KiB), importer test data (`journal-1.txt` at 586 bytes, `journal-2.txt` at 513 bytes).
- **Not copied**: Large archives (`afl-corrupted-journals.tar.zst` - 94KB tarball of AFL corpus; extract on demand), daemon-dependent test data.
- **Deferred regeneration**: FSS key fixtures - require `journalctl --setup-keys` from a live daemon.

## Included Fixtures

| Fixture | Size | Provenance | Purpose |
|------|------|------------|---------|
| `fixtures/systemd/test-data/no-rtc/system.journal.zst` | 297687 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/no-rtc/system.journal.zst` | RTC-less journal file for reader tests |
| `fixtures/systemd/test-data/no-rtc/system@0005ebbfd42fc981-39a8842ec948769a.journal~.zst` | 382554 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/no-rtc/system@0005ebbfd42fc981-39a8842ec948769a.journal~.zst` | RTC-less archived journal for multi-file directory tests |
| `fixtures/systemd/test-data/no-rtc/system@0005ebbfd4346b9f-43185b46162d9fa5.journal~.zst` | 403217 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/no-rtc/system@0005ebbfd4346b9f-43185b46162d9fa5.journal~.zst` | RTC-less archived journal for multi-file directory tests |
| `fixtures/systemd/test-data/no-rtc/system@0005ebbfd4385848-2e5dff5354ab9bcf.journal~.zst` | 288274 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/no-rtc/system@0005ebbfd4385848-2e5dff5354ab9bcf.journal~.zst` | RTC-less archived journal for multi-file directory tests |
| `fixtures/systemd/test-data/no-rtc/user-1000.journal.zst` | 64937 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/no-rtc/user-1000.journal.zst` | RTC-less user journal for UID-specific reader tests |
| `fixtures/systemd/test-data/no-rtc/user-1000@0005ebbfd660bcbe-dbef2eee11f4b575.journal~.zst` | 88958 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/no-rtc/user-1000@0005ebbfd660bcbe-dbef2eee11f4b575.journal~.zst` | RTC-less archived user journal for multi-file directory tests |
| `fixtures/systemd/test-data/no-rtc/user-1000@0005ebbfe89faec4-a5e890e7b00bedd1.journal~.zst` | 129152 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/no-rtc/user-1000@0005ebbfe89faec4-a5e890e7b00bedd1.journal~.zst` | RTC-less archived user journal for multi-file directory tests |
| `fixtures/systemd/test-data/corrupted/zstd-truncated-frame.zst` | 189 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/corrupted/zstd-truncated-frame.zst` | Corrupted truncated zstd frame for error-resilience tests |
| `fixtures/systemd/test-data/corrupted/id:000000,sig:06,src:000711,time:110015157,execs:33104794,op:MOpt_havoc,rep:2.zst` | 1461 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/corrupted/id:000000,sig:06,src:000711,time:110015157,execs:33104794,op:MOpt_havoc,rep:2.zst` | AFL fuzz-generated corrupted journal for resilience tests |
| `fixtures/systemd/test-data/corrupted/id:000000,src:000031,time:210669947,execs:34191940,op:havoc,rep:32.zst` | 2969 bytes | `systemd/systemd @ c0a5a2516d...`, `test/test-journals/corrupted/id:000000,src:000031,time:210669947,execs:34191940,op:havoc,rep:32.zst` | AFL fuzz-generated corrupted journal for resilience tests |
| `fixtures/systemd/test-data/journal-1.txt` | 586 bytes | `systemd/systemd @ c0a5a2516d...`, `test/journal-data/journal-1.txt` | Journal export text fixture for importer tests |
| `fixtures/systemd/test-data/journal-2.txt` | 513 bytes | `systemd/systemd @ c0a5a2516d...`, `test/journal-data/journal-2.txt` | Journal export text fixture for importer tests |

## Excluded with Reason

| Excluded Item | Reason |
|------|--------|
| `test-journal-send.c` | `sd_journal_send` requires daemon socket |
| `test-journal-init.c` | Uses runtime machine-id lookup |
| `test-journal-flush.c` | Runtime daemon coupling |
| `test-journal.c` (full) | Integration test requiring live journal |
| `test-catalog.c` | Catalog parsing, not journal file read/write |
| `test-mmap-cache.c` | Internal mmap cache, not journal file format |
| `test-audit-type.c` | Audit type labels, not journal file behavior |
| `afl-corrupted-journals.tar.zst` | Redundant with contents already in `corrupted/` |
| `TEST-04-JOURNAL.sh` | Main orchestrator, requires live journald |
| `TEST-04-JOURNAL.journal.sh` | Daemon-dependent throughout |
| `TEST-04-JOURNAL.journal-remote.sh` | Remote journal daemon, network sockets |
| `TEST-04-JOURNAL.journal-gatewayd.sh` | HTTP gateway daemon |
| `TEST-04-JOURNAL.journalctl-varlink.sh` | Varlink interface, daemon socket |
| `TEST-04-JOURNAL.bsod.sh` | Crash simulation, requires live system |
| `TEST-04-JOURNAL.reload.sh` | Daemon reload, requires running journald |
| `TEST-04-JOURNAL.stopped-socket-activation.sh` | Socket activation, daemon-coupled |

## Deferred Items

1. **FSS/sealed journal fixtures** - Generate using `journalctl --force --setup-keys --output=json` from a live systemd-journald. Record here when generated.
2. **Imported Rust journal fixture** - Rust SDK (SOW-0004) will generate a reference journal file for cross-language interoperability.
3. **Large corrupted corpus** - Extracted from `afl-corrupted-journals.tar.zst` on demand via `tests/conformance/scripts/extract-corrupted.sh`.
