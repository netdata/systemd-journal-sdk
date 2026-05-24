# SOW-0007 - Python SDK And journalctl

## Status

Status: in-progress

Sub-state: active after Go, Rust, and Node.js SDK slices completed.

## Requirements

### Purpose

Implement the Python SDK and file-backed journalctl rewrite without native journal bindings.

### Assistant Understanding

Facts:

- Python must implement the shared SDK contract without native journal bindings or system journal library linkage.
- The shared harness, Go SDK, Rust SDK, and Node.js SDK slices are complete enough to act as implementation references for this phase.
- Runtime check on this workstation returned Python `3.14.5`.
- Runtime check showed Python `compression.zstd` is present at `/usr/lib/python3.14/compression/zstd/__init__.py`.
- Official Python 3.14.5 documentation says the `compression` package and `compression.zstd` were added in Python 3.14, and that `compression.zstd` provides Zstandard file I/O plus in-memory compression/decompression while remaining an optional CPython module.

Inferences:

- Python should expose binary journal field values as `bytes` in the idiomatic API and accept `bytes`, `bytearray`, `memoryview`, or strings where writing field values is ergonomic and unambiguous.
- The initial Python implementation should be plain Python with no build step, no native journal bindings, and no dependency on system journal libraries.
- Python may use standard-library runtime modules, including `compression.zstd` when present, but must not add native journal bindings or packages that link to system journal libraries.
- Synchronous file parsing/writing is acceptable for the first conformance-compatible SDK slice; memory mapping and streaming can be considered during the benchmark/profiling phase unless correctness validation exposes a need now.

Unknowns:

- No activation-blocking unknowns remain. If implementation exposes a needed behavior that is not representable with bytes-like values, Python integers, iterators, context managers, or explicit facade functions, stop and record the concrete API issue before proceeding.

### Acceptance Criteria

- Python reader and writer expose idiomatic APIs equivalent to the shared SDK contract, plus a libsystemd-compatible reader facade unless a SOW records concrete evidence for a scoped exception.
- Python uses no native journal bindings and no system journal library linkage.
- Python passes the shared conformance suite.
- Python writer passes live one-writer/multiple-reader tests with stock `journalctl --file` and stock libsystemd readers while the writer is appending.
- Python reader passes live-read tests against files actively appended by every repository writer available at this phase, and against stock systemd writers where the environment can provide them without violating repository-boundary rules.
- Python participates in the cross-language interoperability matrix.
- Python journalctl rewrite passes file-backed/query behavior tests.
- Python journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Completed Go, Rust, and Node.js SDK SOWs.
- Shared conformance and live harness contracts.
- Official Python 3.14.5 `compression.zstd` documentation.

Current state:

- SOW-0003, SOW-0004, SOW-0005, SOW-0006, SOW-0010, SOW-0011, SOW-0012, and SOW-0013 are complete.
- Go, Rust, and Node.js provide current reference behavior for reader/facade/journalctl/adapter slices.
- A placeholder `python/.gitkeep` exists; no Python package scaffold exists yet.

Risks:

- Native journal binding leakage would violate the project goal.
- Pure-Python parsing and GIL behavior can become bottlenecks.
- Live concurrency bugs can make the Python writer unreadable by stock readers until close, which is not compatible.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Python currently has no SDK package, journalctl command, adapter, reader, writer, or docs in this repository. The implementation must port the established Go/Rust/Node.js feature slice into a no-native-journal-binding Python package while preserving systemd journal file compatibility, byte-safe fields, match semantics, and live stock-reader writer compatibility.

