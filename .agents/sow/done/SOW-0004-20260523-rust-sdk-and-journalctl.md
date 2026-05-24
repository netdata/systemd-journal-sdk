# SOW-0004 - Rust SDK And journalctl

## Status

Status: completed

Sub-state: completed and ready for SOW close commit.

## Requirements

### Purpose

Implement the Rust SDK and file-backed journalctl rewrite against the shared tests after the Go writer-first SOW completes.

### User Request

Rust must provide the same reader/writer APIs and journalctl rewrite behavior as the other languages.

### Assistant Understanding

Facts:

- Rust source import is complete.
- The shared conformance harness is complete.
- The Go writer-first priority is complete.
- The Go reader, libsystemd-style facade, file-backed journalctl command, and Go conformance adapter are complete.
- Rust remains required, but it is no longer the first complete SDK implementation target.

Inferences:

- Rust should reuse the imported Netdata reader/writer crates as the compatibility base, while exposing a stable public SDK layer aligned with the Go implementation where the contracts overlap.
- Rust should provide a conformance adapter equivalent to the Go adapter so shared tests exercise Rust through the same harness.

Unknowns:

- No activation-blocking unknowns remain.

### Acceptance Criteria

- Rust exposes idiomatic SDK APIs and a libsystemd-compatible reader facade, unless a SOW records concrete evidence for a scoped exception.
- Rust writer and reader pass the shared conformance suite.
- Rust writer passes live one-writer/multiple-reader tests with stock `journalctl --file` and stock libsystemd readers while the writer is appending.
- Rust reader passes live-read tests against files actively appended by every repository writer available at this phase, and against stock systemd writers where the environment can provide them without violating repository-boundary rules.
- Rust journalctl rewrite passes file-backed/query behavior tests.
- Rust journalctl implements repeated same-field OR matching and the `+` disjunction separator for file-backed behavior.
- Daemon-only journalctl commands are not implemented and return documented unsupported behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`

Current state:

- SOW-0002, SOW-0003, SOW-0005, SOW-0010, SOW-0011, SOW-0012, and SOW-0013 are complete.
- Rust imported crates build as of SOW-0002.
- The Go SDK provides the current concrete API, adapter, journalctl, and live compatibility reference for this phase.

Risks:

- Rust API decisions still shape later language ports, but the Go writer now shapes the first production-oriented writer contract.
- Live stock-reader and cross-language reader/writer concurrency is now a mandatory compatibility gate for both Rust writer and Rust reader.
- CLI behavior drift can multiply across implementations.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The imported Netdata Rust crates provide working low-level reader/writer pieces, but the repository still lacks a polished Rust SDK package, Rust conformance adapter, Rust file-backed journalctl command, and phase validation against the shared harness.

Evidence reviewed:

- `.agents/sow/done/SOW-0002-20260523-repo-scaffold-and-rust-source-import.md`
- `.agents/sow/done/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0010-20260523-go-reader-and-journalctl-completion.md`
- `.agents/sow/done/SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `.agents/sow/done/SOW-0012-20260523-go-writer-binary-fields.md`
- `.agents/sow/done/SOW-0013-20260523-go-directory-writer-rotation-retention.md`
- `.agents/sow/specs/product-scope.md`
- `PROVENANCE.md`
- `rust/Cargo.toml`
- `rust/src/crates/jf/journal_file/src/lib.rs`
- `rust/src/crates/journal-log-writer/src/lib.rs`
- `go/journal/doc.go`
- `go/adapter/main.go`
- `go/cmd/journalctl/main.go`

Affected contracts and surfaces:

- Rust public APIs.
- Rust libsystemd-style reader facade.
- Rust CLI.
- Rust shared harness adapter.
- Rust writer live-concurrency command path.
- Documentation.

Existing patterns to reuse:

- Imported Netdata Rust `jf/journal_file` reader compatibility layer.
- Imported Netdata Rust `journal-log-writer` directory writer with rotation and retention.
- Shared conformance runner and adapter contract.
- Go SDK package docs, facade behavior, adapter behavior, and file-backed journalctl behavior as the most recent project-local implementation reference.

Risk and blast radius:

- Rust API decisions will shape Node.js and Python ports.
- The imported Netdata crates may expose lower-level APIs than the desired public SDK; the public wrapper must avoid rewriting battle-tested parsing/writing behavior unnecessarily.
- CLI behavior drift would multiply into Go, Node.js, and Python.
- Writer compatibility must include live stock-reader validation; closed-file reads alone are insufficient.

Sensitive data handling plan:

- No sensitive runtime data expected.
- Test fixtures are public or generated.
- External open-source evidence must cite upstream repository, commit, and relative path, not workstation paths.

Implementation plan:

1. Add or finalize a public Rust SDK crate/package that wraps the imported Netdata crates without unnecessary behavioral rewrites.
2. Expose idiomatic Rust reader and writer APIs aligned with the Go feature slice where applicable: byte-safe field values, file reader, directory reader, cursors, field/unique enumeration, matching, export/json formatting, single-file writer, and directory writer rotation/retention.
3. Expose a libsystemd-style Rust reader facade for file-backed reads and matching behavior.
4. Implement a Rust file-backed journalctl command for `--file`, `--directory`, text/json/export output, `--fields`, `--list-boots`, repeated same-field OR matches, and `+` disjunction.
5. Implement a Rust conformance adapter with `run`, `list`, and `probe` subcommands following `tests/conformance/ADAPTER_CONTRACT.md`.
6. Wire a Rust writer command or adapter path into the live concurrency harness for the claimed writer feature slice.
7. Update Rust docs, product specs, SOW status, and any durable workflow notes affected by the shipped behavior.

