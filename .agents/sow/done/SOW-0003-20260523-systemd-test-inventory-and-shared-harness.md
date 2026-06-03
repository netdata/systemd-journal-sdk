# SOW-0003 - systemd Test Inventory And Shared Harness

## Status

Status: completed

Sub-state: completed after external review round 5 and final validation.

## Requirements

### Purpose

Create a shared conformance harness from applicable systemd journal tests and fixtures.

### User Request

Find all related systemd journal read/write tests and port the applicable file-backed/API behavior into this repo.

### Assistant Understanding

Facts:

- The shared conformance suite must be based on applicable systemd journal tests and fixtures.
- The suite must be language-neutral and reusable across every SDK.

Inferences:

- The harness runner format must be selected before implementation agents start work.

Unknowns:

- No activation-blocking unknowns remain.

### Acceptance Criteria

- Applicable systemd tests are inventoried with include/exclude reason.
- Fixtures from systemd baseline `v260.1` are copied or generated inside this repo with provenance.
- Shared tests are language-neutral and can target every SDK implementation.
- Shared fixture/test schema decisions refine any preliminary directories created by SOW-0002.
- Excluded daemon/service tests have explicit reasons and any extractable file-level behavior is tracked.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/test-journal*.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/test/test-journal-importer.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/journal-data/`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/test-journals/`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/units/TEST-04-JOURNAL*.sh`

Current state:

- SOW-0002 created the initial repo structure and Rust source import.
- Harness runner decision is resolved as Option A.

Risks:

- A weak harness could let language implementations drift while passing local tests.
- Daemon-only behavior can accidentally expand the project scope.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Compatibility needs a single test source of truth before multiple language implementations diverge.

Evidence reviewed:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/test-journal*.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/test/test-journal-importer.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/journal-data/`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/test-journals/`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/units/TEST-04-JOURNAL*.sh`

Affected contracts and surfaces:

- Shared fixtures.
- Shared test schemas.
- Language test adapters.
- journalctl file-backed behavior.

Existing patterns to reuse:

- Decision B: SDK conformance plus file-backed journalctl behavior.
- Product scope spec matching semantics.

Risk and blast radius:

- Porting daemon-only behavior would expand scope incorrectly.
- Weak shared harness would let languages pass incompatible tests.

Sensitive data handling plan:

- systemd fixtures are public upstream artifacts.
- Store provenance as upstream repository plus commit and relative path.

Implementation plan:

1. Inventory tests and fixtures.
2. Classify include/exclude with evidence.
3. Copy allowed fixtures into repo.
4. Design shared test data schema.
5. Create runner contract for each language implementation.

Validation plan:

- Harness can run against a stub or imported Rust implementation and report structured pass/fail.
- Inventory covers every journal-related systemd test file discovered.

Sensitive data gate:

- Durable artifacts must contain only public upstream systemd source/fixture evidence and repository-local schema/test metadata.
- Scan changed durable artifacts for raw sensitive data and workstation-specific paths before review and close.

Artifact impact plan:

- Specs: update test-scope details.
- Runtime project skills: update if harness workflow becomes durable.
- End-user/operator docs: not expected in this phase.
- SOW lifecycle: move to current before implementation.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

1. Shared harness runner format must be selected before implementation.
   - Option A: Language-neutral fixture and test manifests, likely JSON or YAML, with one adapter executable per language returning structured results.
     - Pros: keeps the conformance suite independent from any one SDK language.
     - Cons: requires a small runner contract before implementation starts.
     - Implication: every language can be tested the same way, including journalctl CLI behavior.
     - Risk: the manifest schema must be versioned carefully as journal features expand.
   - Option B: Python-driven harness that invokes each language adapter and owns most assertions.
     - Pros: fast to build and convenient for fixture orchestration.
     - Cons: Python becomes the privileged test language before its SDK exists.
     - Implication: non-Python implementations may be coupled to Python test assumptions.
     - Risk: cross-language failures can be harder to attribute to harness vs SDK behavior.
   - Option C: Rust-driven harness based on the imported Rust implementation.
     - Pros: can reuse imported Rust code early.
     - Cons: risks making Rust behavior the test oracle instead of systemd fixtures and documented rules.
     - Implication: ports may clone Rust bugs instead of systemd-compatible behavior.
     - Risk: undermines the goal of a language-neutral conformance suite.
   - Recommendation: Option A, with systemd fixtures and explicit expected outcomes as the oracle.
   - Selection: Option A.

