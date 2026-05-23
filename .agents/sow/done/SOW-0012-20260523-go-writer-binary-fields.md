# SOW-0012 - Go Writer Binary Fields

## Status

Status: completed

Sub-state: completed; binary field compatibility for the Go writer's current uncompressed feature slice is validated and committed as the gate before later SDK phases.

## Requirements

### Purpose

Deliver byte-safe binary field support in the pure-Go journal writer so it can be used by a Netdata plugin integration without losing or transforming arbitrary field values.

### User Request

The user stated that binary fields are required before the library can be used, and asked to complete this before moving forward.

### Assistant Understanding

Facts:

- The Go writer is the current priority implementation because it is needed for a Netdata plugin integration.
- Current Go writer fields already use `[]byte` values, but binary-field compatibility has not been proven by stock tooling tests.
- systemd journal entries may contain binary non-text data and repeated field names.
- The installed stock tools are systemd `260 (260.1-2-manjaro)` and libsystemd `260`.
- Compression remains out of scope for this SOW.

Inferences:

- The likely implementation change is focused tests and API documentation, not a new on-disk encoding, because journal DATA objects store raw `FIELD=value` payload bytes.
- Production support needs stock `journalctl` and libsystemd readback evidence for NUL bytes, control bytes, non-UTF8 bytes, and newline-containing values.

Unknowns:

- No activation-blocking unknowns remain. If stock tooling exposes a writer serialization bug, this SOW will fix it before close.

### Acceptance Criteria

- Go writer accepts binary field values through the idiomatic `Field{Name, Value []byte}` API without requiring UTF-8 strings.
- Binary values containing NUL, control bytes, non-UTF8 bytes, and newlines are written byte-for-byte into DATA objects.
- Stock `journalctl --verify --file` accepts the generated journal.
- Stock `journalctl --file --output=json` exposes non-printable/non-UTF8 binary values as byte arrays, matching documented journal JSON behavior.
- Stock `journalctl --file --output=export` exposes binary values with binary-safe export framing.
- A stock libsystemd helper using `sd_journal_get_data()` reads the same binary field byte-for-byte.
- Live stock-reader compatibility tests continue to pass for the existing Go writer feature slice.
- Go remains pure Go: no CGO and no system journal library linkage in the Go module.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `AGENTS.md`
- `.agents/skills/project-agent-orchestration/SKILL.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `.agents/sow/specs/product-scope.md`
- `.agents/sow/done/SOW-0005-20260523-go-sdk-and-journalctl.md`
- `.agents/sow/done/SOW-0011-20260523-live-concurrency-compatibility-gate.md`
- `go/journal/writer.go`
- `go/journal/writer_test.go`
- Official systemd Journal Export Formats documentation: `https://systemd.io/JOURNAL_EXPORT_FORMATS/`

Current state:

- `go/journal/writer.go` defines `Field.Value []byte`.
- `go/journal/writer.go` serializes payloads as `Name`, `=`, then raw `Value` bytes.
- `go/journal/writer_test.go` has stock journalctl readback tests for text values, but no byte-for-byte binary field tests.
- `go/README.md` states the writer currently writes uncompressed DATA objects and defers compression.

Risks:

- A text-oriented helper could accidentally become the documented integration path and corrupt binary values.
- journalctl JSON and export output have different binary representations; tests must verify both rather than assuming one CLI output mode is enough.
- The libsystemd proof must not become a Go dependency; any C helper must live only in tests and be compiled by test commands when libsystemd development files are available.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The writer's API and DATA serialization are byte-capable, but the product cannot claim binary field support until compatibility is proven with stock readers.
- systemd's documented export and JSON formats explicitly preserve binary fields: export uses a field-name/newline/64-bit-length/raw-bytes framing, and JSON uses byte arrays for non-printable or non-UTF8 values.
- The missing root cause is coverage, not known incompatible encoding.

Evidence reviewed:

- `go/journal/writer.go`: `Field.Value []byte` and raw payload assembly.
- `go/journal/writer_test.go`: existing journalctl text readback helpers.
- `systemd/systemd @ cf3156842209`
  - `man/systemd.journal-fields.xml`
  - `src/libsystemd/sd-journal/journal-file.c`
  - `src/libsystemd/sd-journal/journal-send.c`
  - `src/shared/logs-show.c`
  - `src/journal/journald-socket.c`
  - `src/libsystemd/sd-journal/test-journal-send.c`
- Installed validation tools: `journalctl --version` reports systemd `260 (260.1-2-manjaro)` and `pkg-config --modversion libsystemd` reports `260`.
- Official systemd docs: `https://systemd.io/JOURNAL_EXPORT_FORMATS/`

Affected contracts and surfaces:

- Go writer public API and docs.
- Go writer conformance tests.
- Stock journalctl compatibility evidence.
- Stock libsystemd compatibility evidence.
- SOW status summary.

Existing patterns to reuse:

- Existing Go `Field` API.
- Existing writer test helpers for journal snapshots, `journalctl --verify`, and `journalctl --output=json`.
- Existing live compatibility harness from SOW-0011.
- Existing no-CGO dependency audit commands.

Risk and blast radius:

- Blast radius should be limited to Go writer tests/docs unless implementation defects are found.
- Byte-for-byte tests reduce risk of silent corruption for plugin payloads.
- C test helper usage is allowed only for validation and must not enter the Go module dependency graph.
- Compression remains explicitly unsupported; this SOW must not mix binary-field support with compressed DATA support.

Sensitive data handling plan:

- Tests use synthetic byte payloads only.
- Generated journal files stay in test temporary directories.
- Durable artifacts must not include raw secrets, credentials, bearer tokens, SNMP communities, personal data, customer data, private endpoints, or proprietary incident details.

Implementation plan:

1. Add focused Go writer binary-field tests for raw DATA object storage, `journalctl --verify`, JSON byte-array output, export binary framing, field matching, and libsystemd byte-for-byte readback.
2. Add or update documentation to make `Append([]Field{...})` the binary-safe API and `AppendMap()` the string-only convenience path.
3. Fix writer serialization only if the tests expose incompatibility.
4. Run the existing Go writer, race, no-CGO, live compatibility, and SOW audit gates.
5. Run external read-only reviewers and iterate until production-grade.

Validation plan:

- `go test -count=1 ./...`
- `go test -race -count=1 ./...`
- `CGO_ENABLED=0 go test -count=1 ./...`
- `go vet ./...`
- `go list -deps -f '{{if .CgoFiles}}{{.ImportPath}} {{.CgoFiles}}{{end}}' ./...`
- Targeted binary-field tests with stock `journalctl`.
- Targeted libsystemd helper build/readback test when `cc` and `pkg-config libsystemd` are available.
- Targeted live concurrency tests for the Go writer.
- `.agents/sow/audit.sh`
- Sensitive data scan over changed durable artifacts.

Artifact impact plan:

- AGENTS.md: no expected update; binary field behavior is product/API scope, not project-wide workflow.
- Runtime project skills: update only if review exposes a recurring compatibility workflow gap.
- Specs: update product scope to record Go writer binary-field support once validated.
- End-user/operator docs: update `go/README.md`.
- End-user/operator skills: none expected; no output/reference skills exist.
- SOW lifecycle: this SOW is current/in-progress and will move to done only with implementation, validation, review, and commit in the same chunk.
- SOW-status.md: update active/next status now and close status when complete.

Open-source reference evidence:

- `systemd/systemd @ cf3156842209`
  - `man/systemd.journal-fields.xml`
  - `src/libsystemd/sd-journal/journal-file.c`
  - `src/libsystemd/sd-journal/journal-send.c`
  - `src/shared/logs-show.c`
  - `src/journal/journald-socket.c`
  - `src/libsystemd/sd-journal/test-journal-send.c`

Open decisions:

- No user decision is currently needed. The user explicitly made binary fields required before moving forward.

## Implications And Decisions

1. Binary field support before later phases
   - Current state: resolved by user request on 2026-05-23.
   - Selection: complete and validate binary field support for the Go writer before moving to Go reader/journalctl completion or other languages.
   - Implication: SOW-0010 remains next only after this SOW closes.
   - Risk: this delays broader SDK work, but avoids building later implementations around an unproven writer contract.

2. Compression separation
   - Current state: resolved by current scope.
   - Selection: keep compression out of this SOW.
   - Implication: binary values must work in uncompressed DATA objects now; compressed DATA support remains a later writer feature.
   - Risk: users needing compression still cannot use it after this SOW.

## Plan

1. Add binary-field compatibility tests for Go writer output.
2. Update Go writer docs and product scope after tests prove behavior.
3. Run local validation and no-CGO checks.
4. Run external read-only review with Minimax plus additional reviewers.
5. Close and commit after all blockers are resolved.

## Delegation Plan

Implementer:

- Local implementation is allowed for this narrow compatibility/test SOW because the user explicitly allowed direct edits when faster. Minimax will be used as a reviewer rather than implementer for this SOW.

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

- Created and activated this SOW for Go writer binary field compatibility.
- Added `TestWriterBinaryFieldCompatibility` to prove raw DATA byte storage, `journalctl --verify`, `journalctl --output=json`, `journalctl --output=export`, binary match lookup for a control-byte value, empty byte-slice value handling, and stock libsystemd `sd_journal_get_data()` byte-for-byte readback.
- Added `tests/conformance/binary/libsystemd_binary_field_reader.c` as a standalone test helper compiled only during tests when `cc`, `pkg-config`, and libsystemd development files are available.
- Updated Go docs and product scope to state that binary values use `Field.Value []byte` through `Append([]Field{...})`, while `AppendMap()` and `StringField()` are string convenience helpers.
- External review round 1 ran with Minimax, Kimi, Mimo, and Qwen. Minimax, Mimo, and Qwen returned production-grade verdicts with only low/nonblocking notes. Kimi ran the full validation set successfully but did not return a final verdict before the reviewer process was stopped by targeted PID after an extended idle period.
- Dispositioned round-1 notes: added empty `[]byte{}` field coverage; hardened the export parser with an explicit remaining-byte check before reading the 64-bit length; treated Minimax's single live-stress failure as intermittent after Minimax re-ran it successfully and local reproduction passed five consecutive runs plus the normal live suite.
- External review round 2 ran with Mimo, Qwen, and GLM after the round-1 fixes. All three returned `PRODUCTION GRADE`.
- Dispositioned round-2 notes: filled the SOW close gates; fixed the C helper's empty-value comparison so it does not call `memcmp()` with a possibly null expected pointer when the expected length is zero.
- Reviewer process note: Minimax round 1 ran `git stash` / `git stash pop` despite the read-only reviewer prompt. The stash was popped and `git stash list` was empty afterward. This was recorded as a reviewer-process violation; it did not change the implementation evidence.

## Validation

Acceptance criteria evidence:

- `TestWriterBinaryFieldCompatibility` writes values containing NUL, control bytes, newline, DEL, non-UTF8 bytes, and an empty byte-slice value.
- Raw DATA object checks compare committed payload bytes with expected `FIELD=value` byte slices.
- Stock `journalctl --file --output=json` returns the non-printable binary fields as JSON byte arrays and returns the empty value as an empty string, matching systemd's text-output behavior for printable empty values.
- Stock `journalctl --file --output=export` returns the non-printable binary fields through binary-safe export framing, and returns the empty value as `FIELD=` with exact empty bytes.
- Stock libsystemd `sd_journal_get_data()` reads the exact expected byte values through the committed C helper.

Tests or equivalent validation:

- `journalctl --version`: systemd `260 (260.1-2-manjaro)`.
- `pkg-config --modversion libsystemd`: `260`.
- `go test -count=1 -run TestWriterBinaryFieldCompatibility -v ./journal`: pass.
- `go test -count=1 ./...`: pass.
- `go vet ./...`: pass.
- `CGO_ENABLED=0 go test -count=1 ./...`: pass.
- `go test -race -count=1 ./...`: pass.
- `go list -deps -f '{{if .CgoFiles}}{{.ImportPath}} {{.CgoFiles}}{{end}}' ./...`: no output.
- `go test -count=1 -run 'TestGoWriterLiveStockReaders|TestGoWriterLiveStockReadersStress|TestGoWriterLiveInterruptionReopenAndVerify|TestGoWriterLiveRejectsSecondWriter' -v ./journal`: pass.
- `python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json`: pass.
- `python3 tests/conformance/runner/manifest_checker.py dry-run tests/conformance/manifests/conformance-v01.json`: pass.
- `git diff --check`: pass.
- `bash .agents/sow/audit.sh`: pass while this SOW was current/in-progress and pass after completion/move to `done/`.
- `SOW_AUDIT_SENSITIVE_CHANGED=1 bash .agents/sow/audit.sh`: pass after completion/move to `done/`.

Real-use evidence:

- A journal written by the Go writer was read by stock `journalctl` and stock libsystemd in `TestWriterBinaryFieldCompatibility`.

Reviewer findings:

- Minimax round 1: `PRODUCTION GRADE`; noted one intermittent `TestGoWriterLiveStockReadersStress` failure, then re-ran successfully; noted low/nonblocking parser robustness and single-entry coverage observations.
- Mimo round 1: `PRODUCTION GRADE`; no blocking findings; noted nonblocking empty-value and large-value coverage observations.
- Qwen round 1: `PRODUCTION GRADE`; no blocking findings; recommended empty-value coverage.
- Kimi round 1: validation commands passed, including binary test, full Go test, vet, no-CGO, race, live suite, manifest validation/dry-run, and SOW audit. Reviewer did not return final verdict before targeted stop after extended idle period.
- Disposition: empty-value coverage added; parser robustness improved; large binary value testing remains broader payload stress coverage and is not required for this SOW's byte-preservation proof.
- Mimo round 2: `PRODUCTION GRADE`; independently verified validation and identified only lifecycle bookkeeping before close.
- Qwen round 2: `PRODUCTION GRADE`; identified only lifecycle bookkeeping before close and explicit staging of the new C helper.
- GLM round 2: `PRODUCTION GRADE`; identified one low-severity C-helper edge case for zero-length `memcmp()` and nonblocking future coverage ideas.
- Disposition: lifecycle gates are filled in this SOW; the new C helper is explicitly included in the commit; the zero-length `memcmp()` ambiguity is fixed; broader large-payload, deduplication, and cross-language binary coverage are mapped to existing follow-on SOWs below.

Same-failure scan:

- `rg -n "binary|BINARY_|Field\{|Value \[\]byte|AppendMap|StringField|output=export|sd_journal_get_data|parseJournalExport|libsystemd_binary|memcmp\(" go tests .agents/sow/specs go/README.md go/journal/doc.go`
- Result: expected binary-field documentation, the new Go binary compatibility test, the new libsystemd helper, existing Go writer byte-slice API, existing live reader helper, and binary encoding helpers only.
- Disposition: no duplicate export parser or binary-field compatibility gap found in the current Go writer scope. The live reader helper's `memcmp()` calls use non-empty static prefixes and are not the same zero-length issue fixed in the new binary helper.

Sensitive data gate:

- `rg -n "[USER_FIRST_NAME]|password|token|secret|bearer|api[_-]?key|SNMP|community|private endpoint|customer|credential" ...changed durable artifacts...`
- Result: only the generic sensitive-data policy sentence in this SOW matched.
- `.agents/sow/audit.sh` sensitive-data guardrail reported no sensitive-data patterns.
- Disposition: clean. Test payloads are synthetic byte arrays and generated journal files stay in test temporary directories.