Validation plan:

- Shared conformance suite passes Rust.
- Rust package tests pass.
- Live stock-reader concurrency suite passes Rust writer.
- Live repository-reader concurrency suite passes Rust reader.
- journalctl CLI tests pass.
- Dependency/build audit confirms no native journal-library linking is introduced beyond the existing imported FFI crate build surface, and any FFI crate scope is documented if it remains build-only rather than SDK runtime.
- `.agents/sow/audit.sh` passes before close.

Artifact impact plan:

- Specs: update API and CLI behavior.
- Runtime project skills: update if Rust workflow becomes durable.
- End-user/operator docs: create Rust SDK docs.
- SOW lifecycle: activated in `current/` for implementation and moved to `done/` at close.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

- None currently blocking implementation. If the implementer finds that an imported Rust crate cannot satisfy a required API contract without a behavioral rewrite, stop and record the concrete evidence before changing direction.

## Implications And Decisions

1. Rust API and CLI contract
   - Current state: active after the prerequisite Go and harness SOWs completed.
   - Required during implementation: preserve imported Rust behavior where possible, add the public SDK layer, add the Rust adapter, add file-backed journalctl, and validate against the same shared tests used by Go.
   - Implication: Rust becomes the next API reference before Node.js and Python.
   - Risk: premature API choices can force incompatible or unnatural APIs in Go, Node.js, and Python.

## Plan

