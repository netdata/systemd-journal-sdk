---
name: project-journal-compatibility
description: "Mandatory compatibility rules when changing journal file readers, writers, fixtures, conformance tests, interoperability tests, or journalctl rewrites."
---
# Project Journal Compatibility

## Purpose

Keep all implementations aligned with the systemd journal file format, Netdata Rust reader/writer sources, and the project scope decisions.

## Scope

Use this skill when:

- importing or adapting Rust journal code;
- implementing journal readers or writers in Rust, Go, Node.js, or Python;
- porting systemd tests or fixtures;
- building shared interoperability tests;
- implementing journalctl rewrites.

Do not use this skill for:

- pure repository bootstrap work;
- generic SOW maintenance unrelated to journal behavior.

## Mandatory Knowledge

- Baseline compatibility target is `systemd/systemd` tag `v260.1`.
- The test scope is SDK conformance plus file-backed journalctl behavior.
- Do not implement daemon-only journalctl commands such as daemon sync, flush, rotate, or relinquish-var operations.
- journalctl already treats repeated matches for the same field as OR alternatives and different fields as AND.
- The `+` separator is a systemd journalctl disjunction feature to replicate for file-backed journalctl behavior; it is not a new extension.
- Each language must provide two API layers: idiomatic SDK API plus a libsystemd-compatible reader facade. The facade is required unless a SOW records concrete evidence that it would require native bindings, violate the pure-language policy, or create an unsafe/unrepresentable API in that language.
- The final writer target includes compression and Forward Secure Sealing, but implementation may be phased.
- Live concurrency compatibility is mandatory for every writer and reader. A writer is not production-compatible unless stock `journalctl --file` and stock libsystemd readers can safely read the file while that writer is appending. A reader is not production-compatible unless it can safely read files while they are being appended by each repository writer and, where testable without violating repository-boundary rules, stock systemd writers.
- The live concurrency contract is one writer plus multiple readers on the same journal file. Tests must cover online state, append publication windows, tail metadata changes, entry-array growth, reader follow/tail behavior, clean close verification, and interruption/reopen scenarios for the claimed feature slice.
- The reusable live-concurrency harness is under `tests/conformance/live/`. Writer tests should use the configured monotonically increasing sequence field, default `LIVE_SEQ`, so stock readers prove complete ordered visibility.
- Stock reader harness adapters may retry transient active-writer `ENODATA` open/read failures or partial snapshots only while the writer is active. After the writer exits, final ordered reads and `journalctl --verify --file` must pass.
- High-level directory writers must apply configured retention once when an active writer is opened or created. Existing-active reopen and eager open enforce during construction; lazy archived-only construction remains side-effect-free until the first append opens the active file, then retention runs before the first entry is written. Active/current files must remain protected and normal retention deletion lifecycle events must be reused.
- For deterministic regular uncompressed writer output, the layout target is byte-for-byte identity with the systemd v260.1 reference ingester for the accepted corpus. Writers must match systemd object order, alignment, initial allocation envelope, v260 header fields, entry-array growth, tail metadata, and hash-chain header behavior for that slice.
- Deterministic byte-identity validation must cover systemd final-state variants: online/plain close, offline close, and archived close.
- Header readers must use the on-disk `header_size` when validating object and hash-table locations. Do not compare historical file offsets against the current in-memory v260 `JournalHeader` struct size, and do not expose bytes beyond the on-disk header as newer header fields.
- Smoke tests are not sufficient evidence for production compatibility. SOW validation must record exact stock systemd version, commands/helpers, stress duration, entry counts, reader counts, and failure criteria.
- Common compression-library dependencies are allowed after dependency review. Journal parsing/writing must not depend on systemd/libjournal; CGO, native Node.js runtime addon loading, and linking to system journal libraries remain disallowed unless the user explicitly changes those separate constraints. Dependency packages may ship native artifacts if the SDK runtime path is constrained and tested to use only non-native implementations (e.g. WASM) and does not load or link native code at runtime.
- Every external-agent prompt must include the canonical repository-boundary block verbatim from `AGENTS.md` or `.agents/skills/project-agent-orchestration/SKILL.md`.
- Compatibility probing must not use the workstation's live journal. Do not run `systemd-cat`, `logger`, live `journalctl`, or write under `/var/log/journal` or `/run/log/journal`; stock-reader validation must use `journalctl --file` or repository-local `--directory` fixtures only.

## Best Practices

- Treat systemd source and tests as the compatibility authority when documentation and implementation disagree.
- Keep shared tests language-neutral and run them against all SDKs.
- Add shared live-concurrency tests before accepting a writer or reader as production-compatible.
- Prove cross-language interoperability with files written by each implementation and read by every implementation.
- Prove stock-reader interoperability while repository writers are actively appending, not only after close.
- For live writer/reader compatibility changes, run
  `tests/interoperability/run_live_matrix.py` and require the feature matrix to
  cover regular, zstd/xz/lz4 DATA compression, compact, compact plus DATA
  compression, and sealed/FSS files. The matrix must include stock
  `journalctl --file`, stock libsystemd, all repository readers, final
  `journalctl --verify --file`, and `--verify-key` for sealed files.