## Implications And Decisions

1. Shared harness runner format
   - Current state: resolved.
   - Selection: Option A, language-neutral fixture and test manifests with one adapter executable per language returning structured results.
   - Rationale: systemd fixtures and explicit expected outcomes remain the oracle, instead of Rust, Python, or another implementation becoming privileged.
   - Implication: this decision becomes the contract every SDK adapter must satisfy.
   - Risk: a weak or language-biased harness can let SDKs drift while still passing their local tests.

## Plan

1. Resolve and record the shared harness runner format decision before implementation.
2. Activate this SOW by moving it to `current/` and setting `Status: in-progress`.
3. Delegate implementation to the selected implementer using the repository-boundary block.
4. Review the inventory, harness schema, fixture provenance, and audit results before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

- 2026-05-23: Activated SOW-0003 after SOW-0002 completed and committed.
- 2026-05-23: Recorded shared harness runner format decision as Option A: language-neutral test manifests plus per-language adapter executables returning structured results.
- 2026-05-23: Implemented SOW-0003:
  - Inventoried all systemd v260.1 (`c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`) journal-related test files.
  - Created `fixtures/systemd/INVENTORY.md` with full upstream test inventory, include/exclude/defer decisions, and extractable behavior.
  - Copied initial fixture inputs from `test/test-journals/no-rtc/`, `test/test-journals/corrupted/`, and `test/journal-data/`; later repair expanded this to the full committed fixture set listed below.
  - Created `tests/conformance/manifest-schema.json` (JSON Schema draft-07) for language-neutral test manifests.
  - Created `tests/conformance/ADAPTER_CONTRACT.md` documenting per-language adapter executable contract (stdin/stdout JSON, exit codes, result formats).
  - Created `tests/conformance/manifests/conformance-v01.json` with 15 test cases covering file-format, entry-parse, matching, stream, enumeration, cursor-navigation, verification, sealing, compression, corruption-resilience, journalctl-cli, import-export.
  - Created `tests/conformance/runner/manifest_checker.py` - Python stdlib + jsonschema manifest validator with validate/list/dry-run/stub subcommands.
  - Ran validation: manifest schema passes, dry-run simulates 15 test cases, audit.sh clean.

## Validation (superseded by later repair validation)

### SOW Audit

```
bash .agents/sow/audit.sh  ->  === SOW initialization complete and clean. ===
```

### Repair Manifest Schema Validation

```
python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json
-> OK: tests/conformance/manifests/conformance-v01.json is valid
```

### Repair Fixture File Validation

```
python3 tests/conformance/runner/manifest_checker.py validate-files tests/conformance/manifests/conformance-v01.json
-> OK: All type:file fixtures exist (0 missing)
```

### Repair Dry-Run Simulation

```
python3 tests/conformance/runner/manifest_checker.py dry-run tests/conformance/manifests/conformance-v01.json
-> 15 test cases simulated; stub adapter returns PASS for all
-> All type:file fixtures resolved as OK (no [MISSING])
```

### Fixture Availability

- `fixtures/systemd/test-data/no-rtc/` - OK (7 committed compressed journal fixtures)
- `fixtures/systemd/test-data/corrupted/` - OK (3 committed compressed corrupted journal fixtures)
- `fixtures/systemd/test-data/journal-1.txt` - OK (copied from systemd v260.1 test/journal-data/)
- `fixtures/systemd/test-data/journal-2.txt` - OK (copied from systemd v260.1 test/journal-data/)

### Repair Sensitive Data Pattern Scan

```
Checked durable artifact files: no sensitive-data patterns found.
```

### Git Status

```
D .agents/sow/pending/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md
 M SOW-status.md
?? .agents/sow/current/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md
?? fixtures/systemd/
?? tests/conformance/
```