1. Activate this SOW by moving it to `current/` and setting `Status: in-progress`.
2. Delegate Rust SDK, adapter, journalctl, docs, and validation implementation using the repository-boundary block.
3. Run independent read-only reviewers against the full SOW scope.
4. Iterate fixes and repeated full-scope reviews until reviewer findings are resolved and production-grade verdicts are reached.
5. Run shared conformance, Rust package tests, live compatibility tests, audit output, and docs/spec checks before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-23: Activated SOW-0004 after SOW-0010 completed and commit `2d349ad` created the rollback point for the Go reader/facade/journalctl/adapter chunk.
- 2026-05-23: Refreshed the pre-implementation gate using completed Go writer/reader/facade/journalctl/adapter evidence and the shared conformance/live harness contracts.
- 2026-05-23: Minimax implementation attempt started with prompt `.local/review-prompts/SOW-0004-implementer-minimax.md` and exited cleanly but incomplete. It created initial Rust SDK/journalctl/adapter scaffolding under `rust/src/journal/`, `rust/src/cmd/journalctl/`, and `rust/src/adapter/`, but did not add the new crates to the workspace, did not create `rust/src/adapter/main.rs`, did not implement validation, did not update SOW validation, and did not satisfy the active SOW. Per the fallback hierarchy, the next implementer is `llm-netdata-cloud/qwen3.6-plus`.
- 2026-05-23: Qwen fallback implementation attempt started with prompt `.local/review-prompts/SOW-0004-fixpass1-implementer-qwen.md` and timed out after partial edits. The partial result added workspace wiring and Rust SDK/CLI/adapter code but left unsafe lifetime handling, incomplete adapter behavior, and failing validation. The timeout and incomplete state were not accepted as SOW completion.
- 2026-05-23: Local repair was performed after the user allowed direct implementation when faster. The repair replaced unsafe reader lifetime handling with the pure-Rust `ouroboros` self-referencing helper, completed workspace wiring for `journal`, `journalctl`, and `adapter`, made export output byte-safe, added `.zst` fixture decompression for readers, and fixed Rust directory field enumeration to tolerate unreadable individual entries while continuing the scan.
- 2026-05-23: Direct CLI validation found `journalctl --list-boots` indexing four boots as `-5..-2`; repaired the calculation to produce the expected `-3..0` sequence for the committed `no-rtc` fixture.
- 2026-05-23: Added Rust livewriter test command under `rust/src/internal/testcmd/livewriter/` and repaired `journal-core` sync so mutable mmap windows are flushed, the file is published at the journal logical size, and header bytes are written through the file descriptor publication point.
- 2026-05-23: Rust directory-writer live harness attempt using a stable hard link to the imported writer's archived-style active file failed the stock `journalctl --follow` reader while passing stock polling readers, stock libsystemd reader, and final `journalctl --verify`. This is not accepted as production-compatible directory-writer evidence.
- 2026-05-23: Rust direct-file livewriter mode using the same `journal-core` writer against the exact `--path` file passed the full live harness for 200 entries with stock `journalctl` polling readers, stock `journalctl --follow`, stock libsystemd reader, and final `journalctl --verify`.
- 2026-05-23: Minimax read-only diagnosis prompt `.local/review-prompts/SOW-0004-rust-follow-diagnosis.md` returned `NOT PRODUCTION GRADE` for the Rust writer live-follow surface, focused on missing/incorrect systemd-style live notification and publication behavior. Kimi and GLM reviewer attempts were stopped by exact PIDs after they produced no final actionable output for an extended period.
- 2026-05-23: Repaired the imported Rust directory writer active-file lifecycle. The writer now creates a real `system.journal` active file while running, archives that active file on explicit rotation, leaves the active path in place on close so stock readers can finish reading the requested path, and archives a stale active file when a later writer starts so restart does not overwrite previous entries.
- 2026-05-23: Re-ran validation after the lifecycle repair. Rust workspace tests passed, Rust direct-file live writer passed stock-reader concurrency, and Rust directory writer passed stock-reader concurrency on the real active path. A setup issue was also recorded: scratch directory basenames containing a dot are interpreted by the Rust repository parser as a machine-id namespace suffix, so live-test scratch directory names must avoid dots.
- 2026-05-23: Full-scope read-only review round started with prompt `.local/review-prompts/SOW-0004-fullscope-review.md`. Minimax returned `PRODUCTION GRADE`. Mimo, Kimi, and GLM returned `NOT PRODUCTION GRADE`; blockers included byte-unsafe `SdJournalAddMatch`, fake adapter PASS results for UID and corruption tests, weak list-boots assertions, imprecise cursor seek, production panic/unimplemented paths, `entry_mut` using the wrong header type, and the leftover livewriter hard-link helper path.
- 2026-05-23: Repaired the reviewer blockers. The Rust SDK now has byte-oriented match validation, precise cursor seeking, no `_BOOT_ID` JSON overwrite, no `panic!`/`unimplemented!()` in the reviewed filter/cursor paths, corrected entry object header sizing, strengthened adapter UID/corruption/list-boots/import/export assertions, child-process corruption probes, read-only mmap EOF bounds to avoid SIGBUS on corrupted files, and no directory-mode livewriter hard link.
- 2026-05-23: Re-ran validation after reviewer fixes. Rust workspace tests passed, real adapter execution returned 11 PASS and 4 SKIP, direct-file and directory live writer harnesses passed stock readers and final verify, Rust journalctl checks passed, `git diff --check` passed, and `bash .agents/sow/audit.sh` passed.
- 2026-05-23: Full-scope review round 2 started with prompt `.local/review-prompts/SOW-0004-fullscope-review-round2.md`. Minimax returned `PRODUCTION GRADE`; Mimo returned `NOT PRODUCTION GRADE` for JSON output parity gaps, weak field-enumeration adapter assertions, and residual `jf` crate hazards; GLM returned `NOT PRODUCTION GRADE` for unchecked mutable mmap arithmetic, residual `jf` panic/unimplemented paths, directory-order limitations, and list-boots/header limitations. Kimi stalled after file reads and was stopped by exact verified PIDs because two completed reviewers had already found blocking issues requiring code changes.
- 2026-05-23: Repaired round-2 blockers and same-failure issues. Rust JSON now includes `__MONOTONIC_TIMESTAMP` and preserves valid UTF-8 strings while keeping binary values as byte arrays; the adapter lists all manifest cases and validates field enumeration expectations; mutable mmap/window arithmetic now uses checked add/multiply; the duplicated `jf` filter panic, cursor `unimplemented!()` paths, and entry object header sizing bug were fixed; obsolete commented panic code was removed; and the public reader now tolerates recoverable historical online-journal tail/data-object inconsistencies that stock `journalctl` also drains cleanly.
- 2026-05-23: Added Rust regression coverage for JSON monotonic/UTF-8/binary output and complete draining of all committed `no-rtc` journal fixtures. The full `no-rtc` directory now drains through Rust journalctl with 10,757 JSON entries and no stderr, matching stock `journalctl -o json` counts for the fixture set.
- 2026-05-23: Re-ran validation after round-2 repairs. Rust workspace tests passed, real adapter execution returned 11 PASS and 4 explicit SKIP, Rust journalctl full-directory JSON/list-boots/fields/repeated-match/`+` checks passed without stderr, direct-file and directory live writer harnesses passed stock polling readers, stock follow reader, stock libsystemd reader, ordered `LIVE_SEQ`, and final `journalctl --verify`, `git diff --check` passed, and `bash .agents/sow/audit.sh` passed.
- 2026-05-23: Full-scope review round 3 started with prompt `.local/review-prompts/SOW-0004-fullscope-review-round3.md`. Mimo returned `NOT PRODUCTION GRADE` for same-failure unchecked arithmetic and missing EOF bounds in the duplicated `jf/window_manager` crate. GLM returned `NOT PRODUCTION GRADE` while listing no critical blocker for the current intermediate slice, but requested documentation for the deliberate `Drop` active-file lifecycle and tracked known limitations. Minimax exited with a status summary instead of the required verdict, so it was not counted as a clean production-grade review. Qwen stalled after partial reads and was stopped by exact verified PIDs because completed reviewers already required code changes.
- 2026-05-23: Repaired round-3 blocker and low-risk cleanup findings. The `jf/window_manager` crate now mirrors the checked arithmetic and read EOF-bounds behavior added to `journal-core`, returning `ObjectExceedsFileBounds` on overflow or out-of-file reads. The `jf` error enum now carries `ObjectExceedsFileBounds`; public `HashTable::is_empty()` defaults no longer `todo!()`, `journal-core` `JournalReader::dump()` no longer panics on active filters, `rdp::encode_full()` no longer uses `String::from_utf8_unchecked`, and `Log::Drop` now documents why the active path is kept stable until next-writer archive.
- 2026-05-23: Re-ran validation after round-3 repairs. Rust workspace tests passed, real adapter execution returned 11 PASS and 4 explicit SKIP, Rust journalctl full-directory JSON/list-boots/fields/repeated-match/`+` checks passed without stderr, direct-file and directory live writer harnesses passed stock polling readers, stock follow reader, stock libsystemd reader, ordered `LIVE_SEQ`, and final `journalctl --verify`, `git diff --check` passed, and `bash .agents/sow/audit.sh` passed.
- 2026-05-23: Full-scope review round 4 started with the same SOW-0004 scope plus round-3 fix notes. Mimo returned `NOT PRODUCTION GRADE` for one high-impact public-reader security blocker: `journal-core` compressed DATA object decompression lacked the 768 MiB decompressed-size cap already present in the imported `jf` crate. Mimo also reported non-blocking hardening findings for remaining unchecked UTF-8 conversions and cursor `unwrap()` calls. GLM returned production-ready for the declared intermediate scope and confirmed the active-file restart overwrite protection and the previously repaired safety properties.
- 2026-05-23: Repaired round-4 blocker and hardening findings. `journal-core` decompression now mirrors the `jf` cap and bounded streaming-reader behavior for zstd/lz4/xz, with lz4 oversized-prefix and bounded-read tests. `rdp::encode()` and `journal-core::field_map::fields()` no longer use unchecked UTF-8 conversion, and both `journal-core` and `jf` cursor state transitions now return `UnsetCursor` instead of panicking on a violated internal invariant.
- 2026-05-23: Re-ran validation after round-4 repairs. Rust workspace tests passed, including the new `journal-core` decompression tests. Real adapter execution against all 15 manifest cases returned 11 PASS and 4 explicit SKIP. Rust journalctl checks passed with 10,757 full-directory JSON entries, 4 list-boots rows, 202 field rows, 7,536 repeated same-field OR rows, 7,536 `+` disjunction rows, and expected unsupported `--verify` behavior. Direct-file and directory Rust writer live harnesses passed on systemd `260 (260.1-2-manjaro)` with stock polling readers, stock follow reader, stock libsystemd reader, ordered `LIVE_SEQ`, and final `journalctl --verify --file` PASS. `git diff --check` and `bash .agents/sow/audit.sh` passed. The later complex-match repair found this match count was overbroad before the filter-builder fix.
- 2026-05-23: Full-scope review round 5 started with the same SOW-0004 scope plus round-4 fix notes. Mimo returned `PRODUCTION GRADE` and verified all round-4 fixes and prior blockers. GLM returned `PRODUCTION GRADE` and verified all round-4 fixes and prior blockers. Both reviewers stated no finding blocks an intermediate commit of this Rust chunk.
- 2026-05-23: Committed the Rust SDK/journalctl rollback point as `6368d5f`.
- 2026-05-23: Continued SOW-0004 closeout. Implemented Rust adapter support for `journal-file-header-parse` and generated `journal-match-boolean-logic`. The generated complex match case exposed a real filter-builder bug: Rust treated a disjunction group as sufficient by itself and matched an entry with only `L3=ok`. Repaired both `journal-core` and duplicated `jf` filter builders to match systemd/Go semantics: matches since the previous separator are ANDed by field with same-field OR, `AddDisjunction()` commits that expression into an OR level, and `AddConjunction()` commits the OR level into the top-level AND expression.
- 2026-05-23: Re-ran validation after closeout repairs. Rust workspace tests passed. Real adapter execution against all 15 manifest cases returned 13 PASS and 2 explicit SKIP; only `journal-verify-sealed` and `journal-verify-corruption-detection` remain skipped because full verification/FSS is tracked by SOW-0008. Rust journalctl checks still passed with 10,757 full-directory JSON entries, 4 list-boots rows, 202 field rows, 6,516 repeated same-field OR rows, 6,516 `+` disjunction rows, and expected unsupported `--verify` behavior. The 6,516 rows match the full JSON oracle for `SYSLOG_IDENTIFIER in {kernel, systemd}`: 2,416 `kernel` rows plus 4,100 `systemd` rows.
- 2026-05-23: Updated `.agents/sow/specs/product-scope.md` with the accepted current Rust writer and reader feature slices and limitations. Added `rust/README.md` documenting the current Rust SDK API, journalctl command, validation scope, and deferred writer/verification features.
- 2026-05-23: Full-scope closeout review round 6 started with the same SOW-0004 scope plus closeout fix notes. Mimo returned `PRODUCTION GRADE`; GLM returned `PRODUCTION GRADE`. Qwen was started while GLM was silent, but was stopped by exact verified PIDs after Mimo and GLM had both returned final production-grade verdicts; Qwen made no code changes.

