# SOW-0166 - Directory Realtime Seek Cost

## Status

Status: open

Sub-state: measured gap from SOW-0122; not activated.

## Requirements

### Purpose

Directory realtime seeks (`--since`, `SeekRealtimeUsec` on directory readers)
should cost about what stock does, independent of how many entries precede the
target.

### User Request

Follow-up of SOW-0122 measurement (performance contract in `AGENTS.md`).

### Assistant Understanding

Facts (SOW-0122, `.local/sow0122/report.md`, 300-file 1.5M-entry directory):

- `--since/--until` 60 s window mid-directory: Rust 3.54 s, Go 3.46 s, stock
  v262 0.21 s. `--since` at the last minute: Rust 17.3 s, Go 5.4 s, stock
  0.16 s (one-off run). Single-file seeks are at parity.

Inferences (unverified):

- Rust `rust/src/journal/src/directory.rs` `fill_candidate` (around lines
  375-392) may step and discard older entries file by file instead of seeking
  each file by realtime or skipping files whose tail is older than the target.
  The Go directory path is not traced yet.

### Acceptance Criteria

- Trace both languages, fix the per-file positioning to use journal-native
  realtime seeks (entry-array bisection) and header head/tail bounds.
- On the SOW-0122 300-file directory fixture and procedure (SOW-0122
  Implications And Decisions: 3 runs, median wall time), the 60 s
  `--since/--until` window and the last-minute `--since` each run in at most
  2x stock's median, or under 0.5 s when stock is below 0.25 s, in both
  languages (proposed gate, agent-chosen; confirm at activation). The query
  matrix and directory matrix still pass.

## Analysis

Pending activation.

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
