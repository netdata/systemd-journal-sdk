# SOW-0108 - Cross-Language Reader Window Accessor Architecture

## Status

Status: in-progress

Sub-state: activated 2026-06-14 after the user explicitly chose to pause
SOW-0105 and prioritize this foundational reader memory architecture work.
Rust phase is first. User design decisions recorded before any code, per the
project decision-recording rule.

## Requirements

### Purpose

Make every journal reader implementation use a bounded random-access window
architecture with one logical API hiding the physical backend: rolling mmap
when available and appropriate, or rolling positioned reads (`pread`/`ReadAt`/
equivalent) when mmap is unavailable, unsupported, explicitly disabled, or not
safe for the target language/runtime.

This SOW exists because reader memory architecture is a correctness and
performance foundation. An error in this layer can corrupt reads, break
row-level data lifetime guarantees, create unbounded memory use, or invalidate
all higher-level APIs such as facade readers, Explorer, Netdata functions, and
journalctl rewrites.

### User Request

2026-06-14: after discovering that the Node.js reader loads whole journal files
into resident memory, the user asked whether Go and Python have the same issue
and what Rust does on Windows. Code inspection showed that the problem is wider
than Node:

- Rust already has a rolling-window mmap manager by default, but has no general
  positioned-read fallback if mmap creation itself is unavailable.
- Go uses whole-file mmap on Unix and whole-file resident `ReadAt` copy on
  non-Unix.
- Python uses whole-file mmap.
- Node.js uses whole-file resident Buffer reads.

The user then approved turning this SOW from a Node-only fix into a
multi-language reader memory architecture SOW. Required implementation sequence:

1. Rust.
2. Go.
3. Python.
4. Node.js.

The user also required a single logical API that hides whether the backend is
mmap or positioned reads, depending on availability.

### Mandatory Reader Window Contract

All languages must converge on the same reader-memory contract:

- Readers use a `WindowAccessor`-style internal abstraction.
- Public options expose the same concepts in idiomatic language form:
  - `Auto`: default. Use rolling mmap where it is supported and selected by the
    implementation policy; fall back to rolling positioned reads when mmap is
    unavailable or unsuitable.
  - `Mmap`: explicit rolling mmap. If mmap cannot be used, fail clearly; do not
    silently change behavior in this explicit mode.
  - `Pread` / `ReadAt`: explicit rolling positioned-read windows.
- Whole-file resident reads are forbidden in production reader paths.
- Whole-file mmap is not the default. It may remain only as an explicit
  experimental/benchmark option where already present, with bounded-window mode
  remaining the production default.
- The bounded memory envelope is:
  `window_size * max_windows + row_arena + fixed reader metadata`.
- Uncompressed DATA returned to consumers should borrow from the active window
  whenever possible.
- Compressed DATA expands into a row-scoped append-only arena.
- Every field/value pointer, slice, memoryview, Buffer view, or equivalent
  returned for the current row remains valid until the next row is fetched or
  the reader is closed.
- Advancing to the next row clears row-scoped pins and compressed arenas.
- Hot paths allocate only for window misses, compressed DATA expansion, and
  explicit owned-result APIs. Repeated allocation during ordinary field
  traversal is a performance bug unless this SOW records evidence and user
  acceptance.
- Live and snapshot bounds semantics must remain explicit:
  - snapshot readers never read beyond the file size captured at open/query
    start;
  - live readers may refresh bounds only at controlled points and must not add
    a metadata syscall to every object access.
- Directory readers must inherit the same per-file accessor behavior; they must
  not bypass it.
- `.journal.zst` whole-file decompression remains a separate compatibility
  path, but the decompressed temporary file must be read through the same
  bounded accessor.

### Assistant Understanding

Facts verified 2026-06-14 by code inspection:

1. Rust has the closest target architecture.
   - `rust/src/crates/journal-core/src/file/mmap.rs:187` defines
     `WindowManager`.
   - `rust/src/crates/journal-core/src/file/mmap.rs:212` constructs it with
     windowed mode by default.
   - `rust/src/journal/src/lib.rs:161` defines the default reader window size
     as 32 MiB.
   - `rust/src/journal/src/lib.rs:176` defaults `ReaderOptions` to live bounds
     and `ExperimentalMmapStrategy::Windowed`.
   - `rust/src/crates/journal-core/src/file/mmap.rs:60` maps through
     `memmap2`; mmap failures propagate instead of falling back to positioned
     read windows.

