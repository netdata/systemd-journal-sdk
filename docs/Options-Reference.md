# Options Reference

This page summarizes options that change compatibility, performance, or
operational behavior. Names use Rust-style spelling first and mention other
languages where useful.

Spec-level names are written in uppercase where they are language-neutral. Rust
and Go APIs use native names, shown in the relevant tables.

## Reader Options

| Option | Default | Effect | Production Guidance |
|---|---:|---|---|
| `ReaderBounds::Live` | yes | Allows the reader to observe published appends during a session. | Use for live file-backed readers. |
| `ReaderBounds::Snapshot` | no | Fixes file bounds at open time. | Use for polling/query workloads where entries appended after query start do not matter. |
| windowed mmap | yes in Rust live readers | Maps rolling file windows. | Good default for large files and bounded virtual memory. |
| whole-file mmap | explicit | Maps the full file. | Measure before enabling; it can reduce access overhead but increases virtual memory pressure. |
| positioned `ReadAt` fallback | target-dependent | Reads through positioned file I/O when mmap is unavailable or disabled. | Diagnostic or portability path, not the high-throughput Unix baseline. |

## Entry Access Options

| Surface | Cost | Use When |
|---|---|---|
| payload visitor | lowest | Consumer already works with `FIELD=value` bytes. |
| raw byte-name entry APIs | low to medium | RAW mode can contain non-UTF8 field names. |
| materialized entry maps | medium to high | Caller needs convenient field maps for selected rows. |
| JSON/export/text formatters | high | Caller is producing external output. |
| verifier APIs | high | Caller is validating integrity, not serving a query hot path. File-path verification is bounded but still walks the object graph and sealed HMAC ranges. |

## Writer Format Options

| Option | Default | Effect | Production Guidance |
|---|---:|---|---|
| regular format | yes | Stock systemd-compatible layout. | Use unless compact is explicitly desired and validated. |
| compact format | no | Smaller compact object layout with 32-bit offsets and 4 GiB ceiling. | Good for footprint-sensitive backends; validate target reader support. |
| keyed hash | yes | Uses the journal file ID for DATA/FIELD hashing. | Required for current writer baseline. |
| unkeyed append | unsupported for writers | Historical files may be readable but not safely appendable. | Directory writers rotate/dispose incompatible active files. |

## Compression Options

| Option | Default | Effect | Production Guidance |
|---|---:|---|---|
| no DATA compression | yes | Maximum write/read speed and no decompression cost. | Best baseline for high-throughput ingestion. |
| zstd DATA compression | explicit | Better footprint, extra CPU, compressed `FIELD=value` payloads. | Use when disk footprint dominates and queries rarely need compressed fields. |
| xz DATA compression | explicit | Higher compression cost. | Compatibility feature, not a high-throughput default. |
| lz4 DATA compression | explicit | Faster compression than xz, still requires decompression on read. | Measure for the workload. |
| compression threshold | systemd default | Compresses payloads above the configured threshold. | Keep default unless a footprint benchmark justifies tuning. |

Compressed DATA hides the field name because the entire `FIELD=value` payload is
compressed. Explorer can skip unrelated compressed DATA only when selected
facets, histogram, FTS, or returned-row expansion do not require it.

## Writer Visibility And Integrity Options

| Option | Default | Effect | Production Guidance |
|---|---:|---|---|
| `live_publish_every_entries = 1` | yes | Publishes metadata after every entry for stock live readers. | Use when stock follow-reader freshness matters. |
| `live_publish_every_entries = 0` | no | Disables explicit SDK live publication. | Use for poll/snapshot consumers after validating the integration. |
| `live_publish_every_entries = N` | no | Publishes every `N` entries. | Good batching compromise for latency-tolerant consumers. |
| FSS / seal options | no | Adds sealed TAG/HMAC tamper evidence. | Enable only when sealed verification is required. |
| optional writer lock helper | no | Cooperating SDK writer exclusion. | Acquire explicitly when deployment needs SDK-level exclusion. |

