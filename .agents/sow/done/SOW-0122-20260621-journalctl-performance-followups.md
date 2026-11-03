# SOW-0122 - Journalctl Performance Followups

## Status

Status: completed

Sub-state: completed 2026-11-01 in the autonomous backlog run. Measured the
envelope, fixed the `--list-invocations` cost and IDX alignment parity in both
CLIs; the three other measured gaps are tracked by SOW-0166, SOW-0167 and
SOW-0168. Scope choices approved by the user 2026-11-03.

## Requirements

### Purpose

Keep the portable Rust and Go `journalctl` commands fit for large offline
journal files and directories while preserving the SOW-0121 stock-compatible
file-backed behavior.

### User Request

SOW-0121 reviewer and local close-out evidence identified remaining
performance-contract work that is not a file-backed correctness blocker but
should be tracked before release-quality performance claims.

### Assistant Understanding

Facts:

- SOW-0121 passed its shared stock-oracle parser, directory, and query matrices
  at its earlier close. New combinations failed the 2026-10-14 audit; the
  originating SOW repaired those failures, then reopened for the 2026-10-26
  CLI review findings, now corrected and independently reviewed.
- SOW-0121's 2026-10-19 unit/invocation audit supersedes the old row-scan
  description. Its provisional implementation enumerated indexed unique IDs
  and reopened readers for unit-restricted bounds. That approximation had
  confirmed correctness gaps. Its completed invocation repair delivers native directional
  discovery, global ID bounds, reader reuse and performance evidence.
- `--follow` uses portable polling over explicit file/directory inputs. That
  preserves non-Linux correctness and purity, but may be more expensive than a
  daemon-backed Linux journal wait path on very large inputs.

Inferences:

- Broader input-size envelopes and portable polling costs are scalability
  opportunities. Confirmed invocation correctness gaps and the performance of
  their repairs were resolved in SOW-0121 and are not deferred here.
- A production-grade release decision should either improve these paths or
  record measured evidence that the existing shape is acceptable for the target
  package.

Unknowns:

- The target large-journal envelope for the portable command has not been set.
- Native macOS, Windows, and FreeBSD runtime performance has not been measured
  for these exact paths.

### Acceptance Criteria

- Measure `--list-invocations`, `--follow`, reverse output, and large
  file/directory selection on representative synthetic fixtures.
- Evaluate remaining large-input costs using journal-native index/facade paths
  against SOW-0121's delivered indexed invocation baseline.
- Preserve SOW-0121 parser and query parity results.
- Record any intentionally retained slower path with measured cost and release
  implications.

## Analysis

Sources checked:

- `.agents/sow/done/SOW-0121-20260621-file-backed-journalctl-full-parity.md`
- `.agents/sow/specs/product-scope.md`
- `.agents/sow/specs/journalctl-v260-parity-matrix.md`
- `docs/Journalctl-CLI.md`

Current state:

- Earlier invocation correctness and performance repairs remain accepted.
  SOW-0121 is completed after CLI review fixes, including reverse cursor seeking;
  completed SOW-0159 owns the approved directional SDK primitives.
- Performance work remains for invocation listing and follow scalability.
- This SOW remains pending and is not a third active workstream in the new
  two-SOW request. Broader optimizations here are not prerequisites for measuring
  or preserving performance of fixes in SOW-0121 and SOW-0159.

Risks:

- A premature release claim could overpromise performance on large archives.
- A rushed optimization could break stock-compatible filtering, boot context,
  invocation context, or cursor order.

## Pre-Implementation Gate

Status: passed 2026-11-01 (measurement below; the original text is kept as
the activation record).

Problem / root-cause model:

- Portable polling and the large-input envelope need measurement after the
  active correctness work. Activation must inspect the delivered indexed
  implementation rather than assume the historical invocation algorithm.

Evidence reviewed:

- SOW-0121 residual performance note and reviewer dispositions.

Affected contracts and surfaces:

- Rust `journalctl` CLI.
- Go `journalctl` CLI.
- Go and Rust facade/index helper APIs if needed.
- Shared parser and interoperability matrices.
- Public CLI documentation and product scope specs.

Existing patterns to reuse:

- FIELD/DATA indexed unique-value enumeration.
- Cursor seek and previous/next facade primitives.
- Shared stock-oracle interoperability tests.

Risk and blast radius:

- Medium. The work touches user-visible ordering, filtering, boot/invocation
  context, and follow behavior.

Sensitive data handling plan:

- Use only synthetic repository-local fixtures. Do not read host journals or
  record raw host/customer data.

Implementation plan:

1. Build large synthetic fixtures for invocation, boot, unit, and follow
   scenarios.
2. Benchmark current Rust and Go behavior against stock file-backed
   `journalctl` where stock can run.
