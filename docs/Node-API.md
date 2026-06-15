# Node.js API

Every example on this page is syntax-checked and executed by repository CI
against synthetic fixtures, except blocks marked illustrative-only.

Install the Node.js package from the repository checkout:

```sh
cd node
npm install
```

The package is published as `@netdata/systemd-journal-sdk` and exposes ES
module APIs:

<!-- illustrative-only: import fragment shown alone -->
```javascript
import { FileReader, Log } from '@netdata/systemd-journal-sdk';
```

The Node.js SDK is pure JavaScript. It does not load native addons or link to
system journal libraries. Node.js is a compatibility and integration surface;
use Rust or Go for high-throughput production ingestion and query paths unless
a fresh benchmark proves the Node.js path fits the deployment.

## Read One File

Use `FileReader` when the caller owns ordering and reads one journal file.

<!-- verify-example: lang=javascript id=node-read-one-file -->
```javascript
import { FileReader } from '@netdata/systemd-journal-sdk';

const reader = FileReader.open('/var/log/journal/example/system.journal');
try {
  reader.addMatch(Buffer.from('PRIORITY=6'));
  reader.seekHead();

  while (reader.step()) {
    const entry = reader.getEntry();
    const message = entry.fields.MESSAGE;
    if (message) {
      console.log(message.toString('utf8'));
    }
  }
} finally {
  reader.close();
}
```

`getEntry()` materializes maps and owned payloads. It is convenient, but it is
not the lowest-cost scan path.

## Scan Payloads With Minimal Work

Use `visitEntryPayloads()` when the consumer can work with `FIELD=value` bytes.

<!-- verify-example: lang=javascript id=node-visit-entry-payloads -->
```javascript
import { FileReader } from '@netdata/systemd-journal-sdk';

const reader = FileReader.open('/var/log/journal/example/system.journal');
try {
  reader.seekHead();

  while (reader.step()) {
    reader.visitEntryPayloads((payload) => {
      const prefix = Buffer.from('MESSAGE=');
      if (payload.subarray(0, prefix.length).equals(prefix)) {
        console.log(payload.subarray(prefix.length).toString('utf8'));
      }
    });
  }
} finally {
  reader.close();
}
```

Node.js uses bounded positioned-read windows in the default package because
Node core has no portable mmap API. Current-row payload buffers remain valid
until the row changes or the reader closes; copy when longer ownership is
required.

## Enumerate Current-Row DATA With Row Lifetime

Use `entryDataRestart()` and `enumerateEntryPayload()` for facade-style
current-row DATA enumeration.

<!-- verify-example: lang=javascript id=node-entry-data-enumeration -->
```javascript
import { FileReader } from '@netdata/systemd-journal-sdk';

const reader = FileReader.open('/var/log/journal/example/system.journal');
try {
  reader.seekHead();

  if (reader.step()) {
    reader.entryDataRestart();
    for (;;) {
      const payload = reader.enumerateEntryPayload();
      if (payload === null) break;
      console.log(payload.toString('utf8'));
    }
  }
} finally {
  reader.close();
}
```

Do not keep row-scoped buffers after advancing, seeking, restarting DATA
enumeration, refreshing, or closing the reader. Copy when longer ownership is
required.

## Read A Directory

Use `DirectoryReader` for stock-like ordering across active and archived files.

<!-- verify-example: lang=javascript id=node-read-directory -->
```javascript
import { DirectoryReader } from '@netdata/systemd-journal-sdk';

const reader = DirectoryReader.open('/var/log/journal');
try {
  reader.seekTail();

  while (reader.stepBack()) {
    const realtime = reader.getRealtimeUsec();
    const entry = reader.getEntry();
    const message = entry.fields.MESSAGE;
    if (message) {
      console.log(String(realtime), message.toString('utf8'));
    }
  }
} finally {
  reader.close();
}
```

Directory reading discovers root journal files plus one machine-ID
subdirectory level and merges files in journal order.

## Use Snapshot Bounds For Query Workloads

The default Node.js reader uses bounded positioned-read windows. Use snapshot
bounds when a query may ignore entries appended after it starts.

<!-- verify-example: lang=javascript id=node-snapshot-bounds -->
```javascript
import {
  FileReader,
  READER_BOUNDS_SNAPSHOT,
} from '@netdata/systemd-journal-sdk';

const reader = FileReader.open('/var/log/journal/example/system.journal', {
  bounds: READER_BOUNDS_SNAPSHOT,
});
try {
  reader.seekHead();
  console.log(reader.accessStats().selectedAccessMode);
} finally {
  reader.close();
}
```

The default package does not advertise mmap as an available mode. Optional
native mmap support belongs behind a separate package/API boundary and is not
part of this default runtime path.

## Query Unique Values Through Indexes

Unique values for one field should use the FIELD object's DATA chain, not a row
scan.