2. Go is not rolling-window today.
   - `go/journal/reader.go:64` defines `ReaderAccessMode`.
   - `go/journal/reader.go:83` defaults to `ReaderAccessMmap`.
   - `go/journal/mmap_unix.go:134` opens a read-only mapping.
   - `go/journal/mmap_unix.go:143` maps the entire file with
     `syscall.Mmap(fd, 0, int(size), ...)`.
   - `go/journal/mmap_other.go:97` remaps by allocating a byte slice for the
     entire file and reading it with `ReadAt`.

3. Python is not rolling-window today.
   - `python/journal/reader.py:890` opens the file for readonly mapping.
   - `python/journal/reader.py:896` calls `mmap.mmap(fd, 0,
     access=mmap.ACCESS_READ)`, mapping the whole file.

4. Node.js is not rolling-window today and is the worst memory case.
   - `node/src/lib/reader.js:78` opens a journal file.
   - `node/src/lib/reader.js:85` reads decompressed `.journal.zst` temp files
     with `safeReadFileSync`.
   - `node/src/lib/reader.js:87` reads normal journal files with
     `safeReadFileSync`.
   - This creates resident Buffers sized to the whole journal file.

5. Directory readers do not solve the problem; they open per-file readers and
   inherit each language's file-reader memory model.

Inferences:

- Rust should be the design authority because it already has the row pinning,
  row overflow arena, window stats, live/snapshot bounds model, and windowed
  mmap default.
- Go should reuse its existing `ReaderAccessMode` surface but replace both the
  Unix whole-file mmap and non-Unix whole-file copy with a rolling window
  accessor.
- Python and Node need deeper internal reader surgery because their current
  APIs are built around whole-file byte containers.
- The public API should expose concepts, not identical names. Names should be
  idiomatic per language, but options and behavior must be equivalent.

Unknowns to resolve at activation:

- Whether Rust's current `WindowManager<M: MemoryMap>` should become a generic
  `WindowAccessor` over mmap and pread backends, or whether a thinner adapter
  layer should wrap it to minimize risk.
- Default `window_size` and `max_windows` for Go, Python, and Node. Rust's
  current 32 MiB default is the starting point, not automatically final.
- Whether Node.js optional native mmap remains in scope for this SOW or is
  deferred after the pure positioned-read backend is production-grade. Node core
  has no mmap; native addons remain opt-in only if used.

### Acceptance Criteria

- Rust, Go, Python, and Node.js expose equivalent reader access options:
  `Auto`, explicit rolling `Mmap`, and explicit rolling positioned-read where
  each language/runtime can represent them.
- Default behavior in every language is bounded by the window budget and never
  loads a whole journal file into resident memory.
- Explicit `Mmap` mode fails clearly if mmap is unavailable; explicit
  positioned-read mode never attempts mmap.
- `Auto` mode records or exposes which backend was selected so tests and users
  can prove what actually ran.
- Row-level lifetime guarantees are tested in every language:
  data returned while on a row remains valid until the next row is fetched, for
  uncompressed DATA borrowed from windows and compressed DATA stored in a
  row-scoped arena.
- Window eviction never invalidates current-row returned data.
- Live and snapshot bounds behavior is preserved in every language.
- Directory readers and journalctl rewrites use the same accessor-backed file
  readers.
- Large-file bounded-memory tests exist for every language and fail if a reader
  loads or maps/copies the whole file outside the configured budget.
- Each language phase must include thorough implementation-specific tests, not
  only smoke coverage. Tests must cover normal reads, boundary offsets,
  cross-window objects, objects larger than one window, row pinning across
  eviction pressure, compressed DATA row arenas, repeated fields, empty and
  malformed files, historical headers, compact files, sealed/FSS files where
  supported, `.journal.zst` directory inputs, live growth, snapshot bounds,
  invalid offsets, short reads, close-after-error behavior, and directory
  reader integration.
- Cross-platform validation covers Linux locally, macOS on the configured macOS
  host, and Windows on the configured Windows host where practical. FreeBSD is
  source/target checked unless a native runner is available.
- Compatibility matrices pass after each language phase for the affected
  language, including conformance, directory, mixed directory, verification,
  journalctl query, and live reader behavior where applicable.
- Performance benchmarks compare before/after for representative large files,
  including uncompressed, compressed, compact, and real-corpus candidates.
- The mmap path performance must be unaffected within measured noise. If a
  phase changes existing mmap behavior, the phase cannot pass unless benchmarks
  show no regression against the pre-SOW mmap baseline for the same dataset,
  same options, same platform, and same benchmark runner. Any statistically
  meaningful slowdown needs a user decision before proceeding.
