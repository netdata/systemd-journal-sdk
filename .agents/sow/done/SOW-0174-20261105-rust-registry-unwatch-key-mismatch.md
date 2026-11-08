# SOW-0174 - Rust Registry Unwatch Leaves Files When The Directory Key Differs

## Status

Status: completed

Sub-state: completed 2026-11-06 with decisions A and 2B.

## Requirements

### Purpose

Make `journal_registry::Registry::unwatch_directory` remove the files its
matching `watch_directory` registered, whatever form the directory string
had.

### User Request

None yet. Found while validating SOW-0173; recorded under the followup rule.

### Assistant Understanding

Facts:

- `unwatch_directory` removes files with `remove_directory(path)`
  (`rust/src/crates/journal-registry/src/registry/mod.rs:69-73`,
  `repository/collection.rs:254`), which drops the repository entry whose
  key equals the given string exactly.
- Repository directory keys come from `File::dir()`
  (`repository/file.rs:437`): the file's parent, or the grandparent when the
  parent is a machine-id directory. That key never has a trailing slash and
  never ends in the machine-id component.
- So the watch string matches the key only for the standard
  `<root>` layout without a trailing slash. Probe on 2026-11-05 (temporary
  test, removed): watching `<tmp>/` (trailing slash), or `<tmp>/<machine-id>`
  directly, registers 1 file, and `unwatch_directory` with the same string
  returns `Ok` and leaves that 1 file registered. Its notify watch is gone,
  so the stale entry is never updated again.
- The behavior predates SOW-0173: `std::path::absolute` returns absolute
  input unchanged, including a trailing slash.
- In-repo callers never call `unwatch_directory` outside tests. The registry
  is Rust-only, so there is no Go parity gap.

Inferences:

- Netdata watches root journal directories without a trailing slash, so it is
  likely unhit; the failure is silent.

### Acceptance Criteria

- `unwatch_directory` forgets every registered file under the directory
  (component prefix), whatever form the watch string had (trailing slash,
  machine-id directory watched directly), except files another remaining
  watch covers. Files on disk are never touched.
- Overlapping watches work in both orders: parent then child, child then
  parent. After either is unwatched, notify events under the remaining
  watch are still applied, and its files stay registered.
- A failed watch or unwatch leaves the registry as it was (watched set,
  notify watches, registered files); no directory can get stuck as watched.
- Unwatching a directory whose notify watch is already gone succeeds.
- Tests fail without the fix and pass with it.

## Analysis

Options (for the user):

1. A: on unwatch, remove every registered file whose path lies under the
   watched directory (path-prefix by components), instead of an exact key.
   Long-term-best: correct for any layout the watch accepted.
2. B: normalize the watch string (drop trailing separators) and keep the
   exact-key removal. Surgical; fixes the trailing-slash case only, not a
   machine-id directory watched directly.
3. C: document that `watch_directory` takes a journal root without a
   trailing slash.

Recommendation: A.

Risks:

- A: overlapping watches (a root and one of its machine-id subdirectories)
  must not remove files the other watch still covers; the plan must define
  that case.

Overlapping watches, probed 2026-11-05 (temporary test, removed), notify
8.2.0 inotify backend:

- `watch_directory(<root>)` then `watch_directory(<root>/<machine-id>)`
  both succeed.
- `unwatch_directory(<root>)` returns `Ok` and also drops the notify watch
  of `<root>/<machine-id>`: a file created there afterwards produced 0
  events in 2 s. Cause: notify keys its watches by path and removes every
  watch under a recursive path (`notify/src/inotify.rs` `remove_watch`, the
  `p.starts_with(&path)` loop).
- `unwatch_directory(<root>/<machine-id>)` then fails with notify "No watch
  was found", and because the error returns before
  `watched_directories.remove`, that directory stays marked as watched: it
  can neither be unwatched nor watched again.

Decision 2 options (for the user):

1. 2A: `watch_directory` refuses a directory inside or containing one that
   is already watched (lexical component prefix on the absolute paths).
   Long-term-best: overlap has no working meaning at the watcher layer.
   Callers that nest watches get an error instead of silent breakage.
2. 2B: support overlap: keep files a remaining watch covers and re-arm the
   remaining notify watches after an unwatch. Works for any caller, at the
   cost of more registry state and tests.
3. 2C: leave overlap undefined and document it. Surgical; the stuck-watch
   failure stays.

Recommendation: 2A. The user chose 2B (2026-11-06).

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- See Assistant Understanding.

Evidence reviewed:

- The code paths above and the 2026-11-05 probe.

Affected contracts and surfaces:

- Rust `journal-registry`: `Registry::watch_directory`,
  `Registry::unwatch_directory`, `Monitor` watch and reset behavior, and
  repository file retention. No public API is added.

Existing patterns to reuse:

- SOW-0173 registry tests (`tests/relative_directory.rs`).

Risk and blast radius:

- Registry unwatch only.

Sensitive data handling plan:

- Synthetic directories only.

Implementation plan:

1. `repository/collection.rs`: `Repository::retain_files(keep)`, dropping
   empty chains and directories; the registry wrapper also prunes metadata.
   Existing public `remove_directory` stays for API stability.
2. `registry/mod.rs`: `armed_directories` beside `watched_directories`;
   `rearm(target)` with remove-before-add and rollback; `reconcile(dir)`;
   `watch_directory` and `unwatch_directory` rebuilt on them, per decision 3.
3. Rustdoc on both methods.
4. Tests in `tests/overlapping_watches.rs` for each acceptance criterion.

Validation plan:

- Registry tests for trailing-slash and machine-id-directory watches, and
  for overlapping watches under option A.

Artifact impact plan:

- AGENTS.md: unaffected.
- Runtime project skills: unaffected.
- Specs: none describe the registry contract.
- End-user/operator docs: registry rustdoc.
- End-user/operator skills: none exist.
- SOW lifecycle: standard.
- SOW-status.md: updated on activation and close.

Open-source reference evidence:

- None.

Open decisions:

- None: A and 2B decided.

## Implications And Decisions

1. 2026-11-05, user: option A. Unwatch removes every registered file under
   the watched directory, whatever form the directory string had. Files on
   disk are never touched; only the registry's in-memory list changes.
2. 2026-11-06, user: 2B. Overlapping watches are supported.
3. Implementation consequences (part of 2B, not new forks):
   - Only outermost watched directories hold a notify watch; nested ones
     are covered by the recursive watch above them, so notify never sees
     overlapping watches on any backend.
   - Watch keys are normalized lexically (absolute, trailing and repeated
     separators dropped), so `<dir>/` and `<dir>` are the same watch.
   - Watch transitions remove notify watches before adding (notify drops
     every watch below a removed path) and roll back on failure; directories
     whose watch changed hands are reconciled against disk (new files added,
     vanished ones forgotten), because notify events in the gap are lost.
   - `watch_directory` arms before scanning, so a file created during the
     scan is not missed; a missing directory now fails with the notify error
     instead of the walk's I/O error.
   - Unwatching a nested directory while its parent's notify watch is armed
     never fails on a missing directory. Unwatching a parent whose nested
     directory vanished from disk fails (cannot re-arm it) and changes
     nothing; unwatching the nested directory first resolves it.
   - Limits: overlap is detected lexically; symlinks or `..` aliases of the
     same directory are not recognized as overlapping.
4. Found during implementation, handled inside 2B's acceptance criteria
   (no failed operation may leave a stuck or broken watch):
   - notify 8.2.0 (inotify) keeps a stale entry after a directory under a
     recursive watch is deleted: its delete handling calls `remove_watch`,
     which removes the `watches` entry, fails `inotify_rm_watch` with
     `EINVAL` (the kernel already dropped it) and returns before removing
     the `paths` entry (`notify/src/inotify.rs` `remove_watch`; error
     swallowed by `.ok()` in `handle_inotify`). Every later unwatch of a
     directory above it fails with `EINVAL` and leaves notify's maps half
     updated. Probe: watch root, delete a subdirectory, unwatch root → error;
     later watch/unwatch pairs of unrelated paths also failed. This is
     independent of overlap: `watch(root)`, a deleted machine directory,
     then `unwatch(root)` failed before this SOW too. No fixed release was
     found (search 2026-11-06) and only 8.2.0 is in the offline cache, so no
     dependency change.
   - Handling: `Monitor` keeps its event sender and can rebuild its
     watcher (`Monitor::reset`, crate-private) without changing the
     caller's receiver. A failed notify removal rebuilds the watcher, arms
     the target set and reconciles every armed directory.
   - Queued events: create and rename-target events for paths outside every
     watched directory are ignored, so events queued before an unwatch do
     not re-register forgotten files.

## Plan

Implemented according to the decisions recorded above; see the Implementation
And Review Plan and the Execution Log.

## Implementation And Review Plan

Implementation:

- Project manager, at activation.

Reviewers:

- Whole-SOW self-review after local validation; tests are the gate. External
  reviewers are not used.

Repository boundary block for every external-agent prompt:

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

- Validation or audit failures are recorded in the Execution Log with
  evidence; the SOW does not advance until they are fixed or a user decision
  is recorded.

## Execution Log

### 2026-11-05

- Created from the SOW-0173 same-failure scan.
- User chose option A. Overlap probe recorded under Analysis; decision 2
  opened.

### 2026-11-06

- User chose 2B. Activated on branch `sow-0174/rust-registry-unwatch`
  (stacked on SOW-0173).
- Implemented `Repository::retain_files`, `armed_directories`, `rearm`,
  `reconcile`, and rebuilt `watch_directory`/`unwatch_directory`.