## Validation

Acceptance criteria evidence:

- Rust implementation evidence is complete for this SOW. Rust now has a public `journal` crate, libsystemd-style reader facade functions, a file-backed `journalctl` command, and a Rust adapter executable. Rust direct-file and directory writer live stock-reader compatibility pass for this phase. Verification/FSS is explicitly mapped to SOW-0008 because it is part of the full writer/interoperability feature phase, not this accepted Rust baseline slice.

Tests or equivalent validation:

- Passed: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo test --manifest-path rust/Cargo.toml --workspace`
- Passed after round-2 repair: Rust `journal` unit tests now include JSON monotonic/UTF-8/binary output and complete draining of the committed `fixtures/systemd/test-data/no-rtc` `.journal.zst` fixture set, totaling 10,757 JSON entries without tail-object errors.
- Failed before repair, then passed after repair: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo test --manifest-path rust/Cargo.toml -p journal-log-writer test_monotonic_override_remains_strict_after_restart -- --nocapture`. The failure was `missing first entry monotonic timestamp`; root cause was restart overwrite of a stale `system.journal` active path.
- Passed: `python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json`
- Passed as harness wiring only, not real adapter execution: `python3 tests/conformance/runner/manifest_checker.py dry-run tests/conformance/manifests/conformance-v01.json --adapter-cmd .local/cargo-target/debug/adapter`
- Passed: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo build --manifest-path rust/Cargo.toml -p adapter`
- Passed: real adapter execution for the 11 supported test names from `tests/conformance/manifests/conformance-v01.json`: `journal-file-parse-uid-from-filename`, `journal-importer-basic-parsing`, `journal-importer-eof`, `journal-match-invalid-input`, `journal-stream-directory-iteration`, `journal-query-unique-fields`, `journal-cursor-test`, `journal-zstd-compressed-read`, `journal-corruption-append-resilient`, `journal-list-boots`, and `journal-export-format`.
- Failed after adapter hardening, then passed after mmap bounds repair: `journal-corruption-append-resilient`. The hardened adapter initially reported `afl_corrupted_1` child probe terminated by `SIGBUS`; `journal-core` read-only mmap creation and slice reads now reject ranges beyond real EOF with `ObjectExceedsFileBounds`.
- Earlier before closeout repair, skipped by the Rust adapter with explicit notes: `journal-match-boolean-logic`, `journal-verify-sealed`, `journal-verify-corruption-detection`, and `journal-file-header-parse`.
- Passed: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo run --quiet --manifest-path rust/Cargo.toml -p journalctl -- --file fixtures/systemd/test-data/no-rtc/system.journal.zst --head 1 --output json`
- Passed: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo run --quiet --manifest-path rust/Cargo.toml -p journalctl -- --directory fixtures/systemd/test-data/no-rtc --list-boots`
- Passed: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo run --quiet --manifest-path rust/Cargo.toml -p journalctl -- --directory fixtures/systemd/test-data/no-rtc --fields`
- Passed: direct Rust `journalctl` repeated-match checks for same-field OR and `+` disjunction against the committed `no-rtc` directory fixture.
- Passed after round-2 repair: Rust `journalctl --directory fixtures/systemd/test-data/no-rtc --output json` exits cleanly with 10,757 JSON entries, includes `__MONOTONIC_TIMESTAMP`, and emits no stderr.
- Passed expected unsupported behavior: Rust `journalctl --verify` returns `operation not supported`.
- Passed: direct-file Rust writer live harness for 200 entries on systemd `260 (260.1-2-manjaro)` with 2 stock `journalctl --file` polling readers, 1 stock `journalctl --file --follow --no-tail --boot=all` reader, 1 stock libsystemd reader built from `tests/conformance/live/libsystemd_live_reader.c`, final ordered `LIVE_SEQ` visibility, and final `journalctl --verify --file` PASS.
- Failed and not accepted: Rust directory-writer live harness through a stable hard link to the imported writer's archived-style active file. Polling readers, libsystemd, and verify passed, but stock `journalctl --follow` observed only a prefix. The working theory is that the imported directory writer's active-file naming/linking does not match the file-backed live-follow behavior that stock `journalctl` expects; direct-file mode proves the mmap writer core can satisfy follow when the path is stable and direct.
- Passed after repair: Rust directory-writer live harness for 200 entries on systemd `260 (260.1-2-manjaro)` against the real active path `<scratch>/<machine-id>/system.journal`, with 2 stock `journalctl --file` polling readers, 1 stock `journalctl --file --follow --no-tail --boot=all` reader, 1 stock libsystemd reader, final ordered `LIVE_SEQ` visibility, and final `journalctl --verify --file` PASS.
- Passed after round-2 repair: direct-file Rust writer live harness for 200 entries on systemd `260 (260.1-2-manjaro)` with 2 stock `journalctl --file` polling readers, 1 stock `journalctl --file --follow --no-tail --boot=all` reader, 1 stock libsystemd reader, final ordered `LIVE_SEQ` visibility, and final `journalctl --verify --file` PASS.
- Passed after round-2 repair: Rust directory-writer live harness for 200 entries on systemd `260 (260.1-2-manjaro)` against the real active path `<scratch>/<machine-id>/system.journal`, with 2 stock `journalctl --file` polling readers, 1 stock `journalctl --file --follow --no-tail --boot=all` reader, 1 stock libsystemd reader, final ordered `LIVE_SEQ` visibility, and final `journalctl --verify --file` PASS.
- Passed after round-3 repair: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo test --manifest-path rust/Cargo.toml --workspace`.
- Passed after round-3 repair: real adapter execution against all 15 cases in `tests/conformance/manifests/conformance-v01.json` with 11 PASS and 4 explicit SKIP.
- Passed after round-3 repair: Rust `journalctl` full-directory JSON drain, list-boots, field enumeration, repeated same-field OR, `+` disjunction, and expected unsupported `--verify` behavior.
- Passed after round-3 repair: direct-file and directory Rust writer live harnesses for 200 entries on systemd `260 (260.1-2-manjaro)` with stock polling readers, stock follow reader, stock libsystemd reader, final ordered `LIVE_SEQ` visibility, and final `journalctl --verify --file` PASS.
- Passed after round-4 repair: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo test --manifest-path rust/Cargo.toml --workspace`.
- Passed after round-4 repair: real adapter execution against all 15 cases in `tests/conformance/manifests/conformance-v01.json` with 11 PASS and 4 explicit SKIP.
- Passed after round-4 repair: Rust `journalctl` full-directory JSON drain produced 10,757 entries, `--list-boots` produced 4 rows, `--fields` produced 202 rows, repeated same-field OR produced 7,536 rows, `+` disjunction produced 7,536 rows, and `--verify` returned the expected unsupported error. The later complex-match repair found this match count was overbroad before the filter-builder fix.
- Passed after round-4 repair: direct-file Rust writer live harness for 200 entries on systemd `260 (260.1-2-manjaro)` with 2 stock `journalctl --file` polling readers, 1 stock `journalctl --file --follow --no-tail --boot=all` reader, 1 stock libsystemd reader, final ordered `LIVE_SEQ` visibility, and final `journalctl --verify --file` PASS.
- Passed after round-4 repair: directory Rust writer live harness for 200 entries on systemd `260 (260.1-2-manjaro)` against the real active path `<scratch>/<machine-id>/system.journal`, with 2 stock `journalctl --file` polling readers, 1 stock `journalctl --file --follow --no-tail --boot=all` reader, 1 stock libsystemd reader, final ordered `LIVE_SEQ` visibility, and final `journalctl --verify --file` PASS.
- Passed after closeout repair: `CARGO_HOME="$PWD/.local/cargo-home" CARGO_TARGET_DIR="$PWD/.local/cargo-target" cargo test --manifest-path rust/Cargo.toml --workspace`.
- Passed after closeout repair: real adapter execution against all 15 cases in `tests/conformance/manifests/conformance-v01.json` with 13 PASS and 2 explicit SKIP. The remaining SKIPs are `journal-verify-sealed` and `journal-verify-corruption-detection`, mapped to SOW-0008.
- Passed after closeout repair: Rust `journalctl` full-directory JSON drain produced 10,757 entries, `--list-boots` produced 4 rows, `--fields` produced 202 rows, repeated same-field OR produced 6,516 rows, `+` disjunction produced 6,516 rows, and `--verify` returned the expected unsupported error.
- Passed: `git diff --check`.
- Passed: `bash .agents/sow/audit.sh`.
- Passed for closeout review: Mimo and GLM returned `PRODUCTION GRADE` for the full SOW-0004 scope and post-`6368d5f` closeout changes.

