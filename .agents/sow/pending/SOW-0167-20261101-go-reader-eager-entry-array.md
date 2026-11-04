# SOW-0167 - Go Reader Eager Entry Array Load

## Status

Status: open

Sub-state: measured gap from SOW-0122; not activated.

## Requirements

### Purpose

Opening a journal file with the Go reader should not load the complete global
entry array, so small queries cost what they cost in Rust and stock.

### User Request

Follow-up of SOW-0122 measurement (performance contract in `AGENTS.md`).

### Assistant Understanding

Facts:

- `go/journal/reader.go` `openFileWithPolicy` calls `loadEntryArray` on open
  (SOW-0122 trace: `reader.go:287` -> `:705`); strace shows the whole file
  mapped. Even `--header` costs about 0.27 s and 160 MB per 931 MB file, about
  800 MB on a 300-file directory.
- Go `-n 100`, `_SYSTEMD_INVOCATION_ID=` and `-r -n 1000` are 26-54x slower
  than stock on the file; Rust has no such floor.

Unknowns:

- Which Go reader paths depend on the materialized entry list (step,
  seek, recovery, live growth, explorer), and whether a lazy chain-walk
  accessor (as Rust uses) preserves their contracts.

### Acceptance Criteria

- Go reader opens without materializing the entry array; positioning uses
  entry-array chains and bisection like Rust and libsystemd.
- Live-growth, recovery and position-contract tests pass; Go small-query cost
  near Rust on the SOW-0122 fixture; no regression on full scans.

## Analysis

Pending activation. Likely a design-level change; options will be presented.

## Pre-Implementation Gate

Not started.

## Implications And Decisions

None yet.

## Plan

Pending activation.

## Execution Log

### 2026-11-01

- Created from SOW-0122 measurements.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

None yet.

## Followup

None yet.

## Regression Log

None.