### Non-ASCII Punctuation Scan

```
LC_ALL=C rg -n "[^\x00-\x7F]" .agents/sow/current/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md fixtures/systemd tests/conformance
-> (no output - all clean)
```

### Sensitive Data Scan

```
Changed durable artifact scan for personal-name, workstation-path, and SOW marker patterns.
-> (no output - all clean)
```

## Execution Log (continued)

- 2026-05-23: Repair run: resolved fixture paths from repo root, added `validate-files` command.
- 2026-05-23: Repair run: copied `journal-1.txt` (586 bytes) and `journal-2.txt` (513 bytes) into `fixtures/systemd/test-data/`.
- 2026-05-23: Repair run: removed duplicate `## Outcome` section; updated validation results.
- 2026-05-23: Repair run: corrected manifest schema top-level required fields, rewrote manifest_checker.py for full manifest validation with stdlib fallback, removed non-ASCII punctuation from durable artifacts.
- 2026-05-23: Implementer fallback applied for SOW-0003 after the preferred implementer left unresolved fixture-path, audit, schema, and inventory issues across two repair passes. Continued repairs use `llm-netdata-cloud/qwen3.6-plus` per the Delegation Plan.
- 2026-05-23: External review round 1 completed with four `NOT PRODUCTION GRADE` verdicts from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, and `llm-netdata-cloud/glm-5.1`.

## External Review Round 1 Findings

Status: repair required before close.

Blocking findings:

- `test-journal-interleaving.c` is missing from the systemd inventory. Reviewers found it is a large file-backed journal test covering multi-file directory reading, skip/seek, cursor validation, boot ID handling, sequence numbers, match filtering, and realtime/monotonic seek behavior. The inventory acceptance criterion is not met until this file is classified and its extractable behavior is tracked.
- The `no-rtc/` fixture strategy is not runnable. The manifest references a generated `fixtures/systemd/test-data/no-rtc` directory, but only one of the seven compressed upstream source files is present. Manifest tests depending on that directory must either become runnable from committed fixture inputs or move to a deferred list.
- The corrupted fixture inventory is inconsistent. The upstream `test/test-journals/corrupted/` directory has three small `.zst` files, but only `zstd-truncated-frame.zst` is present in the repo while the inventory describes the directory as included.
- The shell test inventory is incomplete. Only three of sixteen `TEST-04-JOURNAL*.sh` files are listed individually, while reviewers found several unlisted files with extractable file-backed `journalctl --file` or `--directory` behavior.

Required repair disposition:

- Update `fixtures/systemd/INVENTORY.md` so every discovered journal-related test file is either included, deferred, or excluded with concrete reason.
- Update the manifest and fixture layout so every active `conformance-v01` test has validateable committed inputs or an executable repo-local preparation path. Do not hide missing fixtures behind `type: generated`.
- Add or defer conformance cases for `test-journal-interleaving.c` behavior explicitly.
- Address reviewer non-blocking findings where low-risk, including meaningful corruption error expectations, explicit handling for ancillary `sd-journal` tests (`test-catalog.c`, `test-mmap-cache.c`, `test-audit-type.c`), and an Artifact Maintenance Gate before close.

## External Review Round 1 Repair Execution Log