- Reviewers must review the complete SOW and changed reader surface. The
  required reviewers are `glm`, `minimax`, `qwen`, `kimi`, `mimo`, and
  `deepseek`. All six must agree the implementation is `PRODUCTION GRADE`
  before completion.

## Analysis

### Root Cause Model

The current readers do not share a single memory-access architecture:

- Rust has a bounded window manager, but it is mmap-specific.
- Go has an access-mode option, but the current implementations are whole-file
  backends.
- Python has no public access-mode option and maps the whole file.
- Node has no public access-mode option and reads the whole file into resident
  memory.

This creates inconsistent performance, inconsistent memory ceilings, and
different failure modes across languages. Higher-level APIs cannot be trusted
to have predictable memory behavior until the lowest-level reader byte access
is unified.

### Intended Architecture

The common architecture is:

```text
public reader API
  -> file reader / directory reader / facade / explorer / journalctl rewrite
    -> WindowAccessor
      -> RollingMmapBackend
      -> RollingPreadBackend
```

The reader logic asks the accessor for byte ranges. The accessor owns:

- file size and bounds policy;
- window alignment;
- window lookup;
- window creation;
- LRU eviction;
- row pinning;
- row-scoped overflow/arena data;
- selected backend stats.

The reader logic must not know whether bytes came from mmap or positioned
reads, except for diagnostics and tests.

### Risk And Blast Radius

This is high-risk work because every reader API depends on this layer:

- low-level file reader;
- directory reader;
- libsystemd-like facade;
- field/unique-value APIs;
- Explorer;
- Netdata function wrapper;
- journalctl rewrites;
- verification helpers.

The work must be phased by language and validated after each phase. A failure
in one language must not block preserving a stable rollback point for previous
languages.

### Security And Runtime Purity

Core reader paths must remain pure file-format code:

- no host identity probing;
- no `/proc` or registry use;
- no subprocesses;
- no shell commands;
- no system journal access;
- no live host journal mutation.

Node.js has no core mmap. If optional native mmap is implemented, it must be:

- opt-in;
- not a default;
- not an SDK dependency unless a later user decision changes that policy;
- dynamically loaded only when selected;
- documented as a native acceleration backend, not a core requirement.

## Pre-Implementation Gate

Status: ready for Rust phase

Activation context: SOW-0105 is paused by explicit user decision. This SOW is
the active implementation SOW. The Rust phase starts first, then Go, Python,
and Node.js.

Problem / root-cause model:

- Reader byte access is not unified. Some implementations use whole-file mmap
  or whole-file resident buffers. This breaks bounded-memory guarantees and
  makes higher-level performance conclusions unreliable.

Evidence reviewed:

- Rust window manager and default options:
  `rust/src/crates/journal-core/src/file/mmap.rs:187`,
  `rust/src/journal/src/lib.rs:161`, `rust/src/journal/src/lib.rs:176`.
- Rust mmap dependency/no fallback:
  `rust/src/crates/journal-core/src/file/mmap.rs:60`.
- Go access modes and whole-file backends:
  `go/journal/reader.go:64`, `go/journal/reader.go:83`,
  `go/journal/mmap_unix.go:143`, `go/journal/mmap_other.go:106`.
- Python whole-file mmap:
  `python/journal/reader.py:890`, `python/journal/reader.py:896`.
- Node whole-file resident Buffer:
  `node/src/lib/reader.js:78`, `node/src/lib/reader.js:85`,
  `node/src/lib/reader.js:87`.

Affected contracts and surfaces:

- Rust: `journal-core` file/mmap layer, public `ReaderOptions`, facade,
  Explorer, directory readers, journalctl rewrite paths.
- Go: `ReaderAccessMode`, `readOnlyMapping`, file reader, directory reader,
  facade, Explorer, journalctl rewrite paths.
- Python: file reader memory model, public reader options, directory reader,
  facade, Explorer, journalctl rewrite paths.
- Node.js: file reader memory model, public reader options/types, directory
  reader, facade, Explorer, journalctl rewrite paths.
- Shared tests and benchmarks for conformance, interoperability, large-file
  memory, row lifetime, live/snapshot bounds, and performance reporting.
- Docs/specs describing reader access modes and hot-path guarantees.

Existing patterns to reuse:

- Rust `WindowManager` for window alignment, row pinning, LRU eviction,
  row-overflow storage, stats, and live/snapshot bounds.
- Go `ReaderAccessMode` as an existing public option shape.
- SOW-0061 row-scoped facade lifetime contract.
- Existing interoperability matrices under `tests/interoperability/`.

Risk and blast radius:

- Very high. This is the foundation under all reader APIs and query tools.
- Mitigation: implement in the required order Rust -> Go -> Python -> Node,
  commit each verified language phase before advancing, and require phase
  validation plus final whole-SOW reviewer consensus.

Sensitive data handling plan:

- Use generated fixtures and sanitized corpus metadata only.
- Do not copy raw host journal content into durable artifacts.
- Real-corpus validation may use local paths read-only, but reports must record
  sanitized aggregate metrics only.

Implementation plan:

1. Rust phase:
   - Refactor or wrap the existing `WindowManager` behind the final
     `WindowAccessor` contract.
   - Add a positioned-read window backend.
   - Preserve rolling mmap as the production default where supported.
   - Add backend selection/stats and explicit mmap/pread/auto options.
   - Prove row-level lifetime, eviction safety, live/snapshot bounds, and
     performance are not regressed.
2. Go phase:
   - Replace whole-file Unix mmap with rolling mmap windows.
   - Replace non-Unix whole-file resident copy with rolling `ReadAt` windows.
   - Preserve public access-mode compatibility while adding missing window
     sizing/stats where needed.
   - Match Rust behavior and benchmark against Rust.
3. Python phase:
   - Replace whole-file mmap with the same rolling accessor model.
   - Add positioned-read fallback.
   - Preserve Python API compatibility while exposing equivalent options.
   - Validate memory ceiling and row lifetime.
4. Node.js phase:
   - Replace whole-file Buffer reads with rolling positioned-read windows.
   - Add optional native mmap only if it can be kept opt-in and cleanly tested;
     otherwise record it as a follow-up before completion.
   - Preserve pure default runtime and TypeScript definitions.
5. Shared validation:
   - Extend tests to assert backend selection, bounded memory, row lifetime,
     eviction safety, and no whole-file resident read.
   - Extend benchmark reports to show backend, window size, max windows,
     mapped/read-window bytes, page/read misses, eviction count, and RSS.

Validation plan:

- Per language:
  - focused unit tests for accessor bounds, window misses, eviction, refresh,
    close, and invalid offsets;
  - row lifetime regression tests for uncompressed borrowed data and compressed
    row-arena data;
  - edge-case tests for cross-window object headers and payloads, objects
    larger than the configured window size, repeated fields in one row,
    malformed object sizes, truncated files, historical headers, compact files,
    sealed files where supported, `.journal.zst` inputs, short positioned
    reads, and cleanup after open/read errors;
  - large sparse-file memory ceiling test;
  - conformance tests;
  - directory and mixed-directory interoperability tests;
  - journalctl query matrix;
  - verification matrix where reader changes affect verification;
  - live reader/writer matrix for live bounds;
  - benchmark before/after on representative large files, with separate mmap
    and positioned-read results where both modes exist.
- Cross language:
  - files read by every language produce identical canonical counts/hashes;
  - directory traversal ordering remains identical;
  - facade, Explorer, Netdata function wrapper, and journalctl rewrites retain
    behavior.
- Cross platform:
  - Linux local validation;
  - macOS validation on the configured macOS host;
  - Windows validation on the configured Windows host;
  - FreeBSD target/source validation unless a native runner is available.
- Review:
  - phase-level read-only reviews are allowed for this SOW because the user
    explicitly marked the layer as critical;
  - final whole-SOW review must run the full reviewer pool and all reviewers
    must vote `PRODUCTION GRADE`;
  - a language phase is not accepted if any reviewer identifies untested edge
    cases in that language's accessor implementation or any unproven mmap
    performance regression risk.

Artifact impact plan:

- `AGENTS.md`: add/adjust reader memory performance contract if implementation
  changes project-wide requirements.
- Runtime project skills: update journal compatibility and orchestration skills
  if the workflow for reader memory/access-mode validation changes.
- Specs: update reader API/access-mode contracts and row lifetime guarantees.
- End-user/operator docs: document reader access modes, default behavior,
  memory envelope, and hot-path implications for Rust, Go, Python, and Node.js.
- SOW lifecycle: update `.agents/sow/SOW-status.md` and root `SOW-status.md`.

Open decisions:

- No user-blocking decisions for the Rust phase.
- Final public option names per language should reuse existing names where
  cleanly possible and use idiomatic peer names otherwise. Any incompatible
  public API change returns for a user decision before implementation.
- Whether Node optional native mmap is implemented in this SOW or split after
  the pure rolling positioned-read backend is complete. This is not blocking
  Rust, Go, or Python.
- Exact default `window_size` and `max_windows` per language are measurement
  decisions. Existing defaults are preserved unless benchmarks justify a change
  and the SOW records the evidence.