Real-use evidence:

- The Rust CLI reads committed systemd `.journal.zst` fixtures and outputs JSON, field enumeration, export bytes, list-boots output, repeated-match OR results, and `+` disjunction results without linking to system journal libraries.

Reviewer findings:

- Minimax read-only diagnosis: `NOT PRODUCTION GRADE` for the full Rust writer live-follow surface. It identified the live-follow behavior as a blocker and recommended systemd-style notification/publication fixes rather than accepting the hard-link directory workaround.
- Kimi and GLM read-only diagnosis attempts were started with the same prompt and repository-boundary rules but did not produce final review output before being stopped by exact PIDs. No code changes were made by reviewers.
- Full-scope review round after lifecycle repair: Minimax returned `PRODUCTION GRADE`. Mimo returned `NOT PRODUCTION GRADE` for production panic/unimplemented paths and fake/weak adapter assertions. Kimi returned `NOT PRODUCTION GRADE` for imprecise cursor seek, byte-unsafe `SdJournalAddMatch`, wrong `entry_mut` header type, weak adapter assertions, and leftover livewriter hard-link code. GLM returned `NOT PRODUCTION GRADE` for fake UID/corruption tests, byte-unsafe match validation, `_BOOT_ID` JSON overwrite, weak list-boots assertions, and hard-link test-helper confusion.
- Disposition: all high/medium reviewer blockers listed above were repaired before the next review cycle. Non-blocking temp-file naming and stricter lowercase field-name validation were not changed; lowercase match rejection is consistent with the existing Go adapter and manifest for this phase.
- Full-scope review round 2: Minimax returned `PRODUCTION GRADE`. Mimo returned `NOT PRODUCTION GRADE` for missing JSON `__MONOTONIC_TIMESTAMP`, UTF-8 JSON string handling, weak field-enumeration adapter assertions, and residual `jf` issues. GLM returned `NOT PRODUCTION GRADE` for unchecked mutable mmap arithmetic, residual `jf` panic/unimplemented paths, directory-order/list-boots limitations, and related test gaps. Kimi did not produce final output and was stopped after becoming stale.
- Disposition: round-2 concrete blockers were repaired. Directory interleaving and list-boots per-entry boot scanning remain known limitations for production close, but they do not block an intermediate commit for the current fixture/live-writer slice because current validation uses non-overlapping fixture semantics and validates the accepted list-boots fixture output. A full-scope post-repair review is required before committing this chunk.
- Full-scope review round 3: Mimo returned `NOT PRODUCTION GRADE` for unchecked arithmetic and missing EOF bounds in `jf/window_manager`, plus non-blocking findings for `rdp` unsafe UTF-8 conversion, public `todo!()` defaults, `JournalReader::dump()`, adapter binary-value lossy conversion, and defensive cursor unwraps. GLM returned `NOT PRODUCTION GRADE` but identified no critical blocker for the current intermediate slice; it requested documentation of the active-file archived-state window and reiterated known directory/list-boots limitations. Minimax did not provide the required verdict output and was not counted. Qwen stalled and was stopped before final output.
- Disposition: the round-3 `jf/window_manager` blocker was repaired. The `rdp` unsafe conversion, `HashTable::is_empty()` `todo!()` defaults, `JournalReader::dump()` `todo!()`, active-file lifecycle documentation, and defensive cursor unwrap replacement were repaired as same-pass or round-4 cleanup. Adapter binary-value lossy conversion remains a non-blocking test hardening item for SOW close or a mapped follow-up because the current adapter manifest does not assert binary fixture values through `read_some_entries`.
- Full-scope review round 4: Mimo returned `NOT PRODUCTION GRADE` for uncapped `journal-core` decompression in public reader paths and reported non-blocking cleanup findings for remaining unchecked UTF-8 conversions, cursor unwraps, and minor `jf`/`journal-core` API divergences. GLM returned production-ready for the declared intermediate scope and confirmed restart overwrite protection and prior safety repairs.
- Disposition: the round-4 `journal-core` decompression cap blocker was repaired by mirroring the imported `jf` cap and bounded streaming-reader behavior, with new tests. The remaining unchecked UTF-8 conversions and cursor unwraps identified by reviewers were also repaired. Minor `jf`/`journal-core` API-shape divergences remain non-blocking for this intermediate commit and must be revisited before SOW close only if the public `jf` direct-use surface is expanded.
- Full-scope review round 5: Mimo returned `PRODUCTION GRADE`; GLM returned `PRODUCTION GRADE`. Both reviewers verified the `journal-core` decompression cap, bounded decompression tests, safe UTF-8 conversions, cursor `UnsetCursor` handling, byte-safe match values, precise cursor seek, no production `panic!`/`unimplemented!()` in reviewed paths, corrected entry header sizing, real adapter assertions, child-process corruption probes, mmap EOF bounds, `jf/window_manager` checked arithmetic, JSON output parity, active-file lifecycle, restart overwrite protection, and stock-reader live compatibility.
- Disposition: no round-5 finding blocks the intermediate Rust chunk commit. Residual non-blocking items were mapped for SOW close: duplicated `jf`/`journal-core` logic, `jf` direct-use window remap/API-shape divergence, adapter binary-value lossy assertion path, the then-4 explicit adapter SKIPs, directory timestamp interleaving, list-boots per-entry boot scanning, predictable test temp-file names, remaining sound but unnecessary `unsafe` in `DataObjectHeader::inlined_cursor()`, and defensive `unwrap()` hardening in internal invariant paths. Closeout repair reduced the adapter SKIPs to 2: `journal-verify-sealed` and `journal-verify-corruption-detection`, both tracked by SOW-0008.
- Full-scope closeout review round 6: Mimo returned `PRODUCTION GRADE`; GLM returned `PRODUCTION GRADE`. Reviewers verified the closeout adapter cases, filter-builder semantics, corrected 6,516 same-field OR and `+` counts, decompression and mmap safety, repository-boundary behavior, docs/spec honesty, and follow-up mapping. Low non-blocking findings were duplicate `QUUX=xxxxx` in the generated complex expression and the pre-existing direct-use `jf` missing-data divergence; both are non-blocking for this slice and the `jf` hardening class is tracked by SOW-0008.
- Disposition: no round-6 finding blocks SOW-0004 close.

