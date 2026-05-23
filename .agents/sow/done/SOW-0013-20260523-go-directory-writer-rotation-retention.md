# SOW-0013 - Go Directory Writer Rotation And Retention

## Status

Status: completed

Sub-state: completed; the Go high-level directory writer with rotation and retention is implemented, reviewed, validated, and ready for the close commit.

## Requirements

### Purpose

Deliver a production-usable pure-Go journal writer for a Netdata plugin integration. The writer must not require the caller to manually manage a single ever-growing file; it must manage a journal directory, rotate files, and enforce retention while preserving stock systemd reader compatibility.

### User Request

The user confirmed that the imported Rust writer's directory rotation and retention model should be ported to Go.

### Assistant Understanding

Facts:

- The current Go `journal.Writer` is a low-level single-file writer using `Create(path, opts)` and `Open(path)`.
- The imported Rust `journal-log-writer` crate is the high-level writer layer with directory, rotation, and retention policies.
- The Go writer already supports binary fields and live stock-reader compatibility for its current single-file feature slice.
- systemd journal files are safe to read while active or archived when entries are fully written.
- systemd/journald retention removes archived files, not active files.
- Daemon-only `journalctl --rotate` remains out of scope; this SOW is SDK-managed writer rotation.

Inferences:

- The Go implementation should keep the existing single-file writer as the primitive and add a high-level `Log` or directory-writer API above it.
- The high-level Go API should follow the Rust model closely enough that later cross-language tests can share semantics.
- The first Go directory writer should manage systemd-compatible filenames and directory layout, but does not need compression or FSS in this SOW.

Unknowns:

- No activation-blocking unknowns remain. Edge cases discovered during implementation or stock-tool testing must be resolved before close.

### Acceptance Criteria

- Go exposes a high-level directory writer API above the existing single-file `Writer`.
- The API accepts a directory and configuration with rotation and retention policies.
- Rotation supports at least max entries per file and max active file size.
- Retention supports at least max retained files and max retained bytes, deleting oldest archived files first and never deleting the current active file.
- File names follow the systemd archived-name pattern or the imported Rust writer's compatible pattern, with a stable source/prefix such as `system`.
- Rotated files are marked archived and readable by stock `journalctl`.
- The active file and archived files are readable through stock `journalctl --directory` or equivalent stock directory/file validation.
- Binary fields still work through the directory writer.
- Existing single-file Go writer API and tests remain compatible.
- Go remains pure Go: no CGO and no system journal library linkage.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `AGENTS.md`
- `.agents/skills/project-agent-orchestration/SKILL.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `.agents/sow/specs/product-scope.md`
- `go/journal/writer.go`
- `go/README.md`
- `rust/src/crates/journal-log-writer/src/lib.rs`
- `rust/src/crates/journal-log-writer/src/log/config.rs`
- `rust/src/crates/journal-log-writer/src/log/mod.rs`
- `rust/src/crates/journal-log-writer/src/log/chain.rs`
- `rust/src/crates/journal-log-writer/tests/log_writer.rs`
- `rust/src/crates/journal-registry/src/repository/file.rs`
- `rust/src/crates/journal-registry/src/repository/collection.rs`
- `systemd/systemd @ cf3156842209`
  - `docs/JOURNAL_FILE_FORMAT.md`
  - `man/journald.conf.xml`
  - `man/journalctl.xml`
  - `man/systemd-journald.service.xml`
  - `src/libsystemd/sd-journal/journal-file.c`
  - `src/shared/journal-file-util.c`
- Official systemd journal file format documentation: `https://systemd.io/JOURNAL_FILE_FORMAT/`
- Debian-rendered systemd 260.1 `systemd-journald(8)` documentation.

Current state:

- `go/journal/writer.go` has a single-file `Writer` with `Create(path, opts)` and `Open(path)`.
- `go/README.md` documents single-file usage.
- `rust/src/crates/journal-log-writer/src/lib.rs` describes a high-level directory writer with automatic rotation and retention.
- `rust/src/crates/journal-log-writer/src/log/config.rs` defines rotation by size, duration, and entry count, and retention by file count, total size, and age.
- `rust/src/crates/journal-log-writer/src/log/mod.rs` rotates before writes when policy requires it, archives the old file, creates a successor, and applies retention.
- `rust/src/crates/journal-log-writer/src/log/chain.rs` tracks ordered files and deletes oldest files for retention.
- systemd documentation states journalctl and libsystemd can read active and archived files, and journald removes oldest archived files to limit disk use.

Risks:

- Incorrect archive naming can make stock `journalctl --directory` skip files.
- Deleting the active file would break the one-writer/multiple-reader contract and may lose data.
- Rotation has publication-window risks: the old file must be closed or archived safely before the new file becomes active.
- Size-based rotation must use actual committed file size, not sparse allocation assumptions.
- Retention must not remove unrelated journal files in the same directory.
- Public API choices made here will shape Go, Rust, Node.js, and Python parity.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The low-level Go writer can produce valid single journal files, but a plugin cannot rely on a single unbounded file for production use.
- The imported Rust source already separates the low-level journal file writer from a high-level log writer that owns a directory, file chain, rotation policy, and retention policy.
- systemd itself treats active and archived journal files as one readable stream and vacuums archived files only. The Go SDK needs the same operational layer.

Evidence reviewed:

- `go/journal/writer.go`: single-file `Create(path, opts)`, `Open(path)`, and one `*os.File` inside `Writer`.
- `rust/src/crates/journal-log-writer/src/log/config.rs`: rotation/retention policy fields and builders.
- `rust/src/crates/journal-log-writer/src/log/mod.rs`: `write_entry`, `should_rotate`, `rotate`, active file, lifecycle events.
- `rust/src/crates/journal-log-writer/src/log/chain.rs`: directory scan, file size accounting, oldest-file deletion, duration retention.
- `rust/src/crates/journal-registry/src/repository/file.rs`: active, archived, and disposed journal filename parsing.
- `systemd/systemd @ cf3156842209`
  - `src/libsystemd/sd-journal/journal-file.c`: archive name format uses `original@seqnum_id-head_entry_seqnum-head_entry_realtime.journal`.
  - `man/journald.conf.xml`: only archived files are deleted for size/file-count retention.
  - `man/journalctl.xml`: vacuum size/time/files operate on archived journal files; rotation archives current active files.
  - `docs/JOURNAL_FILE_FORMAT.md`: one-writer/multiple-reader concurrency and sequence-number continuity.

Affected contracts and surfaces:

- Go public API in package `journal`.
- Go writer docs.
- Product scope spec.
- Go tests and stock-tool compatibility evidence.
- Future cross-language writer API parity.

Existing patterns to reuse:

- Existing `Writer`, `Options`, `EntryOptions`, and `Field` types.
- Existing stock `journalctl` helpers in `go/journal/writer_test.go`.
- Existing live stock-reader tests for single files.
- Imported Rust `journal-log-writer` policies and directory chain behavior.

Risk and blast radius:

- Medium API blast radius: adds new API without breaking existing single-file users.
- Medium compatibility risk: stock directory readers must discover active and archived files.
- Data-loss risk if retention deletes current or unrelated files; implementation must scope deletions to files matching the configured source/prefix and archive status.
- Performance risk is low for first implementation because directory scans are small and rotation happens at file boundaries; later SOW-0009 covers profiling.

Sensitive data handling plan:

- Tests use synthetic messages and byte payloads only.
- Generated journal files stay under Go test temporary directories.
- Durable artifacts must not include raw secrets, credentials, bearer tokens, SNMP communities, personal data, customer data, private endpoints, or proprietary incident details.

Implementation plan:

1. Add a high-level Go directory writer using the existing single-file `Writer`.
2. Add filename/source helpers for `system`, user, or custom source prefixes with systemd-compatible active and archived file naming.
3. Add rotation by entry count and size for the first production slice.
4. Add retention by max files and max bytes, scoped to archived files owned by the configured source/prefix.
5. Add tests for rotation, retention, binary fields, stock `journalctl --directory` or file-matrix readback, and no-CGO.
6. Update docs/specs/SOW status.
7. Run external read-only reviewers and iterate until production-grade.

Validation plan:

- `go test -count=1 ./...`
- `go test -race -count=1 ./...`
- `CGO_ENABLED=0 go test -count=1 ./...`
- `go vet ./...`
- `go list -deps -f '{{if .CgoFiles}}{{.ImportPath}} {{.CgoFiles}}{{end}}' ./...`
- Targeted directory writer tests.
- Stock `journalctl --verify` on each generated file.
- Stock `journalctl --directory` readback when the generated directory layout is accepted by stock tools; otherwise record the exact stock-tool limitation and validate every file with `--file`.
- Existing live stock-reader tests for the single-file writer.
- `.agents/sow/audit.sh`
- Sensitive-data scan over changed durable artifacts.

Artifact impact plan:

- AGENTS.md: no expected update; workflow does not change.
- Runtime project skills: update only if implementation exposes recurring rotation/retention compatibility rules.
- Specs: update product scope to record Go directory writer rotation and retention once validated.
- End-user/operator docs: update `go/README.md`.
- End-user/operator skills: none expected.
- SOW lifecycle: this SOW is current/in-progress and will move to done only with implementation, validation, review, and commit in the same chunk.
- SOW-status.md: update active/next status now and close status when complete.

Open-source reference evidence:

- `systemd/systemd @ cf3156842209`
  - `docs/JOURNAL_FILE_FORMAT.md`
  - `man/journald.conf.xml`
  - `man/journalctl.xml`
  - `man/systemd-journald.service.xml`
  - `src/libsystemd/sd-journal/journal-file.c`
  - `src/shared/journal-file-util.c`
- `ktsaou/netdata @ 6a515000ac89`
  - `src/crates/journal-log-writer/src/lib.rs`
  - `src/crates/journal-log-writer/src/log/config.rs`
  - `src/crates/journal-log-writer/src/log/mod.rs`
  - `src/crates/journal-log-writer/src/log/chain.rs`
  - `src/crates/journal-log-writer/tests/log_writer.rs`
  - `src/crates/journal-registry/src/repository/file.rs`

Open decisions:

- Resolved by user: port the imported Rust high-level directory writer model to Go.

## Implications And Decisions

1. Directory writer model
   - Current state: resolved by user confirmation on 2026-05-23.
   - Selection: port the Rust `journal-log-writer` model as a high-level Go API above the existing single-file writer.
   - Implication: the single-file Go writer remains as a primitive; production plugin use should use the new directory writer.
   - Risk: public API expands now, but avoids building plugin integration on an operationally incomplete primitive.

2. Scope of this first Go directory writer
   - Current state: resolved by SOW scope.
   - Selection: implement rotation by entry count and active file size, and retention by file count and total bytes.
   - Implication: duration-based rotation/retention can be implemented in a later chunk if still needed.
   - Risk: max-age policies are not available immediately, but size/count are the critical bounded-disk controls for plugin use.

3. Filename/source behavior
   - Current state: resolved by compatibility target.
   - Selection: support a stable source/prefix with default `system`, and generate systemd-compatible active/archived journal filenames scoped to that prefix.
   - Implication: callers can use a custom prefix for plugin-owned directories without deleting unrelated journal files.
   - Risk: custom prefixes must be validated so stock tools do not skip files.

## Plan

1. Implement directory writer types and filename helpers.
2. Implement rotation and archived close support in or around the single-file writer.
3. Implement retention over owned archived files only.
4. Add stock-tool-backed tests and docs.
5. Run validation, external review, fixes, SOW close, and commit.

## Delegation Plan

Implementer:

- Local implementation is allowed because the user explicitly allowed direct edits when faster. Minimax will be used as a reviewer rather than implementer for this SOW.

Reviewers:

- Use at least three read-only reviewers from `llm-netdata-cloud/minimax-m2.7-coder`, `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Record reviewer failures in this SOW.
- Fix all blocking findings and rerun full-scope review.
- Do not close if audit, validation, or production-grade review gates fail.

## Execution Log

### 2026-05-23

- Created and activated this SOW for Go directory writer rotation and retention.
- Implemented a high-level Go `Log` directory writer in `go/journal/log.go`.
- Extended the low-level Go `Writer` with:
  - configurable first sequence number for rotated successor files;
  - current committed file size reporting;
  - archived close-and-rename support;
  - canonical UUID string formatting.
- Added directory-writer tests for:
  - entry-count rotation;
  - active-file stock `journalctl --directory` readback;
  - close without append;
  - close after creating but not writing an active file;
  - file-size rotation;
  - file-count retention;
  - active-file retention during an open session;
  - byte-size retention;
  - reopening an existing active file and continuing sequence numbers;
  - binary field compatibility;
  - custom source/prefix naming;
  - append rejection after `Log.Close`.
- Updated Go package docs, Go README, product scope spec, and SOW status summary.
- Ran four independent read-only reviewers. All returned `PRODUCTION GRADE`.
- Fixed reviewer hardening findings before close:
  - documented that `Log` is not safe for concurrent method calls;
  - documented that `MaxFiles` counts archived files and `MaxBytes` counts active plus archived bytes while never deleting active;
  - kept the archive rename under the writer lock until after the same-directory rename and parent directory sync;
  - marked `Log` closed after a successful archive even when retention cleanup reports an error;
  - added restart/reopen, active-retention, and append-after-close tests.
- Fixed second-cycle reviewer findings:
  - `NewLog()` now closes a reopened active writer before returning a retention error, preventing leaked file descriptors and retained writer locks;
  - `archiveActive()` now clears a closed writer after post-rename cleanup failure, so `Close()` is idempotent after archive cleanup errors;
  - added synthetic failure tests for both paths.
- Completed the third read-only review cycle after the second-cycle fixes. All four reviewers returned `PRODUCTION GRADE`.
- Applied final low-risk hardening from third-cycle feedback:
  - `Log.Close()` now marks the log closed before returning an empty-active-file close/remove error;
  - added close-without-append and empty-active-file close tests;
  - documented that `WithMaxFileSize` should be larger than the minimum journal file overhead unless rotation after every non-empty append is acceptable.
- Completed the fourth read-only review cycle after final hardening. Minimax, Qwen, GLM, and Kimi returned `PRODUCTION GRADE`; Mimo's replacement-cycle attempt exited without the required verdict and was not counted.

## Validation

Acceptance criteria evidence:

- High-level directory API: `go/journal/log.go` defines `NewLog`, `LogConfig`, `RotationPolicy`, `RetentionPolicy`, `Log.Append`, `Log.AppendMap`, `Log.Sync`, `Log.Close`, `Log.ActivePath`, and `Log.JournalDirectory`.
- Rotation by entry count and active file size: `go/journal/log.go` implements `RotationPolicy.WithMaxEntries`, `RotationPolicy.WithMaxFileSize`, and `Log.shouldRotate`.
- Retention by archived file count and total bytes: `go/journal/log.go` implements `RetentionPolicy.WithMaxFiles`, `RetentionPolicy.WithMaxBytes`, and `Log.enforceRetention`.
- Active file is never deleted by retention: retention scans and deletes only archived files returned by `Log.archivedFiles`; active file size is counted but active file path is not added to the deletion set.
- Systemd-compatible active and archived names: active file is `<source>.journal`; archives are `<source>@<seqnum_id>-<head_seqnum_hex16>-<head_realtime_hex16>.journal`.
- Stock-tool evidence: directory writer tests use `journalctl --directory` for active, archived, binary, rotation, retention, and custom-source readback.
- Binary fields: `TestLogBinaryFieldCompatibility` writes a byte payload through `Log.Append` and validates stock `journalctl --directory --output=json` byte-array output.
- Existing single-file API remains compatible: full Go package tests pass.
- Pure Go: `(cd go && CGO_ENABLED=0 go test -count=1 ./...)` passes and `go list` reports no CGO files.

Tests or equivalent validation:

- `(cd go && go test -count=1 -run 'TestLog' -v ./journal)` passed after the reviewer fixes.
- `(cd go && go test -count=1 -run 'TestLogCloseWithoutAppendDoesNotCreateFile|TestLogCloseRemovesEmptyActiveFile|TestNewLogClosesReopenedWriterOnRetentionFailure|TestLogCloseIsIdempotentAfterArchiveCleanupFailure' -v ./journal)` passed after final hardening.
- `(cd go && go test -count=1 -run 'TestLog|TestNewLogClosesReopenedWriterOnRetentionFailure' -v ./journal)` passed after final hardening.
- `(cd go && go test -count=1 -run 'TestLog|TestWriterBinaryFieldCompatibility' -v ./journal)` passed before reviewer fixes.
- `(cd go && go test -count=1 ./...)` passed after final hardening.
- `(cd go && CGO_ENABLED=0 go test -count=1 ./...)` passed after final hardening.
- `(cd go && go vet ./...)` passed after final hardening.
- `(cd go && go list -deps -f '{{if .CgoFiles}}{{.ImportPath}} {{.CgoFiles}}{{end}}' ./...)` returned no output after final hardening.
- `(cd go && go test -race -count=1 ./...)` passed after final hardening. Earlier in this SOW, one race run hit an existing intermittent live-reader test failure in `sd_journal_get_data(LIVE_SEQ)`; targeted live tests passed, and subsequent full race suites passed.
- `(cd go && go test -count=1 -run 'TestGoWriterLiveStockReaders|TestGoWriterLiveStockReadersStress|TestGoWriterLiveInterruptionReopenAndVerify|TestGoWriterLiveRejectsSecondWriter' -v ./journal)` passed after the reviewer fixes.
- `python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json` passed.
- `python3 tests/conformance/runner/manifest_checker.py dry-run tests/conformance/manifests/conformance-v01.json` passed.
- `git diff --check` passed.
- `bash .agents/sow/audit.sh` passed while this SOW was current/in-progress.

Real-use evidence:

- Stock `journalctl --directory` successfully read files generated by the high-level Go directory writer while active and after close/rotation.
- Existing live stock-reader tests for the low-level Go writer passed with stock `journalctl --file`, stock `journalctl --file --follow --no-tail --boot=all`, and stock libsystemd reader API coverage.

Reviewer findings:

- First read-only review cycle:
  - Minimax: `PRODUCTION GRADE`; low-severity notes on concurrent API use, active byte-retention semantics, append-after-close coverage, and stress coverage. Disposition: documented non-concurrent `Log` contract and active retention semantics; added append-after-close coverage; stress/performance remains covered by later benchmark/profiling SOW.
  - Mimo: `PRODUCTION GRADE`; low-severity notes on non-goroutine-safe `Log`, `archiveTo` edge-case handling, active-retention test coverage, `MaxFiles` archived-only semantics, and exported `ParseUUID`. Disposition: documented `Log` and retention semantics, strengthened `archiveTo`, added active-retention coverage, kept `ParseUUID` exported because it is useful for callers parsing machine IDs and boot IDs.
  - Qwen: `PRODUCTION GRADE` with two medium hardening requests before close: `Close()` state after retention failure and missing reopen-active-file test. Disposition: set `Log.closed` after successful archive even if retention cleanup returns an error; added `TestLogReopensActiveFileAndContinuesSequence`.
  - GLM: `PRODUCTION GRADE`; medium notes on archive rename edge case and missing reopen-active-file test; low notes on broader verify coverage, non-concurrent `Log`, and duplicated `AppendMap` helper logic. Disposition: strengthened `archiveTo`, added reopen test, documented non-concurrent `Log`; broader verify and helper refactor are not product blockers and remain covered by current tests or future maintenance.
- Second read-only review cycle after first fixes:
  - Mimo: `PRODUCTION GRADE`; only low/info notes such as adding an example test and optional retention-error testing. Disposition: not blocking; API example already exists in README and can be added later if examples are expanded.
  - GLM: `PRODUCTION GRADE`; low/info notes about integer type consistency, internal test setup, duplicated helper logic, and empty-file close coverage. Disposition: not blocking; practical overflow impossible for this slice, internal test setup intentionally simulates process release, helper refactor rejected as small, and empty-file close coverage can be added in maintenance.
  - Minimax: `PRODUCTION GRADE`; no blocking issues.
  - Qwen: `NOT PRODUCTION GRADE`; found two real error-path issues: leaked reopened writer when `NewLog()` opens an active file then retention fails, and non-idempotent `Close()` if archive rename succeeded but post-rename cleanup failed. Disposition: both fixed and covered by `TestNewLogClosesReopenedWriterOnRetentionFailure` and `TestLogCloseIsIdempotentAfterArchiveCleanupFailure`.
- Third read-only review cycle after second-cycle fixes:
  - Mimo: `PRODUCTION GRADE`; low-severity notes on empty-active-file close state, non-parallel-safe sync test hook, implicit retention-error behavior during rotation, `AppendMap` duplication, and close-without-append coverage. Disposition: fixed empty-active-file close state and added close-without-append and empty-active-file close tests; remaining notes are non-blocking and documented or intentionally accepted.
  - GLM: `PRODUCTION GRADE`; low-severity notes on `AppendMap` duplication, non-parallel-safe sync test hook, missing rename-failure rollback test, and tiny `MaxFileSize` behavior. Disposition: documented tiny `MaxFileSize` behavior; remaining notes are non-blocking and either already dispositioned or require extra test injection not justified for this slice.
  - Minimax: `PRODUCTION GRADE`; no blocking issues and recommended close.
  - Qwen: `PRODUCTION GRADE`; no blocking issues.
- Fourth read-only review cycle after final hardening:
  - Minimax: `PRODUCTION GRADE`; no blockers, no security issues, no unwanted side effects, and recommended close. Low/info notes: exported `ParseUUID`, small `AppendMap` duplication, package-level sync test hook, and empty-file size guard are accepted as intentional or non-blocking.
  - Qwen: `PRODUCTION GRADE`; no blockers, no security issues, no unwanted side effects, and recommended close. Low/info notes: theoretical rename-failure plus header-restore double I/O failure, missing runnable example for directory writer, and non-parallel-safe sync test hook. Disposition: not blocking; double I/O failure is outside practical recovery for this slice, README has the API example, and tests do not use `t.Parallel`.
  - GLM: `PRODUCTION GRADE`; no blockers, no security issues, no unwanted side effects, and recommended close. Low/info notes matched already dispositioned items: `AppendMap` duplication, non-concurrent `Log`, duration policies tracked outside this SOW, and test-only sync hook.
  - Mimo: replacement-cycle attempt ran validation but exited without the required verdict. Disposition: not counted for the reviewer gate.
  - Kimi: `PRODUCTION GRADE`; no blockers, no security issues, no unwanted side effects, and recommended close. Low/info notes: redundant field validation, `AppendMap` duplication, theoretical `entriesInFile` int overflow on 32-bit systems with impractical file sizes, empty-file close edge behavior, and missing direct rename-failure rollback test. Disposition: not blocking for this slice.

Same-failure scan:

- Searched the affected Go package through the full test suite, race suite, no-CGO test, vet, targeted live stock-reader tests, and new directory writer tests.
- Added tests for the same classes of issues reviewers found: active retention before close, reopen active file after writer release, and append after close.
- Added tests for second-cycle error-path issues: writer cleanup on `NewLog()` retention failure and `Close()` idempotency after archive cleanup failure.
- Added tests for third-cycle low-risk close-path feedback: close without append and close after creating but not writing an active file.
- Fourth-cycle reviewers verified the final hardened scope. The remaining low/info notes are explicitly dispositioned as non-blocking or already tracked.

Sensitive data gate:

- Changed durable artifacts contain only synthetic journal field names/messages, public file paths inside the repository, public upstream identities, and command names.
- No secrets, credentials, tokens, customer identifiers, private endpoints, production data, or personal data were written to durable artifacts.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository workflow and guardrails did not change.
- Runtime project skills: no update needed yet; existing compatibility and orchestration skills already cover the applied rules.
- Specs: updated `.agents/sow/specs/product-scope.md` for the Go directory writer feature slice.
- End-user/operator docs: updated `go/README.md`.
- End-user/operator skills: none exist for this SDK output and none were affected.
- SOW lifecycle: this SOW is completed and will be moved to `.agents/sow/done/` in the close commit.
- SOW-status.md: updated to show this SOW as active.

Specs update:

- `.agents/sow/specs/product-scope.md` now records high-level Go directory writing, rotation by entry count and active file size, and retention by archived file count and total byte size.

Project skills update:

- No project skill update was needed for this slice; no new recurring workflow rule was discovered beyond the existing journal compatibility and orchestration rules.

End-user/operator docs update:

- `go/README.md` now documents current directory-writer scope, deferred duration policies, and a `NewLog` example.

End-user/operator skills update:

- No end-user/operator skills exist or were affected by this SOW.

Lessons:

- Initial reviewers can miss restart/reopen behavior when fresh-path tests pass. Directory writers need explicit active-file reopen tests because plugin restarts are a primary production path.

Follow-up mapping:

- Duration-based rotation and retention remain deferred and are already tracked as future scope in `go/README.md` and by the broader pending SDK roadmap.
- Performance stress/profiling remains tracked by `.agents/sow/pending/SOW-0009-20260523-benchmark-profile-optimize.md`.
- Go reader and journalctl completion remains tracked by `.agents/sow/pending/SOW-0010-20260523-go-reader-and-journalctl-completion.md`.
- The duplicated `AppendMap` helper logic is harmless and small; it is rejected as not worth a separate SOW unless it recurs in another implementation.

## Outcome

Completed.

The Go SDK now has a production-usable high-level directory writer above the existing single-file writer. It creates active journal files below `<directory>/<machine-id>/`, archives files using the systemd-compatible `<source>@<seqnum_id>-<head_seqnum>-<head_realtime>.journal` pattern, supports rotation by entry count and committed file size, supports retention by archived file count and total bytes, preserves binary field support, and remains pure Go with no CGO or system journal library linkage.

## Lessons Extracted

- Directory writer validation needs explicit restart/reopen tests. Fresh-path tests did not prove sequence continuity after a plugin restart.
- Error-path review must include post-success cleanup failures. The archive rename can succeed while directory sync or cleanup still reports an error, and `Close()` must remain idempotent in that state.
- Low-level writer compatibility is easier to protect when directory management is layered above the single-file writer instead of changing existing append semantics.

## Followup

- Duration-based rotation and retention remain outside this SOW and are represented by the broader pending SDK roadmap.
- Performance stress and profiling remain tracked by `.agents/sow/pending/SOW-0009-20260523-benchmark-profile-optimize.md`.
- Go reader and file-backed journalctl completion remains tracked by `.agents/sow/pending/SOW-0010-20260523-go-reader-and-journalctl-completion.md`.
- Cross-language writer parity and full writer features remain tracked by `.agents/sow/pending/SOW-0008-20260523-interoperability-and-full-writer-features.md`.
- The duplicated `AppendMap` helper logic is rejected as not worth a separate SOW unless it grows or recurs across additional language implementations.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