Artifact maintenance gate:

- AGENTS.md: no update needed; repository-wide workflow and guardrails did not change.
- Runtime project skills: no update needed; existing orchestration and journal compatibility skills already require read-only reviewers, repository boundaries, stock-reader compatibility, and no-CGO audits.
- Specs: `.agents/sow/specs/product-scope.md` updated to record the binary-field priority and current Go writer byte-safe feature slice.
- End-user/operator docs: `go/README.md` and `go/journal/doc.go` updated to document `Field.Value []byte` through `Append([]Field{...})` as the binary-safe path.
- End-user/operator skills: no output/reference skills exist in this project.
- SOW lifecycle: this SOW is marked completed and moved to `.agents/sow/done/` together with the implementation/docs/spec updates.
- SOW-status.md: updated to remove the active SOW, add this SOW to completed SOWs, and keep SOW-0010 as the recommended next SOW.

Specs update:

- Updated `.agents/sow/specs/product-scope.md`.

Project skills update:

- No project skill update needed. The reviewer read-only violation was a model compliance failure against existing explicit instructions, not a missing project-skill rule.

End-user/operator docs update:

- Updated `go/README.md` and `go/journal/doc.go`.

End-user/operator skills update:

- No end-user/operator skills exist or were affected.

Lessons:

- Binary field support can exist in the API and still be unsuitable to claim until stock-reader byte-for-byte tests prove it.
- Stock `journalctl` exposes binary fields differently by output format: JSON uses byte arrays for non-printable values, while export uses binary length framing; both need coverage.
- Empty byte-slice values are valid journal values but may be represented by stock tools as text-empty fields when printable rules allow it.
- Reviewer agents can violate read-only prompts; git status, stash state, and final diffs must be checked before accepting reviewer evidence.

Follow-up mapping:

- Compression remains mapped to `.agents/sow/pending/SOW-0008-20260523-interoperability-and-full-writer-features.md`.
- Large binary payload stress, binary DATA deduplication across entries, and cross-language binary-field interoperability are mapped to `.agents/sow/pending/SOW-0008-20260523-interoperability-and-full-writer-features.md`.
- Benchmark and profiling work for binary payloads is mapped to `.agents/sow/pending/SOW-0009-20260523-benchmark-profile-optimize.md`.
- Go reader and journalctl completion remains mapped to `.agents/sow/pending/SOW-0010-20260523-go-reader-and-journalctl-completion.md`.

## Outcome

Completed. The Go writer's current uncompressed feature slice supports byte-safe binary journal field values through `Field{Name, Value []byte}` and `Append([]Field{...})`.

Validated compatibility:

- Raw DATA object bytes preserve `FIELD=value` exactly.
- Stock `journalctl --verify --file` accepts the generated journal.
- Stock `journalctl --file --output=json` reads non-printable/non-UTF8 values as byte arrays.
- Stock `journalctl --file --output=export` preserves binary values through export binary framing.
- Stock libsystemd `sd_journal_get_data()` reads the same binary field bytes.
- Existing live one-writer/multiple-stock-reader compatibility tests still pass.
- Go production code remains pure Go with no CGO or system journal library linkage.

Compression is still not supported by this Go writer slice and remains tracked in SOW-0008.

## Lessons Extracted

- Compatibility evidence must include stock tools and stock libsystemd, not only internal byte inspection.
- Test-only C helpers are acceptable for validation, but they must stay outside the Go module dependency graph and must be audited as ordinary C code.
- SOW close should include explicit mapping for nonblocking reviewer coverage ideas, otherwise they become ambiguous deferred work.

## Followup

- SOW-0008: compression, full writer feature expansion, large binary payload stress, binary DATA deduplication coverage, and cross-language binary-field interoperability.
- SOW-0009: benchmark and profiling coverage, including binary payload workloads.
- SOW-0010: Go reader and file-backed journalctl completion.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