3. Implement indexed or bounded-state paths where measurement justifies them.
4. Preserve correctness with parser, directory, and query matrices.

Validation plan:

- `python tests/parser-parity/check_v260_manifest.py`
- `python tests/parser-parity/run_parser_parity.py`
- `python tests/interoperability/run_directory_matrix.py`
- `python tests/interoperability/run_journalctl_query_matrix.py`
- Targeted large-fixture performance measurements for Rust and Go.
- Read-only external review after local validation.

Artifact impact plan:

- AGENTS.md: expected unaffected.
- Runtime project skills: update only if the work changes durable workflow.
- Specs: update product scope and parity matrix if behavior or performance
  guarantees change.
- End-user/operator docs: update `docs/Journalctl-CLI.md` if limits or claims
  change.
- End-user/operator skills: none expected.
- SOW lifecycle: this SOW tracks SOW-0121 follow-up performance work.
- SOW-status.md: update on activation and close.

Open-source reference evidence:

- None checked yet in this pending SOW. Activation should inspect the CLI's
  `systemd/systemd` v262 target paths
  for invocation listing and follow behavior.

Open decisions:

- Resolved 2026-11-01 (user backlog order): activated as item 8. Envelope
  used: claim only gaps of 5x or more against stock, given host load. The fix
  decisions below were approved by the user on 2026-11-03.

## Implications And Decisions

Measurement (2026-11-01, release builds at commit 96eecf5, stock v262 built
locally, synthetic fixtures from the Go writer: one 931 MB file and a
300-file 1.8 GB directory, 1.5M entries, 6 boots, 45 active units with 221
invocations each; 3 runs, median; host load 16-66 on 112 CPUs, so only gaps
of 5x or more are claimed).

Reproduction (the scripts stay in ignored `.local/sow0122/`; the procedure is
recorded here so the numbers can be re-measured without them):

1. Build release CLIs: `cargo build --release -p journalctl` and
   `CGO_ENABLED=1 go build ./cmd/journalctl`, plus stock v262 journalctl.
2. Generate fixtures with a small Go program using the in-repo Go writer:
   1.5M entries over 6 boots; unit rows rotate over 50 synthetic services with
   about 200 invocation IDs each, plus periodic kernel rows with no unit
   (observed in output: 45 units with 221 invocations listed per unit); once
   as one file and once as 300 files in one directory.
3. Run each command below with `--file`/`--directory` on each fixture, 3 runs,
   median wall time, CPU time and max RSS (`/usr/bin/time -v`), and compare
   output hashes against stock.
4. `--follow`: start each CLI on the fixture, append bursts with the writer
   after a 15 s warm-up, record idle CPU over 30 s and append-to-print latency.

Results before the fix (wall s / CPU s / max RSS MB):

| command | src | rust wall/CPU/RSS | go wall/CPU/RSS | stock wall/CPU/RSS | rust/stock | go/stock | outputs |
|---|---|---|---|---|---|---|---|
| list-inv -u | file | 29.35 / 29.34 / 299 | 12.13 / 12.27 / 227 | 0.05 / 0.05 / 101 | 587.0x | 242.6x | DIFF (222 lines) |
| list-inv -u | dir | 11.88 / 11.87 / 3986 | 16.05 / 23.05 / 4226 | 0.96 / 0.96 / 1488 | 12.4x | 16.7x | DIFF (222 lines) |
| -r -n 1000 | file | 0.01 / 0.00 / 11 | 0.26 / 0.30 / 161 | 0.01 / 0.01 / 6 | 1.0x | 26.0x | match (1000 lines) |
| -r -n 1000 | dir | 0.02 / 0.02 / 12 | 0.40 / 0.45 / 806 | 0.07 / 0.07 / 142 | 0.3x | 5.7x | match (1000 lines) |
| -r (full) | file | 8.64 / 8.63 / 363 | 23.78 / 28.05 / 160 | 16.88 / 16.74 / 351 | 0.5x | 1.4x | match (1500005 lines) |
| -r (full) | dir | 10.48 / 10.47 / 766 | 23.90 / 32.91 / 821 | 45.17 / 45.15 / 780 | 0.2x | 0.5x | match (1500005 lines) |
| forward (full) | file | 8.18 / 8.17 / 220 | 26.80 / 33.02 / 161 | 15.90 / 15.89 / 251 | 0.5x | 1.7x | match (1500005 lines) |
| forward (full) | dir | 10.80 / 10.79 / 766 | 21.75 / 26.55 / 829 | 46.58 / 46.39 / 778 | 0.2x | 0.5x | match (1500005 lines) |
| --since/--until 60s | file | 0.03 / 0.03 / 11 | 0.34 / 0.38 / 161 | 0.06 / 0.06 / 9 | 0.5x | 5.7x | match (6001 lines) |
| --since/--until 60s | dir | 3.54 / 3.53 / 157 | 3.46 / 3.75 / 818 | 0.21 / 0.21 / 101 | 16.9x | 16.5x | match (6001 lines) |
| -u unit | file | 0.30 / 0.29 / 373 | 2.20 / 2.35 / 161 | 0.51 / 0.51 / 245 | 0.6x | 4.3x | match (30005 lines) |
| -u unit | dir | 0.34 / 0.33 / 821 | 0.91 / 1.04 / 942 | 1.11 / 1.10 / 788 | 0.3x | 0.8x | match (30005 lines) |
| -b -5 | file | 1.39 / 1.39 / 64 | 3.57 / 4.31 / 156 | 2.78 / 2.77 / 128 | 0.5x | 1.3x | match (250000 lines) |
| -b -5 | dir | 1.76 / 1.75 / 267 | 3.60 / 4.54 / 930 | 7.12 / 7.12 / 369 | 0.2x | 0.5x | match (250000 lines) |
| _SYSTEMD_INVOCATION_ID= | file | 0.00 / 0.00 / 7 | 0.27 / 0.30 / 161 | 0.00 / 0.00 / 8 | 0.0x | 54.0x | match (137 lines) |
| _SYSTEMD_INVOCATION_ID= | dir | 0.01 / 0.01 / 28 | 0.34 / 0.33 / 828 | 0.03 / 0.02 / 84 | 0.3x | 11.3x | match (137 lines) |
| -n 100 | file | 0.00 / 0.00 / 5 | 0.27 / 0.29 / 161 | 0.00 / 0.00 / 6 | 0.0x | 54.0x | match (100 lines) |
| -n 100 | dir | 0.01 / 0.01 / 6 | 0.33 / 0.30 / 808 | 0.04 / 0.04 / 142 | 0.2x | 8.2x | match (100 lines) |

