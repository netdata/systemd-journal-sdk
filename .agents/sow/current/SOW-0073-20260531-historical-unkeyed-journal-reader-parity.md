# SOW-0073 - Historical Unkeyed Journal Reader Parity

## Status

Status: in-progress

Sub-state: Go reader hotfix implemented to unblock the real-world corpus sweep; full cross-language fixture and parity work remains pending.

## Requirements

### Purpose

Ensure SDK readers can consume historical systemd journal files from old deployed systems with the same logical entry set as stock systemd tooling.

### User Request

While the real-world corpus sweep was running, the user asked to check a RHEL 8.10 host with old systemd journals and verify whether this SDK can read those journals.

### Assistant Understanding

Facts:

- The checked host runs RHEL 8.10 with systemd 239.
- The checked journal is an online `.journal` file with `header_size=240`, `compatible_flags=0x0`, and `incompatible_flags=0x2`.
- `incompatible_flags=0x2` means LZ4 compression is present and keyed hash is absent.
- Stock `journalctl --verify --file` reports `PASS` on a copied snapshot of the file.
- Current Go reader rejects the file before reading because `parseHeader()` requires `incompatibleKeyedHash`.
- Rust and a temporary Go bypass both read the file and expose all entries from the global entry array.
- RHEL 8.10 systemd 239 `journalctl` output suppresses 23 adjacent duplicate-looking entries on copied snapshots.
- A local systemd 260.1 `journalctl` read of the same class of snapshot matches the SDK/global entry-array count.
- The 23 missing entries in RHEL 8.10 systemd 239 output are the later entries in adjacent duplicate-identity groups in forward traversal; reverse traversal suppresses the earlier entries in the same groups.

Inferences:

- Historical unkeyed-hash journals need explicit reader parity coverage.
- Core SDK readers should expose the file-format entry set matching current systemd, not emulate old systemd 239 `journalctl` duplicate suppression.

Unknowns:

- Whether Python and Node have the same rejection/count behavior once the historical fixture is available. Current source inspection found explicit keyed-hash reader gates in Python and Node, so a full parity pass is still required.
- Whether the project needs an optional `journalctl` compatibility mode for exact RHEL 8/systemd 239 CLI output.

### Acceptance Criteria

- Add a sanitized historical unkeyed/LZ4 journal fixture or deterministic fixture generator that reproduces the RHEL 8.10/systemd 239 behavior without committing raw host logs.
- Rust, Go, Python, and Node core readers expose the complete current-systemd/file-format entry set on the historical unkeyed/LZ4 fixture.
- The fixture records the old RHEL 8/systemd 239 CLI count separately as historical CLI behavior, not as the core reader pass condition.
- Go no longer rejects historical unkeyed-hash journals solely because `HEADER_INCOMPATIBLE_KEYED_HASH` is absent.
- Reader traversal behavior is explicitly compared with current stock systemd and historical systemd 239 where available.
- Shared conformance/interoperability tests cover keyed and unkeyed historical hash modes.

## Analysis

Sources checked:

- `go/journal/format.go:353`: rejects headers smaller than the supported minimum.
- `go/journal/format.go:356`: rejects journals without `incompatibleKeyedHash`.
- `go/journal/reader.go:343`: Go reader incompatible-flag mask includes keyed hash, compression, and compact flags.
- `rust/src/crates/journal-core/src/file/hash.rs:325`: Rust hash helper branches on keyed versus unkeyed mode.
- `rust/src/crates/journal-core/src/file/hash.rs:332`: Rust uses Jenkins hash for unkeyed mode.
- `rust/src/crates/journal-core/src/file/file.rs:693`: Rust determines keyed mode from the on-disk header flag.
- RHEL 8.10 test host with systemd 239, checked read-only; raw journal data was not copied into this repository.
- Local subagent investigation found the count difference maps to old systemd 239 duplicate suppression in `sd-journal` traversal.
- `systemd/systemd @ de7436b02bad`: `src/journal/sd-journal.c:441-447` treats entries as equal using boot id, monotonic timestamp, realtime timestamp, and xor hash before comparing seqnum; `src/journal/sd-journal.c:777-803` advances until an entry is different.
- `systemd/systemd @ c0a5a2516d28`: `src/libsystemd/sd-journal/sd-journal.c:620-630` suppresses duplicates only when seqnum also matches and the candidate is from a different file; `src/libsystemd/sd-journal/sd-journal.c:1087-1094` also requires seqnum equality for file-location equality.

Current state:

- Stock systemd 239 can verify and read the historical journal.
- Current Go SDK rejects it with `unsupported journal file`.
- Temporary Go code that bypassed the keyed-hash rejection could read the file and exposed all global entry-array entries.
- Rust built on the RHEL 8.10 host could read the file and exposed all global entry-array entries.
- RHEL 8.10 systemd 239 CLI output is lower by 23 entries due to old duplicate suppression.
- Current systemd behavior matches the SDK/global entry-array count for this class.

