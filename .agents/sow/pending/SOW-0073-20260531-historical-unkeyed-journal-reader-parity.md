# SOW-0073 - Historical Unkeyed Journal Reader Parity

## Status

Status: open

Sub-state: discrepancy found during RHEL 8.10/systemd 239 read check; pending implementation after SOW-0064.

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
- Rust and a temporary Go bypass both read the file, but their logical entry counts differ from stock `journalctl` by 23 entries on copied snapshots.

Inferences:

- Historical unkeyed-hash journals need explicit reader parity coverage, not just removal of the Go keyed-hash gate.
- The +23 entry difference is probably an online/historical traversal boundary mismatch, but the exact root cause is not proven yet.

Unknowns:

- Whether the +23 entries are uncommitted online tail entries, historical entry-array traversal differences, or another systemd 239 rule.
- Whether Python and Node have the same rejection/count behavior once the historical fixture is available.

### Acceptance Criteria

- Add a sanitized historical unkeyed/LZ4 journal fixture or deterministic fixture generator that reproduces the RHEL 8.10/systemd 239 behavior without committing raw host logs.
- Rust, Go, Python, and Node readers match stock `journalctl --file --output=export --all` canonical digest and counts on the historical unkeyed/LZ4 fixture.
- Go no longer rejects historical unkeyed-hash journals solely because `HEADER_INCOMPATIBLE_KEYED_HASH` is absent.
- Reader traversal stops at the same logical boundary as stock systemd for this historical online-file case.
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

Current state:

- Stock systemd 239 can verify and read the historical journal.
- Current Go SDK rejects it with `unsupported journal file`.
- Temporary Go code that bypassed the keyed-hash rejection could read the file, but produced more entries than stock systemd.
- Rust built on the RHEL 8.10 host could read the file, but also produced more entries than stock systemd.

Risks:

- Historical field/hash traversal fixes can affect all readers and should not be folded into the active corpus sweep without a focused fixture and matrix.
- Matching stock systemd may require intentionally ignoring some object-graph entries even if they are structurally valid.
- Raw host journal files may contain sensitive data and must not be committed or copied into durable artifacts.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK reader stack lacks full parity for historical unkeyed-hash LZ4 journals produced by systemd 239.
- Go has an explicit keyed-hash requirement at `go/journal/format.go:356`, which rejects valid older journals.
- Rust has unkeyed hash support, but Rust and a temporary Go bypass both report 23 more entries than stock systemd on copied online snapshots, so traversal semantics also need analysis.

Evidence reviewed:

- RHEL 8.10 systemd version: `systemd 239 (239-82.el8_10.16)`.
- Historical header evidence from the copied journal snapshot: `compatible_flags=0`, `incompatible_flags=2`, `header_size=240`.
- Stock verification evidence: `journalctl --verify --file [snapshot]` returned `PASS`.
- Temporary Go bypass evidence: Go read the file but did not match stock counts/digest.
- Rust-on-RHEL evidence: Rust read the file but did not match stock counts/digest.

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
- The fixture must be sanitized or generated; raw RHEL journal content must remain outside committed artifacts.

Sensitive data handling plan:

- Do not commit raw journal files from the RHEL 8.10 host.
- Do not write raw journal field names, values, messages, hostnames, usernames, IP addresses, or payload bytes into SOWs, specs, docs, skills, or code comments.
- Use counts, digests, header flags, systemd versions, and sanitized paths only.

Implementation plan:

1. Reproduce the historical discrepancy with a sanitized or generated fixture.
2. Analyze systemd 239 and systemd v260.1 reader behavior for unkeyed hash and online-file traversal boundaries.
3. Fix Go header parsing so unkeyed historical journals are accepted.
4. Fix Rust traversal if stock systemd excludes the +23 entries by design.
5. Port equivalent reader behavior to Go, Python, and Node.
6. Add shared conformance and interoperability coverage.

Validation plan:

- Stock `journalctl --verify --file` passes on the fixture.
- Stock `journalctl --file --output=export --all --no-pager` canonical digest matches Rust, Go, Python, and Node readers.
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

- None. This is a correctness bug under the existing historical-reader compatibility goal.

## Implications And Decisions

No user decision is currently required. The SDK already claims historical reader compatibility; this SOW repairs a concrete gap.

## Plan

1. Build or obtain a non-sensitive historical unkeyed/LZ4 fixture.
2. Determine why stock systemd excludes the extra 23 entries.
3. Implement cross-language reader parity.
4. Validate with stock systemd, all SDK readers, and existing matrices.

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

## Validation

Acceptance criteria evidence:

- Pending.

Tests or equivalent validation:

- Pending.

Real-use evidence:

- Pending.

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
- SOW lifecycle: open in `.agents/sow/pending/`.
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