`DIFF` on `list-inv -u` is the IDX column width only (fixed here).

- Rust is at or faster than stock on full forward/reverse output, `-u`,
  `-b -5`, field matches, `-n` and single-file `--since` (full output is 2x
  faster than stock on the file and 4x on the directory).
- Gaps of 5x or more:
  1. `--list-invocations -u`: Rust 29.4 s / Go 12.1 s vs stock 0.05 s on the
     file; 11.9 s / 16.1 s vs 0.96 s on the directory. Fixed here.
  2. Directory `--since` seek: 3.5 s vs 0.21 s (up to 17 s vs 0.16 s near the
     tail). Tracked by SOW-0166.
  3. Go opens load the complete entry array (0.27 s and 160 MB floor per
     file). Tracked by SOW-0167.
  4. `--follow` polls and reopens the input (Go 21-30% idle CPU, Rust 2-6%,
     stock 0%; latency 0.2-0.8 s vs 2-4 ms). Tracked by SOW-0168 (needs a
     user decision).
- Output: default output byte-identical across all three except
  `--list-invocations`, where stock right-aligns a wider IDX column once
  indexes reach three digits plus sign. Fixed here for invocations and boots.
  The `Error:` stderr prefix difference is tracked by SOW-0168.

Decisions (agent-chosen 2026-11-01, approved by the user 2026-11-03):

1. Root cause of gap 1: SOW-0121 added a valid-ID eligibility prefilter (an
   OR of every invocation ID value) to keep sparse-ID journals cheap. Each
   step evaluates every term, so with about 10,000 IDs it is far slower than
   stock's unit/boot walk (logs-show.c `discover_next_id`,
   systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492,
   src/shared/logs-show.c:1978-2183). The prefilter is now kept only for at
   most 32 IDs (Rust `ELIGIBILITY_MAX_TERMS`, Go
   `invocationEligibilityMaxTerms`); with zero IDs the walk still ends at
   once. Rows without a valid ID are skipped by the walker either way, so
   output is unchanged.
2. IDX width: both tables size IDX to the widest index, at least 3 with the
   header, and pad the header (`table_index_width` / `tableIndexWidth`).

## Plan

1. Define fixture sizes and target envelopes.
2. Measure current behavior.
3. Optimize only paths with measurable payoff or release risk.
4. Validate parity and performance.

## Implementation And Review Plan

Implementation:

- The implementation plan remains pending user activation and completion of the
  pre-implementation gate.

Reviewers:

- External review authorization is pending activation. Recommend whole-SOW
  review after local validation and before GitHub submission; when authorized,
  follow the system-wide `external-reviewers` skill.

Repository boundary block for every external-reviewer prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Record benchmark, parity, reviewer, and audit failures before any close.

## Execution Log

### 2026-06-21

- Created as the explicit follow-up mapping for SOW-0121 residual performance
  work.

