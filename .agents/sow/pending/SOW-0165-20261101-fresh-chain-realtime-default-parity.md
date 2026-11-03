# SOW-0165 - Fresh-Chain Realtime Default Parity

## Status

Status: open

Sub-state: decided 2026-11-03 (option A); not activated. Found while
implementing SOW-0123.

## Requirements

### Purpose

High-level Rust and Go directory writers should treat caller-supplied realtime
the same way on a fresh chain, as the Go/Rust parity rule requires.

### User Request

None yet. Tracked under the follow-up rule from SOW-0123.

### Assistant Understanding

Facts:

- With no fresh-chain seed and no existing tail, Rust starts its realtime
  floor at wall-clock time (`RealtimeClock::new()`,
  `rust/src/crates/journal-log-writer/src/log/startup.rs` `initial_log_clock`),
  so a past caller realtime is clamped forward to now.
- Go starts at zero (`Log.lastRealtime` zero value, `go/journal/log.go`
  `entryOptionsForAppend`), so the same past caller realtime is kept.
- SOW-0123 added the same seed option to both languages and kept both defaults
  unchanged; the difference is documented in `docs/Options-Reference.md`,
  `docs/Rust-API.md`, `docs/Go-API.md` and the product spec.

Unknowns:

- Which default the user wants. Changing either one changes behavior that
  callers see.

### Acceptance Criteria

- For option A or B: both languages use the same unset-seed behavior, with
  tests in both and updated docs and spec.
- For option C: the difference is recorded as an approved parity exception in
  the spec and docs, with the user decision cited.

## Analysis

Options for the user:

- A. Rust adopts Go's behavior (unset seed = zero floor). Past caller realtime
  is kept by default; ingestion callers that relied on clamping to now would
  see past timestamps.
- B. Go adopts Rust's behavior (unset seed = wall-clock floor). Past caller
  realtime is clamped by default; Go event-time callers must set
  `FreshRealtimeSeed(0)`.
- C. Keep the difference and record it as an accepted parity exception.

## Pre-Implementation Gate

Not started. Decision A is recorded; at activation, first check every Netdata
Rust `Log` caller for reliance on the wall-clock floor.

## Implications And Decisions

1. 2026-11-03 (user): option A. With no fresh-chain seed and no existing tail,
   Rust adopts Go's behavior (zero floor), so a past caller realtime is kept
   instead of being moved to now. Reason: silently rewriting a caller's
   timestamp loses information. Condition: before changing the default, check
   Netdata's Rust `Log` callers (`netdata/netdata`, `src/crates/netflow-plugin`)
   for reliance on the current clamp, and record the result here.

## Plan

Pending user decision.

## Execution Log

None.

## Validation

Pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None.

## Regression Log

None.