<!-- verify-example: lang=javascript id=node-unique-values -->
```javascript
import { FileReader } from '@netdata/systemd-journal-sdk';

const reader = FileReader.open('/var/log/journal/example/system.journal');
try {
  for (const value of reader.queryUnique('SYSLOG_IDENTIFIER')) {
    console.log(value.toString('utf8'));
  }
} finally {
  reader.close();
}
```

Use `queryUnique()` when the caller needs an owned array of values.

## Explorer Query

Explorer is the API for filters, facets, histogram, FTS, and selected returned
rows.

<!-- verify-example: lang=javascript id=node-explorer-query -->
```javascript
import { ExplorerQuery, FileReader } from '@netdata/systemd-journal-sdk';

const reader = FileReader.open('/var/log/journal/example/system.journal');
try {
  const query = new ExplorerQuery()
    .withFilter('PRIORITY', ['3', '4'])
    .withFacet('SYSLOG_IDENTIFIER')
    .withHistogram('PRIORITY');

  const result = reader.explore(query);
  console.log(String(result.stats.rowsMatched));
} finally {
  reader.close();
}
```

Default Explorer behavior:

- `ExplorerStrategy.Traversal`;
- `ExplorerFieldMode.FirstValue`;
- source realtime enabled;
- indexed filters;
- all-field expansion only for returned rows.

Do not enable `debugCollectColumnFieldsByRowTraversal` in production.

## Compare Explorer Strategies

Use `ExplorerStrategy.Compare` to validate a query shape before selecting the
index strategy.

<!-- verify-example: lang=javascript id=node-explorer-compare -->
```javascript
import {
  ExplorerFieldMode,
  ExplorerQuery,
  ExplorerStrategy,
  FileReader,
} from '@netdata/systemd-journal-sdk';

const reader = FileReader.open('/var/log/journal/example/system.journal');
try {
  const query = new ExplorerQuery().withFacet('PRIORITY');
  query.fieldMode = ExplorerFieldMode.AllValues;
  query.useSourceRealtime = false;
  query.limit = 0;

  const result = reader.exploreWithStrategy(query, ExplorerStrategy.Compare);
  if (result.comparison) {
    console.log(result.comparison.traversalDuration);
    console.log(result.comparison.indexDuration);
  }
} finally {
  reader.close();
}
```

The index strategy is exact only for its supported subset. It is not a
universal faster mode.

## Write One File

Use direct-file writing when the caller owns the file lifecycle.

<!-- verify-example: lang=javascript id=node-write-one-file -->
```javascript
import {
  createJournal,
  stringField,
} from '@netdata/systemd-journal-sdk';

const writer = createJournal('/var/log/journal-sdk/example.journal');
try {
  writer.append([
    stringField('MESSAGE', 'plugin started'),
    stringField('PRIORITY', '6'),
    stringField('SYSLOG_IDENTIFIER', 'example-plugin'),
  ]);
} finally {
  writer.close();
}
```

`append()` is the structured hot path for producers that already have field
names and values split.

## Write Binary Fields

<!-- verify-example: lang=javascript id=node-write-binary -->
```javascript
import {
  binaryField,
  createJournal,
  stringField,
} from '@netdata/systemd-journal-sdk';

const writer = createJournal('/var/log/journal-sdk/example.journal');
try {
  writer.append([
    stringField('MESSAGE', 'sample with binary payload'),
    binaryField('BINARY_PAYLOAD', Buffer.from([0x00, 0x01, 0x02, 0xff])),
  ]);
} finally {
  writer.close();
}
```

Binary values are preserved as field values. The field name remains text.

## Raw Append

Use `appendRaw()` only when the caller already has `KEY=value` payloads.

<!-- verify-example: lang=javascript id=node-raw-append -->
```javascript
import { createJournal } from '@netdata/systemd-journal-sdk';

const writer = createJournal('/var/log/journal-sdk/example.journal');
try {
  writer.appendRaw([
    Buffer.from('MESSAGE=prebuilt payload'),
    Buffer.from('_HOSTNAME=synthetic-host'),
    Buffer.from([0x42, 0x49, 0x4e, 0x41, 0x52, 0x59, 0x5f, 0x50, 0x41, 0x59, 0x4c, 0x4f, 0x41, 0x44, 0x3d, 0x00, 0x01, 0x02, 0xff]),
  ]);
} finally {
  writer.close();
}
```

The first `=` byte splits the field name from the value. Later `=` bytes and
arbitrary value bytes are preserved.

## Directory Writer With Rotation And Retention

Use `Log` for production ingestion directories.

