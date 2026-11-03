# SOW-0160 - Go XZ Reader Dictionary Allocation

## Status

Status: open

Sub-state: analysis done 2026-11-01 (autonomous backlog run); D1 decided by
the user 2026-11-03 (A: Go writer declares 256 KiB). Not activated; no
production code changed.

## Requirements

### Purpose

Assess whether Go XZ DATA decompression can reduce repeated dictionary
allocation while preserving format compatibility, reader isolation and speed.

### User Request

The user requires SDK/journalctl changes to maintain or improve performance.
This separate opportunity was discovered while measuring the approved reader
lifetime repair; no decoder redesign was requested or authorized.

### Assistant Understanding

Facts:

- `go/journal/writer_compression.go:173` creates a fresh `xz.NewReader` for
  each XZ DATA expansion, then bounds decompressed output with `readAllLimited`.
- SOW-0108's immutable 2,048-row compressed corpus exposes approximately
  8.6 GB/high-cardinality query and 17.2 GB/singleton query of cumulative
  allocation in affected XZ cases. These are per-operation allocation totals,
  not peak resident memory. Baseline and repaired implementations share this
  decoder path and its cost; this is not a new repair regression.
- Existing pending/current SOWs do not own this specific optimization.

Inferences:

- Decoder construction/dictionary allocation is a plausible reusable-work
  opportunity. Reuse may retain excessive memory or cross reader lifetimes;
  lower allocation alone would not establish a better implementation.

Unknowns:

- Resolved 2026-11-01: the dependency has no reset API, so reuse is not
  available; the cost follows the writer's declared dictionary (Analysis
  Result). Retained memory returns to baseline after each decode.
- The real-use frequency of XZ journals; it affects how much D1 matters, not
  which option is cheaper.

### Acceptance Criteria

- Reproduce allocation sources with synthetic fixtures and explicit timer
  boundaries; report cumulative allocation and retained memory separately.
- Present measured alternatives and a user-approved design before coding.
- Any implementation preserves Go/Rust observable behavior, pure SDK runtime,
  complete compressed-stream handling and compiler/dependency constraints.
- Preserve or improve affected latency, allocation and memory paths, including
  repeated readers, concurrent independent readers and non-XZ controls.

## Analysis

Sources checked:

- `go/journal/writer_compression.go:173` and indexed DATA expansion in
  `go/journal/explorer.go`.
- SOW-0108's 2026-10-15 regression evidence, immutable codec corpus and retained
  before/candidate allocation observations under ignored task scratch.
- Pending/current SOW inventory; SOW-0159 owns regression prevention, while
  SOW-0122 owns separate CLI invocation/follow scalability.

Current state:

- No decoder source change is included in SOW-0108. Its lifetime/header repair
  must satisfy its own performance gate independently of this opportunity.

Risks:

- Pooling could exchange allocation for retained memory, contention or complex
  ownership. Stream completeness and malformed-input behavior must remain
  consistent. A dependency change could affect compiler floors or portability.

## Analysis Result (2026-11-01)

Root cause, corrected: the reader configuration is not the lever. In
ulikunitz/xz @ v0.5.16 (the version pinned in `go/go.mod`) the LZMA2 filter
reader allocates a dictionary of max(reader `DictCap`, the stream's declared
dictionary) (`lzmafilter.go:63-84`; allocation in `lzma/reader2.go:59-64`,
default cap at `lzma/reader2.go:22-23`), and the declared size comes from the
writer. Each XZ DATA expansion therefore allocates and zeroes the
writer's declared dictionary, regardless of payload size.

Declared dictionaries:

- systemd: 1 MiB (systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced,
  src/basic/compress.c:180).
- Rust writer: 256 KiB (`lzma-rust2` preset 0, `journal-core` `writer.rs`
  `XzOptions::with_preset(0)`).
- Go writer: 8 MiB (library default; `go/journal/writer_compression.go`
  `xzCompress` leaves `DictCap` unset).

Measurement (`.local/sow0160/xz_test.go`, one 600-byte payload, Go
decompression per DATA, 3 runs of 200 iterations):

| Declared dictionary | Time per decode | Allocated per decode |
|---|---|---|
| 8 MiB (Go writer) | 1.24-1.95 ms | 8.42 MB |
| 1 MiB (systemd) | 0.42-0.54 ms | 1.08 MB |
| 256 KiB (Rust writer) | 0.16-0.18 ms | 0.30 MB |
| 64 KiB | 0.08 ms | 0.10 MB |

Setting the reader `DictCap` to the minimum changes nothing (allocation
follows the stream). The table's numbers come from that one 600-byte payload.

Larger payloads (same file, `TestCompressedSizesBySize` and `BenchmarkLarge`;
deterministic varied synthetic log text, not real journals):

