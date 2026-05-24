# Python Journal SDK

Python systemd journal reader and writer SDK. No native journal bindings,
no system journal library linkage.

## Current Features

### Reader

- Read `.journal`, `.journal~`, `.journal.zst`, `.journal~.zst` files
- Zstd compression support via Python standard library `compression.zstd`
- XZ DATA object support via Python standard library `lzma`
- LZ4 DATA object support via `lz4` block compression
- Forward/backward iteration, cursors, timestamps
- Binary field values as `bytes`
- Field enumeration and unique value queries
- Export, JSON, and text output formatting
- libsystemd-compatible `SdJournal` facade
- Directory iteration across multiple files

### Writer

- Create regular, non-compact, keyed-hash journal files
- Byte-safe field values via `bytes`/`bytearray`/`memoryview`
- Optional zstd, xz, and lz4-compressed DATA object writing via
  `compression: 'zstd'`, `compression: 'xz'`, or `compression: 'lz4'`
- Append entries with integer timestamps and sequence numbers
- Directory writer with source-scoped systemd active/archive names, rotation, and retention
- Pure cross-SDK cooperative lockfile with stale-owner detection, plus a secondary advisory `flock`, to prevent multiple SDK writers from opening the same file
- Native systemd writers do not participate in the SDK lock protocol and remain an operational exclusion

### journalctl

- `--file` and `--directory` options
- `--output=default|json|export`
- `--list-boots` and `--fields`
- `--head` and `--tail`
- Repeated same-field OR matching and `+` disjunction
- Daemon-only commands (sync, flush, rotate, verify) return errors

## Requirements

Python 3.14+ (for `compression.zstd` standard library module) and
`lz4==4.4.5` for LZ4 DATA object compression/decompression.

## Basic Reader Usage

```python
from journal import SdJournalOpen

journal = SdJournalOpen('/path/to/journal', 0)

journal.seek_head()
while journal.next() != 0:
    entry = journal.get_entry()
    msg = entry['fields'].get(b'MESSAGE')
    if msg:
        print(msg.decode('utf-8'))

journal.close()
```

## Binary Field Values

```python
from journal import SdJournalOpen

journal = SdJournalOpen('/path/to/journal', 0)
journal.add_match(b'BINARY_PAYLOAD=\xff\x00')

journal.seek_head()
while journal.next() != 0:
    entry = journal.get_entry()
    binary = entry['fields'].get(b'BINARY_PAYLOAD')
    if binary:
        print(bytes(binary))
```

## Writer Usage

```python
from journal import Writer

w = Writer.create('/path/to/plugin.journal')

w.append([
    {'name': 'MESSAGE', 'value': b'plugin started'},
    {'name': 'PRIORITY', 'value': b'6'},
    {'name': 'SYSLOG_IDENTIFIER', 'value': b'netdata-plugin'},
])

w.close()
```

`writer.close()` matches systemd's plain close path and leaves the file in
`ONLINE` state. Use `writer.close_offline()` to finalize a single file as
`OFFLINE`; directory rotation uses `writer.archive_to()` to produce `ARCHIVED`
files.

## Directory Writer Usage

```python
from journal import Log

journal = Log('/path/to/journal-dir', {
    'source': 'system',
    'max_entries': 100000,
    'max_bytes': 128 * 1024 * 1024,
    'max_files': 10,
    'max_retention_bytes': 1024 * 1024 * 1024,
})

journal.append([
    {'name': 'MESSAGE', 'value': b'plugin started'},
    {'name': 'PRIORITY', 'value': b'6'},
])

journal.close()
```

## journalctl CLI

```bash
python3 cmd/journalctl.py --file fixtures/systemd/test-data/no-rtc/system.journal.zst --head 1 --output=json

python3 cmd/journalctl.py --directory /var/log/journal --list-boots

python3 cmd/journalctl.py --file ./sample.journal PRIORITY=3 PRIORITY=4 + MESSAGE=boot
```

## API

### Reader API

- `SdJournalOpen(path, flags)` - Open journal file or directory
- `SdJournalNext(journal)` - Advance to next entry (returns 1 on success, 0 on EOF)
- `SdJournalPrevious(journal)` - Go to previous entry
- `SdJournalSeekHead(journal)` - Seek to first entry
- `SdJournalSeekTail(journal)` - Seek to last entry
- `SdJournalGetEntry(journal)` - Get current entry object
- `SdJournalGetCursor(journal)` - Get current cursor string
- `SdJournalTestCursor(journal, cursor)` - Test if cursor matches current position
- `SdJournalGetRealtimeUsec(journal)` - Get entry realtime timestamp
- `SdJournalEnumerateFields(journal)` - List all field names
- `SdJournalQueryUnique(journal, fieldName)` - Get unique `[fieldName, bytes]` values for a field
- `SdJournalListBoots(journal)` - List boot entries
- `SdJournalAddMatch(journal, data)` - Add match filter (AND)
- `SdJournalAddDisjunction(journal)` - Add OR group for subsequent matches
- `SdJournalSetOutputMode(journal, mode)` - Set output format

### Writer API

- `Writer.create(path, options)` - Create new journal file
- `writer.append(fields, options)` - Append entry
- `writer.sync()` - Sync to disk
- `writer.close()` - Close while preserving `ONLINE` state
- `writer.close_offline()` - Close with `OFFLINE` state
- `writer.archive_to(path)` - Rename and close with `ARCHIVED` state
- `Log(directory, options)` - Create a high-level directory writer
- `log.append(fields, options)` - Append through the directory writer
- `log.sync()` - Sync the active journal file
- `log.close()` - Archive the active file and apply retention
- `log.active_file()` - Return the current active file path
- `log.journal_directory()` - Return the machine-id journal directory

## Limitations

- Compact journal format not supported
- Forward Secure Sealing (FSS) not implemented
- Full journal verification not implemented
- `--follow` not supported (would block the process)
- Daemon-only operations not supported

## Dependencies

Uses Python standard library modules and one compression dependency:

- `os`, `struct`, `tempfile`, `time`, `json` - Core I/O and utilities
- `compression.zstd` - Zstd compression and decompression (Python 3.14+)
- `lzma` - XZ compression and decompression
- `lz4==4.4.5` - LZ4 block compression and decompression

## Conformance

The adapter passes the shared conformance test manifest:

```bash
python3 -m compileall python
python3 adapter.py list | python3 -c "import sys,json; tests=json.load(sys.stdin); print(f'{len(tests)} tests supported')"
```
