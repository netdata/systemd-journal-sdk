# SOW-0006 - Node.js SDK And journalctl

## Status

Status: completed

Sub-state: completed after Go and Rust SDK slices completed.

## Requirements

### Purpose

Implement the Node.js SDK and file-backed journalctl rewrite without native addons.

### Assistant Understanding

Facts:

- Node.js must implement the shared SDK contract without native addons or system journal library linkage.
- The shared harness is stable enough for Node.js implementation.
- The Go SDK and Rust SDK slices are complete and provide the current API/behavior references.
- Runtime check on this workstation returned Node.js `v22.22.2` and npm `10.9.7`.
- Runtime check showed `node:zlib` exposes built-in zstd APIs: `zstdDecompressSync`, `createZstdDecompress`, `zstdCompressSync`, and `createZstdCompress`.

Inferences:

- Node.js should expose binary journal field values as `Buffer` values in the idiomatic API and accept `Buffer`, `Uint8Array`, or strings where writing field values is ergonomic and unambiguous.
- The initial Node.js implementation should be plain JavaScript with no transpilation/build step, to keep the runtime dependency surface small and avoid native addon leakage.
- Node.js may use built-in runtime modules, including `node:zlib` for zstd fixture support, but must not add npm packages that compile native code or link to journal libraries.
- Synchronous file parsing/writing is acceptable for the first conformance-compatible SDK slice; streaming/backpressure and worker-thread offload belong to the benchmark/profiling phase unless validation proves event-loop blocking creates correctness issues.

Unknowns:

- No activation-blocking unknowns remain. If implementation exposes a needed behavior that is not representable with Buffers, plain JavaScript numbers, BigInt, iterators, async iterators, or explicit facade functions, stop and record the concrete API issue before proceeding.

### Acceptance Criteria

- Node.js reader and writer expose idiomatic APIs equivalent to the shared SDK contract, plus a libsystemd-compatible reader facade unless a SOW records concrete evidence for a scoped exception.
- Node.js uses no native addons and no system journal library linkage.
- Node.js passes the shared conformance suite.
- Node.js writer passes live one-writer/multiple-reader tests with stock `journalctl --file` and stock libsystemd readers while the writer is appending.
- Node.js reader passes live-read tests against files actively appended by every repository writer available at this phase, and against stock systemd writers where the environment can provide them without violating repository-boundary rules.
- Node.js participates in the cross-language interoperability matrix.
- Node.js journalctl rewrite passes file-backed/query behavior tests.
- Node.js journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- Product scope spec.
- Earlier Rust and harness SOW drafts.

Current state:

- SOW-0003, SOW-0004, SOW-0005, SOW-0010, SOW-0011, SOW-0012, and SOW-0013 are complete.
- Rust and Go both provide current reference behavior for reader/facade/journalctl/adapter slices.
- No Node.js package scaffold exists yet.

Risks:

- Native dependency leakage would violate the project goal.
- Event-loop blocking and Buffer handling can create correctness or performance issues.
- Live concurrency bugs can make the Node.js writer unreadable by stock readers until close, which is not compatible.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Node.js currently has no SDK package, journalctl command, adapter, reader, writer, or docs in this repository. The implementation must port the established Go/Rust feature slice into a no-native-addon Node.js package while preserving systemd journal file compatibility, byte-safe fields, match semantics, and live stock-reader writer compatibility.

Evidence reviewed:

- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `.agents/sow/done/SOW-0004-20260523-rust-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
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
- `tests/conformance/ADAPTER_CONTRACT.md`
- `tests/conformance/manifests/conformance-v01.json`

Affected contracts and surfaces:

- Node.js package API.
- Node.js libsystemd-style reader facade.
- Node.js file-backed journalctl CLI.
- Shared harness adapter.
- Dependency policy.
- Live writer command or harness integration.
- Node.js README/package metadata.
- Product scope spec and SOW status.

Existing patterns to reuse:

- Go SDK API/adapter/journalctl behavior as the most complete non-Rust language reference.
- Rust SDK and adapter closeout behavior for header parsing, boolean match semantics, and zstd/lz4/xz reader coverage where applicable.
- Shared conformance adapter contract and live concurrency harness.
- Directory writer rotation/retention behavior from Go and Rust.
- File-backed journalctl behavior for repeated same-field OR and `+` disjunction.

Risk and blast radius:

- Native dependency leakage would violate the project goal.
- Pure JavaScript performance may need profiling before optimization claims.
- JavaScript number precision can corrupt 64-bit journal offsets, timestamps, sequence numbers, and object sizes if not handled with `BigInt` internally.
- Buffer slicing and endianness bugs can corrupt binary field values or journal object parsing.
- Event-loop blocking can make a correct parser impractical on large journal files; this SOW must avoid performance claims beyond conformance/live compatibility.
- Built-in `node:zlib` zstd support is available in Node.js `v22.22.2`, but compatibility with older Node versions must be documented if the package depends on it.
- Live concurrency bugs can make the Node.js writer unreadable by stock readers until close, which is not compatible.

Sensitive data handling plan:

- No sensitive runtime data expected.
- Test fixtures are committed project fixtures or generated local temporary journal files.
- Durable artifacts must avoid personal names, secrets, production endpoints, account identifiers, and raw private data.

Implementation plan:

1. Create a `node/` package scaffold with plain JavaScript runtime code, package scripts, adapter executable, journalctl executable, README, and tests.
2. Implement low-level binary helpers using Buffer and BigInt for journal file structures, little-endian integer parsing/writing, alignment, object traversal, and Jenkins lookup3 hashing where required.
3. Implement the current writer slice: regular non-compact uncompressed journal files, byte-safe field values, keyed hash tables, single-file writer, high-level directory writer, rotation, retention, sync/close/reopen behavior, and live stock-reader-compatible append publication.
4. Implement the current reader slice: `.journal`, `.journal~`, `.journal.zst`, `.journal~.zst`, zstd fixture decompression through built-in `node:zlib`, forward/backward iteration, cursors, timestamps, binary field values, field/unique enumeration, export/json/text formatting, and match tree behavior.
5. Implement a libsystemd-style reader facade with Node.js function names that mirror the C/Rust/Go facade semantics where possible.
6. Implement file-backed `node/journalctl` behavior for `--file`, `--directory`, text/json/export output, `--fields`, `--list-boots`, repeated same-field OR matches, `+` disjunction, and documented unsupported daemon-only commands.
7. Implement `node/adapter` with `run`, `list`, and `probe` subcommands for the shared manifest.
8. Wire Node.js writer/livewriter command into the live concurrency harness for this feature slice.
9. Update product specs, Node.js README, SOW validation, SOW-status, and follow-up mapping.

Validation plan:

- Shared conformance suite passes Node.js.
- Node.js package tests pass.
- Live stock-reader concurrency suite passes Node.js writer.
- Live repository-reader concurrency suite passes Node.js reader.
- Dependency audit confirms no native addons.
- Journalctl fixture checks pass for full-directory JSON drain, list-boots, fields, repeated same-field OR, `+` disjunction, and unsupported `--verify`.
- Cross-language smoke checks read Go and Rust writer output where practical in this phase; full matrix remains SOW-0008.
- `git diff --check` passes.
- `.agents/sow/audit.sh` passes before close.

Artifact impact plan:

- Specs: update if Node.js exposes language-specific contract differences.
- End-user/operator docs: create Node.js SDK docs.
- Runtime project skills: update only if a durable Node.js workflow lesson is discovered.
- SOW lifecycle: active in `current/` during implementation, then move to `done/` at close.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- Node.js implementation strategy is recorded for this SOW: plain JavaScript, no TypeScript compile step, no native addons, Buffer/Uint8Array byte values, BigInt internally for 64-bit journal values, built-in Node runtime modules allowed, npm native packages forbidden.
- Full writer compression, FSS, compact journal support, full cross-language interoperability, and benchmark/profiling optimization remain tracked by SOW-0008/SOW-0009 unless the user changes scope.

## Implications And Decisions

1. Node.js API and pure-JavaScript implementation strategy
   - Current state: activated after SOW-0004 completed.
   - Decision: use a plain JavaScript package, Buffer/Uint8Array for binary payloads, BigInt internally for 64-bit journal values, no native addons, and built-in Node runtime modules only unless a dependency is proven pure JavaScript.
   - Implication: Node.js must remain pure JavaScript/TypeScript with no native addons while still passing shared tests.
   - Risk: event-loop blocking or native dependency leakage can violate project goals even when correctness tests pass.

## Plan

1. Activate this SOW by moving it to `current/` and setting active status.
2. Delegate Node.js SDK, writer, reader, facade, adapter, journalctl, docs, and validation implementation using the repository-boundary block.
3. Run independent read-only reviewers against the full SOW scope.
4. Iterate fixes and repeated full-scope reviews until reviewer findings are resolved and production-grade verdicts are reached.
5. Run shared conformance, Node.js package tests, live compatibility tests, audit output, and docs/spec checks before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-23: Activated SOW-0006 after SOW-0004 completed and commit `97506b8` created the rollback point for the Rust SDK/journalctl chunk.
- 2026-05-23: Refreshed the pre-implementation gate using completed Go and Rust SDK slices, shared conformance/live harness contracts, Node.js runtime evidence (`node v22.22.2`, `npm 10.9.7`), and built-in `node:zlib` zstd availability.
- 2026-05-23: Minimax implementation attempt started with the repository-boundary prompt and was stopped by exact verified PIDs after becoming stuck debugging a broken Node adapter. The partial scaffold created files under `node/`, but it is not accepted: `node --check node/src/lib/header.js` fails at `export const OBJECT_TYPE.footer = 7;`, and `node node/adapter/index.js run < /tmp/test-case.json` wrote `{}` for a simple adapter case because the adapter serialized an unresolved Promise. Per the fallback hierarchy, the next implementer is `llm-netdata-cloud/qwen3.6-plus`, unless local repair is faster.
- 2026-05-24: Qwen fallback implementation attempt was stopped by exact verified PIDs after repeatedly producing broken Node.js modules. Concrete evidence: `node/src/lib/reader.js` contains nested `import {` lines and `node --check node/src/lib/reader.js` fails with `SyntaxError: Unexpected reserved word`; the reader failed `journal-file-header-parse` against `fixtures/systemd/test-data/no-rtc/system.journal.zst` with `invalid entry array object`; the attempt also introduced and then removed an invalid `import { flock } from 'node:fs'` because Node.js v22.22.2 does not export `fs.flock`. The Qwen scaffold is not accepted. Per the fallback hierarchy, the next implementer is `llm-netdata-cloud/glm-5.1`.
- 2026-05-24: GLM fallback implementation repaired most of the Node.js scaffold, including journal object header parsing, entry-array iteration, BigInt hash handling, writer object layout, facade wrappers, adapter execution, and journalctl argument parsing. The run was stopped by exact verified PIDs after stalling on the remaining complex-match failure. Local repair was used for the small evidence-backed remaining issues: the adapter's binary match literals used string `'='` inside `Buffer.from([...])`, which coerced to byte `0`; `node/src/lib/binary.js` called non-existent `Buffer.writeUint8()` instead of `writeUInt8()`; `node/package.json` referenced missing `node/test/all.js`; no Node livewriter command existed for the live harness; and `node/src/lib/directory-writer.js` called non-existent `Writer.append()`.
- 2026-05-24: Implemented the Node.js package slice under `node/`: reader, writer, directory writer, libsystemd-style facade, journalctl CLI, conformance adapter, package test runner, README, and livewriter test command.
- 2026-05-24: Fixed review-discovered Node.js issues: Jenkins finalization fallthrough, incompatible writer flags, binary export output as raw Buffers, newline export formatting, directory-writer archive state, seqnum/boot/machine identity preservation across rotation, binary `QueryUnique()` values, source-scoped directory writer naming, archive-on-close behavior, owned-retention filtering, directory-reader machine-id subdirectories, facade flag/match validation, defensive hash-bucket object checks, and package test coverage for these cases.
- 2026-05-24: Read-only reviewer Mimo returned `VERDICT: PRODUCTION GRADE` after the final fixes, with only non-blocking notes about corrupt-entry handling, direct lz4 rejection coverage, close-time temp cleanup, year-2255-range BigInt-to-Number boot timestamps, and sequential directory filter behavior.
- 2026-05-24: Read-only reviewer Kimi ran useful validation but failed to emit a final verdict because of an opencode socket timeout. The run validated live harness behavior, stock `journalctl` directory/verify behavior, CLI fixture behavior, adapter behavior, Node reader output, Node reading Rust files, Node writer interoperability with stock/Go/Rust readers, and match parsing before timing out.
- 2026-05-24: Read-only reviewer GLM ran useful validation but timed out before a final verdict. The timeout was recorded as reviewer failure, not approval.
- 2026-05-24: Replacement read-only reviewer Minimax returned `VERDICT: PRODUCTION GRADE`. Its only non-blocking note was a proposed SipHash vector test; the concrete vector example in the review used mismatched standard vector lengths, and live stock-reader plus Go/Rust interoperability validation already exercises the current hash behavior. Qwen replacement review was stopped by exact verified PIDs after Minimax completed and two production-grade reviewer verdicts were available.

## Validation

Acceptance criteria evidence:

- Implemented: Node.js package uses plain JavaScript, no TypeScript build, no native addons, no npm dependencies, Buffer/Uint8Array field values, and BigInt internal 64-bit journal values.
- Implemented: idiomatic SDK API plus libsystemd-style facade functions.
- Implemented: file-backed Node.js journalctl for `--file`, `--directory`, default/json/export output, `--fields`, `--list-boots`, `--head`, `--tail`, repeated same-field OR, `+` disjunction, and documented unsupported daemon-only operations including `--verify`.
- Implemented: direct `Writer` and high-level `Log` directory writer with active/archive naming, entry-count and byte-size rotation, archived file-count and byte-size retention.
- Implemented: Node.js livewriter command for the shared live concurrency harness.

Tests or equivalent validation:

- Passed: `for f in $(find node -type f -name '*.js' | sort); do node --check "$f"; done`.
- Passed: `node -e "import './node/src/index.js'; console.log('runtime import OK');"`.
- Passed: `npm test` in `node/`, which syntax-checks the package, imports the SDK entry point, and runs all cases in `tests/conformance/manifests/conformance-v01.json`.
- Passed: direct adapter manifest run returned 13 PASS, 0 FAIL, 0 ERROR, 2 SKIP. The two SKIPs are `journal-verify-sealed` and `journal-verify-corruption-detection`, both tracked by SOW-0008/FSS verification scope.
- Passed: Node journalctl full `fixtures/systemd/test-data/no-rtc` JSON drain returned 10,757 rows.
- Passed: Node journalctl `--list-boots` returned 4 rows.
- Passed: Node journalctl `--fields` returned 202 rows.
- Passed: Node journalctl repeated same-field OR check `SYSLOG_IDENTIFIER=kernel SYSLOG_IDENTIFIER=systemd` returned 6,516 rows.
- Passed: Node journalctl `+` disjunction check `SYSLOG_IDENTIFIER=kernel + SYSLOG_IDENTIFIER=systemd` returned 6,516 rows.
- Passed: Node journalctl `--verify` returned unsupported behavior with exit code 1 and error `--verify is not supported in the pure-JavaScript journalctl`.
- Passed: direct file-writer smoke created `.local/node-live-smoke/test.journal` and stock `journalctl --verify --file` returned `PASS`.
- Passed: directory writer smoke rotated 5 entries across two archived files and one active file, Node journalctl read all 5 entries, and stock `journalctl --verify --file` passed for each file.
- Passed: dependency audit `npm ls --all` returned `(empty)`, no `package-lock`, `node_modules`, `binding.gyp`, `.node`, `node-gyp`, `ffi`, `bindings`, `sd_journal`, or `require()` matches were found under `node/`.

Real-use evidence:

- Passed: shared live concurrency harness with systemd `260 (260.1-2-manjaro)`, 100 entries, 2 stock polling `journalctl --file` readers, 1 stock `journalctl --file --follow --no-tail --boot=all` reader, 1 stock libsystemd reader, ordered `LIVE_SEQ`, and final `journalctl --verify --file` PASS.
- Passed: Node reader live polling against the Node writer while active observed ordered `LIVE_SEQ` values and final 100-entry ordered snapshot.

Reviewer findings:

- Mimo: `VERDICT: PRODUCTION GRADE`; non-blocking notes were recorded with no close-blocking changes required.
- Minimax: `VERDICT: PRODUCTION GRADE`; no blocking findings. The SipHash note is tracked as non-blocking test-depth advice because the cited vector example used incorrect lengths and current stock-reader/Go/Rust interoperability validation passed.
- Kimi: reviewer run failed to emit a final verdict because of an opencode socket timeout after running useful validation; not counted as approval.
- GLM: reviewer run timed out after running useful validation; not counted as approval.
- Qwen replacement: stopped by exact verified PIDs after two production-grade reviewer verdicts were already available.

Same-failure scan:

- Completed for the repaired failure classes before review and closeout: searched for malformed binary match literals, non-existent `writeUint8`, native-addon markers, `require()`, native/systemd linkage strings under `node/`, incompatible flag handling, binary export formatting, `QueryUnique()` binary values, directory writer source/archive naming, and append-after-close behavior.

Sensitive data gate:

- Passed pre-review scan by inspection: durable artifacts added for Node.js contain only synthetic fixture paths, `.local` scratch examples, and public SDK documentation. No raw secrets, credentials, bearer tokens, SNMP communities, community member names, customer names, personal data, non-private customer-identifying IPs, private endpoints, or proprietary incident details were introduced.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-wide workflow and compatibility guardrails already cover the Node.js slice.
- Runtime project skills: no update needed; the existing orchestration and journal compatibility skills already captured the workflow lessons needed for this SOW.
- Specs: updated `.agents/sow/specs/product-scope.md` with the current Node.js writer, reader, journalctl, and limitation slice.
- End-user/operator docs: added and repaired `node/README.md`.
- End-user/operator skills: no output/reference skill is produced or consumed by this SOW.
- SOW lifecycle: this SOW is completed and moved from `current/` to `done/` as part of closeout.
- SOW-status.md: updated at close.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- No update needed; no durable workflow rule changed.

End-user/operator docs update:

- Added `node/README.md`.

End-user/operator skills update:

- No update needed; no end-user/operator skill artifact exists for this SDK slice.

Lessons:

- Adapter syntax checks are not enough for ES module correctness; runtime import checks caught missing exports/local bindings.
- Manifest conformance can pass while `npm test` is broken if the package script references a missing test file; package tests must be part of the same validation gate.
- Binary match literals in JavaScript must use numeric bytes, not string elements inside numeric arrays.
- Directory writer and livewriter paths need explicit validation because adapter conformance mostly exercises direct reader/writer behavior.
- Standard crypto/hash vectors must be recorded with exact message lengths; a mislabeled vector can create a false reviewer finding even when interoperability tests pass.

Follow-up mapping:

- Full writer compression, FSS, compact journal support, full cross-language interoperability, and benchmark/profiling optimization are tracked by SOW-0008/SOW-0009.

## Outcome

Completed.

The Node.js SDK slice now provides a pure JavaScript journal reader, writer, high-level directory writer, libsystemd-style reader facade, file-backed journalctl command, conformance adapter, package tests, livewriter harness command, and README. The implementation is byte-safe for binary fields, uses BigInt for journal 64-bit values internally, has no npm dependencies or native addon linkage, passes the shared Node package/conformance tests, passes live stock-reader compatibility for the current writer slice, and has two read-only external production-grade reviewer verdicts.

## Lessons Extracted

- Keep package-level tests in the validation gate, not only adapter-level tests.
- For JavaScript binary data, use numeric byte arrays and Buffer values throughout; string coercion can silently corrupt payloads.
- Directory writer compatibility needs explicit stock `journalctl --directory` validation while the writer is open and again after close/rotation.
- Reviewer vector findings should be checked against exact upstream vector length semantics before being treated as implementation blockers.

## Followup

- SOW-0008 tracks full cross-language interoperability and remaining writer-format expansion.
- SOW-0009 tracks benchmark, profiling, and optimization work.

## Regression Log

None yet.