- For compressed-DATA writer changes, run `tests/interoperability/run_compression_matrix.py`
  and require the structural oracle to validate object order, offsets, flags,
  counters, hash-chain consistency, tail metadata, references, and compression
  flags plus stock journalctl, stock libsystemd, and all repository reader
  checks.
- For compact writer changes, run `tests/interoperability/run_compact_matrix.py`
  and require the structural oracle to validate compact object layout,
  32-bit compact offset constraints, optional compression flags, stock
  journalctl, stock libsystemd, and all repository reader checks.
- For deterministic writer layout changes, run
  `tests/interoperability/run_byte_identity.py --final-state all` and require
  `all_equal: true` for the accepted uncompressed corpus unless the active SOW
  records exact byte deltas and a user decision changes the pass condition.
  The accepted corpus must keep deliberate DATA hash-bucket collisions, and the
  byte-identity runner must fail if `data_hash_chain_depth` does not match the
  systemd reference value for every language and final state.
- For writer lock changes, run `tests/interoperability/run_lock_matrix.py`
  and require all SDK writer pairs to reject a second active writer before the
  contender publishes a ready file, plus stale-lock cleanup after crashed
  writers.
- For directory reader or file-backed `journalctl --directory` changes, run
  `tests/interoperability/run_directory_matrix.py` and require stock
  journalctl plus Rust, Go, Node.js, and Python rewrites to agree on traversal,
  ordering, filtering, boot listing, corrupt-file skipping, empty directories,
  and repository `.journal.zst` directory discovery.
- For mixed directory feature changes, run
  `tests/interoperability/run_mixed_directory_matrix.py` and require stock
  journalctl plus Rust, Go, Node.js, and Python rewrites to agree on mixed
  regular/compact files, uncompressed and zstd/xz/lz4 DATA-compressed files,
  sealed/unsealed reads, directory verification with and without keys, and the
  repository whole-file `.journal.zst` directory extension.
- For file-backed journalctl query or follow changes, run
  `tests/interoperability/run_journalctl_query_matrix.py` and require stock
  journalctl plus Rust, Go, Node.js, and Python rewrites to agree on
  `--since`/`--until` realtime ranges, `--boot` descriptors, and live
  `--follow` reads from actively appended file and directory inputs, including
  no-tail, default-tail, and boot-plus-since cases.
- For verifier changes, run `tests/interoperability/run_verify_matrix.py` and
  require stock `journalctl --verify --file` plus Rust, Go, Node.js, and Python
  verification paths to agree on positive regular, zstd/xz/lz4 DATA-compressed,
  compact, compact plus DATA-compressed, and sealed files, and on negative
  object type, object size, payload hash, hash-chain, entry-array,
  header-counter, seqnum, monotonic, and TAG/FSS corruption classes.
- Separate reader support for existing historical files from writer feature milestones.
- Record excluded upstream tests with a reason and extract file-level behavior where practical.

## Bad Practices

- Do not implement daemon/service behavior just to satisfy journalctl daemon-control options.
- Do not rely on one language's tests as sufficient for all languages.
- Do not claim compatibility from closed-file `journalctl --verify` alone.
- Do not treat live-reader smoke tests as sufficient for production compatibility.
- Do not load or link native code at runtime in any SDK implementation to pass tests; dependency packages may ship native artifacts if the SDK runtime path uses only non-native implementations and does not load or link native code.
- Do not silently skip corrupted fixture behavior; record expected errors and recovery behavior.

## Workflow Checklist

1. Confirm the active SOW names the exact compatibility surface being changed.
2. Identify relevant systemd tests, fixtures, and Netdata Rust source paths.
3. Add or update shared tests before accepting implementation as complete.
4. Add or update live stock-reader and cross-language concurrency tests before accepting writer/reader compatibility.
5. Run the same conformance suite across every affected language.
6. Run interoperability tests across every writer/reader pair affected by the SOW.
7. Run live one-writer/multiple-reader tests for every affected writer and reader.
8. Record benchmark or profiling evidence when the SOW includes performance claims.

## Validation Checklist

Before claiming production-grade compatibility:

- Shared conformance tests pass for every targeted language.
- Cross-language writer/reader matrix passes for every targeted file variant.
- journalctl behavior is tested against file-backed fixtures.
- Stock `journalctl --file` reads each targeted writer's files while the writer is appending.
- Stock libsystemd reader APIs read each targeted writer's files while the writer is appending.
- Each targeted reader reads live files produced by each targeted writer.
- Reader follow/tail behavior is compared with stock `journalctl` file-backed behavior.
- Daemon-only journalctl commands are not implemented and have documented behavior.
- Dependency audit confirms no CGO, native Node.js runtime addon loading/linking, or system journal library linkage in the SDK runtime path unless a SOW records an explicit user policy change.

## Evidence

- `AGENTS.md`: project goals and scope decisions.
- `.agents/sow/specs/product-scope.md`: product scope and compatibility contracts.
- `.agents/sow/done/SOW-0001-20260523-project-bootstrap-and-orchestration.md`: initial decisions and evidence ledger.

## Update Rules

Update this skill when:

- compatibility baseline changes;
- a new journal file feature becomes in scope;
- reviewer findings expose a missed compatibility or validation requirement;
- a phase adds a durable implementation workflow that future agents must repeat.
