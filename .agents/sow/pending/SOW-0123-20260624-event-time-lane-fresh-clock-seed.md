# SOW-0123 - Event-time lane: caller-seedable fresh-journal realtime clock

## Status

Status: open

Sub-state: Project-manager diagnosis and implementation plan updated. External read-only reviewer
verification completed for both the gap analysis and the plan; reviewer refinements are incorporated.

Python/Node were retired in SOW-0116, so only Rust + Go are in scope. The consumer is the Netdata
netflow plugin's cloud VPC flow-log "vpc ingestion" event-time lane, specified in the Netdata repo
SOW `.agents/sow/active/SOW-20260624-netflow-vpc-flow-logs.md` (Decision #4). This SDK change is the
prerequisite that unblocks that lane.

## Requirements

### Purpose

Let the high-level directory journal writers expose a caller-chosen **fresh-chain realtime floor** so
a consumer that writes entries whose **event time is in the past** (and in non-decreasing event-time
order) gets those event times onto the journal `__REALTIME_TIMESTAMP` axis instead of having them
clamped forward to an ingestion-time floor.

This unblocks **event-time-ordered journals**: the Netdata netflow VPC flow-log lane writes cloud
flow records (which arrive minutes late, out of order) into a per-source reorder buffer, sorts them
by event time, and flushes them in order. It needs the journal's realtime axis to equal event time so
the existing realtime query serves event-time queries unchanged.

Rust has the functional fresh-lazy gap. Go already behaves like seed `0` for caller-supplied
fresh-lazy event-time writes, but it lacks an explicit parity knob, tests, and documentation for the
same public contract.

### User Request

The user asked for this SOW so another agent can implement the fresh-chain realtime-clock seed. The
user then asked the project manager to perform an independent gap analysis and implementation plan,
and to have `glm`, `minimax`, `kimi`, `mimo`, `deepseek`, and `qwen` verify both.

### Assistant Understanding

Facts verified against this repository at `944fe5c6662a`:

- Rust `RealtimeClock::new()` seeds `max_seen` from wall-clock `Microseconds::now()`:
  `rust/src/crates/journal-common/src/time.rs:255-259`.
- Rust already has the exact primitive needed for an explicit seed:
  `RealtimeClock::with_initial(initial)` at
  `rust/src/crates/journal-common/src/time.rs:261-268`.
- Rust `RealtimeClock::observe(candidate)` adopts only values strictly greater than the previous
  maximum; otherwise it returns `max_seen + 1`:
  `rust/src/crates/journal-common/src/time.rs:287-300`.
- Rust high-level startup seeds from an existing chain tail when present, else uses
  `RealtimeClock::new()`:
  `rust/src/crates/journal-log-writer/src/log/startup.rs:320-328`. The call site is
  `rust/src/crates/journal-log-writer/src/log/startup.rs:396-405`.
- Rust append honors caller-supplied `entry_realtime_usec`, but still routes it through
  `self.clock.observe(...)`:
  `rust/src/crates/journal-log-writer/src/log/mod.rs:200-207`.
- Rust `Config` is the public high-level writer configuration surface:
  `rust/src/crates/journal-log-writer/src/log/config.rs:107-167`.
- Go high-level `Log` carries `lastRealtime` as an in-memory clamp floor:
  `go/journal/log.go:192-211`.
- Go reopens an existing chain by copying tail realtime into `lastRealtime`:
  `go/journal/log.go:367-380`.
- Go fresh-lazy startup does not initialize `lastRealtime` to wall-clock time; it remains the zero
  value when no tail exists:
  `go/journal/log.go:236-268` and `go/journal/log.go:367-380`.
- Go append preserves a caller-supplied non-zero `RealtimeUsec` or explicitly set zero
  `RealtimeUsecSet`, fills wall-clock time only when the caller omitted realtime, and clamps only
  when the chosen realtime is `<= lastRealtime`:
  `go/journal/log.go:832-839`.
- Go direct-file writer fills wall-clock realtime only when direct writer callers omit
  `EntryOptions.RealtimeUsec`:
  `go/journal/writer.go:444-448`. This SOW targets high-level directory writers, not that low-level
  direct-file omission behavior.
- Rust eager-open creates an active file before any entry exists by peeking an empty timestamp
  override:
  `rust/src/crates/journal-log-writer/src/log/mod.rs:281-284`. The peek uses wall-clock time when no
  entry realtime override exists:
  `rust/src/crates/journal-log-writer/src/log/mod.rs:88-98`.
- Go eager-open similarly calls `entryOptionsForAppend(EntryOptions{})` and `ensureWriter(...)`
  before any entry exists:
  `go/journal/log.go:264-266`. Non-strict active file naming uses entry realtime, falling back to
  wall-clock time when that value is zero:
  `go/journal/log.go:715-733`.

Open-source reference evidence:

- `systemd/systemd @ 926bf653591e`
- `src/libsystemd/sd-journal/journal-file.c:2533-2568`: `journal_file_append_entry()` accepts an
  explicit `dual_timestamp`; it calls `dual_timestamp_now()` only when no timestamp is supplied.
- `src/libsystemd/sd-journal/journal-file.c:2338-2365`: strict-order mode rejects backward realtime
  within the file.
- `src/basic/time-util.c:76-83`: `dual_timestamp_now()` uses current realtime and monotonic clocks.

Diagnosis:

- Rust has the functional gap. A fresh lazy Rust directory writer starts `RealtimeClock.max_seen` at
  wall-clock `now()`. Event-time entries from the past are therefore clamped to `now + k usec`, even
  when the caller supplies valid non-decreasing event timestamps.
- Go does not have the same fresh-lazy functional gap for normal event-time writes. With no existing
  tail, `lastRealtime == 0`, so a caller-supplied Unix microsecond event timestamp greater than zero
  is adopted. The previous wording that Go had the same root cause was incorrect.
- Go still needs an explicit API parity contract because this project promises Rust/Go behavior
  parity for high-level writers. Without a public seed knob and tests, Go cannot express a non-zero
  fresh floor and has no durable contract matching Rust's new behavior.
- Eager-open is a separate caveat. If the writer creates the active file before the first event, the
  writer cannot infer that future event's realtime. Consumers that require active chain filenames to
  include the first event-time floor must use lazy open. Renaming the active file on first append is
  out of scope because it affects live-reader path stability.
- The strict `>` monotonic clamp remains correct and in scope to document, not change. Equal event
  timestamps still become `last + 1 usec`; out-of-order timestamps still clamp forward.

### Acceptance Criteria

- AC1 (Rust fresh seed works): a **fresh lazy Rust** directory journal opened with seed `0`, then fed
  entries whose `entry_realtime_usec` values are in the past and strictly increasing, writes those
  values verbatim to `__REALTIME_TIMESTAMP` instead of clamping to wall-clock `now()`.
- AC2 (Rust default unchanged): with the knob unset, a fresh Rust writer still uses the existing
  wall-clock `RealtimeClock::new()` behavior. Existing Rust writer tests remain green, and a focused
  test proves the default path still clamps a past event-time value forward.
- AC3 (restart tail wins): Rust and Go writers with an existing valid non-zero chain tail use the
  tail realtime as the clamp floor regardless of the configured fresh seed. Reopen tests prove a
  lower configured seed cannot move the axis backward.
- AC4 (monotonicity preserved): Rust and Go still clamp duplicate or out-of-order caller timestamps to
  `last + 1 usec`. Tests document this as intended behavior, not a defect.
- AC5 (public API exposure): the high-level Rust `journal_log_writer::Config` and Go
  `journal.LogConfig` expose an optional fresh-chain realtime seed. Rust keeps `Config::new(...)`
  source-compatible; Go's nil/zero-value config keeps existing behavior.
- AC6 (Go parity contract): Go implements the same optional public seed contract and tests. Go's
  explicit seed `0` test records current fresh-lazy behavior and proves explicit zero is equivalent
  to the nil/unset path for positive caller-supplied event realtime; a non-zero seed test proves Go
  can now express a configured fresh floor and clamps a lower first event.
- AC7 (scope boundaries and open mode): low-level direct-file writers are unchanged except for
  tests/docs if needed to prove they are out of scope. Lazy open with an explicit first event realtime
  uses the first event realtime for the non-strict active filename. Eager open still creates the
  active file before the first event exists, so consumers that need active chain filenames to reflect
  event time must use lazy open.
- AC8 (invalid maximum seed rejected): Rust rejects `u64::MAX` and Go rejects `math.MaxUint64` as
  configured fresh seeds because no strictly greater realtime can be produced from that floor without
  saturation or overflow. Tests cover the validation error.

## Analysis

Sources checked: `rust/src/crates/journal-common/src/time.rs`,
`rust/src/crates/journal-log-writer/src/log/{startup.rs,config.rs,mod.rs}`,
`go/journal/log.go`, `go/journal/writer.go`, docs/API examples, and upstream
`systemd/systemd @ 926bf653591e`. Current state and evidence are in the
Pre-Implementation Gate. Risk is in Risk and blast radius.

## Pre-Implementation Gate

### Problem / root-cause model

- Rust high-level directory writer realtime is strictly non-decreasing through
  `RealtimeClock::observe`. For event-time consumers, the initial `max_seen` value determines whether
  past-dated event times are adopted or clamped.
- Rust fresh lazy startup currently seeds `max_seen = now()`. Past-dated event times are then all
  `< now`, so `observe(event_time)` returns `now + k usec`; the axis carries ingestion order rather
  than event time.
- Go fresh lazy startup currently leaves `lastRealtime == 0` when no chain tail exists, so Go already
  adopts normal positive Unix microsecond event timestamps in that case. The Go work is parity API,
  explicit tests, and non-zero seed support.
- The minimal Rust fix is to let the consumer seed the fresh clock at or below its first event time.
  Seed `0` is sufficient for Unix microsecond event timestamps and self-bootstraps the first
  `observe(event_time)`.

### Evidence reviewed

See the file:line references in Assistant Understanding. Evidence covers Rust, Go, and upstream
systemd behavior.

### Affected contracts and surfaces

- Rust `journal_log_writer::Config` (`rust/src/crates/journal-log-writer/src/log/config.rs:107-167`).
- Rust `initial_log_clock` (`rust/src/crates/journal-log-writer/src/log/startup.rs:320-328`).
- Rust high-level append timestamp path
  (`rust/src/crates/journal-log-writer/src/log/mod.rs:200-207`).
- Go `journal.LogConfig` (`go/journal/log.go:172-187`).
- Go high-level chain startup and append clamp path (`go/journal/log.go:236-268`,
  `go/journal/log.go:367-380`, `go/journal/log.go:832-839`).
- Docs and examples for Rust and Go high-level writer APIs.
- Tests under `rust/src/crates/journal-log-writer/` and `go/journal/`.

### Clean-end-state target

- Rust and Go high-level directory writers expose a documented optional "fresh-chain realtime seed"
  with equivalent semantics.
- Rust default remains unchanged: unset seed means fresh chains still use wall-clock `now()`.
- Go default remains unchanged: unset seed means fresh lazy chains keep current `lastRealtime == 0`
  behavior for caller-supplied realtime, and omitted realtime still uses wall-clock `now()`.
- Existing tail state wins over the fresh seed in both languages.
- No change to readers, query behavior, low-level direct-file writers, monotonic timestamp policy,
  duplicate/out-of-order timestamp clamping, compression, file format, or chain scanning.
- Netdata-side consumer wiring is tracked in the Netdata SOW and is out of scope here.
- Eager-open cannot infer first event time. This SOW records that lazy open is required for event-time
  active filename floors; it does not add active-file rename-on-first-append behavior.

### Existing patterns to reuse

- Reuse Rust `RealtimeClock::with_initial(...)` for the new seed.
- Preserve Rust `Config::new(...)` as the stable construction path and add a builder method matching
  existing `with_*` methods in `log/config.rs`.
- In Go, add a pointer field to `LogConfig` to distinguish "unset" from an explicit seed `0`, matching
  the pointer-field optionality convention used inside `RotationPolicy`, `RetentionPolicy`, and
  `Options` fields such as `MaxFileSize *uint64`, `MaxAge *time.Duration`, and
  `LivePublishEveryEntries *uint64`.
- Preserve the existing chain-tail resume paths before applying any fresh-only seed.

### Risk and blast radius

- **Low-to-medium / additive.** Runtime behavior changes only when callers set the new seed.
- **Rust source compatibility risk:** adding a public field can break downstream struct literals.
  Mitigation: do not change `Config::new(...)`; initialize the field there; provide a builder method;
  update in-repo struct literals if any exist.
- **Go source compatibility risk:** adding a field to `LogConfig` is compatible for keyed literals but
  can break unkeyed composite literals. In-repo uses appear keyed; implementer must grep and record.
- **Semantic risk:** eager-open cannot use first event time because the first event is not available
  yet. Mitigation: document lazy open as the required mode for event-time chain filenames.
- **False-positive scope risk:** Go direct writer has a wall-clock default for omitted realtime, but
  that is not the high-level fresh-chain clamp problem. Do not expand this SOW to low-level direct
  writer API changes without new evidence.
- **Maximum seed risk:** a maximum-value seed cannot produce `last + 1 usec` safely. Rust would
  saturate and Go would risk unsigned wrap if the clamp path advanced from `MaxUint64`. Reject this
  seed value during config validation in both languages.

### Sensitive data handling plan

Public technical content only; no secrets/credentials/customer data. Placeholders if any examples are
added.

### Implementation plan

1. Rust API: add `pub fresh_realtime_seed_usec: Option<u64>` to
   `journal_log_writer::Config`, initialize it to `None` in `Config::new(...)`, and add
   `with_fresh_realtime_seed_usec(seed: u64) -> Self`. Add maximum-seed validation to
   `validate_config` in `rust/src/crates/journal-log-writer/src/log/helpers.rs`.
2. Rust startup: change `initial_log_clock` to receive `&Config`. Existing tail realtime remains
   first priority. If no tail exists and `config.fresh_realtime_seed_usec` is `Some(seed)`, return
   `RealtimeClock::with_initial(Microseconds::new(seed))`; otherwise keep `RealtimeClock::new()`.
3. Rust tests:
   - seed `0` fresh lazy writer adopts strictly increasing past event times verbatim;
   - unset seed fresh lazy writer still clamps a past event-time value forward;
   - non-zero seed `X` adopts caller realtime `Y > X` verbatim;
   - non-zero seed `X` clamps caller realtime `Y <= X` to `X + 1`;
   - existing tail wins over configured seed after reopen;
   - duplicate and out-of-order timestamps still clamp to `last + 1 usec`;
   - `u64::MAX` fresh seed is rejected by config validation before append;
   - seed `0` does not interfere with wall-clock default behavior when caller realtime is omitted.
4. Go API: add `FreshRealtimeSeedUsec *uint64` to high-level `LogConfig`. Add a small helper only if
   it matches local style and improves call-site clarity; otherwise tests can take an address of a
   local `uint64`. Validate that `*FreshRealtimeSeedUsec != math.MaxUint64` in
   `validateNewLogConfig` in `go/journal/log.go`.
5. Go startup: after constructing `l` in `NewLog` and before calling `openExistingChain`, if
   `FreshRealtimeSeedUsec != nil`, initialize `l.lastRealtime = *FreshRealtimeSeedUsec`.
   `applyChainTailState` must continue to overwrite `l.lastRealtime` with the existing tail when a
   tail exists; when no tail exists, its early return preserves the configured fresh seed for lazy
   append and eager-open floor behavior.
6. Go tests:
   - explicit seed `0` fresh lazy writer records current adoption of positive event times and matches
     the nil/unset path;
   - non-zero seed `X` adopts caller realtime `Y > X` verbatim;
   - non-zero seed clamps a lower first event to `seed + 1`;
   - existing tail wins over configured seed after reopen;
   - duplicate and out-of-order timestamps still clamp to `last + 1 usec`;
   - `math.MaxUint64` fresh seed is rejected by config validation before append;
   - seed `0` does not interfere with wall-clock default behavior when caller realtime is omitted.
7. Docs/specs: update `.agents/sow/specs/product-scope.md`, `docs/Rust-API.md`,
   `docs/Go-API.md`, and `docs/Options-Reference.md` with the fresh-chain seed contract, default
   behavior, restart precedence, duplicate/out-of-order behavior, validation of maximum seed values,
   omitted-realtime behavior, and eager-open caveat. Omitted realtime still chooses wall-clock first;
   the seed is a floor, not a replacement timestamp.
8. Reference search: grep and record construction sites for `Config::new(`, `Config {`,
   `LogConfig{`, `LogConfig {`, and `NewLog(`. Update only sites affected by source compatibility or
   docs/tests.

### Validation plan

- Rust focused tests for AC1-AC4.
- Go focused tests for AC3, AC4, and AC6.
- Invalid maximum-seed validation tests in Rust and Go.
- Existing Rust writer suite.
- Existing Go `go test ./...` or the narrowest documented equivalent if full Go suite is not viable.
- Search for shared conformance and cross-language interoperability tests that exercise fresh-journal
  timestamp behavior; update them or record evidence-backed reason none are affected.
- Search `tests/` and `fixtures/` for explicit fresh-journal realtime writes and read-back assertions;
  update or rerun affected tests and record evidence.
- Same-failure search over construction sites and timestamp clamp paths.
- External read-only reviewer pool verifies the gap and implementation plan before implementation
  begins.

### Artifact impact plan

- Specs: update `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: update `docs/Rust-API.md`, `docs/Go-API.md`, and
  `docs/Options-Reference.md` where the high-level directory writer configuration and caller
  realtime behavior are described.
- Project skills: no workflow change expected; confirm at close.
- AGENTS.md: no workflow change expected; confirm at close.
- SOW status summary: update `.agents/sow/SOW-status.md` when SOW state changes.

### Open decisions

- None requiring user input before implementation. The project-manager recommendation is the
  long-term-best API shape in this SOW: Rust optional field plus builder without changing
  `Config::new(...)`; Go pointer field to preserve unset-vs-explicit-zero semantics.

## Implications And Decisions

- **Chosen approach: high-level config seed floor, reusing Rust `with_initial`.** Rationale: smallest
  Rust behavioral fix, no `RealtimeClock` state-model change, self-bootstrapping for seed `0`, and
  default behavior untouched.
- **Go parity approach: explicit optional `LogConfig` seed.** Rationale: Go fresh lazy already adopts
  normal event-time values from zero, but a public optional seed gives Rust/Go the same documented
  contract and lets callers choose a non-zero floor.
- **Eager-open decision: document, do not rename.** Rationale: the first event timestamp is unknown at
  eager-open time; renaming an active file on first append would affect live-reader path stability and
  is outside this SOW.
- **Maximum seed decision: reject.** Rationale: a maximum seed cannot advance to `last + 1 usec`;
  rejecting it during config validation preserves the strict monotonic contract and avoids Go unsigned
  wrap.
- Alternative considered: an explicit "adopt-first observed timestamp" mode on `RealtimeClock`
  (start `max_seen` unset; first `observe` adopts). More invasive (changes `RealtimeClock`'s state
  model) for no functional gain over seeding to `0`. Rejected unless the implementer finds a concrete
  reason the `0` floor is unsafe.

## Plan

1. Get read-only reviewer verification from `glm`, `minimax`, `kimi`, `mimo`, `deepseek`, and `qwen`
   for this diagnosis and plan.
2. Apply reviewer corrections to this SOW before implementation if they find a real gap.
3. Implement Rust high-level seed API/startup behavior and tests.
4. Implement Go high-level parity seed API/startup behavior and tests.
5. Update specs and docs.
6. Run validation, same-failure searches, and external read-only production-grade review after the
   implementation chunk is complete.
7. Close the SOW only after validation, reviewer disposition, artifact gates, and status/directory
   consistency are recorded.

## Delegation Plan

To be executed by the implementing agent the user pairs with. This SOW is self-contained on the SDK side;
the consumer wiring lives in the Netdata repo SOW.

## Execution Log

### 2026-06-24

- SOW drafted from the Netdata netflow VPC SOW reconciliation. Root cause (fresh-journal clock seeds
  to `now()`, clamping past-dated event-time writes) verified in `journal-log-writer/src/log/startup.rs`
  and `journal-common/src/time.rs`. Fix scoped to a default-off `Config` seed reusing `with_initial`.
- Project-manager analysis refreshed before implementation. Correction recorded: Rust has the
  fresh-lazy functional gap; Go already adopts positive caller-supplied event realtime on fresh lazy
  chains because `lastRealtime` starts at zero, but still needs explicit parity API/tests/docs and
  non-zero fresh-floor support.
- External read-only reviewer pool completed. Durable disposition is recorded below; local full logs
  are under `.local/sow0123-reviews/`.

## Reviewer Verification - 2026-06-24

Reviewer pool: `glm`, `minimax`, `kimi`, `mimo`, `deepseek`, and `qwen`, run read-only against this
SOW and the cited Rust/Go code.

- `glm`: approved the Rust gap, Go correction, API plan, source compatibility, and eager-open caveat.
  Findings M1-M5 were wording/test-plan refinements and are incorporated.
- `minimax`: first run completed without a final verdict; rerun completed with `PROCEED TO
  IMPLEMENTATION` recommendation after small clarifications. Clarifications A-E are incorporated:
  exact validation sites, eager/lazy wording, seed-floor tests, and conformance/fixture search.
- `kimi`: approved the diagnosis and plan; suggested documenting maximum-seed behavior and pinning
  the spec file. Incorporated as maximum-seed validation and `.agents/sow/specs/product-scope.md`.
- `mimo`: approved the diagnosis and plan with no required corrections.
- `deepseek`: approved with refinements for conformance/interop validation and explicit Rust
  `&Config` startup plumbing. Incorporated.
- `qwen`: approved after one clarification for Go seed placement where tail state is preserved.
  Incorporated by applying the seed before `openExistingChain`, letting `applyChainTailState` override
  it when a tail exists.

Consensus: all usable reviewer verdicts confirmed the Rust fresh-lazy gap, confirmed Go fresh-lazy
already adopts positive caller-supplied realtime from zero, found no security issues, and judged the
plan source-compatible after the refinements above.

## Validation

Pre-implementation reviewer verification is complete. Implementation validation remains pending.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

- Netdata netflow VPC event-time lane wiring (Netdata repo SOW-20260624-netflow-vpc-flow-logs).

## Regression Log

None.
