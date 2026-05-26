# SOW-0020 - Directory Traversal Parity

## Status

Status: completed

Sub-state: Completed on 2026-05-26 after local implementation, full validation, and read-only external review. Implementation was local; external models were reviewers only.

## Requirements

### Purpose

Bring SDK directory readers and file-backed journalctl `--directory` behavior to parity with stock journalctl for supported journal directory layouts.

### User Request

The user requires journalctl rewrites and SDK readers to interoperate across files produced by all writers. SOW-0008 proved live file compatibility, but full directory traversal parity remains separate.

### Assistant Understanding

Facts:

- Current directory readers handle active/archive files and simple machine-id subdirectories.
- The live matrix discovers a generated file and validates file-backed readers; it does not prove complete `--directory` traversal parity.
- Product scope records sequential directory iteration limitations for overlapping multi-file directories.

Inferences:

- Full parity needs fixtures with multiple machine IDs, namespaces where applicable, active and archived files, whole-file `.zst` files, overlapping realtime ranges, boot IDs, and corrupted files.
- Ordering semantics should be compared against stock journalctl, not invented independently.

Unknowns:

- Exact subset of namespace and subdirectory behavior expected for file-backed SDK use versus daemon-only behavior.

### Acceptance Criteria

- A directory traversal fixture suite covers active files, archived files, machine-id subdirectories, namespace-like layouts where file-backed stock journalctl supports them, whole-file `.zst` journal files, overlapping realtime ranges, multiple boot IDs, and corrupted/unreadable files.
- Rust, Go, Node.js, and Python directory readers return the same supported entries and ordering as stock `journalctl --directory` for accepted fixtures.
- File-backed journalctl rewrites in all four languages match stock filtering, boot listing, field listing, JSON/export/text output, repeated-match OR, `+` disjunction, and error behavior for directory inputs.
- Unsupported daemon-only or environment-dependent behavior is documented and tested as controlled unsupported behavior.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `.agents/sow/current/SOW-0008-20260523-interoperability-and-full-writer-features.md`
- `.agents/sow/specs/product-scope.md`
- `tests/interoperability/run_live_matrix.py`
- `tests/interoperability/README.md`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `man/journalctl.xml`

Current state:

- Directory iteration exists but is validated primarily for non-overlapping active/archive files.
- The live matrix validates file-backed reader compatibility by discovering specific generated files, not by requiring complete directory traversal parity.

Risks:

- Incorrect ordering can silently change query results for overlapping files.
- Recursing too broadly can read unrelated journals or artifacts.
- Error parity is subtle for corrupted or unreadable files.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The project has strong file-level compatibility, but directory traversal semantics remain weaker than stock journalctl behavior. This can affect user-visible journalctl rewrites and SDK directory readers when users point them at real journal directories.

Evidence reviewed:

- SOW-0008 explicitly records directory traversal parity as a remaining gap.
- Product scope records sequential directory iteration and overlapping multi-file ordering limitations.
- Interoperability README states live matrix does not prove directory traversal parity.

Affected contracts and surfaces:

- DirectoryReader APIs in all languages.
- File-backed journalctl `--directory` behavior in all languages.
- Boot listing, field listing, matching, ordering, and output formatting.
- Interoperability fixtures and docs.

Existing patterns to reuse:

- `tests/interoperability/run_matrix.py` query checks.
- `tests/interoperability/run_live_matrix.py` generated directory layouts.
- Stock journalctl comparison harness.
- Shared conformance manifest patterns.

Risk and blast radius:

- Medium. Directory traversal touches reader selection/order logic but should not affect writer object format.

Sensitive data handling plan:

- Use synthetic generated directory fixtures under `.local/` or committed sanitized fixtures only. Do not inspect or commit real system journal content.

Implementation plan:

1. Inventory stock journalctl `--directory` behavior for supported layouts.
2. Build synthetic directory fixtures and expected stock outputs.
3. Update readers and journalctl rewrites to match supported stock behavior.
4. Add shared directory parity matrix.
5. Update docs/specs with supported and unsupported directory behavior.

Validation plan:

- Directory parity matrix comparing stock and repository outputs.
- Existing file-level, live, binary, compression, and lock matrices remain passing.
- Tests cover ordering, filters, boot listing, fields, JSON/export/text, and controlled corrupt-file behavior.
- External reviewers check unwanted recursion, deletion, or artifact side effects.

