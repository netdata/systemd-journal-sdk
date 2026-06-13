# SOW-0108 - Node Reader Memory Architecture: Pread Rolling Window And Opt-In Mmap

## Status

Status: open

Sub-state: pending; blocked on SOW-0105 close (one implementation SOW at a
time). User design decisions recorded 2026-06-14 before any code, per the
project decision-recording rule.

## Requirements

### Purpose

Make the Node.js journal reader safe and bounded on real, large journal files.
Today the Node reader loads the entire file into an anonymous-memory Buffer,
which OOMs on real journals (observed: a single live-journal query grew a Node
process to ~49 GB RSS and was OOM-killed). Replace the whole-file read with a
bounded, journal-native random-access memory model, and offer consumers an
optional faster native-mmap backend behind a unified API.

### User Request

2026-06-14, during SOW-0105 validation, the project manager's repeated
comparator runs against the host's live 144 GB `/var/log/journal` OOM-killed
Node wrapper processes (dmesg: PID 1947218 ~18 GB RSS at 23:35; PID 1956815
~49.6 GB RSS at 23:58, both `node-MainThread`). Investigation found the Node
`FileReader.open` does `readFileSync(path)` — whole file into RSS — while
Rust uses a rolling-window mmap (`WindowManager`), and Go and Python use
demand-paged mmap with positioned-read (`ReadAt` / `os.pread`) fallbacks, all
bounded.

The user asked whether Node can mmap cross-platform without native addons
(answer: no — Node core has no mmap, issue nodejs/node#41069 is closed
"not planned"; WASM/WASI mmap is an unmerged proposal that would not give
host-side mmap; native addons are the only route and are forbidden by the
project purity policy). The user then proposed, and approved, a unified
accessor abstraction offering BOTH a pure-JS positioned-read backend (default)
and an optional native mmap backend the consumer can pick — and accepted the
native mmap path as "the only exception to the purity, due to the nature of
the language."

### Assistant Understanding

Facts (verified 2026-06-14 by code reading and web research):

- Node `FileReader.open` (`node/src/lib/reader.js:78-110`) calls
  `safeReadFileSync(path)` -> whole file in an anonymous Buffer; all
  navigation slices that buffer. No paging, no eviction -> RSS == file size.
- Rust: rolling-window mmap. `WindowManager`
  (`rust/src/crates/jf/window_manager/src/lib.rs:114`) maps chunk-aligned
  windows (default 64 KiB, `file.rs:259`), caps `max_windows`, LRU-evicts
  (`find_window_to_evict`). Default `ExperimentalMmapStrategy::Windowed`.
- Go: `ReaderAccessMode` of `ReaderAccessMmap` (default Unix, demand-paged
  `syscall.Mmap`) or `ReaderAccessReadAt` (positioned `file.ReadAt`) -
  `go/journal/reader.go:67-117`, `go/journal/mmap_unix.go`,
  `go/journal/mmap_other.go:68`.
- Python: whole-file `mmap.mmap(fd, 0, ACCESS_READ)` (stdlib, demand-paged,
  `python/journal/reader.py:896`) plus `os.pread`
  (`python/journal/_platform_io.py:16`).
- mmap is demand-paged: a whole-file map costs near-zero RSS until pages are
  touched and the OS evicts under pressure. `readFileSync` is not paged, so
  the whole file becomes resident. That is why only Node OOMs.
- Node has no core mmap and no pure-JS/WASM route. The only native option is a
  compiled addon. The maintained one is `@riaskov/mmap-io` (node-addon-api /
  N-API, ships precompiled `.node` binaries; tested Node 16-22 per its docs -
  Node 24/26, musl, arm64, FreeBSD prebuilt coverage unverified).

Inferences:

- A two-backend accessor mirrors the Go SDK's `ReaderAccessMmap |
  ReaderAccessReadAt` design, so it increases cross-language parity rather than
  inventing a new shape.
- A positioned-read rolling-window cache (pread + bounded LRU windows) gives
  the same bounded-memory property as Rust's mmap `WindowManager`, in pure JS,
  on every platform.

Unknowns (resolve during implementation):

- `@riaskov/mmap-io` prebuilt-binary coverage for Node 26 / musl / arm64 /
  FreeBSD; whether live-file refresh (append) needs remap on the mmap backend.
- Optimal default window size and `max_windows` for the pread backend
  (benchmark; Rust uses 64 KiB chunks; Go bench uses 32 MiB).

### Acceptance Criteria

- The Node reader never loads a whole file into resident memory in the default
  path; a large-file fixture (well beyond any window budget) reads with bounded
  RSS, proven by a regression test that asserts a memory ceiling.
- A single `Accessor` interface backs the reader; both backends return
  byte-identical results, proven by running the full Node conformance and
  interoperability suites in BOTH `Pread` and `Mmap` modes.
- `accessMode` defaults to `Pread` (pure JS). The SDK's default and tested
  runtime path loads no native code.
- The mmap backend dynamically loads a consumer-installed `@riaskov/mmap-io`
  only when `accessMode: Mmap` is selected; mmap-io is never in the SDK's
  dependency tree (not `dependencies`, not `optionalDependencies`). Absent ->
  a clear error (or documented fallback), never a silent default change.
- Live-append refresh works in both backends (one-writer/multi-reader
  contract preserved).
- The purity-exception decision is recorded in `AGENTS.md`, the
  journal-compatibility skill, and consumer docs.
- Rust and Go sources unmodified; Python untouched by this SOW.
- Whole-SOW reviewer batch returns production-grade.

## Analysis

Sources checked:

- `node/src/lib/reader.js`, `node/src/lib/header.js` (existing whole-file read
  and the header-only positioned-read pattern already added in SOW-0105).
- Rust `WindowManager`, Go `reader.go`/`mmap_*.go`, Python `reader.py` for the
  three reference memory models.
- Web research 2026-06-14: nodejs/node#41069 (closed not planned), Node 26
  release notes (no mmap), WASI #304 (mmap proposal only), `@riaskov/mmap-io`.

Risks:

- Dual-mode testing roughly doubles the reader test matrix; CI must gate the
  mmap mode on mmap-io being installable for the runner platform.
- The mmap backend reintroduces a per-platform native-binary surface for
  consumers who opt in; coverage gaps (musl/arm64/FreeBSD/Node 26) are the
  consumer's risk because the default is pure pread.
- Positioned reads are slower than mmap (extra syscalls and copies); the user
  has accepted this for the default path.
- Refactoring the reader's buffer-slicing internals to an accessor interface
  is broad within `node/` and risks reader regressions; mitigated by the
  existing conformance/interop matrices run in both modes.

## Pre-Implementation Gate

Status: blocked

Blocked on: SOW-0105 close (one implementation SOW at a time). Refresh this
gate at activation with current `@riaskov/mmap-io` platform coverage and a
chosen default window size from a benchmark.

Problem / root-cause model:

- The Node reader's whole-file `readFileSync` is unbounded resident memory;
  every other implementation is bounded (mmap demand-paging or positioned
  reads). Small test fixtures hid this until a real large journal was read.

Evidence reviewed:

- Listed in Analysis; dmesg OOM evidence in User Request.

Affected contracts and surfaces:

- `node/src/lib/reader.js` (FileReader internals: open, buffer access, entry/
  data offset reads, live refresh), `node/src/lib/directory-reader.js`,
  a new accessor module, `node/src/lib/header.js` (already positioned-read),
  `node/src/index.js` (ReaderOptions/accessMode export and `.d.ts`),
  `node/test/*`, `node/README.md`, `AGENTS.md`, the journal-compatibility
  skill, consumer docs.

Existing patterns to reuse:

- Rust `WindowManager` (rolling window + LRU eviction) as the design template
  for the pread backend; Go `ReaderAccessMode` as the API-shape template;
  the SOW-0105 `readFileHeader` positioned-read as the read primitive.

Risk and blast radius:

- Node-only; no Rust/Go/Python changes. Reader-internal but wide within node/.

Sensitive data handling plan:

- Synthetic fixtures only; the large-file fixture is generated, not the host
  journal. No comparator runs against `/var/log/journal` in this SOW.

Implementation plan:

1. Define the `Accessor` interface (`bytesAt(offset, len)`, `size()`,
   `refresh()`, `close()`) and route all reader byte access through it.
2. Implement `PreadWindowAccessor` (default): fd + chunk-aligned rolling-window
   LRU cache via `fs.readSync`. Port the Rust WindowManager semantics
   (window size, max_windows, eviction, live-tail refresh).
3. Add `ReaderOptions.accessMode` (`Pread` default, `Mmap`) mirroring Go;
   update exports and `.d.ts`.
4. Implement `MmapAccessor` (opt-in): dynamic import of a consumer-installed
   `@riaskov/mmap-io`; map file/windows; live refresh/remap on append.
5. Wire the directory reader and the zst-decompressed-temp path through the
   accessor (decompressed temp is a real file; positioned reads apply).
6. Add a large-file bounded-RSS regression test and run the full conformance/
   interop suites in both modes.
7. Record the purity exception in AGENTS.md, the compatibility skill, and docs.

Validation plan:

- Bounded-RSS large-file test (assert a ceiling); full Node conformance +
  interop matrices in Pread and Mmap modes; live append/refresh tests in both;
  benchmark pread window sizes; `.agents/sow/audit.sh`.

Artifact impact plan:

- AGENTS.md: record the opt-in-native purity exception and the `accessMode`
  default.
- Runtime project skills: journal-compatibility skill gains the Node memory
  model and the purity exception.
- Specs: note the Node reader access-mode contract if a spec covers reader
  options.
- End-user/operator docs: `node/README.md` documents `accessMode`, the
  consumer-installed mmap option, and the platform caveats.
- SOW-status.md: add to Pending now.

Open-source reference evidence:

- nodejs/node#41069 (closed not planned); WebAssembly/WASI#304 (mmap
  proposal); in-repo Rust/Go/Python readers are the design references.

Open decisions:

- Resolved by the user (see Implications And Decisions). Remaining are tuning
  (window size) and mmap-io platform coverage, resolved at implementation.

## Implications And Decisions

1. 2026-06-14 unified two-backend reader accessor (user decision)
   - Decision: the Node reader exposes a single `Accessor` interface with two
     backends - a pure-JS positioned-read rolling-window (default) and an
     optional native mmap backend - and consumers pick via
     `ReaderOptions.accessMode`, mirroring the Go SDK.
   - Implication: bounded memory by default on any file size; optional mmap
     speed for consumers who want it.
   - Risk: dual-mode test matrix; reader-internal refactor blast radius.

2. 2026-06-14 native mmap as the sole purity exception (user decision)
   - Decision: the optional native mmap backend (`@riaskov/mmap-io`) is the
     ONLY permitted exception to the no-native-addon purity policy, justified
     by the nature of the Node language (no core mmap, no pure-JS/WASM route).
   - Implication: the project's "no native addon loading at runtime" rule is
     evolved to "pure by default; one consumer-opt-in native acceleration
     backend for the Node reader." This must be recorded in AGENTS.md and the
     compatibility skill.
   - Risk: a native surface re-enters the ecosystem; contained because it is
     never an SDK dependency and never the default.

3. 2026-06-14 mmap-io is consumer-installed, never an SDK dependency (user
   decision, from the proposed design the user approved)
   - Decision: `@riaskov/mmap-io` is not in `dependencies` or
     `optionalDependencies`. The SDK dynamically imports it only when
     `accessMode: Mmap` is selected; if absent, the SDK errors clearly (or
     uses a documented fallback), never silently.
   - Implication: the SDK's default install and tested runtime path remain
     100% native-free, satisfying the letter of SOW-0072.
   - Risk: a consumer enabling Mmap on an unsupported platform fails; that is
     the consumer's explicit, informed risk, and pread always works.

4. 2026-06-14 default accessMode is Pread (user decision)
   - Decision: the default reader access mode is the pure-JS positioned-read
     rolling window. Mmap is strictly opt-in.
   - Implication: out-of-the-box behavior is bounded, pure, and universal.

## Plan

1. Accessor interface + reader routing.
2. PreadWindowAccessor (default) + bounded-RSS test.
3. accessMode option + exports/.d.ts.
4. MmapAccessor (opt-in, dynamic import).
5. Dual-mode conformance/interop + live refresh.
6. Artifact/policy updates.
7. Reviews, audit, close.

## Delegation Plan

Implementer:

- Pool implementer per the active routing at activation; Rust/Go frozen,
  Python untouched.

Reviewers:

- The five-model `llm-netdata-cloud` pool, read-only, whole-SOW batches.

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

- Never validate against the host `/var/log/journal` (it OOM'd before the
  fix). Use generated large-file fixtures under `.local/` or `/tmp` with a
  bounded-RSS assertion. Record stalls and rotate implementer models on
  repeated gateway failure, as in SOW-0105.

## Execution Log

### 2026-06-14

- Created from the SOW-0105 validation OOM discovery. Recorded the user's four
  design decisions (unified accessor; native mmap as the sole purity
  exception; mmap-io consumer-installed and never an SDK dependency; Pread
  default) before any code. Blocked on SOW-0105 close.

## Validation

Acceptance criteria evidence:

- Pending implementation.

Tests or equivalent validation:

- Pending implementation.

Real-use evidence:

- Pending implementation.

Reviewer findings:

- Pending implementation.

Same-failure scan:

- Pending implementation.

Sensitive data gate:

- This SOW contains no raw sensitive data; OOM evidence is sanitized PIDs and
  RSS figures only.

Artifact maintenance gate:

- Pending close.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- Small test fixtures hid an architectural memory bug for the entire life of
  the Node reader (since SOW-0054); large-file fixtures with bounded-RSS
  assertions are now mandatory for any file-reading SDK path.

Follow-up mapping:

- Pending implementation.

## Outcome

Pending.

## Lessons Extracted

Pending.

## Followup

None yet.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and
later testing or use found broken behavior. Use a dated `## Regression -
YYYY-MM-DD` heading at the end of the file. Never prepend regression content
above the original SOW narrative.
