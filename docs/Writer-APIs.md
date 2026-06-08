# Writer APIs

## Writer Layers

```text
Consumer
  |
  +-- High-level directory writer
  |     active file, rotation, retention, naming, lifecycle events
  |
  +-- Direct-file writer
  |     one journal file, caller controls lifecycle
  |
  +-- Structured append
  |     field name + byte value, canonical producer hot path
  |
  +-- Raw append
        full KEY=value bytes, systemd-compatible low-level payload path
```

## Structured Append

Structured append is the production hot path when the caller already has field
names and values separately.

Use it for:

- NetFlow, SNMP traps, OTEL logs, and other structured ingestion;
- binary values;
- avoiding `KEY=value` construction and re-parsing;
- avoiding avoidable allocations.

Rust has two structured surfaces:

- high-level `journal::Log` accepts the public SDK field shapes;
- low-level direct-file writers use `journal_log_writer::StructuredField`,
  `journal_log_writer::EntryField`, or the same types from
  `journal_core::file`.

Go uses `journal.Field`. Node.js and Python expose equivalent `{name, value}`
shapes.

## Raw Append

Raw append accepts full `KEY=value` bytes. The first `=` splits the field name
from the value. Later `=` bytes and arbitrary value bytes are preserved.

Use it only when:

- the caller already has valid systemd-style payloads;
- compatibility with a lower-level journal payload API is the goal;
- benchmark comparability with systemd `sd_journal_sendv()`-style payloads is
  required.

Do not convert structured data to `KEY=value` just to call raw append. That is
avoidable work.

## Field-Name Policies

The docs use language-neutral spec names. Language APIs use native casing and
names:

| Spec Policy | Rust | Go | Use Case | Compatibility |
|---|---|---|---|---|
| `JOURNALD` | `FieldNamePolicy::Journald` | `FieldNamePolicyJournald` | trusted producer emulating journald or a journald-like backend | stock systemd-friendly |
| `JOURNAL-APP` | `FieldNamePolicy::JournalApp` | `FieldNamePolicyJournalApp` | untrusted application input accepted through journald rules | stock systemd-friendly |
| `RAW` | `FieldNamePolicy::Raw` | `FieldNamePolicyRaw` | exact file-format experiments or producers that need names beyond systemd rules | journal file only; stock systemd tooling may reject |

Producer-specific remapping does not belong in the SDK. Transform fields before
calling the writer.

## Direct-File Writer

Use direct-file writing when the caller owns one journal file and lifecycle:

- create or append supported files;
- choose regular or compact layout;
- choose DATA compression;
- set live publication cadence;
- close online, close offline, or archive through explicit calls;
- optionally enable FSS.

Direct append-open of arbitrary historical/systemd-created variants is not a
general compatibility promise. Unsupported append targets return controlled
errors before mutation.

## High-Level Directory Writer

Use the directory writer for production ingestion directories:

- active file management;
- chain active naming by default;
- optional strict systemd active naming;
- rotation by entries, size, or file duration;
- retention by file count, bytes, or age;
- retention enforcement on open and on demand;
- lifecycle events for created, rotated, and retained-deleted files.

The directory writer follows journald-style reliable-open behavior: if an
existing active file cannot be appended safely, it archives or disposes that
file and creates a new active file.

## Live Publication

`live_publish_every_entries` controls when the writer explicitly publishes
metadata for stock live readers:

- `1`: default, systemd-compatible publication after every entry;
- `0`: disable explicit publication for poll/snapshot consumers;
- `N > 1`: publish every `N` entries.

This is not `fsync`. It is a visibility/wakeup setting. Setting it above `1`
can improve throughput but narrows live-follow freshness.

## Compact Format

Regular format remains the default. Compact format is explicit.

Compact can reduce file footprint and can be preferred by Netdata-style
backends. It has a 4 GiB offset ceiling and uses compact object layout. Confirm
the target reader/tooling compatibility before making compact the default for a
consumer.

## Compression

Compression is optional and uses common compression libraries, not systemd
libraries.

Compression can reduce disk footprint, but it adds CPU cost and can slow reads
when query paths need compressed DATA. Explorer paths avoid decompressing
unrelated DATA where possible, but returned rows, FTS, and selected compressed
facet/histogram values still require decompression.

## Forward Secure Sealing

FSS adds TAG/HMAC data for tamper evidence. Use it when sealed verification is
required. It increases write work and verification work; it is not a general
query acceleration feature.

Rust low-level FSS options are exposed through
`journal_core::seal::SealOptions`. Go exposes `journal.SealOptions`.

## Optional Locking And Identity

The journal file format requires one writer per file as an operational contract,
but systemd does not define a portable journal-file lock protocol.

The SDK exposes optional cooperating-writer lock helpers. Core writer
constructors do not acquire them automatically.

Core writers also do not discover host identity. Pass machine ID, boot ID, and
timestamps explicitly, or call an optional helper outside the core writer path.

Rust low-level writer locks are exposed through `journal_core::file::lock`.
Go exposes `journal.AcquireWriterLock`. These helpers are independent from
systemd compatibility.