Live publication is not durability. Filesystem sync and crash-consistency policy
are separate operational choices.

Rust low-level FSS uses `journal_core::seal::SealOptions`; Go uses
`journal.SealOptions`. Rust low-level writer locks live under
`journal_core::file::lock`; Go uses `journal.AcquireWriterLock`.

## Field-Name Policy Options

| Spec Policy | Rust | Go | Default | Effect | Use When |
|---|---|---|---:|---|---|
| `JOURNALD` | `FieldNamePolicy::Journald` | `FieldNamePolicyJournald` | yes | Trusted journald-compatible field names, including protected `_` fields. | Backend acts as journald or a trusted journal producer. |
| `JOURNAL-APP` | `FieldNamePolicy::JournalApp` | `FieldNamePolicyJournalApp` | no | Untrusted application-facing journald rules; protected fields are dropped. | Emulate application logging through journald. |
| `RAW` | `FieldNamePolicy::Raw` | `FieldNamePolicyRaw` | no | Allows any non-empty field name without `=`. | File-format-level tooling or tests; stock systemd compatibility is not guaranteed. |

The SDK does not perform OTEL, Netdata, or application-specific remapping.
Transform fields before calling the SDK.

## Directory Writer Options

| Option | Default | Effect | Production Guidance |
|---|---:|---|---|
| chain active naming | yes | Uses `<source>@<seqnum-id>-<head-seqnum>-<head-realtime>.journal`. | Default for Netdata-style backends. |
| strict systemd naming | no | Uses `<source>.journal` active name. | Use when matching systemd naming is more important than chain naming. |
| entry-count rotation | disabled | Archives active file after configured entry envelope. | Use when row count bounds matter. |
| file-size rotation | disabled | Archives active file around configured size envelope. | Use for predictable file chunks. |
| duration rotation | disabled | Archives active file by time span. | Use when time-bounded files simplify retention/query. |
| file-count retention | disabled | Deletes older owned archives beyond count. | Use with size/age limits as needed. |
| byte-size retention | disabled | Deletes older owned archives beyond bytes. | Use to bound disk footprint. |
| age retention | disabled | Deletes older owned archives beyond age. | Use for time-based retention policy. |

Retention counts the tracked active/current file in file-count and byte
envelopes, but the tracked active/current file is not selected for deletion.

## Explorer Options

| Option | Default | Effect | Production Guidance |
|---|---:|---|---|
| `ExplorerStrategy::Traversal` | yes | Indexed filters plus candidate-row traversal for selected outputs. | Default production strategy. |
| `ExplorerStrategy::Index` | no | FIELD/DATA-chain aggregation for exact supported all-values shapes. | Use only after measuring or validating with `Compare`. |
| `ExplorerStrategy::Compare` | no | Runs traversal and index, verifies equality, returns diagnostics. | Testing/validation only. |
| `ExplorerFieldMode::FirstValue` | yes | Counts at most one value per selected field per row and can stop early. | Default for explorer/UI workloads. |
| `ExplorerFieldMode::AllValues` | no | Counts repeated same-field values exactly. | Use only when duplicate same-field values matter. |
| FTS terms | none | Requires payload content matching. | Enable only for text search requests. |
| returned rows | caller-selected | Expands all fields only for selected returned rows. | Keep row limits bounded. |
| `debug_collect_column_fields_by_row_traversal` | disabled/rejected in production | Discovers columns by scanning rows. | Debug discrepancies only; invalid production or benchmark evidence. |

## Netdata Function Options

| Option | Effect | Production Guidance |
|---|---|---|
| timeout | Cancels long function runs and can return partial table data. | Always wire for interactive services. |
| progress callback | Reports selected-file and scan progress. | Wire to Netdata function progress reporting. |
| cancellation callback | Lets caller stop work when the client disappears. | Always wire in request/response services. |
| caller state | Carries source metadata and learned realtime drift. | Use when replacing Netdata plugin behavior. |

The Netdata function layer is an adapter over Explorer. Keep application
presentation and enrichment there, not in core journal file code.