| Payload | 256 KiB dict | 1 MiB dict | 8 MiB dict |
|---|---|---|---|
| 600 B | 296 B | 296 B | 296 B |
| 4 KiB | 1,200 B | 1,200 B | 1,200 B |
| 64 KiB | 15,904 B | 15,900 B | 15,900 B |
| 256 KiB | 62,260 B | 62,260 B | 62,252 B |
| 1 MiB | 247,944 B | 248,164 B | 248,160 B |
| 4 MiB | 990,224 B | 989,164 B | 982,812 B |

- Compression loss of 256 KiB against 8 MiB is under 0.1% up to 1 MiB and
  0.75% at 4 MiB. Payloads with long-distance repeats beyond 256 KiB could lose
  more; this synthetic text does not prove the bound for every workload.
- Decode allocation grows with the declared dictionary at every size: 0.53 /
  1.31 / 8.65 MB per 64 KiB decode and 4.0 / 4.8 / 12.1 MB per 1 MiB decode.
  The decode times on this busy host overlapped across the three runs, so time
  is inconclusive for large payloads; it is dominated by the payload itself.
- Retained memory (`TestRetainedMemory`, 4 KiB payload): live heap mid-stream
  is about the declared dictionary (0.29 / 1.08 / 8.42 MB), and after the reader
  is released it returns to baseline (within 5 KB) for all three sizes. The SDK
  drops the reader after each DATA expansion, so the cost is per-decode
  allocation and peak, not retained memory. Reuse is not available: the
  library has no reset API for `xz.Reader` or `lzma.Reader2`.
- Other readers (not measured here; inference): liblzma (stock `journalctl`)
  and the Rust reader (`lzma-rust2`) also size their LZMA2 dictionary from
  the stream's declared value, so they would likely benefit too. D1 does not
  depend on this: option A is justified by the Go measurements above and by
  Go/Rust writer agreement. Validation under A measures the Rust reader
  before and after.

Reproduction (the scratch harness is not committed; its shape is recorded
here so the numbers can be regenerated):

