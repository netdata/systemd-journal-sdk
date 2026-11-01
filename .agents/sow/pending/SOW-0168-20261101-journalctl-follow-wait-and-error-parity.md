# SOW-0168 - Journalctl Follow Wait And Error Parity

## Status

Status: open

Sub-state: measured gap from SOW-0122; needs a user decision before design.

## Requirements

### Purpose

Make `journalctl --follow` in both CLIs cheap while idle and fast to deliver
new entries, and align remaining stderr wording with stock.

### User Request

Follow-up of SOW-0122 measurement.

### Assistant Understanding

Facts (SOW-0122, `.local/sow0122/report.md`):

- Idle follow CPU: Rust 2.1% (file) / 6.2% (directory), Go 21% / 29.7%,
  stock 0%. New-entry latency: Rust 0.24-0.71 s, Go 0.31-0.81 s, stock
  2-4 ms.
- Both CLIs poll and reopen the whole input each poll (Rust `main.rs`
  follow loop, Go `main.go` follow loop); the initial tail scans the input
  forward to keep the last N entries. Stock waits on inotify.
- Both CLIs prefix errors with `Error:`; stock prints the bare message (for
  example `Using --list-invocations requires a unit...`).

### Acceptance Criteria

- Follow reuses open readers and positions instead of reopening; the initial
  tail uses tail seeks.
- Event-driven waiting where the platform allows it, per the user decision;
  portable polling stays the fallback.
- Error output wording matches stock where the parser-parity manifest covers
  it.

## Analysis

Decision needed (user):

- A. Linux inotify (via the platform C library, which the CLI-only native
  code exception already allows) with polling elsewhere.
- B. Portable polling only, made cheap: reuse readers, check header
  `n_entries`/`tail_entry_seqnum` per poll, back off while idle.
- Recommendation: B first (removes most cost on every platform, no new
  native code), then A if latency must reach stock.

## Pre-Implementation Gate

Not started: user decision first.

## Implications And Decisions

Pending user decision.

## Plan

Pending.

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