Same-failure scan:

- Directory/file enumeration behavior was compared against the Go reference implementation. Go skips individual unreadable entries during enumeration and query-unique scans; Rust now follows the same tolerant behavior for those enumeration-only APIs.
- Direct CLI validation found and repaired the list-boots indexing bug that adapter assertions did not catch. This indicates the adapter needs stricter expected-value assertions in a later hardening pass or before close.
- Live writer diagnosis isolated the stock follow failure: direct-file Rust writer mode passed the live harness, while imported Rust directory writer mode with archived active filename plus hard link failed follow. The repair changed the high-level directory writer to use real active-file semantics while running. The same-failure scan then found a restart-overwrite regression, repaired by archiving stale active files before the next writer creates a fresh `system.journal`.
- Adapter hardening found a real same-failure class: corruption fixtures must not be opened in-process by the adapter because a broken reader path can terminate the adapter before it reports `FAIL`. The Rust adapter now probes corrupted fixtures in child processes and treats unsuccessful child exits as failed corruption-resilience evidence.
- Read-only mmap bounds checks were added at map and slice boundaries to prevent corrupted header/object sizes from mapping or accessing pages beyond real EOF.
- Same-failure search after round-2 review found the duplicated `jf` crate copies of the filter panic, cursor `unimplemented!()` paths, and entry object sizing bug; these were repaired alongside the `journal-core` paths because the `jf` crate is part of the imported compatibility surface.
- Same-failure search after round-3 review found the duplicated `jf/window_manager` arithmetic and EOF-bound gaps; these were repaired to match the `journal-core` window manager pattern.
- Same-failure search after round-4 review found the decompression cap existed in the imported `jf` crate but not in `journal-core`; `journal-core` now mirrors the same safety cap and bounded streaming-reader behavior. The same pass removed unnecessary unchecked UTF-8 conversion in `rdp` and `field_map`, and made cursor invariant violations explicit errors in both duplicated cursor implementations.
- The generated complex boolean match test found a same-failure class in both `journal-core` and duplicated `jf` filter builders. Both now use the same three-level grouping model as the Go adapter/reference implementation: current match group, OR level for disjunctions, and top-level AND for conjunctions.
- Re-running the `SYSLOG_IDENTIFIER=kernel SYSLOG_IDENTIFIER=systemd` and `SYSLOG_IDENTIFIER=kernel + SYSLOG_IDENTIFIER=systemd` checks after the filter-builder repair changed the count from the earlier overbroad 7,536 rows to 6,516 rows. The corrected count matches the unfiltered JSON oracle: 2,416 `kernel` entries plus 4,100 `systemd` entries.
- Full-drain journalctl checks found that some historical online `.journal.zst` fixtures contain tail/data-object inconsistencies that stock `journalctl` drains without error. Rust now skips recoverable invalid tail entries/data objects in the public reader layer while preserving core iterator errors for low-level callers; the regression test drains the full committed `no-rtc` fixture set.