Evidence reviewed:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `.agents/sow/done/SOW-0004-20260523-rust-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0006-20260523-node-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0010-20260523-go-reader-and-journalctl-completion.md`
- `.agents/sow/done/SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `.agents/sow/done/SOW-0012-20260523-go-writer-binary-fields.md`
- `.agents/sow/done/SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- `go/journal/doc.go`
- `go/adapter/main.go`
- `go/cmd/journalctl/main.go`
- `rust/README.md`
- `rust/src/journal/src/lib.rs`
- `rust/src/adapter/main.rs`
- `rust/src/cmd/journalctl/main.rs`
- `node/README.md`
- `node/src/index.js`
- `node/adapter/index.js`
- `node/cmd/journalctl/index.js`
- `tests/conformance/ADAPTER_CONTRACT.md`
- `tests/conformance/manifests/conformance-v01.json`
- `tests/conformance/live/run_live_concurrency.py`
- Local runtime evidence: `python3 --version` returned `Python 3.14.5`; `importlib.util.find_spec("compression.zstd")` found the standard-library module.
- Official docs evidence: `https://docs.python.org/3.14/library/compression.zstd.html` documents `compression.zstd` as added in Python 3.14 with `.zst` file I/O and in-memory decompression.

Affected contracts and surfaces:

- Python package API.
- Python libsystemd-style reader facade.
- Python file-backed journalctl CLI.
- Shared harness adapter.
- Dependency policy.
- Live writer command or harness integration.
- Python README/package metadata.
- Product scope spec and SOW status.

Existing patterns to reuse:

- Go SDK API/adapter/journalctl behavior as the most complete non-Rust language reference.
- Rust SDK and adapter closeout behavior for header parsing, boolean match semantics, and zstd/lz4/xz reader coverage where applicable.
- Node.js SDK package organization for language-local reader/writer/facade/adapter/journalctl/test files.
- Shared conformance adapter contract and live concurrency harness.
- Directory writer rotation/retention behavior from Go, Rust, and Node.js.
- File-backed journalctl behavior for repeated same-field OR and `+` disjunction.

Risk and blast radius:

- Native dependency leakage would violate the project goal.
- Pure Python performance may need profiling before optimization claims.
- Python integer precision avoids JavaScript-style 64-bit truncation, but struct packing/unpacking and signedness mistakes can still corrupt offsets, timestamps, sequence numbers, and object sizes.
- `bytes` and `str` boundaries can corrupt binary field values if not kept explicit.
- Pure-Python binary parsing, GIL behavior, and synchronous file I/O can become performance bottlenecks; this SOW must avoid performance claims beyond conformance/live compatibility.
- Python `compression.zstd` is available on this workstation but is an optional CPython module; implementation must fail clearly or report adapter SKIP where a runtime lacks it.
- Live concurrency bugs can make the Python writer unreadable by stock readers until close, which is not compatible.

Sensitive data handling plan:

- No sensitive runtime data expected.
- Test fixtures are committed project fixtures or generated local temporary journal files.
- Durable artifacts must avoid personal names, secrets, production endpoints, account identifiers, and raw private data.

Implementation plan:

1. Create a `python/` package scaffold with plain Python runtime code, package tests, adapter executable, journalctl executable, README, and livewriter test command.
2. Implement low-level binary helpers using `bytes`, `bytearray`, `memoryview`, `struct`, and Python integers for journal file structures, little-endian integer parsing/writing, alignment, object traversal, Jenkins lookup3 hashing, and SipHash where required.
3. Implement the current writer slice: regular non-compact uncompressed journal files, byte-safe field values, keyed hash tables, direct-file writer, high-level directory writer, rotation, retention, sync/close/reopen behavior, and live stock-reader-compatible append publication.
4. Implement the current reader slice: `.journal`, `.journal~`, `.journal.zst`, `.journal~.zst`, zstd fixture decompression through `compression.zstd`, forward/backward iteration, cursors, timestamps, binary field values, field/unique enumeration, export/json/text formatting, and match tree behavior.
5. Implement a libsystemd-style reader facade with Python function names that mirror the C/Rust/Go/Node facade semantics where practical.
6. Implement file-backed Python journalctl behavior for `--file`, `--directory`, text/json/export output, `--fields`, `--list-boots`, repeated same-field OR matches, `+` disjunction, and documented unsupported daemon-only commands.
7. Implement `python/adapter` with `run`, `list`, and `probe` subcommands for the shared manifest.
8. Wire Python writer/livewriter command into the live concurrency harness for this feature slice.
9. Update product specs, Python README, SOW validation, SOW-status, and follow-up mapping.

