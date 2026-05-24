# Node.js Journal SDK

Pure-JavaScript systemd journal reader and writer SDK. No native addons,
no system journal library linkage.

## Current Features

### Reader

- Read `.journal`, `.journal~`, `.journal.zst`, `.journal~.zst` files
- Zstd compression support via built-in `node:zlib`
- Forward/backward iteration, cursors, timestamps
- Binary field values as `Buffer`
- Field enumeration and unique value queries
- Export, JSON, and text output formatting
- libsystemd-compatible `SdJournal` facade
- Directory iteration across multiple files

### Writer

- Create regular, non-compact, keyed-hash journal files
- Byte-safe field values via `Buffer`/`Uint8Array`
- Optional zstd-compressed DATA object writing via `compression: 'zstd'`
- Append entries with BigInt timestamps and sequence numbers
- Directory writer with source-scoped systemd active/archive names, rotation, and retention

### journalctl

- `--file` and `--directory` options
- `--output=default|json|export`
- `--list-boots` and `--fields`
- `--head` and `--tail`
- Repeated same-field OR matching and `+` disjunction
- Daemon-only commands (sync, flush, rotate, verify) return errors

## Installation

Requires Node.js v22+ (for built-in `node:zlib` zstd support).

```bash
npm install .
```

## Basic Reader Usage

```javascript
import { SdJournalOpen } from '@netdata/systemd-journal-sdk';

const journal = SdJournalOpen('/path/to/journal', 0);

journal.seekHead();
while (journal.next() !== 0) {
  const entry = journal.getEntry();
  const message = entry.fields['MESSAGE'];
  if (message) {
    console.log(message.toString('utf8'));
  }
}

journal.close();
```

## Binary Field Values

```javascript
import { SdJournalOpen } from '@netdata/systemd-journal-sdk';

const journal = SdJournalOpen('/path/to/journal', 0);

journal.addMatch(Buffer.from('MESSAGE=hello'));

// Binary values are returned as Buffer
journal.seekHead();
while (journal.next() !== 0) {
  const entry = journal.getEntry();
  const binary = entry.fields['BINARY_PAYLOAD'];
  if (binary) {
    console.log(Buffer.isBuffer(binary)); // true
  }
}

journal.close();
```

## Writer Usage

```javascript
import { createJournal } from '@netdata/systemd-journal-sdk';

const writer = createJournal('/path/to/plugin.journal');

writer.append([
  { name: 'MESSAGE', value: Buffer.from('plugin started') },
  { name: 'PRIORITY', value: Buffer.from('6') },
  { name: 'SYSLOG_IDENTIFIER', value: Buffer.from('netdata-plugin') },
]);

writer.close();
```

## Directory Writer Usage

```javascript
import { Log } from '@netdata/systemd-journal-sdk';

const journal = new Log('/path/to/journal-dir', {
  source: 'system',
  maxEntries: 100000,
  maxBytes: 128 * 1024 * 1024,
  maxFiles: 10,
  maxRetentionBytes: 1024 * 1024 * 1024,
});

journal.append([
  { name: 'MESSAGE', value: 'plugin started' },
  { name: 'PRIORITY', value: '6' },
]);

journal.close();
```

`Log` stores files below `<directory>/<machine-id>/`, writes the active file as
`<source>.journal`, archives closed/rotated files as
`<source>@<seqnum-id>-<head-seqnum>-<head-realtime>.journal`, and only deletes
owned archived files for retention.

## journalctl CLI

```bash
node cmd/journalctl/index.js --file fixtures/systemd/test-data/no-rtc/system.journal.zst --head 1 --output=json

node cmd/journalctl/index.js --directory /var/log/journal --list-boots

node cmd/journalctl/index.js --file ./sample.journal PRIORITY=3 PRIORITY=4 + MESSAGE=boot
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
- `SdJournalQueryUnique(journal, fieldName)` - Get unique `[fieldName, Buffer]` values for a field
- `SdJournalListBoots(journal)` - List boot entries
- `SdJournalAddMatch(journal, data)` - Add match filter (AND)
- `SdJournalAddDisjunction(journal)` - Add OR group for subsequent matches
- `SdJournalSetOutputMode(journal, mode)` - Set output format

### Writer API

- `createJournal(path, options)` - Create new journal file
- `writer.append(fields, options)` - Append entry
- `writer.sync()` - Sync to disk
- `writer.close()` - Close and finalize
- `new Log(directory, options)` - Create a high-level directory writer
- `log.append(fields, options)` - Append through the directory writer
- `log.sync()` - Sync the active journal file
- `log.close()` - Archive the active file and apply retention
- `log.activeFile()` - Return the current active file path
- `log.journalDirectory()` - Return the machine-id journal directory

## Limitations

- Compact journal format not supported
- Xz/lz4 compressed DATA objects and xz/lz4 DATA writing not supported
- Forward Secure Sealing (FSS) not implemented
- Full journal verification not implemented
- `--follow` not supported (event-loop blocking concern)
- Daemon-only operations not supported

## Dependencies

No external npm packages. Uses only built-in Node.js modules:

- `node:fs` - File I/O
- `node:zlib` - Zstd compression (Node.js v22+)
- `node:util` - Argument parsing

## Conformance

The adapter passes the shared conformance test manifest:

```bash
npm test
```

`npm test` syntax-checks the package, verifies the SDK entry point can be
imported, and runs every case in `../tests/conformance/manifests/conformance-v01.json`.