Artifact impact plan:

- AGENTS.md: no update expected.
- Runtime project skills: update if directory matrix becomes mandatory for reader changes.
- Specs: update product scope with exact directory behavior.
- End-user/operator docs: update README and journalctl help/behavior docs.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending until activated; may split fixture inventory from implementation if needed.
- SOW-status.md: update when activated or closed.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
- `man/journalctl.xml`
- `src/libsystemd/sd-journal/sd-journal.c`

Open decisions:

- None blocking activation. If namespace directory behavior depends on daemon state rather than file-backed stock behavior, record evidence and exclude daemon-only behavior from this SOW.

## Implications And Decisions

1. Directory parity boundary
   - Decision: match stock file-backed `journalctl --directory` behavior, not daemon-only discovery or live daemon state.
   - Reason: project scope is file-backed SDK/journalctl behavior.
   - Risk: unsupported daemon-dependent behavior must be explicit to avoid false parity claims.

2. Review batching
   - Decision: external reviewers review coherent implementation batches, not tiny incremental patches.
   - Batch for this SOW: directory traversal and interleaving changes across Rust, Go, Node.js, and Python, plus tests, docs/spec updates, and validation evidence.
   - Reason: reviewers need enough context to identify cross-language drift, unwanted side effects, and API/behavior inconsistencies.
   - Risk: local defects may be discovered later in the batch; mitigate with frequent local test runs before the review gate.

## Plan

1. Inventory stock directory traversal behavior.
2. Build directory fixtures and expected outputs.
3. Implement reader and journalctl parity fixes.
4. Add shared matrix and docs.
5. Review and commit verified chunks.

## Delegation Plan

Implementer:

- Current routing is local implementation by the project manager. Do not run external implementer agents unless the user explicitly changes this decision.

Reviewers:

- At least two reviewers from the approved pool.

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

- Record implementer failure, reviewer failure, audit failure, fixture uncertainty, or model unavailability before changing plan or model.

## Execution Log

### 2026-05-26

- Activated after SOW-0027 completed, committed, and pushed.
- Recorded current implementation routing: local implementation only; external models are read-only reviewers.
- Verified the baseline directory traversal and ordering rules against `systemd/systemd @ c3cd6e5bdb07` (`v260.1`):
  - `src/libsystemd/sd-journal/sd-journal.c:1867` accepts root/subdirectory journal files ending `.journal` or `.journal~` and accepts regular files, symlinks, and unknown dentry types.
  - `src/libsystemd/sd-journal/sd-journal.c:1879` accepts only immediate root subdirectories whose names are a 128-bit ID or a 128-bit ID plus namespace suffix.
  - `src/libsystemd/sd-journal/sd-journal.c:2017` enumerates journal files in each accepted directory and recurses only one level from the root directory into accepted subdirectories.
  - `src/basic/syslog-util.c:109` validates namespace suffixes using the same filename/unit-instance/safe-string/glob checks used by journald namespace names.
  - `src/libsystemd/sd-journal/sd-journal.c:1076` compares candidate entries by identical-entry tuple, same-seqnum-source seqnum, same-boot monotonic time, comparable boot ordering, realtime, then entry xor hash.
  - `src/libsystemd/sd-journal/sd-journal.c:1124` scans all open files on each next/previous call and chooses the earliest/latest candidate instead of exhausting one file before moving to the next.
- Local reader gap confirmed:
  - Rust and Go directory readers only scan top-level files.
  - Node.js and Python scan every immediate subdirectory without validating machine-id or namespace-like directory names.
  - All four directory readers currently exhaust one file before the next, so overlapping realtime ranges are not stock-order compatible.
- Implemented local directory parity batch:
  - Rust, Go, Node.js, and Python directory readers now merge candidates across all files using the systemd entry ordering model instead of exhausting one file at a time.
  - `OpenDirectory`/`SdJournalOpenDirectory` now scans root journal files and one immediate 128-bit machine-id subdirectory level, follows symlinks to regular files, skips namespace-suffix subdirectories by default, and avoids deeper recursion.
  - Rust, Go, Node.js, and Python file-backed journalctl `--directory` and `--verify --directory` use the same traversal rules.
  - Empty directories now open/read successfully with zero entries.
  - Directory read and verify paths skip files they cannot open as journals, while explicit `--verify --file` still fails for a corrupt named file.
  - Rust `FileReader` now has a read-only raw-path open path so stock-accepted filenames such as arbitrary `.journal~` files are not rejected by Netdata chain filename parsing.
  - Python directory filtering now uses filtered file-reader steps so directory-level matches apply correctly while preserving interleaved ordering.