- 2026-05-23: Fallback repair pass using qwen3.6-plus after preferred implementer failed two passes and four reviewers rejected as NOT PRODUCTION GRADE.
- 2026-05-23: Copied all 7 no-rtc .zst fixtures from upstream `test/test-journals/no-rtc/` into `fixtures/systemd/test-data/no-rtc/` (total ~1.66 MiB compressed).
- 2026-05-23: Copied all 3 corrupted .zst fixtures from upstream `test/test-journals/corrupted/` into `fixtures/systemd/test-data/corrupted/` (total ~4.6 KiB).
- 2026-05-23: Removed old root-level `system.journal.zst` and `zstd-truncated-frame.zst` from `fixtures/systemd/test-data/` (now organized in subdirectories).
- 2026-05-23: Rewrote `fixtures/systemd/INVENTORY.md`:
  - Added `test-journal-interleaving.c` (1341 lines) as **include** with extractable behaviors: directory open, forward/backward iteration, skip/seek, cursor validation, boot ID handling, sequence numbers, match filtering, realtime/monotonic seek.
  - Added ancillary sd-journal tests (`test-catalog.c`, `test-mmap-cache.c`, `test-audit-type.c`) as **exclude** with evidence they are not journal file read/write SDK behavior.
  - Expanded TEST-04-JOURNAL shell inventory from 3 to all 16 files with individual include/exclude/defer dispositions.
  - Updated Included Fixtures table with all 12 committed fixtures (7 no-rtc, 3 corrupted, 2 importer text).
  - Updated Excluded table with all 16 excluded items and concrete reasons.
- 2026-05-23: Rewrote `tests/conformance/manifests/conformance-v01.json`:
  - Changed all no-rtc tests from `type:generated` to `type:file` pointing to committed `fixtures/systemd/test-data/no-rtc/` directory.
  - Fixed `journal-file-parse-uid-from-filename`: removed irrelevant fixture reference (tests filename string parsing only, no file content needed).
  - Fixed `journal-verify-corruption-detection`: changed fixture to `corrupted/zstd-truncated-frame.zst`, added meaningful `error_contains: "truncated"`.
  - Updated the compressed-read and file-header parse cases to reference `no-rtc/system.journal.zst`.
  - Updated `journal-corruption-append-resilient` to reference all 3 corrupted fixtures.
  - Added `source_path: "daemon-required"` to `journal-verify-sealed` FSS fixture (genuinely requires live daemon).
- 2026-05-23: Updated `tests/conformance/manifest-schema.json`:
  - Added `source_path` property to fixture_ref for generated fixtures.
  - Added `allOf` conditional requiring `source_path` when `type: "generated"`.
  - Added `minLength: 1` to `error_contains` to reject empty strings.
- 2026-05-23: Updated `tests/conformance/runner/manifest_checker.py`:
  - Added validation that `type:generated` fixtures have `source_path` (or `"daemon-required"` marker).
  - Added validation that `error_contains` is non-empty for `result_format: error`.
  - Added directory detection for `type:file` fixtures (reports "dir" vs "file" in dry-run).
  - Updated `validate_manifest` to run both jsonschema and stdlib checks.

## Validation Results

### py_compile

```
python3 -m py_compile tests/conformance/runner/manifest_checker.py
-> PASS
```

### Manifest Schema Validation

```
python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json
-> OK: tests/conformance/manifests/conformance-v01.json is valid
```

### Fixture File Validation

```
python3 tests/conformance/runner/manifest_checker.py validate-files tests/conformance/manifests/conformance-v01.json
-> OK: All type:file fixtures exist (0 missing)
```

### Final Dry-Run Simulation

```
python3 tests/conformance/runner/manifest_checker.py dry-run tests/conformance/manifests/conformance-v01.json
-> 15 test cases simulated; all fixtures resolved as OK or GENERATED
-> no-rtc directory fixtures detected as "exists (dir)"
-> corrupted fixtures detected as "exists (file)"
```

### Repair SOW Audit

```
bash .agents/sow/audit.sh
-> === SOW initialization complete and clean. ===
```

### Git Whitespace Check

```
git diff --check
-> (no output - clean)
```

### Python Bytecode Cleanup

```
find tests/conformance fixtures/systemd -name '__pycache__' -o -name '*.pyc' -o -name '*.pyo'
-> Cleaned tests/conformance/runner/__pycache__
```

### Repair Non-ASCII Punctuation Scan

```
LC_ALL=C rg -n "[^\x00-\x7F]" .agents/sow/current/SOW-0003-20260523-systemd-test-inventory-and-shared-harness.md fixtures/systemd tests/conformance
-> (no output - all clean)
```

### Repair Sensitive Data Scan

```
Changed durable artifact scan for personal-name, workstation-path, and SOW marker patterns.
-> (no output - all clean)
```

## Artifact Maintenance Gate