- A standalone Go test package that depends only on
  `github.com/ulikunitz/xz` (the SDK's own XZ dependency). It compresses with
  `xz.WriterConfig{NoCheckSum: true, DictCap: D}` for D in 64 KiB, 256 KiB,
  1 MiB and 8 MiB, and decodes with `xz.ReaderConfig{DictCap:
  lzma.MinDictCap}.NewReader`, the reader configuration that `MinDictCap`
  shows has no effect.
- Payloads: one 600-byte log line for the first table; for the size table,
  deterministic synthetic flow-log text from a fixed-seed xorshift generator,
  truncated to 600 B, 4 KiB, 64 KiB, 256 KiB, 1 MiB and 4 MiB.
- Per-decode cost: `go test -bench . -benchmem -count 3`; compressed sizes:
  `len()` of each stream; retained memory: `runtime.MemStats.HeapAlloc`
  after a GC, before the reader, after reading one byte, and after the reader
  is released.
- Under A, the implementation commits an equivalent decode benchmark beside
  the Go XZ code, so the before/after numbers come from repository code.

Decision D1 (decided 2026-11-03: A; changes Go writer output bytes):

- A: Go writer declares 256 KiB, the Rust writer's value. Pros: Go/Rust
  writers agree; Go reads of new Go-written files about 9x faster with 28x
  less allocation; stock readers benefit too. Cons: payloads over 256 KiB
  compress slightly worse; existing 8 MiB files keep their cost.
- B: Go writer declares 1 MiB, systemd's value. Pros: matches journald.
  Cons: about 3x slower decode than A; Go/Rust writers still differ.
- C: keep 8 MiB. No output change; the cost stays.
- Existing Go-written XZ files keep their declared size under every option;
  a reader-side cap would need rewriting stream headers before decoding,
  which is rejected as unsafe.

Recommendation: A (long-term-best): it is the cheapest decode, with under
0.1% measured compression loss up to 1 MiB synthetic payloads, and it removes a Go/Rust
writer difference. Validation for A: byte-level check that Go output declares
256 KiB, cross-language and stock `journalctl --verify` on Go XZ files, the
existing XZ fixtures, and a payload over 256 KiB round trip.

## Pre-Implementation Gate

Status: ready (D1 = A, user decision 2026-11-03)

Problem / root-cause model:

- Each XZ DATA decode allocates the dictionary size declared in the stream
  header, and the Go writer declares 8 MiB. Decoder reuse is not available
  (no reset API) and a reader-side cap is unsafe, so the open choice is the
  Go writer's declared dictionary (D1), not decoder reuse.

Evidence reviewed:

- The dependency source, the writer declarations and the measurements in
  Analysis Result.

Affected contracts and surfaces:

- Go writer XZ output bytes (`go/journal/writer_compression.go` `xzCompress`)
  under options A and B; readers in all languages benefit without code change.
  Existing files keep their declared dictionary.

Existing patterns to reuse:

- Immutable codec fixtures, matching indexed/traversal checks, per-DATA output
  limits, complete-stream controls and frozen paired benchmark helpers.

Risk and blast radius:

- Changed Go writer bytes (byte-identity fixtures or tests that pin Go XZ
  output), and slightly worse compression of very large payloads with
  long-distance repeats.

Sensitive data handling plan:

- Use synthetic fields and identities. Keep raw profiles/fixtures in ignored
  repository-local scratch; commit only aggregate findings and source pointers.

Implementation plan:

1. Set `DictCap` to 256 KiB in the Go writer's XZ configuration
   (`go/journal/writer_compression.go` `xzCompress`) and update any test or
   fixture that pins Go XZ bytes.
2. Commit a Go decode benchmark beside the XZ code (declared 256 KiB against
   8 MiB) and record the before/after numbers here.

Validation plan:

- A byte-level check that Go output declares the chosen dictionary; Go, Rust
  and stock `journalctl --verify` on Go-written XZ files; the existing XZ
  fixtures and compression matrix; a round trip of a payload larger than the
  dictionary; and the decode benchmark rerun against the numbers above.

Artifact impact plan:

- AGENTS.md: no workflow change is proposed.
- Runtime project skills: update compression guidance if the Go writer's
  declared dictionary changes.
- Specs: update only actual operational guarantees changed by an approved design.
- End-user/operator docs: explain any changed options or memory guarantees;
  an internal improvement may require no usage changes.
- End-user/operator skills: none currently exist in this repository.
- SOW lifecycle: keep open/pending until explicitly activated; no current-chain
  scope is transferred here.
- SOW-status.md: list this opportunity in both canonical and root indexes.

Open-source reference evidence:

- This tracking record cites project source and retained project measurements;
  no external repository checkout is used as evidence. On activation, record
  dependency identity and checked revision for any external source analysis.

Open decisions:

- None. D1 = A (user decision 2026-11-03).

## Implications And Decisions

1. Track independently instead of expanding the approved lifetime/header repair.
   No pooling, decoder replacement, new dependency or public option is approved.
2. 2026-11-03 (user): D1 = A. The Go writer declares a 256 KiB XZ dictionary,
   the Rust writer's value. Existing files keep their declared size; every
   reader already decodes any declared dictionary, so files from earlier SDK
   releases are unaffected.

## Plan

1. Retain the measured opportunity and source pointers in this pending SOW.
2. Await activation, then complete analysis and present design options before code.

## Implementation And Review Plan

Implementation:

- The project manager owns any future analysis and implementation sequentially.
  No runtime implementation is part of creating this tracking record.

Reviewers:

- No external review is authorized. Follow the system-wide external-reviewers
  skill after complete local validation if this SOW is later activated.

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

- Retain unfavorable measurements and failed controls. Do not accept a lower
  allocation count that increases latency, retained memory or compatibility gaps.

## Execution Log

### 2026-10-15

- Created from the project SOW template as SOW-0108's concrete follow-up mapping.
  No runtime code, dependency or current execution order changes.

## Validation

Acceptance criteria evidence:

- This is an open tracking record; optimization acceptance is not claimed.

Tests or equivalent validation:

- Source/ownership search and existing matched codec measurements establish
  the lead. New implementation tests are neither required nor run for this record.

Real-use evidence:

- Existing Explorer XZ queries reproduce the cumulative allocation cost with
  synthetic on-disk journal files; consumer prevalence remains to be measured.

Reviewer findings:

- Read-only ownership analysis found no existing specific follow-up SOW.
  No final implementation review has occurred.

Same-failure scan:

- The shared decoder serves multiple SDK callers; their actual blast radius
  must be enumerated before selecting or implementing a design.

Sensitive data gate:

- This record contains only synthetic aggregate measurements and source paths.

Artifact maintenance gate:

- AGENTS.md: unchanged; no workflow rule changes.
- Runtime project skills: unchanged for this tracking-only record.
- Specs: unchanged; no product behavior or guarantee changes.
- End-user/operator docs: unchanged; no new user behavior exists.
- End-user/operator skills: none currently exist.
- SOW lifecycle: open under pending; not an implementation prerequisite.
- SOW-status.md: canonical and root indexes include this opportunity.

Specs update:

- No implementation or shipped contract changes exist yet.

Project skills update:

- Existing compression/performance workflow already governs later analysis.

End-user/operator docs update:

- No usage or memory policy is changed by tracking an opportunity.

End-user/operator skills update:

- No output/reference skills are present or affected.

Lessons:

- Separate cumulative allocated bytes from peak or retained memory, and do not
  mix an unchanged decoder opportunity into a lifetime regression's acceptance.

Follow-up mapping:

- SOW-0108's XZ dictionary-allocation opportunity is tracked here. Required
  SDK/CLI regression-prevention work stays in SOW-0159/SOW-0121.

## Outcome

Open; no optimization has been authorized or implemented.

## Lessons Extracted

The allocation-accounting distinction above is retained for future analysis.

## Followup

This SOW is the concrete tracking destination; it creates no further workstream.

## Regression Log

No completed outcome exists; this record tracks an unchanged baseline opportunity.