- Added `tests/interoperability/run_directory_matrix.py`.
  - Stock-parity fixture covers root `.journal`, root `.journal~`, one machine-id subdirectory level including dashed UUID form, invalid/nested/namespace-suffix skip behavior, overlapping realtime ranges, match semantics, JSON/export/text, fields, boot listing, and directory verify.
  - Separate corrupt/unreadable fixture proves stock and repository readers skip files they cannot open.
  - Separate repository-extension fixture proves `.journal.zst` directory discovery in all repository readers.
- Updated durable artifacts:
  - `.agents/sow/specs/product-scope.md` now records current directory traversal, ordering, empty-directory, corrupt-skip, and `.journal.zst` extension behavior.
  - `tests/interoperability/README.md` documents `run_directory_matrix.py` and marks directory traversal complete.
  - `go/README.md`, `rust/README.md`, `node/README.md`, and `python/README.md` no longer describe directory iteration as sequential/non-overlapping only.
  - `.agents/skills/project-journal-compatibility/SKILL.md` now requires the directory matrix for future directory reader or `journalctl --directory` changes.
- Addressed reviewer-discovered quality items before closure:
  - Node.js `DirectoryReader` now starts with `index = -1`, matching the other implementations' no-current-entry state.
  - Node.js `FilterBuilder` import moved with the other top-level imports.
  - Rust `.journal.zst` open now removes the decompressed temporary file if the decompressed journal fails to open.
  - Rust facade multi-file test now explicitly asserts the first unfiltered entry before adding a match, documenting and protecting the stale-candidate reset case.

## Validation

Local validation completed after the final reviewer fixes:

- `PYTHON=/home/costa/Documents/systemd-journal-sdk/.local/python-venv/bin/python python3 tests/interoperability/run_directory_matrix.py --keep-files` - PASS.
  - Stock version recorded by the runner: `systemd 260 (260.1-2-manjaro)`.
  - Stock journalctl and Rust, Go, Node.js, Python rewrites passed JSON, export, text, fields, boot listing, match OR/AND, `+` disjunction, corrupt-skip, verify-skip, `.journal.zst` extension, and empty-directory checks.
- `GOMODCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go/pkg/mod GOCACHE=/home/costa/Documents/systemd-journal-sdk/.local/go-build GOPATH=/home/costa/Documents/systemd-journal-sdk/.local/go go test ./journal ./cmd/journalctl` in `go/` - PASS.
- `CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target cargo test -p journal -p journalctl` in `rust/` - PASS.
- `node --check cmd/journalctl/index.js && node --check src/lib/directory-reader.js && node --check src/lib/reader.js` in `node/` - PASS.
- `npm test -- --runInBand` in `node/` - PASS.
- `python3 -m py_compile python/cmd/journalctl.py python/journal/directory_reader.py python/journal/reader.py tests/interoperability/run_directory_matrix.py` - PASS.
- `/home/costa/Documents/systemd-journal-sdk/.local/python-venv/bin/python python/test_all.py` - PASS.
- `CARGO_HOME=/home/costa/Documents/systemd-journal-sdk/.local/cargo-home CARGO_TARGET_DIR=/home/costa/Documents/systemd-journal-sdk/.local/cargo-target cargo test --workspace` in `rust/` - PASS.
- Syntax/format checks:
  - `gofmt -w cmd/journalctl/main.go cmd/journalctl/main_test.go journal/reader.go journal/facade_test.go` - PASS.
  - `cargo fmt --all` in `rust/` - PASS.
  - Node.js, Python, and Go formatting/syntax checks listed above - PASS.

Reviewer gate:

- `llm-netdata-cloud/minimax-m2.7-coder` first pass - PRODUCTION GRADE; no blocking findings.
- `llm-netdata-cloud/kimi-k2.6` first/follow-up passes - PRODUCTION GRADE. Findings and disposition:
  - Rust match mutation needed to reset merge state after cached multi-file candidates existed. Fixed by resetting merge state after `add_match`, `add_conjunction`, `add_disjunction`, and `flush_matches`; Rust facade regression coverage added.
  - Go realtime seek needed the same defensive bound used by other languages. Fixed and covered by the directory matrix.
  - Node.js initial `index` state, import placement, and Rust `.journal.zst` temp cleanup were small quality items. Fixed before closure.
  - Cross-language filter architecture differs internally but the matrix proves equivalent behavior; no follow-up needed.
- `llm-netdata-cloud/qwen3.6-plus` first/follow-up passes - PRODUCTION GRADE. Findings and disposition:
  - Python and Node.js child subdirectory scans needed to tolerate races/unreadable child directories. Fixed with guarded child `readdir`/`scandir` handling and rerun validation.
  - Go single-dash multi-character flag behavior is broader journalctl option-parity cleanup, not directory traversal behavior; mapped to SOW-0022.
  - Rust `SdJournalOpen` filesystem-based dispatch differs from the filename-based dispatch in other languages only for unusual edge paths such as a directory named `*.journal`; explicit `SdJournalOpenFile`/`SdJournalOpenDirectory` remain available and directory matrix behavior is unaffected. No change in this SOW.
- `llm-netdata-cloud/glm-5.1` follow-up pass - PRODUCTION GRADE. Findings and disposition:
  - Traversal helper duplication between SDK and journalctl verify paths is accepted for this SOW because each language's matrix covers both paths. No follow-up needed unless future drift appears.
  - Python private helper import avoids duplication inside one package; accepted as internal implementation detail.
  - Node.js `verifyFileWithKey` parameter bug was fixed while implementing directory verify behavior.

Same-failure search:

- Searched all four implementations for match mutation state reset, realtime seek bounds, child directory enumeration, `.journal.zst` discovery, and verify directory traversal. The final directory matrix exercises each reader and each journalctl rewrite after these fixes.

Sensitive data gate:

- Only synthetic fixtures under `.local/` and committed source/test code were used. No real journal data, credentials, customer data, SNMP communities, private endpoints, or other sensitive values were written to durable artifacts.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; repository-wide process and role rules did not change.
- Runtime project skills: `.agents/skills/project-journal-compatibility/SKILL.md` updated so future directory reader or file-backed `journalctl --directory` changes must run the directory matrix.
- Specs: `.agents/sow/specs/product-scope.md` updated with current directory traversal, ordering, empty-directory, corrupt-skip, and `.journal.zst` extension behavior.
- End-user/operator docs: language READMEs and `tests/interoperability/README.md` updated.
- End-user/operator skills: none exist for this project and none were affected.
- SOW lifecycle: SOW-0020 moved from pending to current for this work and will move to done with `Status: completed` after audit.
- `SOW-status.md`: updated when activated and will be updated again at close.

Follow-up mapping:

- SOW-0024 remains responsible for mixed-format directories combining compact/regular, compression variants, and sealed/unsealed files.
- SOW-0022 remains responsible for broader object-graph verification parity and file-backed journalctl option parity, including the single-dash multi-character flag edge noted by review.
- SOW-0009 remains responsible for benchmark/profile/optimization after compatibility feature work.

## Outcome

Completed. Directory traversal and file-backed `journalctl --directory` behavior now match stock file-backed behavior for the supported fixtures across Rust, Go, Node.js, and Python. The shared directory matrix passes against stock `journalctl` from systemd 260.1 and all repository rewrites.

## Lessons Extracted

- Stock `journalctl --directory` does not pass namespace flags for default file-backed directory opens, so default namespace-suffix subdirectory traversal must be skipped even though lower-level systemd helper code can validate namespace directory names.
- Stock `journalctl --list-boots --directory` depends on `_BOOT_ID` field data, not only header boot IDs, so parity fixtures must include `_BOOT_ID` as an entry field when boot listing is part of the assertion.
- Do not partially fix journalctl option parsing in one language when the issue is broader CLI parity. Track it under SOW-0022 so the stock behavior can be specified and tested consistently.

## Followup

- SOW-0024 remains responsible for mixed-format directory combinations: compact/regular, compression variants, and sealed/unsealed files in one directory.
- SOW-0022 remains responsible for broader object-graph verification parity and remaining file-backed journalctl option parity.
- SOW-0009 remains responsible for benchmark/profile/optimization after compatibility feature work.

## Regression Log

None yet.