| Artifact Class | Updated | Reason |
|----------------|---------|--------|
| **AGENTS.md** | Yes | Updated the SOW ordering rule so Go writer-first work activates immediately after SOW-0003. |
| **Runtime project skills** | Yes | Updated the reviewer pool rule so Minimax is available as reviewer when implementation is local or done by another model. |
| **Specs** | No | `.agents/sow/specs/` contains preliminary spec directories from SOW-0002. This SOW adds fixtures and harness infrastructure, not product behavior specs. Specs will be updated when SDK implementations (SOW-0004+) define concrete API contracts. |
| **End-user/operator docs** | No | This SOW produces test infrastructure, not user-facing documentation. |
| **End-user/operator skills** | No | No output/reference skills created or consumed outside normal repo work. |
| **SOW lifecycle** | Yes | This SOW is completed and moved to `done/` during closeout. |
| **SOW-status.md** | Yes | SOW-0003 completion and SOW-0005 next activation are recorded during closeout. |

## Lessons Extracted

- systemd v260.1 test files use ASSERT_* macros that couple tests to systemd's `tests.h` infrastructure; only file-level behavior can be extracted for SDK use.
- Daemon-coupled tests (`systemctl`, `journalctl --sync`, `journalctl --rotate`, live cursors, varlink sockets) cannot run without a live systemd-journald; these must be excluded by category.
- The importer test data files (`journal-1.txt` at 586 bytes, `journal-2.txt` at 513 bytes) are small text-export fixtures and are copied into `fixtures/systemd/test-data/`.
- FSS (Forward Secure Sealing) fixtures require a live daemon for key generation; defer as `generated` type with `journalctl --force --setup-keys` as the regeneration command.
- The `corrupted/` directory AFL corpus is a tarball that overlaps with individually stored `.zst` files; no need to copy the tarball when individual files suffice.

## External Review Round 2 Findings

Status: repair required before close.

Review result:

- `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE.
- `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE.
- `llm-netdata-cloud/kimi-k2.6`: NOT PRODUCTION GRADE.
- `llm-netdata-cloud/qwen3.6-plus`: NOT PRODUCTION GRADE.

Blocking findings:

- `journal-file-header-parse` expected the wrong journal header signature and a non-existent version field. Correct systemd journal header signature is the 8-byte value `LPKSHHRH`, and the header has no version field.
- The previous compression case name and description conflated compressed fixture reading with `SYSTEMD_JOURNAL_COMPRESS` writer algorithm selection and `SYSTEMD_JOURNAL_COMPACT` offset layout behavior.
- The directory iteration note included stale synthetic fixture wording from earlier manifest drafts.
- `ADAPTER_CONTRACT.md` contradicted itself by documenting `run <test-manifest.json>` while the harness sends a single test-case JSON object on stdin.
- `error_contains` was only checked for non-empty when present; `result_format: "error"` did not require it.

Repair disposition:

- Corrected `journal-file-header-parse` to assert signature `LPKSHHRH`, require `signature`, `state`, `compatible_flags`, `incompatible_flags`, and `header_size`, and document that there is no version field.
- Renamed the compression case to `journal-zstd-compressed-read` and documented that it tests `.zst` fixture decompression/read behavior only.
- Clarified `journal-stream-directory-iteration` as a fixture-backed directory iteration case; synthetic skip/seek cases remain inventoried for later generated fixtures.
- Updated `ADAPTER_CONTRACT.md` so `adapter run` takes stdin JSON only, fixture paths are repo-root relative, `.zst` fixtures must be decompressed or stream-decompressed without systemd libraries, and expected outcome semantics are explicit.
- Updated `manifest-schema.json` and `manifest_checker.py` so `result_format: "error"` requires non-empty `error_contains`.

Validation after round 2 repair:

```
python3 -m py_compile tests/conformance/runner/manifest_checker.py
-> PASS

python3 tests/conformance/runner/manifest_checker.py validate tests/conformance/manifests/conformance-v01.json
-> OK: tests/conformance/manifests/conformance-v01.json is valid

python3 tests/conformance/runner/manifest_checker.py validate-files tests/conformance/manifests/conformance-v01.json
-> OK: All type:file fixtures exist (0 missing)

python3 tests/conformance/runner/manifest_checker.py dry-run tests/conformance/manifests/conformance-v01.json
-> 15 test cases simulated; all type:file fixtures resolved as OK

bash .agents/sow/audit.sh
-> === SOW initialization complete and clean. ===

git diff --check
-> PASS

find tests/conformance fixtures/systemd -name '__pycache__' -o -name '*.pyc' -o -name '*.pyo'
-> PASS after removing tests/conformance/runner/__pycache__

ASCII and sensitive-data scans over changed durable artifacts
-> PASS
```

## External Review Round 3 Findings

Status: repair required before close.

Review result:

- `llm-netdata-cloud/minimax-m2.7-coder`: PRODUCTION GRADE.
- `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE, but review details contained wrong counts and were treated as weak supporting evidence only.
- `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE with low-risk non-blocking findings.
- `llm-netdata-cloud/mimo-v2.5-pro`: NOT PRODUCTION GRADE.
- `llm-netdata-cloud/kimi-k2.6`: incomplete/hung after local validation output; stopped by targeted PID after another reviewer found a blocking issue.

Blocking finding:

- `journal-export-format` expected field `TRANSPORT`, but systemd journal entries use `_TRANSPORT`. This would make correct adapters fail the shared conformance test.

Non-blocking finding repaired now:

- The corrupted journal verification case used `error_contains: "truncated"`, while `journalctl --verify` against the decompressed upstream corrupted fixture reports journal-object corruption text such as `File corruption detected` and `Bad message`. A broader substring is safer for systemd-compatible implementations.

Repair disposition:

- Changed `journal-export-format` expected field from `TRANSPORT` to `_TRANSPORT`.
- Changed `journal-verify-corruption-detection` `error_contains` from `truncated` to `corrupt`; the note already permits truncation or corruption wording.

## Followup

1. **FSS/sealed journal fixtures** - Generate `sealed.journal` using `journalctl --force --setup-keys --interval=2 --output=json` against a live systemd-journald. Deferred since it requires daemon. Tracking path: SOW-0008 decides the FSS/sealing phase split after baseline implementations exist.
2. **Fixture materialization helper** - Decide whether later adapters should stream-decompress `.zst` fixtures directly or share a repo-local helper that materializes decompressed copies under `.local/`. Tracking path: SOW-0005 must choose the first adapter strategy; SOW-0008 can factor a shared helper if later implementations need one.
3. **Rust adapter stub** - SOW-0004 should implement the Rust adapter following `ADAPTER_CONTRACT.md` after the Go writer-first SOW completes.
4. **Go/Node/Python adapters** - SOW-0005, SOW-0006, SOW-0007, and SOW-0010 implement their language adapters following the same contract, with SOW-0005 focused on the Go writer first.
5. **Corrupted corpus extraction script** - `tests/conformance/scripts/extract-corrupted.sh` to extract `afl-corrupted-journals.tar.zst` on demand. Tracking path: SOW-0008 includes remaining interoperability and writer-feature test infrastructure.
6. **journalctl CLI test cases** - Expand `journal-list-boots` and `journal-export-format` into full `journalctl` behavior coverage (grep, fields, output formats). Tracking path: SOW-0010 handles Go reader/journalctl completion, and the language-specific journalctl SOWs must keep the shared manifest aligned.

## External Review Round 4 Findings

Status: repair required before close.

Review result:

- `llm-netdata-cloud/minimax-m2.7-coder`: PRODUCTION GRADE.
- `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE with non-blocking cleanup findings.
- `llm-netdata-cloud/qwen3.6-plus`: NOT PRODUCTION GRADE due a stale `AGENTS.md` ordering rule.
- `llm-netdata-cloud/glm-5.1`: identified the same stale `AGENTS.md` priority conflict during review.

Blocking finding:

- `AGENTS.md` still said SOW-0005 could run after SOW-0004, contradicting the user-directed Go writer-first priority recorded in `SOW-status.md`, `product-scope.md`, SOW-0004, and SOW-0005.

Non-blocking findings repaired now:

- SOW-0003 outcome status still referenced external review round 2.
- The earlier validation section could be confused with the later repair validation.
- `ADAPTER_CONTRACT.md` allowed either adapter or harness decompression for `.zst` fixtures; this could split adapter behavior.
- `error_contains` matching did not specify case sensitivity.
- The importer test note mentioned the original field-count behavior while the manifest intentionally checks a subset.

Repair disposition:

- Updated `AGENTS.md` so SOW-0005 (Go writer first) activates immediately after SOW-0003 and before Rust/Node/Python/interoperability/benchmark work.
- Labeled the earlier validation section as superseded by later repair validation.
- Updated the outcome status to pending external review round 5 / final close.
- Clarified `.zst` decompression responsibility as adapter-owned unless a future harness version materializes decompressed copies.
- Documented case-insensitive `error_contains` matching.
- Clarified the importer note as a four-key ordered subset check.

## External Review Round 5 Findings

Status: production-grade; closeout cleanup completed.

Review result:

- `llm-netdata-cloud/minimax-m2.7-coder`: PRODUCTION GRADE.
- `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE.
- `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE.
- `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE with non-blocking cleanup findings.

Non-blocking findings repaired now:

- SOW-0004 still referenced completed SOW-0002 under `pending/` in two evidence lists.
- Followup items for fixture materialization, corrupted corpus extraction, and journalctl CLI expansion needed explicit tracking paths before this SOW moved to `done/`.
- Artifact Maintenance Gate still reflected the earlier in-progress state.

Repair disposition:

- Updated SOW-0004 SOW-0002 references to the completed `done/` path.
- Added explicit tracking paths to each SOW-0003 followup item.
- Updated this SOW status, artifact gate, and SOW-status lifecycle metadata for closeout.

## External Review Round 6 Findings

Status: production-grade; non-blocking cleanup completed.

Review result:

- `llm-netdata-cloud/minimax-m2.7-coder`: PRODUCTION GRADE.
- `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE with non-blocking findings.
- `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE with non-blocking findings.
- `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE with non-blocking findings.

Non-blocking findings repaired now:

- SOW-0004 still had Rust-first wording in the Purpose and Implications sections.
- Runtime project skills referenced SOW-0001 under `current/` even though SOW-0001 is completed under `done/`.

Non-blocking findings tracked for later:

- Several test cases intentionally use adapter-asserted `entries_match: true`; implementation SOWs must strengthen these assertions when concrete language behavior produces known expected outputs.
- `.zst` decompression is adapter-owned by contract and must be validated per language implementation.

Repair disposition:

- Updated SOW-0004 Rust wording to reflect the Go writer-first priority.
- Updated project skill SOW-0001 evidence paths to the completed `done/` path.

## Outcome

Status: completed.

- All 7 no-rtc compressed fixtures committed (total ~1.66 MiB).
- All 3 corrupted compressed fixtures committed (total ~4.6 KiB).
- `INVENTORY.md` covers every journal-related systemd test file: 12 `test-journal*.c` files, 1 `test-journal-importer.c`, 3 ancillary tests, 2 importer text fixtures, 2 test-journal directories, 16 TEST-04-JOURNAL shell tests.
- Every fixture-backed conformance-v01 test has validateable committed fixture inputs.
- Generated FSS input is marked with `source_path: "daemon-required"` and tracked for SOW-0008.
- `type:generated` fixtures require `source_path` (committed inputs or `"daemon-required"` marker).
- `error_contains` must be non-empty for `result_format: error`.
- `journal-file-parse-uid-from-filename` no longer references irrelevant fixture.
- `journal-verify-corruption-detection` uses corrupted fixture with meaningful error expectation.
- Manifest schema enforces `source_path` for generated fixtures via `allOf` conditional.
- `manifest_checker.py` validates generated fixture source paths and empty error_contains.
- SOW audit clean, no sensitive data, no non-ASCII punctuation, no whitespace errors.

## Regression Log

None yet.
