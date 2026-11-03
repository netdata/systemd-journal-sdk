---
name: project-journal-compatibility
description: "Use when changing journal file readers, writers, verifiers, fixtures, conformance or interoperability tests, or the Rust/Go journalctl rewrites."
---
# Project Journal Compatibility

Files written by any SDK must be readable by systemd and by every other SDK,
and the SDKs must read what systemd writes, including while a writer is
appending. This skill names what must be protected and which tests prove it.
Project-wide rules (targets, runtime purity, performance contract, Go/Rust
parity, CLI exceptions) live in `AGENTS.md`; they are not repeated here.

Detailed per-surface controls from past findings are in
[references/validation-recipes.md](references/validation-recipes.md). Read the
section for the surface you are changing; it is not a checklist for every change.

## Authority

- systemd source and tests are the authority when docs and implementation
  disagree: SDK format behavior targets `v260.1`, the journalctl rewrites
  target `v262` (see `AGENTS.md`).
- journalctl parity claims need the pinned full-feature `v262` oracle passed
  with `--journalctl PATH` to every helper. Host builds differ in version and
  features, so they cannot certify parity.
- When porting systemd tests, record each excluded upstream test with its
  reason, and extract its file-level behavior where practical.
- Never probe the workstation's live journal: no `systemd-cat`, `logger`,
  bare `journalctl`, or writes under `/var/log/journal` or `/run/log/journal`.
  Use `--file` or repository-local `--directory` fixtures only. Live probing
  mutates or depends on host state and makes results unreproducible.

## Format invariants

Each of these was a real compatibility requirement; breaking one produces
files systemd rejects or reads differently.

- Jenkins lookup3 must match systemd `jenkins_hashlittle2()`, including
  `0xdeadbeefdeadbeef` for the empty payload.
- Writers sort ENTRY DATA references by DATA offset and remove duplicates by
  default, as systemd does. A trusted-unique option may skip only duplicate
  removal, never sorting.
- Writers expose a raw `KEY=value` payload layer and a structured binary-safe
  `{name, value}` layer (the hot path), with the same three field-name
  policies: `JOURNALD` (default, keeps trusted fields such as `_HOSTNAME`),
  `JOURNAL-APP` (application-facing journald restrictions, drops invalid
  fields) and `RAW` (anything DATA can represent: non-empty, no `=`). The SDK
  never remaps producer-specific names; consumers do that before calling.
- Readers use the on-disk `header_size`, never the in-memory v260 struct size,
  and never expose bytes past it as newer fields.
- Historical unkeyed files (no `HEADER_INCOMPATIBLE_KEYED_HASH`) must open with
  the unkeyed hash path and their compression flags. Writers may refuse to
  append to them, but must refuse cleanly before any mutation.
- Directory writers replace an unsupported active file the way systemd does:
  keep sequence identity from readable header metadata, move the old file to a
  collision-safe `*.journal~` name, and create a fresh active file. Never
  archive it into readable history.
- Regular uncompressed deterministic output is byte-identical to the systemd
  `v260.1` reference ingester for the accepted corpus, across online, offline
  and archived final states.
- Stable-file checks (verification, append-open, excluded-writer snapshots)
  validate declared arenas and header counters; live readers must not, because
  a writer may publish capacity before allocating it.
- Changes to `journal-engine` index semantics, `FileIndexKey`, serialized
  index contents or cache partitioning must decide whether `CACHE_VERSION`
  changes, and record the reindex impact or why old entries stay safe.
- Writer feature target: compression and Forward Secure Sealing. Reader
  support for historical files is separate from writer milestones.

## Live concurrency

The contract is one writer plus many readers on the same file. A writer is
production-compatible only if stock `journalctl --file` and stock libsystemd
read it safely while it appends; a reader only if it safely reads files every
repository writer (and, where testable, systemd) is appending. Closed-file
`--verify` and smoke tests do not establish this.

- Writer tests use a monotonically increasing sequence field (default
  `LIVE_SEQ`) so stock readers prove complete ordered visibility; the harness
  is under `tests/conformance/live/`.
- Stock-reader adapters may retry transient `ENODATA` or partial snapshots
  only while the writer is active; after it exits, final ordered reads and
  `journalctl --verify --file` must pass.
- `live_publish_every_entries` defaults to `1` (stock-follow compatible).
  Other values are not stock-follow compatible unless the live matrix covers
  that exact mode.

## Recovery

Reader recovery never mutates source journals. Keep static damaged-file
extraction, real writer publication and synthetic fault injection separate in
evidence: restoring bytes by hand does not prove what journald does. Exercise
recovery through public default constructors and with fresh queries, because
an earlier full drain can activate recovery and hide a missing-pointer bug.
Record expected errors for corrupted fixtures; never skip them silently.

## Performance evidence

The performance contract is in `AGENTS.md`. When timing a change, run the same
frozen helper sources against baseline and candidate, keep allocation separate
from elapsed time, and remember sampled cumulative allocation is not peak
memory. A failing or crashing baseline cannot certify a speedup.

## Test matrices

Choose the matrices that cover what changed, and run shared tests against both
SDKs; one language's tests never stand in for the other's.

| Change | Run |
|---|---|
| Live writer/reader behavior | `tests/interoperability/run_live_matrix.py` (regular, zstd/xz/lz4, compact, compact+compression, sealed; stock journalctl and libsystemd, all SDK readers, final verify) |
| Arena growth / publication window | `tests/interoperability/run_live_growth.py` (supplements the live matrix) |
| Compressed DATA writing | `tests/interoperability/run_compression_matrix.py` |
| Compact writing | `tests/interoperability/run_compact_matrix.py` |
| Deterministic layout | `tests/interoperability/run_byte_identity.py --final-state all` (requires `all_equal: true`, including hash-chain depth) |
| Optional writer-lock helper | `tests/interoperability/run_lock_matrix.py` |
| Directory reading / `--directory` | `tests/interoperability/run_directory_matrix.py`, `tests/interoperability/run_mixed_directory_matrix.py` |
| journalctl query / follow | `tests/interoperability/run_journalctl_query_matrix.py` |
| journalctl options / parser | `tests/parser-parity/check_v260_manifest.py`, `tests/parser-parity/run_parser_parity.py` |
| Verifier | `tests/interoperability/run_verify_matrix.py` (stock `journalctl --verify --file` from `PATH`, version recorded in the report, oracle pinning tracked by SOW-0169; Rust and Go agree on regular, zstd/xz/lz4, compact, compact+compression and sealed files, and on object type/size, payload hash, DATA hash-table bucket, entry-array, header-counter, seqnum, monotonic and TAG/FSS corruption) |
| Traversal, compressed readers, recovery | `tests/interoperability/run_reader_hardening.py` with both adapters built and an explicit stock oracle |
| Reader performance | `tests/benchmarks/run_reader_regression_comparison.py` |

Commands with native dependencies (the CLIs) need native OS CI jobs; a
CGO-disabled SDK cross-build says nothing about CLI portability.

## Done

A compatibility change is done when the shared conformance tests and the
matrices covering it pass for every affected language and file variant,
cross-language files round-trip for every affected writer/reader pair, stock
readers read live files from each affected writer, and the SOW records the
stock systemd version, commands, durations, entry and reader counts, and
failure criteria used.
