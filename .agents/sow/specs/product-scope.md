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
- Cross-language writer/reader interoperability tests.
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
