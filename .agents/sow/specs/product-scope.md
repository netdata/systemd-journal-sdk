# Product Scope Specification

## Purpose

This project produces pure SDKs and file-backed journalctl-compatible tools for systemd journal files.

## Language Targets

- Rust
- Go
- Node.js
- Python

## Delivery Priority

- The Go writer is the first implementation deliverable after the shared test harness is accepted.
- The Go writer is prioritized because the user needs a pure-Go journal writer for a Netdata plugin integration.
- The Go writer must support binary field values before later SDK phases continue, because the Netdata plugin integration requires byte-safe payloads.
- Rust, Go reader/journalctl completion, Node.js, Python, full interoperability, benchmarks, and optimization remain required, but they must not be started ahead of the Go writer unless the user changes this priority.

## Core Contracts

- Implementations must not link to system journal libraries.
- Go implementations must not use CGO.
- Node.js implementations must not use native addons.
- Python implementations must not use native journal bindings.
- Each language must provide two API layers: an idiomatic SDK API and a libsystemd-compatible reader facade.
- The libsystemd-compatible reader facade is required unless a SOW records concrete evidence that it would require native bindings, violate the pure-language policy, or create an unsafe/unrepresentable API in that language.
- Pure-language dependencies are allowed after dependency review.
- Cross-language interoperability is mandatory: every reader must read journal files produced by every writer.
- The system must preserve systemd journal file concurrency expectations: one writer and multiple readers may operate on the same journal file according to journal rules.
- Live concurrency compatibility is a MUST, not a follow-up optimization. No writer or reader implementation may be called production-compatible until this is confirmed with stock systemd tooling and the shared cross-language suite.

## Live Concurrency Compatibility Contract

Writer compatibility:

- Every writer implementation must produce files that stock `journalctl --file` can read while the writer is still appending.
- Every writer implementation must produce files that stock libsystemd journal readers can read while the writer is still appending.
- Every writer implementation must support one active writer and multiple concurrent readers on the same file.
- Every writer implementation must keep stock readers safe during append publication windows; stock readers must not crash, report corruption for committed entries, spin, or require the writer to close the file before reading.
- Every writer implementation must pass stock `journalctl --verify --file` after clean close and after tested interruption/reopen scenarios for the feature slice claimed by that writer.

Reader compatibility:

- Every reader implementation must read files while they are being appended by stock systemd journal writers when the test environment can provide one.
- Every reader implementation must read files while they are being appended by every writer implementation in this repository.
- Every reader implementation must correctly handle online journal state, tail metadata changes, entry-array growth, data hash-table growth by chaining, and observable file-size changes without treating normal live updates as corruption.
- Every reader implementation must support multiple readers observing the same live file concurrently.
- Reader follow/tail behavior must be validated against stock `journalctl` semantics for file-backed operation.

Required validation evidence:

- A committed live-concurrency harness must exercise stock `journalctl --file` readers against each repository writer while appends are in progress.
- A committed live-concurrency harness must exercise stock libsystemd reader APIs against each repository writer while appends are in progress.
- The shared live-concurrency harness lives under `tests/conformance/live/` and uses a configurable monotonically increasing sequence field, defaulting to `LIVE_SEQ`, so stock readers prove ordered complete visibility.
- Stock reader adapters may retry transient active-writer `ENODATA` open/read failures or partial snapshots only while the writer is active; after the writer exits, final ordered reads and `journalctl --verify --file` must pass without masking incompatibility.
- A committed live-concurrency harness must exercise each repository reader against live files produced by each repository writer.
- Where a stock systemd writer can be exercised without violating repository-boundary rules, each repository reader must be tested against it. If the environment cannot provide a stock writer safely, the SOW must record the missing evidence and cannot claim full reader compatibility.
- Compatibility claims must record exact stock systemd version, commands/helpers used, stress duration, entry counts, reader counts, and failure criteria.
- Smoke tests are not sufficient for production compatibility; stress tests and race-window tests are required.

## Compatibility Baseline

Baseline compatibility target:

```text
systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced
tag: v260.1
```

Known reference evidence:

```text
systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced
man/journalctl.xml
src/libsystemd/sd-journal/journal-def.h
src/libsystemd/sd-journal/sd-journal.c
test/journal-data/
test/test-journals/
test/units/TEST-04-JOURNAL*.sh
```

Netdata Rust source evidence:

```text
ktsaou/netdata @ 6a515000ac89
src/crates/jf/
src/crates/journal-core/
src/crates/journal-log-writer/
```

## Test Scope

In scope:

- systemd journal file/API tests applicable to pure SDK behavior.
- systemd importer tests applicable to journal file parsing.
- systemd journal fixtures and corrupted journal fixtures.
- File-backed journalctl behavior against journal files or journal directories.
- Live stock `journalctl` and stock libsystemd reader behavior against actively written journal files.
- Cross-language writer/reader interoperability tests.
- Cross-language live writer/reader concurrency tests.
- Benchmarks, profiling, and optimization evidence.

Out of scope:

- journald daemon lifecycle.
- systemd service management.
- journal-remote, journal-gatewayd, and journal-upload services.
- varlink service APIs.
- socket activation.
- daemon setup for Forward Secure Sealing.
- reboot/boot lifecycle tests.

## Writer Target

Final writer target:

- keyed hash;
- regular and compact journal formats where applicable;
- compression where systemd journal files define it;
- Forward Secure Sealing where systemd journal files define it.

Delivery may be phased. Earlier phases may write a smaller feature subset if the SOW records the gap, shared readers/tests support the compatibility envelope, and follow-up SOWs track the remaining writer features.

Current Go writer feature slice:

- regular, non-compact, uncompressed journal files;
- keyed hash tables using the journal file ID;
- byte-safe DATA field values through `Field.Value []byte`;
- live one-writer/multiple-reader compatibility with stock `journalctl --file` and stock libsystemd readers for the current writer slice.

## Reader Target

Readers must support applicable historical journal files represented by the shared fixture suite, including corrupted fixture behavior where the expected result is a controlled error or partial recovery.

## journalctl Target

Implement journalctl rewrites in Rust, Go, Node.js, and Python for file-backed/query behavior.

Matching semantics:

- Different fields are ANDed.
- Repeated matches for the same field are OR alternatives.
- The `+` separator creates explicit disjunction groups and must be replicated for file-backed journalctl behavior.
- No new `KEY in [values]` syntax is required.

Daemon-only commands are not implemented in this project. They must return documented unsupported behavior rather than silently pretending to perform daemon operations.

Daemon-only commands include:

- sync;
- flush;
- rotate;
- relinquish-var;
- smart-relinquish-var.

## Repository Boundary

Implementation and review agents may inspect external references read-only when the active SOW requires it.

They must not write, edit, delete, move, reset, checkout, install, generate, cache, or format anything outside this repository.

The only write exception outside the repository is `/tmp`. Prefer `.local/` inside this repository for scratch work.

## Open Questions

None currently blocking bootstrap. Implementation-phase SOWs may expose narrower decisions and must record them before coding starts.
