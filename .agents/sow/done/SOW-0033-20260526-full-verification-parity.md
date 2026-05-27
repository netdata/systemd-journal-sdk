# SOW-0033 - Full Verification Parity

## Status

Status: completed

Sub-state: Verification parity matrix implemented, validated, reviewed, and ready to archive. Split from SOW-0022 Gap 2.

## Requirements

### Purpose

Make repository verification APIs reject the same practical corruption classes as stock systemd verification for the supported journal feature slices.

### User Request

The SDKs must not falsely claim compatibility when stock `journalctl --verify --file` would reject a journal file.

### Assistant Understanding

Facts:

- SOW-0019 added useful unsealed and sealed verification APIs in all four languages.
- SOW-0022 found those APIs still shallower than systemd object-graph verification.
- Full parity requires corrupted fixture families and stock systemd as an oracle.

Inferences:

- This is a larger validation and implementation SOW than header parsing or threshold work.

Unknowns:

- Some systemd verification classes may be impractical or unsafe to generate as committed fixtures. Each exception must be recorded with evidence.

### Acceptance Criteria

- Shared corrupted fixtures cover practical object type, size, hash, chain, entry-array, header-counter, main-entry-array, seqnum, monotonic, and TAG/FSS corruption classes.
- Stock `journalctl --verify --file` rejects each negative fixture.
- Rust, Go, Node.js, and Python verification APIs reject each negative fixture with controlled verification errors.
- Positive fixtures for supported feature slices still pass.

## Analysis

Sources checked:

- `SOW-0022-20260525-compatibility-test-gap-audit.md`
- `SOW-0019-20260524-forward-secure-sealing.md`
- `product-scope.md`
- Current `verify` implementations in all four languages
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`

Current state:

- Verification APIs walk important entry/data/TAG paths.
- They are not yet documented as full systemd object-graph verification parity.

Risks:

- Shallow verification can accept files that stock systemd rejects.
- Corruption fixture generation must avoid fragile binary hacks without a clear oracle.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- Repository verification covers core readable paths and sealed TAG checks, but systemd verification also checks object graph reachability, hash-table membership, entry-array sortedness, counter consistency, and multiple metadata invariants.

Evidence reviewed:

- Go `go/journal/verify.go`
- Rust `rust/src/journal/src/lib.rs`
- Node.js `node/src/lib/verify.js`
- Python `python/journal/verify.py`
- `tests/conformance/manifests/conformance-v01.json`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/journal-verify.c`

Affected contracts and surfaces:

- Verification APIs and controlled error types.
- File-backed journalctl `--verify` and `--verify-key`.
- Conformance fixtures/manifests/adapters.
- Product compatibility claims.

Existing patterns to reuse:

- Existing verification APIs from SOW-0019.
- Existing conformance adapter negative cases.
- Stock journalctl verification oracle.

Risk and blast radius:

- High compatibility value and high implementation complexity. Changes can affect all readers and CLI verification paths.

Sensitive data handling plan:

- Generate synthetic fixtures only. Do not use live host journals or private data.

Implementation plan:

1. Inventory systemd verification classes and map each to a practical fixture plan.
2. Add fixture generators and manifest cases with stock systemd oracle checks.
3. Implement missing checks in Rust, Go, Node.js, and Python.
4. Preserve controlled errors and avoid panics/crashes on malformed input.
5. Update docs/specs to state the verified parity envelope and any explicit exceptions.

Validation plan:

- Run full conformance verification cases.
- Run stock `journalctl --verify --file` oracle for every negative and positive fixture.
- Run file-backed journalctl `--verify` in all languages.
- Run read-only reviewers after implementation.

Artifact impact plan:

- AGENTS.md: no expected update.
- Runtime project skills: update `project-journal-compatibility` if full verification fixture coverage becomes a mandatory gate.
- Specs: update `product-scope.md` with verification parity envelope and exceptions.
- End-user/operator docs: update CLI/API docs if verification behavior changes.
- End-user/operator skills: likely unaffected.
- SOW lifecycle: activate only this SOW when implementing.
- SOW-status.md: update on activation and completion.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`
  - `src/libsystemd/sd-journal/journal-verify.c`
  - `src/libsystemd/sd-journal/journal-def.h`

Open decisions:

- None. SOW-0022 already split full verification parity into this SOW.

## Implications And Decisions

- No user decision is required before implementation, unless fixture feasibility exposes a parity class that cannot be safely generated.

## Plan

1. Build the corruption fixture inventory.
2. Add stock-oracle negative tests.
3. Implement missing verification checks.
4. Validate and review as one batch.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current routing decision.

Reviewers:

- Read-only reviewers from the active pool after implementation: minimax, kimi, qwen, glm. Mimo is skipped.

Failure handling:

- Record any unsupported verification class with evidence and a user decision before closing.

## Execution Log

- 2026-05-27: Activated for local implementation with read-only external reviewers after implementation.
- 2026-05-27: Added cross-language object-graph verification for Rust, Go, Node.js, and Python, wired into each public verification API and file-backed journalctl verification path.
- 2026-05-27: Added `tests/interoperability/run_verify_matrix.py` with stock `journalctl --verify --file` as the oracle for positive and negative repo-local fixtures.
- 2026-05-27: Added bounded DATA decompression checks for zstd/xz paths and sealed-verification malformed-header hardening after reviewer findings.
- 2026-05-27: Excluded FIELD hash-table bucket disconnection from the required negative corpus after a repo-local synthetic fixture showed stock systemd 260 accepts that mutation.

## Validation

Acceptance criteria evidence:

- Positive fixture coverage: regular, zstd DATA, xz DATA, lz4 DATA, compact, compact+zstd, compact+xz, compact+lz4, and sealed/FSS journals.
- Negative fixture coverage: object type, object size, decompressed DATA size, DATA payload hash, DATA hash-chain bucket membership, entry-array ordering, header counter, missing main entry array, entry seqnum, tail seqnum, tail monotonic timestamp, and TAG/HMAC corruption classes.
- Oracle result: `tests/interoperability/run_verify_matrix.py` passed with `positive_count=9`, `negative_count=12`, and `failures=[]` on `systemd 260 (260.1-2-manjaro)`.
- Stock systemd parity exception: FIELD hash-table bucket corruption was tested against a synthetic repo-local journal and accepted by stock `journalctl --verify --file`, so it is not treated as a required rejection class.

Test and validation commands:

- `python3 -m py_compile python/journal/compress.py python/journal/entry.py python/journal/writer.py python/journal/verify_graph.py python/journal/verify.py tests/interoperability/run_verify_matrix.py`
- `node --check node/src/lib/compress.js node/src/lib/entry.js node/src/lib/writer.js node/src/lib/verify-graph.js node/src/lib/verify.js`
- `tests/interoperability/run_verify_matrix.py`
- `go test ./journal -run 'Verify|verify'`
- `(cd go && go test ./journal)`
- `PYTHONPATH=python python3 -m pytest python/test_all.py -k 'verify_file or journalctl_verify'`
- `cargo test -p journal verify_file -- --nocapture`
- `(cd node && npm test)`
- `git diff --check`

Real-use evidence:

- The matrix uses file-backed repo-local fixtures only and compares stock `journalctl --verify --file` with the Rust, Go, Node.js, and Python verification paths.
- Sealed fixtures use repo-local deterministic test keys and validate `--verify-key` behavior without probing the workstation live journal.

Reviewer findings and dispositions:

- Kimi found a Rust sealed-verification panic risk on crafted files shorter than the sealed-header read window. Fixed by replacing runtime fixed-width `unwrap` reads with bounded helpers and adding `verify_file_with_key_rejects_short_sealed_header_without_panic`.
- Kimi found a Go `dataReferencesEntry` underflow risk when a DATA object's `n_entries` value is zero. Fixed by returning false before subtracting.
- Minimax found the Go normal DATA zstd read path still used an unbounded decompressor. Fixed by routing that path through the bounded zstd helper.
- GLM noted a possible FIELD hash-table negative gap. The gap was tested against stock systemd 260; stock accepted the mutation, so the matrix intentionally follows the stock oracle and does not require repository verifiers to reject it.
- Qwen reported production-grade readiness after fixes and noted a residual `n_entries == 1` concern in Go. The reviewed path returns a controlled verification failure for the crafted state because a one-entry DATA object cannot legally carry an entry-array offset; no valid-file behavior changes were made.

Same-failure search:

- Runtime Rust sealed-verification fixed-width `unwrap` reads were removed from the verification path; remaining `try_into().unwrap()` matches in `rust/src/journal/src/lib.rs` are test helpers or unrelated writer/fixture paths.
- Compressed DATA verification paths in Go, Node.js, Python, and Rust now use bounded decompression helpers for verifier-controlled reads.

Sensitive data gate:

- All fixtures are synthetic and generated inside the repository-local `.local/interoperability/verify` area. No host journal, customer data, credentials, SNMP community strings, private endpoints, or live system logs were used.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; project-wide workflow did not change.
- Runtime project skills: `.agents/skills/project-journal-compatibility/SKILL.md` updated with the mandatory verify matrix gate.
- Specs: `.agents/sow/specs/product-scope.md` updated with the current object-graph verification envelope.
- End-user/operator docs: no separate README or published guide update needed; this SOW changes verification depth and test gates, not user-facing CLI syntax.
- End-user/operator skills: none exist for this SDK output, so no update was needed.
- SOW lifecycle: this file is marked `completed` and moved to `.agents/sow/done/` during closeout.
- `SOW-status.md`: updated during closeout.

Lessons extracted:

- Matrix negatives must remain stock-oracle based. Stock systemd 260 accepts at least one FIELD hash-table disconnection mutation that looks suspicious structurally.
- Verification code must treat malformed journal headers as untrusted byte slices; historical and truncated headers need header-size-aware reads rather than fixed-offset assumptions.

Followup mapping:

- No new work is split from SOW-0033.
- Performance work remains tracked by `SOW-0009-20260523-benchmark-profile-optimize.md`.
- Netdata SDK integration remains tracked by `SOW-0026-20260526-netdata-sdk-integration.md`.
- File-backed journalctl query parity remains tracked by `SOW-0034-20260526-file-backed-journalctl-query-parity.md`.

## Outcome

Completed. Rust, Go, Node.js, and Python verification APIs now run shared object-graph checks before existing sealed/unsealed verification, and `tests/interoperability/run_verify_matrix.py` proves agreement with stock systemd for the accepted positive and negative fixture envelope.

## Lessons Extracted

- Do not expand negative verification requirements beyond what the stock oracle rejects unless the user explicitly chooses stricter-than-systemd behavior.
- Keep verifier corruption fixtures generated and checked through a shared language-neutral matrix so all four SDKs fail the same way.

## Followup

No new work is split from this SOW. Existing work remains tracked by `SOW-0009-20260523-benchmark-profile-optimize.md`, `SOW-0026-20260526-netdata-sdk-integration.md`, and `SOW-0034-20260526-file-backed-journalctl-query-parity.md`.

## Regression Log

None yet.