### 2026-10-14

- Updated the dependency/reference after SOW-0121 regression reopening. The
  user requires performance as-is or better for both requested workstreams;
  their correctness and no-regression gates cannot be deferred to this SOW.
- Kept broad invocation/follow scalability work pending without activation or
  merging it into the two requested scopes.

### 2026-10-19

- Reconciled the historical invocation description with the confirmed native
  correctness repair in SOW-0121. This SOW retains only broader envelope and
  polling opportunities; it does not defer required behavior or performance
  preservation and remains pending.

## Validation

Acceptance criteria evidence:

- Large-input envelope measured for both CLIs against stock v262 (table in
  Implications And Decisions).
- `--list-invocations -u` after the fix (same fixtures, single runs, load
  about 36): Rust 0.23 s file / 0.67 s directory (was 27.7 / 13.0); Go
  0.66 s / 1.81 s (was 14.5 / 15.4); stock 0.06 s / 1.03 s. Output of the
  old and new binaries is identical before the IDX fix; after it, output is
  byte-identical to stock for `--list-invocations` with and without `-q`, and
  for `--list-boots`, on the file and the directory, in both languages.
- Each remaining gap of 5x or more has a pending SOW (Follow-up mapping).

Tests or equivalent validation:

- New tests: Rust `table_index_width_follows_widest_index`, Go
  `TestTableIndexWidthFollowsWidestIndex`.
- `go test ./cmd/journalctl` and `cargo test -p journalctl` pass.
- `tests/parser-parity/check_v260_manifest.py`: OK.
  `run_parser_parity.py`: 134/134 in each language.
- `tests/interoperability/run_journalctl_query_matrix.py --journalctl`
  (v262 oracle): 3587/3587 PASS.

Real-use evidence:

- Both release CLIs were run on the synthetic 931 MB file and the 300-file
  directory with the commands in the table.

Reviewer findings (self-review of the whole SOW diff; tests are the gate, no
external reviewers):

1. The 32-ID bound is a heuristic, not a measured crossover. Journals with
   33 to a few hundred IDs and many ID-less rows were not measured.
   Disposition: accepted and recorded as agent-chosen. Above the bound the
   walker visits the unit's rows as stock does and skips ID-less rows, so
   output is unchanged and cost is bounded by row count, not ID count.
2. IDX padding also changes `--list-boots` once indexes reach three digits
   plus sign. Disposition: intended; verified byte-identical to stock.
3. Rust and Go bounds are separate constants. Disposition: each cites the
   other in its comment; the parity tests cover the table width, not the
   bound, which changes cost only.

Same-failure scan:

- The eligibility OR is the only value-list prefilter in either CLI
  (`grep -n eligibility`); both tables share the width helper.

Sensitive data gate:

- Synthetic fixtures and identities only; fixtures and raw results stay in
  ignored `.local/sow0122/`. This SOW records aggregate numbers only.

Artifact maintenance gate:

- AGENTS.md: no new rule; the change follows the existing performance contract.
- Runtime project skills: `project-journal-compatibility` names the query
  matrix and parser-parity gates already used here; no new rule.
- Specs: no spec states the prefilter or IDX width; the parity matrix spec
  is unaffected (output is now closer to stock).
- End-user/operator docs: `docs/Journalctl-CLI.md` makes no claim about
  these paths; no change.
- End-user/operator skills: none exist.
- SOW lifecycle: `Status: completed` and the move to `done/` are in the same
  commit as the code; three follow-up SOWs were created.
- SOW-status.md: both the canonical ledger and the root index are updated.

Specs update:

- None needed (see the artifact maintenance gate).

Project skills update:

- None needed.

End-user/operator docs update:

- None needed.

End-user/operator skills update:

- None exist.

Lessons:

- See Lessons Extracted.

Follow-up mapping:

- Directory `--since` seek gap: tracked by SOW-0166.
- Go eager entry-array load: tracked by SOW-0167.
- `--follow` polling/reopen cost and the `Error:` prefix: tracked by SOW-0168
  (decided 2026-11-03: cheap portable polling first).
- Runtime purity: CLI-only changes; no new native code. Nothing else remains.

## Outcome

Completed. `--list-invocations` is 40-120x faster in Rust and 8-22x in Go on
many-ID journals, faster than stock on directories in Rust, and its table now
matches stock. The remaining measured gaps are tracked by SOW-0166, SOW-0167
and SOW-0168.

## Lessons Extracted

A prefilter tuned for a sparse case needs a bound for the dense case; measure
both shapes before keeping it.

## Followup

- SOW-0166: directory realtime seek cost.
- SOW-0167: Go reader eager entry-array load.
- SOW-0168: follow waiting/reopen cost and `Error:` prefix parity.