Risks:

- Historical field/hash traversal fixes can affect all readers and should not be folded into the active corpus sweep without a focused fixture and matrix.
- Emulating systemd 239 duplicate suppression in core readers would intentionally hide valid same-file entries and would diverge from current systemd behavior.
- Raw host journal files may contain sensitive data and must not be committed or copied into durable artifacts.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK reader stack lacks full Go support for historical unkeyed-hash LZ4 journals produced by systemd 239.
- Go has an explicit keyed-hash requirement at `go/journal/format.go:356`, which rejects valid older journals.
- Rust has unkeyed hash support and exposes all file-format entries.
- The observed +23 difference versus RHEL 8.10 systemd 239 is old same-file duplicate suppression in that historical `journalctl` implementation, not unreadable objects, LZ4 failure, unkeyed-hash failure, online-tail corruption, or entry-array corruption.

Evidence reviewed:

- RHEL 8.10 systemd version: `systemd 239 (239-82.el8_10.16)`.
- Historical header evidence from the copied journal snapshot: `compatible_flags=0`, `incompatible_flags=2`, `header_size=240`.
- Stock verification evidence: `journalctl --verify --file [snapshot]` returned `PASS`.
- Temporary Go bypass evidence: Go read the file and exposed all global entry-array entries.
- Rust-on-RHEL evidence: Rust read the file and exposed all global entry-array entries.
- RHEL 8.10 systemd 239 evidence: forward and reverse output both suppress 23 entries, but suppress opposite sides of the adjacent duplicate-looking groups.
- Current systemd evidence: local systemd 260.1 output matches the SDK/global entry-array count.
- Upstream history evidence: `b6849042d6` and `b17f651a17` fixed same-file duplicate and same-seqnum suppression behavior after v239.

Affected contracts and surfaces:

- Rust, Go, Python, and Node readers.
- libsystemd-compatible reader facade semantics.
- Corpus evaluation and shared conformance fixtures.
- Historical compatibility claims in specs/docs.

Existing patterns to reuse:

- `tests/corpus_eval/canonical.py` for stock-vs-SDK logical digest comparison.
- `go/internal/testcmd/corpus_digest` and `rust/src/internal/testcmd/corpus_digest` for SDK digest checks.
- Historical-header tests from SOW-0028.
- Mixed-directory and reader interoperability harnesses for cross-language checks.

Risk and blast radius:

- Reader traversal changes can alter results for live/online files, directory readers, and journalctl rewrites.
- Unkeyed-hash support must not weaken keyed-hash parsing or writer compatibility.
- RHEL 8/systemd 239 exact CLI emulation, if needed, belongs in an optional journalctl compatibility mode, not in core readers.
- The fixture must be sanitized or generated; raw RHEL journal content must remain outside committed artifacts.

Sensitive data handling plan:

- Do not commit raw journal files from the RHEL 8.10 host.
- Do not write raw journal field names, values, messages, hostnames, usernames, IP addresses, or payload bytes into SOWs, specs, docs, skills, or code comments.
- Use counts, digests, header flags, systemd versions, and sanitized paths only.

Implementation plan:

1. Reproduce the historical unkeyed/LZ4 reader case with a sanitized or generated fixture.
2. Fix Go header parsing so unkeyed historical journals are accepted.
3. Verify Rust, Go, Python, and Node core readers expose the current-systemd/file-format entry set.
4. Add shared conformance and interoperability coverage for historical unkeyed hash mode.
5. Document RHEL 8/systemd 239 duplicate suppression as historical CLI behavior, not core reader behavior.
6. If the user later requires exact RHEL 8 `journalctl` output compatibility, create a separate optional journalctl-239 compatibility SOW.

Validation plan:

- Stock `journalctl --verify --file` passes on the fixture.
- Current stock `journalctl --file --output=export --all --no-pager` canonical digest matches Rust, Go, Python, and Node core readers.
- RHEL 8/systemd 239 CLI duplicate-suppression behavior is recorded as a separate historical observation when that host is available.
- Existing reader, directory, mixed-format, compression, and corpus smoke tests still pass.
- Reviewer pool reviews the whole SOW after implementation.

Artifact impact plan:

- AGENTS.md: likely unaffected.
- Runtime project skills: update `project-journal-compatibility` if a new mandatory historical-reader rule is established.
- Specs: update product scope historical-reader compatibility details.
- End-user/operator docs: update reader support notes if public docs overclaim historical compatibility.
- End-user/operator skills: likely unaffected unless docs/spec workflow changes.
- SOW lifecycle: this SOW tracks the follow-up from the RHEL 8.10 discrepancy.
- SOW-status.md: update now and on completion.