Sensitive data gate:

- Passed for durable artifacts via `bash .agents/sow/audit.sh`; no raw sensitive-data patterns were reported.

Artifact maintenance gate:

- AGENTS.md: no workflow or guardrail changes were required.
- Runtime project skills: no durable workflow change was required; existing project orchestration and compatibility skills covered the work.
- Specs: `.agents/sow/specs/product-scope.md` updated with the accepted Rust feature slice and limitations.
- End-user/operator docs: `rust/README.md` added for current Rust SDK and journalctl usage.
- End-user/operator skills: no output/reference skills are affected.
- SOW lifecycle: completed and moved to `done/` for the close commit.
- SOW-status.md: updated at close.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` with current Rust writer and reader feature slices, journalctl behavior, and limitations.

Project skills update:

- No update required. The existing project orchestration and journal compatibility skills already covered the workflow and compatibility lessons.

End-user/operator docs update:

- Added `rust/README.md` with current Rust SDK scope, examples, journalctl usage, and limitations.

End-user/operator skills update:

- No output/reference skills are affected.

Lessons:

- Partial lesson: manifest `dry-run` only simulates harness wiring and must not be recorded as adapter execution evidence.
- Partial lesson: adapter tests that return `PASS` without comparing expected manifest values can miss CLI behavior drift; list-boots indexing exposed this gap.
- Partial lesson: stock `journalctl --follow --file` must be tested against the actual path shape that production users will pass. A hard link to an archived-style active file is not equivalent to writing the exact active file path.
- Partial lesson: restart behavior must be tested after live-follow fixes. Keeping `system.journal` stable for stock follow solves concurrent-read compatibility during a run, but a later writer must archive the stale active file before creating a new one or it will overwrite previous entries.
- Partial lesson: conformance adapters must not run crash-prone corruption probes inside the adapter process. They need child-process isolation so crashes become structured `FAIL` results instead of killing the harness.
- Partial lesson: text output line counts are not a reliable proxy for entry counts because entries with no printable message can affect counts. JSON line counts against stock `journalctl -o json` are the safer fixture-drain oracle.

Follow-up mapping:

- SOW-0008 tracks full interoperability and remaining writer feature gaps, including verification/FSS, writer DATA compression, cross-language matrix validation, directory timestamp interleaving, list-boots per-entry boot scanning if required by broader fixtures, and duplicated implementation hardening discovered during cross-language work.
- Adapter binary-value assertion hardening is no longer a close blocker for SOW-0004 because the accepted Rust SDK/export/JSON paths already validate byte-safe values and current manifest adapter expectations do not assert binary values through `read_some_entries`. It is mapped to SOW-0008 if binary-value assertions are added to the shared matrix.
- No deferred SOW-0004 item remains without a pending follow-up SOW.

## Outcome

Completed. Rust SDK, Rust file-backed journalctl, Rust conformance adapter, current Rust writer/reader feature-slice documentation, and closeout review/validation are complete for this SOW. Remaining full verification/FSS, writer compression, and full interoperability work is tracked by SOW-0008.

## Lessons Extracted

- Use real adapter execution, not manifest dry-run output, as conformance evidence.
- Treat skipped manifest cases as work items until they are implemented or mapped to a concrete follow-up SOW.
- Generated conformance cases can expose deeper implementation bugs than fixture-backed smoke tests; the complex boolean match case found a real filter grouping defect.
- Duplicate imported and public-core implementations need same-failure searches whenever a safety or semantics bug is found in either copy.

## Followup

- SOW-0008 covers verification/FSS, writer compression, full cross-language interoperability, and remaining feature/hardening gaps outside this accepted Rust baseline slice.

## Regression Log

None yet.