Validation plan:

- Shared conformance suite passes Python.
- Python package tests pass.
- Live stock-reader concurrency suite passes Python writer.
- Live repository-reader concurrency suite passes Python reader.
- Dependency audit confirms no native journal bindings.
- Journalctl fixture checks pass for full-directory JSON drain, list-boots, fields, repeated same-field OR, `+` disjunction, and unsupported `--verify`.
- Cross-language smoke checks read Go, Rust, and Node writer output where practical in this phase; full matrix remains SOW-0008.
- `python3 -m compileall` passes for the package.
- `git diff --check` passes.
- `.agents/sow/audit.sh` passes before close.

Artifact impact plan:

- Specs: update if Python exposes language-specific contract differences.
- End-user/operator docs: create Python SDK docs.
- Runtime project skills: update only if a durable Python workflow lesson is discovered.
- SOW lifecycle: active in `current/` during implementation, then move to `done/` at close.
- SOW-status.md: update when this SOW moves to current or closes.

Open-source reference evidence:

- No local mirrored open-source repositories outside the project were checked for this gate. The implementation references are the completed project-local Go, Rust, and Node.js SDK slices plus official Python documentation for standard-library zstd behavior.

Open decisions:

- Python implementation strategy is recorded for this SOW: plain Python, no native journal bindings, bytes-like binary values, Python integers for 64-bit journal values, standard-library modules allowed, and native journal-binding packages forbidden.
- Full writer compression, FSS, compact journal support, full cross-language interoperability, and benchmark/profiling optimization remain tracked by SOW-0008/SOW-0009 unless the user changes scope.

## Implications And Decisions

1. Python API and pure-Python implementation strategy
   - Current state: activated after SOW-0006 completed.
   - Decision: use a plain Python package, `bytes`/`bytearray`/`memoryview` for binary payloads, Python integers for 64-bit journal values, no native journal bindings, and standard-library runtime modules where available.
   - Implication: Python must remain free of native journal bindings while still passing shared tests.
   - Risk: pure-Python performance and GIL behavior can become bottlenecks unless benchmark work verifies real hot paths later.

## Plan

1. Activate this SOW by moving it to `current/` and setting active status.
2. Delegate Python SDK, writer, reader, facade, adapter, journalctl, docs, and validation implementation using the repository-boundary block.
3. Run independent read-only reviewers against the full SOW scope.
4. Iterate fixes and repeated full-scope reviews until reviewer findings are resolved and production-grade verdicts are reached.
5. Run shared conformance, Python package tests, live compatibility tests, audit output, and docs/spec checks before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-24: Activated SOW-0007 after SOW-0006 completed and commit `3dd2a58` created the rollback point for the Node.js SDK/journalctl chunk.
- 2026-05-24: Refreshed the pre-implementation gate using completed Go, Rust, and Node.js SDK slices, shared conformance/live harness contracts, Python runtime evidence (`Python 3.14.5`), and standard-library `compression.zstd` availability.

## Validation

Validation starts after implementation. Planned closeout evidence is listed in the validation plan above and must include Python package tests, shared conformance, live compatibility, dependency audit, read-only external reviewer results, `git diff --check`, and `.agents/sow/audit.sh`.

Sensitive data gate:

- Planned closeout validation must confirm durable artifacts contain no raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details.

## Outcome

Implementation not complete yet.

## Lessons Extracted

No Python-specific lessons yet.

## Followup

Full writer compression, FSS, compact journal support, full cross-language interoperability, and benchmark/profiling optimization are tracked by SOW-0008/SOW-0009.

## Regression Log

None yet.