Open-source reference evidence:

- None checked yet. Implementation should inspect `systemd/systemd` v239-era source and the project baseline `systemd/systemd` v260.1 source.

Open decisions:

- None for core readers. The default decision is not to emulate old systemd 239 same-file duplicate suppression in core readers.
- A later user decision is needed only if exact RHEL 8 `journalctl` CLI output compatibility is required.

## Implications And Decisions

No user decision is currently required for core readers. The SDK should accept valid historical unkeyed journals and expose the complete file-format entry set matching current systemd behavior. Exact systemd 239 CLI duplicate suppression is a separate optional behavior if needed later.

## Plan

1. Build or obtain a non-sensitive historical unkeyed/LZ4 fixture.
2. Remove Go's invalid keyed-hash requirement for readers.
3. Implement or verify cross-language reader parity for unkeyed historical journals.
4. Validate with current systemd, RHEL 8/systemd 239 observations, all SDK readers, and existing matrices.

## Delegation Plan

Implementer:

- Local implementation unless the user explicitly changes routing.

Reviewers:

- Reviewer pool after complete implementation and local validation: minimax, kimi, qwen, glm, and mimo.

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

- If the fixture cannot be sanitized or generated, record the blocker and ask the user for an approved fixture handling path.
- If systemd behavior differs between v239 and v260.1, record the compatibility target decision needed before implementation.

## Execution Log

### 2026-05-31

- Created from the RHEL 8.10/systemd 239 read check performed while SOW-0064 corpus evaluation was running.
- Local subagent investigation found that the 23-entry difference is old RHEL 8/systemd 239 same-file duplicate suppression. Current systemd 260.1 and SDK/global entry-array traversal expose all entries. The SOW was updated so core reader acceptance targets current file-format behavior, while exact RHEL 8 CLI duplicate suppression is optional journalctl compatibility work if requested later.
- Paused the SOW-0064 full corpus evaluator cleanly at 20,497 completed checks, all `ok`, to avoid continuing the scan with a known Go historical-reader gap.
- Removed Go's reader/header parser requirement that `HEADER_INCOMPATIBLE_KEYED_HASH` must be present. Reader/verifier code can now parse historical unkeyed headers and use the existing unkeyed Jenkins hash branch where applicable.
- Kept Go writer append-open conservative: `journal.Open()` / `OpenWithOptions()` still reject unkeyed historical files because the Go writer appends keyed-hash objects and must not corrupt unkeyed historical files.
- Validated the patched Go digest helper directly on the RHEL 8.10 host. Stock systemd 239 verified the checked file and exported 27,436 entries; patched Go opened and read 27,459 entries from the same live file. The +23 delta matches the previously identified old systemd 239 same-file duplicate suppression behavior.

## Validation

Acceptance criteria evidence:

- Go no longer rejects historical unkeyed-hash journals solely because `HEADER_INCOMPATIBLE_KEYED_HASH` is absent. Evidence: `go/journal/format.go` no longer rejects unkeyed headers; `go/journal/format_test.go` covers an unkeyed LZ4 historical header; RHEL 8.10 real-use check opened and read an unkeyed LZ4 journal with the patched Go digest helper.
- Full Rust, Go, Python, and Node historical fixture parity remains pending.

Tests or equivalent validation:

- `go test ./journal`: passed.
- `go test ./...` in the Go module: passed.
- RHEL 8.10 real-use check with patched `go/internal/testcmd/corpus_digest`: passed open/read of the historical unkeyed LZ4 journal.

Real-use evidence:

- RHEL 8.10 systemd version checked: `systemd 239 (239-82.el8_10.16)`.
- Stock `journalctl --verify --file [redacted-host-journal]`: `PASS`.
- Stock systemd 239 export count on the checked live file: 27,436.
- Patched Go digest count on the same checked live file: 27,459.
- The 23-entry difference is the previously diagnosed old systemd 239 duplicate-suppression behavior, not a Go open/read failure.

Reviewer findings:

- Pending.

Same-failure scan:

- Pending.

Sensitive data gate:

- Raw RHEL journal content was not committed. Durable evidence uses header flags, counts, and digests only.

Artifact maintenance gate:

- AGENTS.md: pending final review.
- Runtime project skills: pending final review.
- Specs: pending final review.
- End-user/operator docs: pending final review.
- End-user/operator skills: pending final review.
- SOW lifecycle: in-progress in `.agents/sow/current/`. This hotfix unblocks SOW-0064; full SOW-0073 completion remains pending.
- SOW-status.md: updated when this SOW was created.

Specs update:

- Pending.

Project skills update:

- Pending.

End-user/operator docs update:

- Pending.

End-user/operator skills update:

- Pending.

Lessons:

- Pending.

Follow-up mapping:

- Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
