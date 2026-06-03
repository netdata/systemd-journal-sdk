# Per-Language Adapter Contract

## Role

Each language SDK (Rust, Go, Node.js, Python) ships an **adapter executable** that:

1. Receives a test case from the shared conformance harness
2. Runs the test against the language's journal implementation
3. Returns structured JSON results

The adapter is the only language-specific part of the conformance suite.

## Executable Contract

```
adapter [subcommand] [options]
```

### Subcommands

| Subcommand | Description |
|------------|-------------|
| `run` | Execute one test case provided on stdin |
| `list` | List all supported test case names |
| `probe` | Return adapter version and capabilities |

### `adapter run` - Input

The harness invokes `adapter run` with no positional manifest argument, then pipes a **test case JSON object** (matching `test_case` in `manifest-schema.json`) to stdin:

```json
{
  "test_name": "journal-file-parse-uid-from-filename",
  "category": "file-format",
  "description": "Verify UID extraction from user journal filenames",
  "fixtures": {
    "journal_file": {
      "type": "file",
      "path": "fixtures/systemd/test-data/no-rtc/system.journal.zst",
      "description": "Compressed system journal fixture"
    }
  },
  "adapter_cmd": [],
  "expected": {
    "result_format": "entry-list",
    "entries_match": [
      {"_BOOT_ID": "1531fd22ec84429e85ae888b12fadb91"},
      {"_TRANSPORT": "journal"}
    ]
  }
}
```

Fixture paths are repository-root relative. A `type: "file"` fixture may point
to either a single file or a directory; directory fixtures are journal
directories and adapters must iterate their journal files. Committed `.zst`
fixtures are compressed source fixtures; adapters must decompress or
stream-decompress them before parsing journal bytes unless a future harness
version explicitly materializes decompressed copies. Passing the suite must not
require libsystemd, systemd-journald, or any system journal library.

`adapter_cmd` contains the command arguments for the behavior under test, for
example journalctl-compatible CLI flags. Fixture locations always come from the
`fixtures` object, not positional command-line arguments.

### `adapter run` - Output

The adapter writes a **result object** to stdout:

```json
{
  "test_name": "journal-file-parse-uid-from-filename",
  "status": "PASS",
  "result_format": "entry-list",
  "actual": [
    {"_BOOT_ID": "1531fd22ec84429e85ae888b12fadb91"},
    {"_TRANSPORT": "journal"}
  ],
  "expected": [...],
  "duration_ms": 12,
  "note": null
}
```

**Required fields in result:**

- `test_name` (string)
- `status` (string: `"PASS"`, `"FAIL"`, `"ERROR"`, `"SKIP"`)
- `result_format` (string, mirrors input)
- `actual` (adapter-defined, matches expected structure)
- `duration_ms` (integer, wall-clock milliseconds)
- `error` (string, present when status is ERROR)

**Optional fields:**

- `note` (string): explanation for SKIP or unexpected behavior
- `evidence` (object): raw parsed data for debugging

### Exit Codes

| Exit code | Meaning |
|-----------|---------|
| 0 | Test executed; see JSON `status` for pass/fail |
| 1 | Adapter error - malformed input or internal failure |
| 2 | `SKIP` - adapter cannot run this test (missing deps, platform) |

### Result Format Mappings

| `result_format` in manifest | Expected `actual` type |
|---------------------------|------------------------|
| `entry-list` | `Entry[]` - array of field maps |
| `cursor-list` | `string[]` - cursor strings |
| `field-list` | `string[]` - field name strings |
| `export` | `string` - raw export output |
| `count` | `integer` |
| `boolean` | `boolean` |
| `error` | `string` (error message) |

### Expected Outcome Semantics

- `entries_match: true` means the adapter must perform the behavior-specific
  assertion named by the test and return evidence in `actual` or `evidence`.
- `entries_match` as an array of objects is an ordered expected subset unless
  the test note explicitly says exact output is required.
- `fields_present` lists keys that must be present in each relevant returned
  entry or object.
- `result_format: "error"` requires an `error` string containing
  `error_contains`; substring matching is case-insensitive.

## Adapter Interface (Per Language)

### Rust Adapter

```
rust/adapter --help
```

Exposes `run`, `list`, `probe` subcommands.

### Go Adapter

```
go/adapter --help
```

Exposes `run`, `list`, `probe` subcommands.

### Node.js Adapter

```
node/adapter --help
```

Exposes `run`, `list`, `probe` subcommands.

### Python Adapter

```
python/adapter --help
```

Exposes `run`, `list`, `probe` subcommands.

## Harness Runner

The shared harness at `tests/conformance/runner/` orchestrates:

1. Loads manifests from `tests/conformance/manifests/`
2. For each test case, forks the appropriate language adapter
3. Pipes the test case JSON to the adapter
4. Parses the JSON result
5. Compares `actual` vs `expected` per the manifest
6. Aggregates results

## Live Concurrency Harness

The live concurrency harness at `tests/conformance/live/` validates the
mandatory one-writer/multiple-reader compatibility contract. This harness is
separate from per-language adapters because it intentionally invokes stock
systemd tools as external compatibility oracles.

Writer live-test commands must:

1. create or open the requested journal file;
2. append at least one synthetic entry;
3. create the provided ready-file after the first entry is committed;
4. continue appending synthetic entries until the requested count or test mode
   is reached;
5. include a monotonically increasing `LIVE_SEQ` field starting at `000000`;
6. exit with the expected status.

The harness then runs these readers while the writer is still appending:

- stock `journalctl --file` polling readers;
- stock `journalctl --file --follow --no-tail --boot=all` readers;
- stock libsystemd readers compiled from `libsystemd_live_reader.c`.

All live readers validate the configured sequence field so publication-window
bugs cannot pass by exposing entries out of order. `--boot=all` is required for
`journalctl --follow` because stock journalctl enables current-boot filtering in
follow mode, while these compatibility fixtures use synthetic boot IDs.

The polling and follow `journalctl --file` readers, plus stock libsystemd
readers, may retry transient active-writer `ENODATA` open/read failures or
partial snapshots observed while a writer is actively mutating the file. The
final post-writer snapshot or stream must pass sequence validation and
`journalctl --verify`.

Production-compatible writer claims require the live harness to pass for the
claimed writer feature slice. Production-compatible reader claims require the
corresponding repository reader to handle live files produced by every
repository writer, plus stock systemd writer evidence where the environment can
provide it safely.

## Plug-In for New SDKs

To add a new language SDK:

1. Create `tests/conformance/manifests/<new-lang>-*.json`
2. Implement an adapter executable following the contract above
3. Register the adapter path in `tests/conformance/runner/config.json`
4. All existing manifests are language-neutral and need no changes