<!-- verify-example: lang=javascript id=node-directory-writer -->
```javascript
import {
  LOG_IDENTITY_STRICT,
  LOG_OPEN_EAGER,
  Log,
} from '@netdata/systemd-journal-sdk';

const machineId = Buffer.from('00112233445566778899aabbccddeeff', 'hex');
const bootId = Buffer.from('ffeeddccbbaa99887766554433221100', 'hex');

const log = new Log('/var/log/journal-sdk', {
  source: 'example-plugin',
  openMode: LOG_OPEN_EAGER,
  identityMode: LOG_IDENTITY_STRICT,
  machineId,
  bootId,
  compact: true,
  livePublishEveryEntries: 64,
  rotationPolicy: {
    maxEntries: 100000,
    maxBytes: 128 * 1024 * 1024,
    maxDurationUsec: 3_600_000_000n,
  },
  retentionPolicy: {
    maxFiles: 8,
    maxBytes: 1024 * 1024 * 1024,
    maxAgeUsec: 7n * 24n * 3_600_000_000n,
  },
});
try {
  log.append([
    { name: 'MESSAGE', value: Buffer.from('plugin started') },
    { name: 'PRIORITY', value: Buffer.from('6') },
  ]);
} finally {
  log.close();
}
```

`Log` stores files below `<directory>/<machine-id>/`. By default it uses
Netdata-compatible chain active names. Use `strictSystemdNaming` only when the
consumer needs `<source>.journal` active naming.

## Field-Name Policy

<!-- verify-example: lang=javascript id=node-field-name-policy -->
```javascript
import {
  FIELD_NAME_POLICY_JOURNALD,
  createJournal,
  stringField,
} from '@netdata/systemd-journal-sdk';

const writer = createJournal('/tmp/example.journal', {
  fieldNamePolicy: FIELD_NAME_POLICY_JOURNALD,
});
try {
  writer.append([
    stringField('MESSAGE', 'trusted producer'),
  ]);
} finally {
  writer.close();
}
```

Use:

- `FIELD_NAME_POLICY_JOURNALD` for trusted journald-like producers;
- `FIELD_NAME_POLICY_JOURNAL_APP` for untrusted application-facing rules;
- `FIELD_NAME_POLICY_RAW` only for file-format-level tools and tests.

`RAW` files are journal files, but stock systemd tooling is not guaranteed to
accept invalid systemd field names.

## Optional Writer Lock

Core writers do not lock. Acquire the optional cooperating-writer lock helper
when the deployment needs SDK-level exclusion.

<!-- verify-example: lang=javascript id=node-writer-lock -->
```javascript
import { WriterLock } from '@netdata/systemd-journal-sdk';

const lock = WriterLock.acquire('/var/log/journal-sdk/example.journal');
try {
  console.log(lock.path.endsWith('.lock'));
} finally {
  lock.release();
}
```

This helper is independent from systemd compatibility.

## Netdata Function Boundary

Use the Netdata function API when the consumer needs Netdata-shaped logs
function output.

<!-- verify-example: lang=javascript id=node-netdata-function -->
```javascript
import {
  NetdataFunctionRunOptions,
  NetdataJournalFunction,
} from '@netdata/systemd-journal-sdk';

const fn = NetdataJournalFunction.systemdJournal();
const request = {
  after: 0,
  before: 0,
  last: 200,
  facets: ['PRIORITY', 'SYSLOG_IDENTIFIER'],
  histogram: 'PRIORITY',
};

const response = fn.runDirectoryRequestJsonWithOptions(
  '/var/log/journal',
  request,
  NetdataFunctionRunOptions.fromTimeoutSeconds(30),
);
console.log(JSON.stringify(response));
```

Customize `NetdataFunctionConfig.sourceSelectorName` and
`sourceSelectorHelp` when the same function shape serves a domain-specific
journal backend. The wire id remains `__logs_sources`; only the label and help
shown by Netdata change.

<!-- verify-example: lang=javascript id=node-netdata-source-selector -->
```javascript
import {
  NetdataFunctionConfig,
  NetdataJournalFunction,
  SystemdJournalProfile,
} from '@netdata/systemd-journal-sdk';

const config = NetdataFunctionConfig.systemdJournal();
config.sourceSelectorName = 'Trap Jobs';
config.sourceSelectorHelp = 'Select the trap job to query';
const fn = NetdataJournalFunction.new(config, new SystemdJournalProfile());
console.log(fn !== null);
```

This layer is Netdata-specific. Generic log explorers should use Explorer
directly unless they need the Netdata request and response shape.

## Verify A File

<!-- verify-example: lang=javascript id=node-verify-file -->
```javascript
import { verifyFile } from '@netdata/systemd-journal-sdk';

verifyFile('/var/log/journal/example/system.journal');
```

Use `verifyFileWithKey()` for sealed files when a verification key is
available. Verification is for integrity checks, not normal query serving.
File-path verification uses the same bounded reader access architecture as
normal file reads, so it avoids whole-file resident buffers while still walking
the object graph and sealed HMAC ranges.