- First test run: `failed_unwatch_changes_nothing_and_never_sticks` failed
  with notify `EINVAL` on a later watch. Root cause is the notify stale-entry
  bug in decision 4; added the watcher rebuild and the queued-event guard.
- Self-review: a failed scan of a nested watch (no notify change) rebuilt the
  whole watcher needlessly; restore now runs only when the armed set
  changed. A test that relied on a 200 ms sleep now waits for the event.

## Validation

Acceptance criteria evidence (`journal-registry/tests/overlapping_watches.rs`):

- Trailing slash and either spelling: `unwatch_with_trailing_slash_forgets_files`.
- Machine directory watched directly: `unwatch_of_machine_directory_forgets_its_files`.
- Overlap, parent first: `parent_then_child_unwatch_parent_keeps_child_live`
  (child keeps its files and gets live events after the parent goes) and
  `parent_then_child_unwatch_child_keeps_parent_files`.
- Overlap, child first: `child_then_parent_both_orders_stay_live` (both
  subtrees live under the parent; after unwatching the parent only the
  child's files remain and remove events still apply; files on disk remain).
- Failure leaves state unchanged and nothing sticks:
  `failed_unwatch_changes_nothing_and_never_sticks`,
  `failed_watch_changes_nothing`.
- Watch already gone: `unwatch_after_watched_directory_vanished_succeeds`,
  `unwatch_root_after_subdirectory_deleted_succeeds` (notify bug path).
- Queued events: `queued_events_do_not_restore_unwatched_files`.

Tests or equivalent validation:

- With the committed (pre-SOW) `registry/mod.rs`, `monitor.rs` and
  `collection.rs` restored temporarily, 9 of 10 new tests fail;
  `failed_watch_changes_nothing` passes on both (guard). With the change,
  all pass on 5+ consecutive runs.
- SOW-0173 `relative_directory.rs` still passes.
- `cargo test --workspace`: 695 passed, 0 failed.
- `cargo clippy -p systemd-journal-sdk-registry --all-targets`: no warning
  in changed lines (existing `redundant_field_names` in
  `update_time_range` and `items_after_test_module` in `file.rs` remain).
- `rustfmt --edition 2024 --check` clean on changed files.

Real-use evidence:

- The tests drive the real notify inotify watcher and filesystem, the
  runtime path. In-repo `Registry` users (`journal-engine` index example
  and `index_integrity` tests) pass unchanged.

Reviewer findings:

- PR #25 bot review (2026-11-08): T47 and T48 fixed (a watcher rebuild attempts
  every directory, records exactly those armed and returns the first error;
  unit test added), T49 fixed (rescans keep indexed time ranges; test added),
  T50 addressed by documenting that a post-success rescan failure is logged,
  because the requested watch change is already in place, T51 and T52 fixed.
- Self-review of the whole diff; two findings fixed (Execution Log
  2026-11-06). Accepted costs: watch/unwatch hold the write lock while
  scanning (as before); a watcher rebuild rescans every watched directory,
  only on the failure path; overlap detection is lexical (decision 3).

Same-failure scan:

- Other exact-key directory matches in the registry: `remove_directory`
  had the only one; it is no longer used by `Registry` (kept on the
  workspace-internal `Repository` for API stability). Insert paths from
  events were the other way stale state could return; guarded.
- Go has no registry; no parity work.

Sensitive data gate:

- Synthetic machine IDs and temporary directories only.

Artifact maintenance gate:

- AGENTS.md: unaffected.
- Runtime project skills: unaffected.
- Specs: none describe the registry contract; unaffected.
- End-user/operator docs: rustdoc on `watch_directory`,
  `unwatch_directory` and `Monitor::reset`; `docs/` does not cover the
  registry.
- End-user/operator skills: none exist.
- SOW lifecycle: completed, moved to `done/` with the work.
- SOW-status.md: updated (backlog, Pending, Recently Closed).

Specs update:

- None needed.

Project skills update:

- None needed.

End-user/operator docs update:

- Rustdoc only.

End-user/operator skills update:

- None exist.

Lessons:

- See Lessons Extracted.

Follow-up mapping:

- notify stale-entry bug: worked around here; a notify upgrade, when a fixed
  release exists, would make the rebuild path rare but is not required.
  No pending SOW; the workaround is complete.

## Outcome

`Registry` supports overlapping watches in any order, forgets exactly the
files no remaining watch covers on unwatch (any directory spelling), never
leaves a directory stuck after a failed operation, and recovers from the
notify 8.2 stale-entry bug by rebuilding its watcher.

## Lessons Extracted

- Probe the dependency's failure behavior, not only its happy path: the
  notify bug surfaced only in the failure-and-retry test.

## Followup

- None.

## Regression Log

None yet.