## Implications And Decisions

1. 2026-06-14 multi-language scope (user decision)
   - Decision: SOW-0108 is no longer Node-only. It covers Rust, Go, Python,
     and Node.js reader memory architecture.
   - Implication: the same bounded accessor contract applies to every language
     and every reader API.
   - Risk: larger SOW, but less risk of fixing one language while leaving the
     same architectural bug elsewhere.

2. 2026-06-14 implementation order (user decision)
   - Decision: implement in this order: Rust -> Go -> Python -> Node.js.
   - Implication: Rust becomes the reference; Go ports the optimized model;
     Python and Node follow the proven design.
   - Risk: Node's urgent OOM remains pending until the reference architecture
     is correct. This is accepted because a wrong low-level design would be
     worse than a delayed implementation.

3. 2026-06-14 single logical accessor API (user decision)
   - Decision: the reader exposes one logical access model hiding whether the
     backend is rolling mmap or rolling positioned reads.
   - Implication: higher-level APIs do not branch on mmap versus pread.
   - Risk: the accessor abstraction must be extremely lean; abstraction
     overhead in the hot path must be measured and minimized.

4. 2026-06-14 explicit mode semantics (project-manager recommendation pending
   activation confirmation)
   - Recommendation: `Auto` may fall back from mmap to positioned reads at
     open-time only and must expose the selected backend; explicit `Mmap` fails
     if mmap is unavailable; explicit `Pread` never tries mmap.
   - Rationale: production defaults remain robust, while benchmarks and tests
     can force exact behavior.
   - Status: record as proposed unless the user changes it before activation.

## Plan

1. Refresh current code state for the Rust reader.
2. Rust phase: accessor contract + mmap backend preservation + positioned-read
   backend + tests + benchmark + commit.
3. Go phase: rolling mmap/read windows + tests + benchmark + Rust comparison +
   commit.
4. Python phase: rolling mmap/read windows + tests + benchmark + Rust/Go
   comparison + commit.
5. Node.js phase: rolling positioned-read windows, optional mmap decision,
   tests + benchmark + commit.
6. Whole-SOW validation across shared matrices and cross-platform hosts.
7. Reviewer pool review until every reviewer votes `PRODUCTION GRADE`.
8. Update specs, docs, skills, status ledgers, audit, close.

## Delegation Plan

Implementation:

- Use the active routing at activation time. Current project routing says local
  implementation in this repository unless the user explicitly changes it.
- Because this SOW is foundational, each language phase should have its own
  validation checkpoint and rollback commit.

Reviewers:

- Use the current reviewer pool from `AGENTS.md`:
  `llm-netdata-cloud/glm-5.1`,
  `llm-netdata-cloud/kimi-k2.6`,
  `llm-netdata-cloud/mimo-v2.5-pro`,
  `llm-netdata-cloud/qwen3.6-plus`,
  `llm-netdata-cloud/minimax-m3-coder`,
  `llm-netdata-cloud/deepseek-v4-pro`.
- Reviewer short names for this SOW are `glm`, `kimi`, `mimo`, `qwen`,
  `minimax`, and `deepseek`; all six must explicitly answer whether the
  implementation is `PRODUCTION GRADE`.
- Reviewers are read-only.
- Ask reviewers specifically to look for:
  - invalid row lifetime guarantees;
  - stale slices/views after eviction;
  - hidden whole-file reads/maps/copies;
  - live-file race conditions;
  - extra hot-path allocation;
  - missing edge-case tests for the language being reviewed;
  - mmap benchmark regressions or benchmarks that do not isolate mmap mode;
  - accidental host probing or subprocess use;
  - API divergence across languages;
  - benchmark evidence that does not prove the claimed path.

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

## Execution Log

### 2026-06-14

- Original Node-only SOW was created from the SOW-0105 validation OOM discovery.
- User later clarified that Go/Python/Rust fallback architecture must be covered
  too and approved converting SOW-0108 into a multi-language reader window
  accessor SOW.
- Renamed the SOW from Node-only memory architecture to cross-language reader
  window accessor architecture.
- Recorded mandatory implementation order: Rust -> Go -> Python -> Node.js.
- User explicitly chose to pause SOW-0105 and activate SOW-0108 now, and to
  commit the SOW planning before implementation begins.

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

- This SOW contains no raw sensitive data. File paths are repository-local
  source paths or sanitized local validation targets only.

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

- Small test fixtures hid reader-memory architecture risks. Large-file bounded
  memory tests are mandatory for every reader backend, not only Node.js.

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
