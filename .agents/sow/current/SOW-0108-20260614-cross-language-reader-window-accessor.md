# SOW-0108 - Cross-Language Reader Window Accessor Architecture

## Status

Status: in-progress

Sub-state: activated 2026-06-14 after the user explicitly chose to pause
SOW-0105 and prioritize this foundational reader memory architecture work.
Rust pread work was dropped by user decision after platform evidence showed
Rust already has mmap support on Linux, FreeBSD, macOS, and Windows through
`memmap2`. The current active implementation target is Go. The Go plan passed
the pre-code gate in reviewer round 5: `glm`, `kimi`, `mimo`, `qwen`,
`minimax`, and `deepseek` all voted `READY FOR IMPLEMENTATION`. User design
decisions are recorded before code, per the project decision-recording rule.
Go implementation is now locally implemented and validated on Linux plus
Windows/macOS/FreeBSD target checks. Reviewer round 2 found additional Go
test-coverage blockers and one row-arena contract hardening item; those fixes
are implemented and locally validated. Go production review round 3 passed with
all six reviewers voting `PRODUCTION GRADE`. The Go phase is ready to commit
before advancing to Python.

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
multi-language reader memory architecture SOW. Initial required implementation
sequence was:

1. Rust.
2. Go.
3. Python.
4. Node.js.

The user also required a single logical API that hides whether the backend is
mmap or positioned reads, depending on availability.

2026-06-14 update: after Rust platform support was checked, the user agreed that
Rust should not be refactored to add a positioned-read backend just for symmetry
because `memmap2` supports the four target OS families in scope. Rust remains
the mmap/windowing reference. The first implementation target for this SOW is
therefore Go.

### Mandatory Reader Window Contract

All languages must converge on the same reader-memory contract:

- Readers use a `WindowAccessor`-style internal abstraction.
- Public options expose the same concepts in idiomatic language form where the
  target language/runtime needs them:
  - `Auto`: default. Use rolling mmap on the supported OS families where mmap
    exists and is implemented by that language. If the initial mmap backend
    probe fails before any public data is returned, `Auto` may fall back to
    rolling positioned reads only with a recorded selected backend and fallback
    reason. Do not silently downgrade an mmap-capable production target to a
    slower whole-file copy path after mmap has been selected.
  - `Mmap`: explicit rolling mmap. If mmap cannot be used, fail clearly; do not
    silently change behavior in this explicit mode.
  - `Pread` / `ReadAt`: optional explicit rolling positioned-read windows for
    runtimes without mmap or for existing compatibility/diagnostic surfaces. It
    is not required for languages where rolling mmap covers Linux, FreeBSD,
    macOS, and Windows.
- Whole-file resident reads are forbidden in production reader paths.
- Whole-file mmap is not the default. It may remain only as an explicit
  experimental/benchmark option where already present, with bounded-window mode
  remaining the production default.
- The bounded memory envelope is:
  `window_size * max_windows + max_row_arena_bytes + scratch + fixed reader metadata`.
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
- Journal files are append-only for the reader contract. High-level file and
  directory queries may treat each selected file as the file existed when that
  query started. Query-scoped readers may cache file size, header-derived
  metadata, hash-table metadata, index boundaries, and derived lookup state for
  the lifetime of that query. They must not refresh these structures on every
  object or field access merely because a writer might append more data.
- Live readers are the explicit exception. They may refresh append-visible
  bounds only at controlled live/tail points, and any newly visible data is
  outside the immutable snapshot assumed by an already-running query pass.
- Directory readers must inherit the same per-file accessor behavior; they must
  not bypass it.
- `.journal.zst` whole-file decompression is an SDK repository extension, not
  standard systemd runtime journal behavior. It must mirror Rust behavior in
  every language that supports it: stream-decompress the whole-file zstd input
  into a temporary `.journal`, then read that temporary journal through the same
  bounded accessor as normal files.

### Assistant Understanding

Facts verified 2026-06-14 by code inspection:

1. Rust has the closest target architecture and does not need a pread backend
   for the supported target OS list in this SOW.
   - `rust/src/crates/journal-core/src/file/mmap.rs:187` defines
     `WindowManager`.
   - `rust/src/crates/journal-core/src/file/mmap.rs:212` constructs it with
     windowed mode by default.
   - `rust/src/journal/src/lib.rs:161` defines the default reader window size
     as 32 MiB.
   - `rust/src/journal/src/lib.rs:176` defaults `ReaderOptions` to live bounds
     and `ExperimentalMmapStrategy::Windowed`.
   - `rust/Cargo.toml` depends on `memmap2`.
   - Local dependency source for `memmap2-0.9.10` selects Unix, Windows, or
     stub implementations with `#[cfg_attr(unix, path = "unix.rs")]`,
     `#[cfg_attr(windows, path = "windows.rs")]`, and
     `#[cfg_attr(not(any(unix, windows)), path = "stub.rs")]`.
   - Linux, FreeBSD, and macOS are Go/Rust Unix-family targets; Windows is a
     Windows-family target. All four target OS families therefore have a
     `memmap2` mmap implementation.

2. Go is not rolling-window today and has a misleading explicit mmap mode on
   non-Unix targets.
   - `go/journal/reader.go:64` defines `ReaderAccessMode`.
   - `go/journal/reader.go:67` defines `ReaderAccessReadAt`.
   - `go/journal/reader.go:68` defines `ReaderAccessMmap`.
   - `go/journal/reader.go:83` defaults to `ReaderAccessMmap`.
   - `go/journal/mmap_unix.go:134` opens a read-only mapping.
   - `go/journal/mmap_unix.go:143` maps the entire file with
     `syscall.Mmap(fd, 0, int(size), ...)`.
   - `go/journal/mmap_other.go:97` remaps by allocating a byte slice for the
     entire file and reading it with `ReadAt`.
   - `go/journal/mmap_other.go:1` is built for `!unix`, so Windows explicit
     `ReaderAccessMmap` currently does not mmap; it silently becomes a
  whole-file resident `ReadAt` copy.
   - `go/journal/reader.go:328` routes copied reads through the mapping when
     present.
   - `go/journal/reader.go:336` returns slices from the mapping when present,
     otherwise allocates a new buffer per request.
   - `go/journal/reader_entry.go:19` documents `VisitEntryPayloads` as the
     current DATA payload visitor path.
   - `go/journal/reader_entry.go:111` documents
     `EnumerateEntryPayload` as the libsystemd-style current-row payload path.
   - `go/journal/reader_entry.go:205` reads DATA objects and decompresses when
     needed.
   - `go/journal/directory_reader.go:34` and `go/journal/directory_reader.go:56`
     pass `ReaderOptions` into per-file readers, so directory readers inherit
     the file-reader backend.

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
- Go should reuse and extend its existing `ReaderAccessMode` surface but
  replace both the Unix whole-file mmap and non-Unix whole-file copy with a
  rolling window accessor.
- Go explicit `ReaderAccessMmap` must fail clearly where mmap is not actually
  implemented; it must not silently use a resident whole-file copy.
- Go needs an explicit `Auto` mode so default behavior can choose rolling mmap
  on Unix-family targets and Windows, or a recorded rolling `ReadAt` fallback
  only when the initial mmap backend probe fails before public data is returned.
- Python and Node need deeper internal reader surgery because their current
  APIs are built around whole-file byte containers.
- The public API should expose concepts, not identical names. Names should be
  idiomatic per language, but options and behavior must be equivalent.

Unknowns to resolve at activation:

- Default `window_size` and `max_windows` for Go, Python, and Node. Rust's
  current 32 MiB default is the starting point, not automatically final.
- Whether Node.js optional native mmap remains in scope for this SOW or is
  deferred after the pure positioned-read backend is production-grade. Node core
  has no mmap; native addons remain opt-in only if used.
- Go native Windows mmap is in scope for this SOW. The user decision is to
  replicate Rust where possible; since Rust uses mmap across Linux, FreeBSD,
  macOS, and Windows, Go should implement rolling mmap for Unix-family targets
  and Windows rather than defaulting Windows to positioned reads.
- Whether Go should cache hash tables as query-scoped metadata slices or keep
  them window-backed. The current pre-code recommendation is to cache fixed
  header-derived hash table metadata at reader open/query start only when this
  improves hot-path locality and remains inside the declared memory envelope.

### Acceptance Criteria

- Rust, Go, Python, and Node.js expose equivalent reader access concepts in the
  forms each runtime can safely support. Rolling `Mmap` is the production path
  where supported. Explicit rolling positioned-read remains optional/legacy
  where mmap covers the target OS list, and required only where mmap is not
  available.
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

Status: Go pre-code gate passed; Go implementation may start.

Activation context: SOW-0105 is paused by explicit user decision. This SOW is
the active implementation SOW. Rust mmap-only scope was accepted after evidence
review. Go is the first implementation target for the rolling mmap / rolling
`ReadAt` accessor work.

Problem / root-cause model:

- Reader byte access is not unified. Some implementations use whole-file mmap
  or whole-file resident buffers. This breaks bounded-memory guarantees and
  makes higher-level performance conclusions unreliable.

Evidence reviewed:

- Rust window manager and default options:
  `rust/src/crates/journal-core/src/file/mmap.rs:187`,
  `rust/src/journal/src/lib.rs:161`, `rust/src/journal/src/lib.rs:176`.
- Rust mmap support evidence:
  `rust/Cargo.toml`, local `memmap2-0.9.10` source cfg attributes for Unix and
  Windows.
- Go access modes and whole-file backends:
  `go/journal/reader.go:64`, `go/journal/reader.go:83`,
  `go/journal/mmap_unix.go:143`, `go/journal/mmap_other.go:106`.
- Go reader hot-path and facade surfaces:
  `go/journal/reader.go:328`, `go/journal/reader.go:336`,
  `go/journal/reader_entry.go:19`, `go/journal/reader_entry.go:111`,
  `go/journal/reader_entry.go:205`, `go/journal/facade.go:448`.
- Go directory inheritance:
  `go/journal/directory_reader.go:34`, `go/journal/directory_reader.go:56`,
  `go/journal/directory_reader.go:276`, `go/journal/directory_reader.go:324`.
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
- Mitigation: keep Rust as the reference, implement in the executable order
  Go -> Python -> Node, commit each verified language phase before advancing,
  and require phase validation plus final whole-SOW reviewer consensus.

Sensitive data handling plan:

- Use generated fixtures and sanitized corpus metadata only.
- Do not copy raw host journal content into durable artifacts.
- Real-corpus validation may use local paths read-only, but reports must record
  sanitized aggregate metrics only.

Implementation plan:

1. Rust reference confirmation:
   - No pread implementation in this SOW.
   - Preserve the existing rolling mmap default and whole-file mmap benchmark
     option.
   - Add only documentation/spec clarification if needed: Rust uses mmap for
     Linux, FreeBSD, macOS, and Windows; unsupported targets fail clearly.
   - Keep Rust as the row-level window lifetime reference for Go/Python/Node.
2. Go phase:
   - Introduce an internal reader accessor abstraction for readers only. Do not
     touch writer `mappedArena` except if required to split reader and writer
     types cleanly.
   - Replace Unix whole-file mmap with rolling mmap windows.
   - Replace Windows/non-Unix whole-file resident copy with native rolling mmap
     on Windows, matching Rust's mmap-first architecture.
   - Keep any explicit `ReadAt` mode as a rolling-window compatibility/
     diagnostic backend only where the public API already exposes it or where a
     platform truly has no mmap backend.
   - Add `ReaderAccessAuto` while keeping existing `ReaderAccessReadAt` and
     `ReaderAccessMmap` source-compatible.
   - Make `DefaultReaderOptions()` use `ReaderAccessAuto`.
  - Select rolling mmap for `Auto` on Unix-family targets and Windows. If the
    initial mmap backend probe fails before any public data is returned, `Auto`
    may fall back to rolling `ReadAt` with the selected backend and fallback
    reason exposed in stats. Once `Auto` has selected mmap, later mapping
    failures are surfaced as read errors rather than silently changing backend.
   - Make explicit `ReaderAccessMmap` fail clearly on targets where Go mmap is
     not implemented instead of silently using resident `ReadAt`.
   - Add window size, max windows, selected backend, and accessor stats to
     `ReaderOptions` / diagnostics in idiomatic Go form.
   - Preserve public access-mode compatibility while adding missing window
     sizing/stats where needed.
   - Preserve snapshot semantics by capturing file size/header-derived
     metadata at open/query start. Live mode may refresh only at controlled
     `Refresh()` / tail points, not per object access.
   - Preserve row-level guarantees for `EnumerateEntryPayload` and
     `SdJournalEnumerateAvailableData` by pinning windows that back current-row
     uncompressed payload slices until row advance, seek/file switch, or close.
   - Keep `VisitEntryPayloads` callback-scoped unless a later API change
     explicitly upgrades it; implementation still must not invalidate a slice
     before the callback returns.
   - Store compressed DATA in a row-scoped arena that is cleared only at row
     advance, seek/file switch, or close.
   - Ensure window eviction cannot unmap or overwrite any window backing
     current-row returned slices.
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

Resolved Go implementation decisions:

- Native Windows rolling mmap is in scope now. This follows the user's decision
  that Rust is the reference and Go should not default Windows to a slower
  positioned-read path when mmap is available.
- Go defaults are `ReaderAccessAuto`, 32 MiB windows, and four active windows
  per file reader. These values mirror Rust's window size and keep the first Go
  implementation bounded and measurable. A later benchmark may justify tuning,
  but implementation starts with these explicit values.
- Go default `max_row_arena_bytes` is 256 MiB per reader. This bounds
  compressed DATA expansion and cross-window/oversized row-returned copies
  while remaining far above normal row sizes. If a single row needs more
  row-arena memory, the reader returns a controlled row-arena-limit error
  instead of exhausting process memory. Explicit options may raise the limit for
  trusted workloads, and stats must report row arena peak bytes.
- Go hash-table and index reads remain window-backed in this phase. Do not add
  a separate hash-table metadata cache while replacing whole-file access. A
  cache can be added only after profiling proves window churn and the SOW
  records the memory cost inside the accessor budget.
- Go directory readers keep the current eager-open structure for this phase.
  The memory envelope is explicitly per selected file:
  `selected_files * (window_size * max_windows + max_row_arena_bytes + scratch + fixed metadata)`.
  A shared directory-level window budget is useful future work, but it is not
  required to remove the current whole-file mmap/copy behavior.
- Direct `Refresh()` preserves current-row returned slices. If refresh
  discovers new live bounds, it updates accessor-visible metadata without
  unmapping or overwriting row-pinned windows or row-arena memory. If an error
  occurs, already returned current-row slices still remain valid until row
  advance, seek/file switch, or close.
- Go `.journal.zst` support is fixed in this phase. It must stream-decompress
  into a temporary `.journal`, then open that temporary journal through the
  same bounded accessor. `os.ReadFile` and whole-file `DecodeAll` are forbidden
  for `.journal.zst` reader open.
- `openFileWithOptions` must create the accessor for every access mode through
  one constructor path. The current `if opts.AccessMode == ReaderAccessMmap {
  newReadOnlyMapping(...) }` shape is replaced by a single constructor that
  accepts normalized options, opens/probes the selected backend, records
  `ReaderAccessAuto` fallback if needed, and returns the concrete accessor plus
  selected access stats.
- Node optional native mmap remains a later Node-phase decision. It does not
  block Go or Python planning.

### Go Pre-Code Design Analysis - 2026-06-14

Status: Go plan is ready for the next pre-code reviewer gate. The completed
reviewers were correct to block the first Go plan. The revised plan below
defines the accessor API, build-tag layout, row lifetime state machine,
cross-window fallback, live refresh behavior, Windows mmap requirements, public
options, `.journal.zst` streaming behavior, and validation scope.

Ground-truth facts:

- Current Go `ReaderAccessMmap` on Unix maps the whole file:
  `go/journal/mmap_unix.go:134`, `go/journal/mmap_unix.go:142`,
  `go/journal/mmap_unix.go:168`.
- Current Go `ReaderAccessMmap` on non-Unix is not mmap. It allocates a
  resident whole-file byte slice and reads the entire file:
  `go/journal/mmap_other.go:89`, `go/journal/mmap_other.go:97`,
  `go/journal/mmap_other.go:106`.
- Current Go `ReaderAccessReadAt` avoids whole-file loading but allocates a new
  buffer for every `readSlice` call:
  `go/journal/reader.go:328`, `go/journal/reader.go:336`,
  `go/journal/reader.go:343`.
- Current Go live refresh reads the header and file size directly from the file,
  bypassing the mapping/access path:
  `go/journal/reader.go:391`, `go/journal/reader.go:404`.
- Current Go refresh can replace the mapping and clears current-row state:
  `go/journal/reader.go:430`, `go/journal/reader.go:436`,
  `go/journal/reader.go:437`, `go/journal/reader.go:446`.
- Current Go `.journal.zst` handling reads and decodes the entire compressed
  file in memory before writing a temporary decompressed journal:
  `go/journal/reader.go:258`, `go/journal/reader.go:264`,
  `go/journal/reader.go:274`.
- Current Go directory readers open all selected file readers at construction:
  `go/journal/directory_reader.go:34`, `go/journal/directory_reader.go:40`,
  `go/journal/directory_reader.go:56`.
- Current Go writer `mappedArena` lives in the same platform files as reader
  `readOnlyMapping`, so reader changes can accidentally modify writer behavior:
  `go/journal/mmap_unix.go:12`, `go/journal/mmap_unix.go:18`,
  `go/journal/mmap_other.go:12`, `go/journal/mmap_other.go:17`.
- Rust has the reference semantics:
  - `WindowManager` tracks window count, row pin count, mapped bytes, map count,
    remap count, evictions, and row overflow objects:
    `rust/src/crates/journal-core/src/file/mmap.rs:27`,
    `rust/src/crates/journal-core/src/file/mmap.rs:187`.
  - `get_slice()` is the ordinary internal parse path:
    `rust/src/crates/journal-core/src/file/mmap.rs:755`.
  - `get_row_pinned_slice()` is the row-returned-data path:
    `rust/src/crates/journal-core/src/file/mmap.rs:530`.
  - row pins are cleared before row advance/reset:
    `rust/src/crates/journal-core/src/file/row_view.rs:86`,
    `rust/src/crates/journal-core/src/file/row_view.rs:246`.
  - when pinning would exceed the row pin limit, Rust copies into row overflow
    storage instead of invalidating existing row slices:
    `rust/src/crates/journal-core/src/file/mmap.rs:511`,
    `rust/src/crates/journal-core/src/file/mmap.rs:518`,
    `rust/src/crates/journal-core/src/file/mmap.rs:530`.

Official platform docs checked:

- Go build constraints support file suffixes and `//go:build` constraints, so
  the reader accessor can be split by platform without affecting unrelated
  writer code. Source: `go/build` documentation, Build Constraints.
- Go standard-library source exposes Windows mapping primitives through
  `syscall`: `CreateFileMapping`, `MapViewOfFile`, and `UnmapViewOfFile`.
  Evidence: Go `src/syscall/syscall_windows.go`.
- Go standard-library source exposes Windows constants `PAGE_READONLY` and
  `FILE_MAP_READ`. Evidence: Go `src/syscall/types_windows.go`.
- Go standard-library source has an internal example that converts a Windows
  mapped view address into a Go byte slice with `unsafe.Slice`. Evidence:
  Go `src/internal/fuzz/sys_windows.go`.
- Microsoft documents `MapViewOfFile` as mapping a selected view of a file.
  The high/low file offset must be aligned to system allocation granularity.
  Evidence: Microsoft Learn `MapViewOfFile` documentation.
- Microsoft documents "Creating a View Within a File" as the correct method for
  mapping a subsection of a file: align the view start down to allocation
  granularity, then offset inside the mapped view to reach the requested bytes.
  Evidence: Microsoft Learn "Creating a View Within a File".
- Microsoft documents that `UnmapViewOfFile` invalidates the view address
  range. This confirms why row-pinned Windows views must not be unmapped while
  returned slices can still be used.
- `go doc runtime.KeepAlive` and `go doc unsafe.Slice` were checked for mapped
  view lifetime and slice construction rules. The implementation must keep the
  accessor and mapping handles live for the lifetime of every slice derived from
  a view.

Design principle:

- Go must copy Rust's lifetime model, not Rust's type system.
- Internal parsing uses short-lived accessor slices or copied scratch buffers.
- Only APIs that return borrowed current-row payload slices request row-lifetime
  storage.
- Compressed DATA and cross-window DATA returned to consumers use a row arena.
- The accessor is reader-only. Writer `mappedArena` must be separated or left
  untouched with tests proving writer behavior is unchanged.

Go reader surfaces and required lifetime class:

| Surface | Current behavior | Required accessor lifetime |
|---|---|---|
| `GetEntry()` | returns owned copies | temporary/internal slices only |
| `VisitEntryPayloads()` | callback-scoped payloads; docs forbid retention | temporary/internal slices only; valid until callback returns |
| `CollectEntryPayloads()` | owned copies via visitor | temporary/internal slices only |
| `GetEntryPayload()`, `GetRaw()`, `GetRawValues()` | owned copies | temporary/internal slices only |
| `EntryDataRestart()` + `EnumerateEntryPayload()` | libsystemd-style current-row borrowed payloads | row-lifetime slices |
| `SdJournalEnumerateAvailableData()` | facade current-row borrowed payloads | row-lifetime slices |
| Explorer row scan and filters | internal classification, no borrowed data exposed | temporary/internal slices only |
| Explorer returned rows | currently collected as owned payload copies | temporary/internal slices only, then copy |
| `VisitUnique()` / field enumeration | callback data is not current-row data | temporary/internal slices unless API docs are expanded |

Proposed Go internal accessor API:

```text
readerAccessor:
  kind() ReaderSelectedAccess
  size() uint64
  readAt(dst []byte, offset uint64) error
  tempSlice(offset, size uint64) ([]byte, error)
  rowSlice(offset, size uint64) ([]byte, error)
  clearRow() error
  snapshotVisibleBounds() readerAccessorVisibleSnapshot
  restoreVisibleBounds(snapshot readerAccessorVisibleSnapshot)
  refreshVisibleBounds() (header journalHeader, changed bool, size uint64, error)
  stats() ReaderAccessStats
  close() error
```

Semantics:

- `tempSlice()` is for parsing and callback-scoped reads. It may return a
  borrowed window slice if the range fits one window, or a reusable scratch copy
  if the range crosses windows, exceeds one window, or no evictable window is
  available.
- `VisitEntryPayloads()` uses `tempSlice()` and preserves the existing
  per-callback lifetime: the payload slice is valid until that callback returns,
  not until the outer visitor loop completes. The implementation may evict or
  reuse the temporary window/scratch after the callback returns. The public docs
  must state this clearly.
- `rowSlice()` is for data returned by `EnumerateEntryPayload()` and facade
  enumeration. It returns a borrowed slice only when the range fits one window
  that can be row-pinned. If not, it copies the range into the row arena.
- DATA payload routing is split by lifetime class:
  - `readDataPayloadTemp(offset)` reads the DATA header and payload through
    `tempSlice()`. It calls the existing pure `decompressDataPayload(flag,
    payload)` helper when needed. Uncompressed output may be the temporary
    slice; compressed output may be a temporary allocation. Callers that return
    owned data clone it after this call.
  - `readDataPayloadRow(offset)` reads the DATA header through `tempSlice()`.
    For uncompressed DATA, it reads the payload through `rowSlice()`, so the
    returned slice is window-pinned or row-arena-backed until the row is left.
    For compressed DATA, it reads compressed bytes through `tempSlice()`, calls
    the existing pure `decompressDataPayload(flag, payload)` helper, appends the
    decompressed bytes into the row arena with `max_row_arena_bytes` accounting,
    and returns the arena-backed slice.
  - The existing `decompressDataPayload` function remains pure and does not
    know about accessors or arenas. Lifetime ownership is handled by the caller.
  - `VisitEntryPayloads`, `GetEntry`, `GetEntryPayload`, `GetRaw`,
    `GetRawValues`, Explorer, field/unique APIs, and verification paths use the
    temp variant unless they explicitly return owned copies.
  - `EnumerateEntryPayload` and `SdJournalEnumerateAvailableData` use the row
    variant.
- `clearRow()` unpins row windows and clears the row arena only when the current
  row is left: `Next`, `Previous`, seek operations, `setCurrentEntryOffset`
  when it changes the row, directory file switches, and `Close`.
- `EntryDataRestart()` and DATA enumeration reset operations must not call
  `clearRow()` while staying on the same row. They may reset enumeration
  cursors, but row-returned slices from the same row remain valid until the row
  is left.
- `snapshotVisibleBounds()` and `restoreVisibleBounds()` capture and restore
  only the accessor's logical visible file-size/header bounds used by snapshot
  and live reads. They do not copy, clear, or restore row-pinned windows or row
  arena memory.
- `refreshVisibleBounds()` reads the current file size and header through the
  accessor/file descriptor path only at explicit live refresh points. It must
  not invalidate current-row slices. It may map additional windows for newly
  visible file regions, but it must preserve all row-pinned views and row-arena
  allocations until the row is left.
- `stats()` must expose selected backend, file size, window size, max windows,
  current windows, row pins, row overflow objects/bytes, mapped/read buffer
  bytes, map/read count, eviction count, temp-copy count, and fallback count.

Proposed Go backend split:

- `reader_access.go`: common interface, options, stats, row arena, constants.
- `reader_access_mmap_unix.go`: rolling mmap windows for Unix targets only.
- `reader_access_mmap_windows.go`: rolling mmap windows for Windows.
- `reader_access_readat.go`: rolling positioned-read windows for explicit
  legacy/diagnostic use or unsupported mmap targets.
- `reader_access_mmap_unsupported.go`: explicit mmap unsupported constructor
  for targets where the Go SDK has no mmap backend.
- Existing writer arena code must either remain in existing files untouched or
  move to writer-named files in a mechanical split. Any move must be
  behavior-preserving and validated by writer tests/benchmarks.

Concrete Go file/build-tag contract:

- Keep writer `mappedArena` behavior unchanged. Reader work must not change the
  writer arena API, writer growth, or writer mmap/read/write paths unless a
  compile split forces a mechanical move with no behavior changes.
- Remove reader-only `readOnlyMapping` responsibility from `mmap_unix.go` and
  `mmap_other.go`; those files should stop deciding reader behavior.
- `reader_access.go` owns `ReaderAccessMode`, `ReaderOptions` normalization,
  `ReaderAccessStats`, the `readerAccessor` interface, row arena, and common
  bounds helpers.
- `reader_access_mmap_unix.go` uses `//go:build unix` and maps aligned windows
  with `syscall.Mmap`. Window bases are aligned down to the OS page size.
- `reader_access_mmap_windows.go` uses `//go:build windows` and maps aligned
  windows with `CreateFileMapping` plus `MapViewOfFile`. Window bases are
  aligned down to the Windows allocation granularity obtained from
  `GetSystemInfo`, not merely the page size.
- `reader_access_mmap_unsupported.go` uses `//go:build !unix && !windows` and
  provides a clear unsupported-mmap constructor for explicit `ReaderAccessMmap`.
- `reader_access_readat.go` is platform-independent and implements explicit
  rolling positioned-read windows. It reuses fixed window buffers and must not
  allocate a fresh buffer on every `readSlice`.
- `reader.go` must call the accessor for all reader byte access after the
  initial file open/options validation. Direct `file.ReadAt` calls in
  `readSlice`, `readCurrentHeader`, live refresh, and DATA/header helpers must
  be removed or routed through the accessor.
- `openFileWithOptions` replaces the current mapping-specific branch with one
  accessor constructor call for `ReaderAccessAuto`, `ReaderAccessMmap`, and
  `ReaderAccessReadAt`. `ReadAt` must not keep the old per-call allocation
  behavior; it is a rolling-window accessor backend.
- Accessor constructor shape:
  `newReaderAccessor(file *os.File, opts ReaderOptions) (readerAccessor, ReaderAccessStats, error)`.
  The constructor owns backend selection, initial file-size capture, initial
  mmap probe, fallback cleanup, selected backend stats, and fallback reason.
  `openFileWithOptions` parses the journal header only after this accessor is
  constructed, by reading the header through `accessor.readAt()`.
- `ReaderAccessAuto` mmap probe:
  - The probe maps the first real rolling window: `min(file_size, window_size)`
    bytes, aligned according to the backend. This becomes the first cached
    window if mmap succeeds. For files smaller than one window, the probe maps
    the whole visible file.
  - If the probe fails before any public data is returned, the constructor
    closes and discards all partial mmap state, constructs the rolling `ReadAt`
    accessor, and records selected backend plus fallback reason in stats.
  - If explicit `ReaderAccessMmap` probe fails, the constructor closes partial
    mmap state and returns a clear error.
  - `loadEntryArray` and all header/object reads after open use the accessor
    returned by this constructor; they must not run on a temporary
    pre-accessor file-read path.
- The `Reader` struct removes `mapping *readOnlyMapping` and replaces it with
  `accessor readerAccessor`. `Reader.Close()` calls `accessor.close()`.
  `Reader.readAt()` delegates to `accessor.readAt()`. `Reader.readSlice()` is
  removed or becomes an internal `tempSlice()` wrapper that delegates to the
  accessor; row-returned APIs must not call it.
- `readerRefreshSnapshot` removes `mapping *readOnlyMapping`. It stores only
  logical reader state that may need rollback: header, entry offsets, current
  index, visible file size, and an accessor visible-bounds snapshot. It does
  not copy or reset row-pinned windows or row arena.
- Benchmark support files are part of the Go phase:
  `go/internal/testcmd/reader_core_bench/main.go` and
  `tests/benchmarks/run_reader_core_benchmarks.py` must understand the new
  `auto`, rolling mmap, explicit `read-at`, window size, max windows, selected
  backend, and accessor stats so the no-regression benchmark evidence proves
  the intended path.

Windows mmap details:

- The implementation must create one read-only file mapping handle per reader
  and map/unmap views per accessor window.
- File offsets passed to `MapViewOfFile` must be split into high/low DWORDs and
  aligned to allocation granularity. The returned Go slice begins at
  `mapped_view[requested_offset - aligned_base:]`.
- `unsafe.Slice` is acceptable only inside the Windows mmap backend, where the
  view lifetime is owned by the accessor. The accessor must keep mapping handles
  and view records live until the corresponding window is evicted or row-pinned
  state is cleared after the row is left.
- Each Windows mmap window struct must store the exact `uintptr` returned by
  `MapViewOfFile`, the aligned base, mapped length, logical slice, row-pin
  state, and LRU state. The accessor's live window list and row-pin list are the
  owners keeping that window struct live. `UnmapViewOfFile` uses the stored
  `uintptr` from the window being evicted, never a derived slice pointer.
- `UnmapViewOfFile` must never run on a row-pinned view. This is mandatory
  because unmapping invalidates the virtual address range used by returned Go
  slices.
- `ReaderAccessReadAt` remains available on Windows only when the caller asks
  for it explicitly. It is not the default Windows production path.

Proposed Go public options:

- Add `ReaderAccessAuto` without changing the existing integer values. Since
  `ReaderAccessMode` values are public, changing existing values may be a
  breaking change. Recommended shape: keep `ReaderAccessReadAt = 0`, keep
  `ReaderAccessMmap = 1`, add `ReaderAccessAuto = 2`, and make
  `DefaultReaderOptions()` explicitly select `ReaderAccessAuto`.
- Keep `WithMmap(true)` as explicit mmap, not auto.
- Keep `WithMmap(false)` as explicit `ReadAt`.
- Add `WithWindowSize(bytes uint64)` and `WithMaxWindows(count int)`.
- Add `WithMaxRowArenaBytes(bytes uint64)` and report
  `RowArenaBytes`, `RowArenaPeakBytes`, and `RowArenaLimitBytes` in accessor
  stats.
- Add `SelectedAccessMode()` or equivalent diagnostics so tests and callers can
  prove whether `Auto` selected mmap or read windows.
- `ReaderOptions.normalized()` must not collapse unknown/non-mmap modes into
  `ReadAt`; it must validate known modes and preserve `Auto`.
- Update Go public doc strings:
  - `VisitEntryPayloads()` slices are valid only until the callback returns.
  - `EnumerateEntryPayload()` and `SdJournalEnumerateAvailableData()` slices
    are valid until the reader advances, seeks, switches files, or closes.
    DATA restart/exhaustion and direct `Refresh()` do not invalidate them while
    staying on the same row.

Recommended Go backend policy:

- On Unix-family targets: `Auto` selects rolling mmap.
- On Windows: `Auto` selects native rolling mmap.
- On Unix-family targets and Windows, `Auto` probes rolling mmap at open by
  mapping the initial header/window. If that initial probe fails before public
  data is returned, `Auto` may fall back to rolling `ReadAt` and must record the
  fallback reason in `ReaderAccessStats`. If the mmap probe succeeds, the
  backend is fixed to mmap for the reader lifetime and later map failures return
  errors.
- On other targets without an implemented mmap backend, `Auto` may select
  rolling `ReadAt` only if that target remains in supported scope; otherwise it
  should fail clearly.
- Explicit `Mmap` on targets without an implemented Go mmap backend fails at
  open with a clear unsupported-mode error.
- Native Windows mmap is part of the Go phase because Rust is the reference and
  Rust already has mmap support on Windows through `memmap2`.

Window and row memory model:

- Default starting point: `window_size = 32 MiB`, matching Rust.
- Default starting point: `max_windows = 4`, subject to benchmark adjustment.
- Memory envelope per reader:
  `window_size * max_windows + max_row_arena_bytes + scratch + fixed metadata`.
- Row-pinned windows count against `max_windows`. If a new row slice cannot be
  pinned without invalidating an older row slice, copy the requested range into
  the row arena.
- If a temporary parse read needs a new window while all windows are row-pinned,
  use scratch/copy for that temporary read instead of exceeding the window
  budget.
- Rolling `ReadAt` windows follow the same pinning discipline as mmap windows.
  A `rowSlice()` from a `ReadAt` backend pins the window buffer until the row is
  left. A later `tempSlice()` must not reuse or overwrite a row-pinned `ReadAt`
  buffer. If no unpinned window buffer is available, `tempSlice()` uses scratch
  copy memory and records the fallback in stats.
- If a requested range crosses a window boundary or is larger than one window:
  - temporary/internal path uses scratch/copy;
  - row-returned path uses row arena;
  - no public borrowed slice may point to memory that needs two windows to stay
    valid unless the implementation explicitly proves both windows are pinned
    inside the budget.
- Row-arena append checks the configured `max_row_arena_bytes` before copying.
  Crossing the limit returns a controlled reader error and leaves previously
  returned current-row slices valid until the row is left.

Live and snapshot model:

- Snapshot mode captures file size and header-derived boundaries at open/query
  start and never extends them.
- Live mode may refresh at `Refresh()`, `Next()` after tail, and realtime seek
  refresh points already present in the code. It must not add file stat/header
  reads to normal object access.
- `readCurrentHeader()` should be folded into the accessor refresh path so live
  header and file-size refresh is centralized.
- Current-row returned slices must remain valid until the row is left. A direct
  `Refresh()` while on a row preserves row pins and row arena. If implementation
  analysis finds this impossible in Go, the work must stop for a user decision
  before code changes weaken the contract.
- `refreshEntryOffsets()` must remove the current success-path call to
  `clearCurrentEntryState()` when serving direct `Refresh()` on the same row.
  It must not call accessor `clearRow()` while the current row is still active.
- The rolling accessor is not replaced wholesale during direct refresh. Refresh
  updates visible file size/header-derived bounds and reloads entry-array state
  through the accessor. Existing row-pinned windows and row-arena allocations
  remain owned by the accessor until the row is left.
- Direct refresh sequence:
  1. Snapshot logical reader state and accessor visible bounds.
  2. Call accessor refresh to read current header/size through the accessor path
     and update provisional visible bounds.
  3. Reload entry-array state into temporary local data.
  4. On success, commit header, visible size, entry offsets, and current index
     without clearing row pins or row arena.
  5. On failure, restore the prior header, visible size, entry offsets, current
     index, and accessor visible bounds; row pins and row arena remain untouched.
- Internal refreshes that happen after tail or as part of seeking may leave the
  row first and then call `clearRow()`. The code path must make that explicit;
  direct `Refresh()` must not use that row-clearing path.

Directory-reader memory model:

- Current `DirectoryReader` opens every selected file reader eagerly. With
  bounded per-file windows, the worst-case resident/mapped budget is still:
  `selected_files * window_size * max_windows`.
- For this Go phase, the minimum acceptable improvement is that no reader maps
  or copies a whole file. However, large directories still need a documented
  budget.
- Recommended first implementation: keep eager open, but expose stats and a
  `ReaderOptions` budget so directory benchmarks can measure real memory.
- Required follow-up decision before calling the directory design final:
  whether to add a directory-level shared window budget / lazy file reader
  opening. This is not needed to fix the whole-file mapping bug, but it may be
  needed for very large journal directories.

`.journal.zst` model:

- The current Go path is an explicit whole-file memory violation.
- User decision: Go must do the same as Rust for this SDK extension.
- Implementation inside this SOW: stream-decompress `.journal.zst` into the
  existing temporary journal file path, then read the decompressed temp file
  through the same accessor.
- `.journal.zst` must not remain a whole-file memory exception in Go.

Tests that must exist before the Go phase can pass:

- `ReaderAccessAuto` selects rolling mmap on Unix-family targets and Windows.
  Cross-compile, injected backend tests, or native Windows tests must verify
  both platform families.
- `ReaderAccessAuto` fallback is tested by forcing the initial mmap probe to
  fail before public data is returned. The reader must select rolling `ReadAt`,
  expose selected backend plus fallback reason, and still pass read correctness.
  A later mmap failure after mmap has been selected must return an error, not
  silently switch backend.
- Explicit `ReaderAccessMmap` fails on targets where no Go mmap backend exists.
- Explicit `ReaderAccessReadAt` never maps.
- A small-window test forces multiple windows and proves correct reads.
- A row-lifetime test keeps slices from `EnumerateEntryPayload()`, forces window
  pressure by reading other offsets in the same row, and verifies retained
  slices remain valid until row advance.
- The same row-lifetime test must run under explicit `ReaderAccessReadAt` and
  prove row-pinned `ReadAt` window buffers are not overwritten by later
  temporary reads.
- A row-advance test verifies old slices are no longer guaranteed after `Next`
  or `Previous`; tests must not rely on unsafe use after invalidation, but stats
  must show row pins cleared.
- A DATA restart/exhaustion test verifies `EntryDataRestart()`,
  `ClearEntryDataState()`, and end-of-enumeration state changes do not clear
  row-pinned windows or row arena while staying on the same row.
- A cross-window DATA object test proves row-returned data is copied into row
  arena and remains valid.
- An object-larger-than-window test proves the same arena fallback.
- A compressed DATA test proves decompressed payloads are row-arena-backed for
  row-returned APIs and temporary/scratch-backed for internal parse paths.
- A row-arena-limit hostile-file test creates a row that would exceed
  `max_row_arena_bytes` through compressed DATA or cross-window row copies and
  proves the reader returns the controlled limit error without invalidating
  prior current-row slices.
- A live refresh test proves refresh after append sees new rows without
  invalidating current-row slices.
- A direct `Refresh()` failure-path test forces entry-array reload/header
  refresh failure and proves previous visible state and current-row returned
  slices remain valid.
- A `VisitEntryPayloads()` callback lifetime test proves a slice remains valid
  until its callback returns and that callers must copy before retaining beyond
  the callback.
- A snapshot bounds test proves appended rows are invisible.
- A directory memory test opens many files and proves no whole-file file copy or
  mapping occurs.
- A `.journal.zst` test proves streaming temp-file decode, not whole-file
  `os.ReadFile` + `DecodeAll`.
- Compact, sealed/FSS, historical unkeyed, and compressed DATA fixtures must
  continue to pass existing matrices.
- Benchmarks must compare pre-SOW Go whole-file mmap vs post-SOW Go rolling
  mmap on large files and report selected backend/stats. Any meaningful Unix
  mmap slowdown requires a user decision.

Recorded Go decisions before code:

1. Windows Go mmap scope.
   - Decision: implement native Windows rolling mmap now, using Rust as the
     behavior reference.
   - Implication: Go Windows uses the same mmap-first production model as Rust,
     rather than a slower positioned-read default.
   - Risk: Windows mapping APIs add handle lifetime, allocation-granularity, and
     view-lifetime complexity. This must be covered by focused tests and
     reviewer scrutiny.
2. Directory memory budget.
   - Decision: keep eager directory open for the Go phase and record per-file
     budget/stats.
   - Implication: this removes the current whole-file map/copy bug without
     redesigning directory traversal.
   - Risk: large directories can still multiply the per-file window budget.
     This risk is explicit and measurable through directory stats; a shared
     directory-level window budget can be a later SOW if real measurements show
     it is needed.
3. Direct `Refresh()` row-lifetime contract.
   - Decision: preserve current-row returned slices across refresh.
   - Implication: refresh is a live-boundary update, not a row-lifetime reset.
   - Risk: implementation is more complex, but weakening the row-level
     guarantee would violate the user contract.
4. `.journal.zst` scope.
   - Decision: fix `.journal.zst` streaming in the Go phase.
   - Rationale: Rust already supports `.journal.zst` as an SDK repository
     extension by streaming decompression to a temporary journal and opening the
     temp file normally. Go should do the same.
   - Implication: Go must remove the current whole-file compressed/decompressed
     in-memory path for `.journal.zst`.

## Implications And Decisions

1. 2026-06-14 multi-language scope (user decision)
   - Decision: SOW-0108 is no longer Node-only. It covers Rust, Go, Python,
     and Node.js reader memory architecture.
   - Implication: the same bounded accessor contract applies to every language
     and every reader API.
   - Risk: larger SOW, but less risk of fixing one language while leaving the
     same architectural bug elsewhere.

2. 2026-06-14 implementation order (user decision, revised after Rust platform
   evidence)
   - Initial decision: implement in this order: Rust -> Go -> Python ->
     Node.js.
   - Revised decision: do not add Rust pread. Keep Rust's existing rolling mmap
     implementation as the reference and implement the new bounded accessor
     work first in Go, then Python, then Node.js.
   - Implication: Rust remains the row-lifetime and windowing reference; Go
     ports the optimized model without forcing a risky Rust refactor that no
     supported target currently needs.
   - Risk: Node's urgent OOM remains pending until the reference architecture
     is correct. This is accepted because a wrong low-level design would be
     worse than a delayed implementation.

3. 2026-06-14 single logical accessor API (user decision)
   - Decision: the reader exposes one logical access model hiding whether the
     backend is rolling mmap or rolling positioned reads.
   - Implication: higher-level APIs do not branch on mmap versus pread.
   - Risk: the accessor abstraction must be extremely lean; abstraction
     overhead in the hot path must be measured and minimized.

4. 2026-06-14 explicit mode semantics (user decision)
   - Decision: `Auto` may fall back from mmap to positioned reads only on
     targets where mmap is not implemented or not available, at open-time only,
     and must expose the selected backend; explicit `Mmap` fails if mmap is
     unavailable; explicit `Pread` never tries mmap.
   - Rationale: production defaults remain robust, while benchmarks and tests
     can force exact behavior.
   - Status: accepted for the Go/Python/Node accessor work. Rust keeps existing
     mmap strategy options because Rust pread is not in this SOW.

5. 2026-06-14 `.journal.zst` Go behavior (user decision)
   - Decision: Go must mirror Rust for `.journal.zst`.
   - Evidence: upstream systemd treats only `.journal` and `.journal~` as
     standard runtime journal files, while Rust supports `.journal.zst` as a
     repository extension by stream-decompressing to a temporary `.journal`.
   - Implication: Go keeps `.journal.zst` support for SDK/test/archive
     compatibility, but implements it as bounded streaming decompression to a
     temp journal followed by normal bounded reader access. Go must not
     `os.ReadFile` the compressed input or `DecodeAll` the whole decompressed
     journal into memory.

6. 2026-06-14 Go Windows mmap behavior (user decision)
   - Decision: Go should replicate Rust's mmap-first reader architecture on
     Windows too.
   - Evidence: Rust uses `memmap2`, whose supported implementations include
     Unix and Windows. The earlier Go plan to make Windows `Auto` use
     positioned reads would make Go slower than necessary on a supported target.
   - Implication: the Go phase must include a native Windows rolling mmap
     backend. `ReaderAccessReadAt` may remain only as an explicit legacy/
     diagnostic path or unsupported-target fallback, not as the Windows
     production default.

## Plan

1. Rust reference confirmation: keep existing rolling mmap architecture;
   document that no pread backend is required for Linux, FreeBSD, macOS, and
   Windows in this SOW.
2. Go phase: rolling mmap/read windows + tests + benchmark + Rust comparison +
   commit.
3. Python phase: rolling mmap/read windows + tests + benchmark + Rust/Go
   comparison + commit.
4. Node.js phase: rolling positioned-read windows, optional mmap decision,
   tests + benchmark + commit.
5. Whole-SOW validation across shared matrices and cross-platform hosts.
6. Reviewer pool review until every reviewer votes `PRODUCTION GRADE`.
7. Update specs, docs, skills, status ledgers, audit, close.

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
- User asked for all external reviewers to review planning gaps once per
  language, starting with Rust.
- Rust planning review round ran read-only reviewers `glm`, `kimi`, `mimo`,
  `qwen`, `minimax`, and `deepseek`. All six returned `PLAN NOT READY`.
  Consensus blockers: unresolved Rust accessor architecture, pread storage and
  row-lifetime semantics, hash-table handling under the memory contract, public
  API migration, whole-file mode semantics, missing pread benchmark mode, and
  missing tests for pread row lifetime / mmap no-regression.
- User asked whether Rust really needs a pread path. Local evidence showed
  `memmap2` supports Unix and Windows implementations, covering Linux,
  FreeBSD, macOS, and Windows. User agreed Rust should not be refactored for
  pread when mmap is already available on the supported target OS list.
- Rust reviewer blockers are therefore dispositioned by removing Rust pread
  implementation from SOW-0108 scope. Rust remains the mmap/window-lifetime
  reference model.
- Go pre-code analysis recorded:
  - Go's public `ReaderAccessMode` exists, but it has only `ReadAt` and `Mmap`
    modes, with no `Auto`.
  - Unix `ReaderAccessMmap` maps the whole file.
  - Non-Unix `ReaderAccessMmap` silently loads the whole file into resident
    memory through `ReadAt`.
  - Directory readers, facade, Explorer, and Netdata wrapper all inherit the
    file-reader backend through `ReaderOptions`.
  - `GetEntry` returns owned copies, while `VisitEntryPayloads`,
    `EnumerateEntryPayload`, and `SdJournalEnumerateAvailableData` are the
    relevant hot paths for borrowed/current-row payload data.
- Go pre-code reviewer round:
  - Completed reviewer outputs captured from `glm`, `kimi`, `mimo`, and
    `minimax`. All four returned `PLAN NOT READY`.
  - `qwen` and `deepseek` did not produce captured final reviews. The user
    interrupted/stopped further reviewer work after deciding the existing
    answers were enough and that more analysis is required.
  - Consensus blockers:
    - no concrete Go `WindowAccessor` interface or file/build-tag layout;
    - no Go row-pin state machine for mmap slices and mutable `ReadAt` window
      buffers;
    - no defined cross-window / object-larger-than-window semantics;
    - no complete row arena model for compressed DATA and cross-window copies;
    - no safe live refresh behavior for pinned windows and visible file-size
      changes;
    - missing `readCurrentHeader` / direct file access integration into the
      accessor;
    - missing hash-table metadata caching decision;
    - missing directory-reader memory envelope across many open files;
    - public API compatibility not fully specified for `ReaderAccessAuto`,
      `WithMmap`, normalization, window size, max windows, selected backend,
      and stats;
    - `.journal.zst` whole-file read/decode memory exception not explicitly
      dispositioned;
    - validation lacks concrete tests for eviction, cross-window objects,
      oversized objects, compressed DATA arena, Windows explicit mmap failure,
      large-file memory ceiling, compact/sealed files, live refresh, and
      no-regression benchmarks.
- User clarified that Go should mirror Rust where mmap is available, including
  Windows, and that `.journal.zst` support should mirror Rust's streaming temp
  journal behavior.
- Revised Go pre-code plan:
  - native rolling mmap is required for Go on Unix-family targets and Windows;
  - `ReaderAccessReadAt` remains explicit legacy/diagnostic/fallback, not the
    Windows default;
  - defaults are `ReaderAccessAuto`, 32 MiB windows, and four active windows;
  - current-row returned slices survive DATA enumeration restart and `Refresh()`
    until the row is left;
  - `.journal.zst` must stream-decompress to a temp journal before normal
    bounded accessor reads;
  - hash-table/index reads remain window-backed until profiling proves a
    separate metadata cache is needed.
- Go pre-code review round 3:
  - `glm`, `kimi`, `mimo`, and `deepseek` voted `READY FOR IMPLEMENTATION`.
  - `qwen` voted `PLAN NOT READY`.
  - `minimax` timed out before producing a final vote.
  - Qwen blockers:
    - `readerRefreshSnapshot` and direct refresh accessor migration were not
      mechanically specified;
    - row arena was included in the memory envelope but did not have a limit;
    - `Reader` struct migration from `mapping *readOnlyMapping` to the new
      accessor was not specified.
- Round 3 blocker resolutions recorded before rerun:
  - `Reader` owns `accessor readerAccessor`, not `mapping *readOnlyMapping`;
  - `readerRefreshSnapshot` captures only logical reader state and accessor
    visible bounds, not row-pinned windows or row arena;
  - direct refresh snapshots visible state, refreshes bounds through the
    accessor, reloads entry-array state into temporary data, commits on success,
    and restores visible state on failure without touching row pins;
  - Go row arena is bounded by `max_row_arena_bytes`, default 256 MiB, with a
    controlled limit error and stats;
  - accessor interface now includes visible-bound snapshot/restore methods and
    `refreshVisibleBounds()`.
- Go pre-code review round 4:
  - `glm`, `kimi`, `mimo`, `minimax`, and `deepseek` voted
    `READY FOR IMPLEMENTATION`.
  - `qwen` voted `PLAN NOT READY`.
  - Qwen blockers:
    - the `ReaderAccessAuto` mmap probe and fallback cleanup path were not
      mechanically specified;
    - compressed DATA routing to temporary storage versus row arena was not
      mechanically specified.
- Round 4 blocker resolutions recorded before rerun:
  - `newReaderAccessor(file *os.File, opts ReaderOptions)` owns initial file
    size capture, backend selection, first-window mmap probe, partial mmap
    cleanup, `Auto` fallback, selected-backend stats, and fallback reason;
  - the `Auto` probe maps the first real rolling window
    `min(file_size, window_size)` and keeps it as the first cached window on
    success;
  - explicit `ReaderAccessMmap` probe failure is a clear error after cleanup;
  - all header/object reads after open use the returned accessor;
  - DATA payload reading is split into `readDataPayloadTemp()` and
    `readDataPayloadRow()`, with the existing pure `decompressDataPayload`
    remaining accessor/arena agnostic and row-lifetime compressed data appended
    to the bounded row arena.
- Go pre-code review round 5:
  - `glm`, `kimi`, `mimo`, `qwen`, `minimax`, and `deepseek` all voted
    `READY FOR IMPLEMENTATION`.
  - Non-blocking watchpoints to carry into implementation:
    - Go stdlib `syscall` provides `CreateFileMapping`, `MapViewOfFile`,
      `UnmapViewOfFile`, `PAGE_READONLY`, and `FILE_MAP_READ`, but not a public
      `GetSystemInfo` wrapper. Windows allocation granularity must be obtained
      with a local `kernel32!GetSystemInfo` call or another explicitly reviewed
      mechanism; using page size is invalid.
    - `reader_core_bench` currently treats `whole-file` as an alias for mmap.
      The Go phase must update benchmark mode naming so rolling mmap,
      `Auto`, explicit `ReadAt`, and any whole-file benchmark-only mode are not
      confused.
    - `restoreRefreshSnapshot()` and direct `Refresh()` must avoid any call path
      that reaches accessor `clearRow()` while staying on the same row.
    - `readDataPayloadWithHeader()` in Explorer and unique/field helpers must
      use the temp payload path, not the row-lifetime path.
    - Existing tests that iterate only `ReaderAccessReadAt` and
      `ReaderAccessMmap` must include or separately cover `ReaderAccessAuto`.
  - Gate result: Go implementation may start.

### Go Implementation Evidence - 2026-06-14

Status: locally implemented; pending external `PRODUCTION GRADE` review.

Changed Go reader architecture:

- Added `ReaderAccessAuto`, window sizing, max-window count, row-arena limit,
  `SelectedAccessMode()`, and `AccessStats()`:
  `go/journal/reader.go`, `go/journal/reader_access.go`.
- Replaced reader-owned whole-file `readOnlyMapping` with
  `accessor readerAccessor`:
  `go/journal/reader.go`.
- Added shared rolling accessor logic for window lookup, LRU eviction,
  row-pinned windows, row-scoped arena, visible-bounds snapshots, and selected
  backend stats:
  `go/journal/reader_access.go`.
- Added rolling Unix mmap backend using aligned `syscall.Mmap` windows:
  `go/journal/reader_access_mmap_unix.go`.
- Added rolling Windows mmap backend using `CreateFileMapping`,
  `MapViewOfFile`, exact `UnmapViewOfFile` view pointers, and
  `kernel32!GetSystemInfo` allocation granularity:
  `go/journal/reader_access_mmap_windows.go`.
- Added explicit unsupported mmap constructor for targets outside Unix and
  Windows:
  `go/journal/reader_access_mmap_unsupported.go`.
- Removed the obsolete reader-only whole-file mapping/copy code from
  `go/journal/mmap_unix.go` and `go/journal/mmap_other.go`. Writer
  `mappedArena` remains in place.
- Changed `.journal.zst` reader open from whole-file `os.ReadFile` +
  `DecodeAll` to streaming zstd decompression into a temporary `.journal`,
  which is then read through the same bounded accessor:
  `go/journal/reader.go`.
- Split DATA payload reads into temp/callback/internal and row-lifetime paths:
  `readDataPayloadTemp()` and `readDataPayloadRow()` in
  `go/journal/reader_entry.go`.
- Preserved direct `Refresh()` row-lifetime guarantees by snapshotting logical
  reader state and accessor visible bounds without calling `clearRow()`:
  `go/journal/reader.go`.
- Fixed a live `ReadAt` cache invalidation edge case discovered during local
  tests: journal files can be preallocated, so header/entry-array metadata may
  change while file size is unchanged. The rolling `ReadAt` backend now marks
  cached windows stale on refresh after reading a fresh header directly inside
  the accessor.
- Fixed a Windows mmap growth edge case discovered during local review:
  Windows file mapping objects are sized at `CreateFileMapping` time, so the
  backend refreshes the mapping handle when the visible file size grows while
  keeping existing mapped views valid until their normal unmap.
- Updated Go reader tests to cover `ReaderAccessAuto`, selected backend stats,
  row payload validity across refresh with a single 1 KiB window, and compressed
  row-arena limit errors:
  `go/journal/reader_test.go`.
- Updated the Go reader benchmark helper so `--window-size` affects
  `ReaderOptions`, `--mmap-strategy auto` is explicit, and Go `mmap` now means
  rolling mmap, not whole-file:
  `go/internal/testcmd/reader_core_bench/main.go`,
  `tests/benchmarks/run_reader_core_benchmarks.py`.

Local validation passed:

- `cd go && go test ./...`
- `cd go && GOOS=windows GOARCH=amd64 go test ./...`
- `cd go && GOOS=darwin GOARCH=arm64 go test -exec=true ./...`
- `cd go && GOOS=freebsd GOARCH=amd64 go test -exec=true ./...`
- `python3 tests/interoperability/run_matrix.py --writers go --readers go stock --entries 20`
  - result: 11 total checks, 11 passed, 0 failed.
- `python3 tests/interoperability/run_directory_matrix.py --readers go stock`
  - result: PASS, including Go `.journal.zst` directory extension coverage.
- Go reader-core smoke on a real Netdata raw journal with 1 MiB windows:
  - `auto`: 121,159 records, 5,028,947 fields, selected mode `mmap`, mapped
    bytes 4 MiB, read buffers 0.
  - `mmap`: 121,159 records, 5,028,947 fields, selected mode `mmap`, mapped
    bytes 4 MiB, read buffers 0.
  - `read-at`: 121,159 records, 5,028,947 fields, selected mode `read-at`,
    mapped bytes 0, read buffers 4 MiB.

Remaining Go gate before advancing:

- Run the full reviewer pool against this SOW and the Go changed surface.
- Iterate until `glm`, `kimi`, `mimo`, `qwen`, `minimax`, and `deepseek` all
  vote `PRODUCTION GRADE`.

### Go Production Review Round 1 - 2026-06-14

Reviewer outputs are stored under `.local/agent-reviews/sow-0108-go-prod-round1/`.

Votes:

- `glm`: `PRODUCTION GRADE`.
- `kimi`: `PRODUCTION GRADE`.
- `mimo`: `PRODUCTION GRADE`.
- `qwen`: `PRODUCTION GRADE`.
- `deepseek`: `PRODUCTION GRADE`.
- `minimax`: `NOT PRODUCTION GRADE`.

Blocking finding accepted:

- `minimax` found that `readDataPayloadRow()` used the row-lifetime path to
  read compressed DATA bytes before decompression. This could pin a window or
  consume row-arena budget for compressed bytes that are temporary and unused
  after decompression. This violated the SOW rule that compressed DATA raw
  bytes use the temp path and only decompressed bytes enter the row arena.

Fixes after round 1:

- `readDataPayloadRow()` now checks `objectCompressedMask` first. Compressed
  DATA reads raw compressed bytes with `readSlice()` and copies only the
  decompressed payload to the row arena; uncompressed DATA returns
  `readRowSlice()` directly.
- Row-arena growth now uses amortized capacity growth instead of exact-size
  reallocation for each row-copy allocation.
- Added `TestReaderOversizedPayloadUsesRowArena`, which forces a 512-byte
  window and verifies a 2 KiB uncompressed payload is returned correctly and
  uses the row arena.

Post-fix validation passed:

- `cd go && go test ./...`
- `cd go && GOOS=windows GOARCH=amd64 go test ./...`
- `cd go && GOOS=darwin GOARCH=arm64 go test -exec=true ./...`
- `cd go && GOOS=freebsd GOARCH=amd64 go test -exec=true ./...`
- `python3 tests/interoperability/run_matrix.py --writers go --readers go stock --entries 20`
  - result: 11 total checks, 11 passed, 0 failed.
- `python3 tests/interoperability/run_directory_matrix.py --readers go stock`
  - result: PASS.
- Go reader-core post-fix smoke on the same real Netdata raw journal with
  1 MiB windows:
  - `auto`: 121,159 records, 5,028,947 fields, selected mode `mmap`, mapped
    bytes 4 MiB, read buffers 0.
  - `mmap`: 121,159 records, 5,028,947 fields, selected mode `mmap`, mapped
    bytes 4 MiB, read buffers 0.
  - `read-at`: 121,159 records, 5,028,947 fields, selected mode `read-at`,
    mapped bytes 0, read buffers 4 MiB.

Round 2 reviewer gate:

- Failed. Rerun round 3 with the same full review scope and the round-2 fixes
  included. Require all six reviewers to vote `PRODUCTION GRADE`.

### Go Production Review Round 2 - 2026-06-14

Reviewer outputs are stored under `.local/agent-reviews/sow-0108-go-prod-round2/`.

Votes:

- `glm`: `PRODUCTION GRADE`.
- `kimi`: `PRODUCTION GRADE`.
- `mimo`: `PRODUCTION GRADE`.
- `deepseek`: `NOT PRODUCTION GRADE`.
- `minimax`: `NOT PRODUCTION GRADE`.
- `qwen`: `NOT PRODUCTION GRADE`.

Blocking findings accepted:

- `deepseek` and `minimax` found that the SOW-required focused test coverage
  was still incomplete for bounded large-file window behavior, cross-window
  DATA object fallback, refresh failure rollback, callback/row lifetime
  behavior, and `.journal.zst` streaming/temp cleanup.
- `qwen` found that the single growing Go row arena could reallocate its backing
  array while callers held previous row slices. Go keeps the old backing array
  alive through those slices, so the exact use-after-free framing was not
  correct. The finding was still accepted as a row-lifetime contract hardening
  issue: returned row slices should be backed by segments that are never moved
  during the row, so the implementation does not depend on subtle backing-array
  lifetime reasoning.
- During the refresh-failure test design, local review found that
  `refreshEntryOffsets()` captured its rollback snapshot after
  `readCurrentHeader()` had already refreshed accessor-visible bounds. This
  meant an entry-array reload failure could restore entry/header state while
  leaving the accessor with the newly visible file size. This violated the
  intended previous-visible-state rollback contract.

Fixes after round 2:

- Replaced the single growing `rowArena []byte` with segmented row-arena
  storage. Row allocations now return slices from non-moving segments; existing
  slices are never invalidated by later row-arena growth. Inactive segments are
  reused or released to keep allocated row-arena capacity bounded by
  `MaxRowArenaBytes`.
- Added row-arena snapshot/restore for failed cross-window `rowSlice()` reads,
  so a failed positioned read rolls back only the attempted row allocation.
- Moved `refreshEntryOffsets()` rollback snapshot capture before
  `readCurrentHeader()`, so failed entry-array reload restores both old reader
  state and previous accessor-visible bounds.
- Added focused tests:
  - `TestReaderRefreshFailureRestoresStateAndKeepsRowPayload`
  - `TestReaderRowArenaSegmentsPreserveCompressedSlices`
  - `TestReaderCrossWindowPayloadUsesRowArena`
  - `TestReaderBoundedWindowsForLargePayload`
  - `TestReaderRowPinsClearOnAdvance`
  - `TestReaderJournalZstdUsesTempAccessorAndCleansUp`
- Existing focused tests retained:
  - `TestReaderRowPayloadSurvivesRefresh`
  - `TestReaderRowArenaLimitForCompressedPayload`
  - `TestReaderOversizedPayloadUsesRowArena`

Post-round-2-fix validation passed:

- `go test ./journal -run 'TestReader(RefreshFailureRestoresStateAndKeepsRowPayload|RowArenaSegmentsPreserveCompressedSlices|OversizedPayloadUsesRowArena|CrossWindowPayloadUsesRowArena|BoundedWindowsForLargePayload|RowPinsClearOnAdvance|JournalZstdUsesTempAccessorAndCleansUp)'`
- `go test ./...`
- `GOOS=windows GOARCH=amd64 go test ./...`
- `GOOS=darwin GOARCH=arm64 go test -exec=true ./...`
- `GOOS=freebsd GOARCH=amd64 go test -exec=true ./...`
- `go test -race ./journal -run 'TestReader(RefreshFailureRestoresStateAndKeepsRowPayload|RowArenaSegmentsPreserveCompressedSlices|CrossWindowPayloadUsesRowArena|BoundedWindowsForLargePayload|RowPinsClearOnAdvance|JournalZstdUsesTempAccessorAndCleansUp)'`
- `python3 tests/interoperability/run_matrix.py --writers go --readers go stock --entries 20`
  - result: 11 total checks, 11 passed, 0 failed.
- `python3 tests/interoperability/run_directory_matrix.py --readers go stock`
  - result: PASS.
- Go reader-core real-journal smoke on a 201,326,592-byte Netdata raw journal
  with 1 MiB windows:
  - `auto`: 123,210 records, 5,110,520 fields, checksum
    `3904379478172528663`, selected `mmap`, mapped bytes 4 MiB,
    read buffers 0.
  - `mmap`: same records/fields/checksum, selected `mmap`, mapped bytes
    4 MiB, read buffers 0.
  - `read-at`: same records/fields/checksum, selected `read-at`, mapped bytes
    0, read buffers 4 MiB.
- `git diff --check`
- `.agents/sow/audit.sh`

Round 3 reviewer gate:

- Passed. All six reviewers voted `PRODUCTION GRADE`.

### Go Production Review Round 3 - 2026-06-14

Reviewer outputs are stored under `.local/agent-reviews/sow-0108-go-prod-round3/`.
The first `minimax` and `deepseek` round-3 processes timed out and were not
counted as votes. They were rerun with the same full review scope; the rerun
outputs are the counted votes.

Votes:

- `glm`: `PRODUCTION GRADE`.
- `kimi`: `PRODUCTION GRADE`.
- `mimo`: `PRODUCTION GRADE`.
- `qwen`: `PRODUCTION GRADE`.
- `deepseek`: `PRODUCTION GRADE` from rerun output.
- `minimax`: `PRODUCTION GRADE` from rerun output.

Blocking findings:

- None.

Non-blocking risks recorded:

- No automated test directly forces `Auto` mmap probe failure and fallback to
  `ReadAt`.
- No automated test directly exercises explicit mmap on an unsupported
  non-Unix/non-Windows target.
- Windows `GetSystemInfo` failure is inferred from zero allocation granularity
  rather than checked as a returned error.
- Some close-after-error and short-read paths remain covered indirectly rather
  than by one focused test per branch.

Disposition:

- These are accepted as non-blocking for the Go phase. They do not contradict
  the rolling-window contract, row-lifetime guarantee, or runtime-purity
  contract. They remain useful candidates if future SOW-0108 phases add
  injected backend test hooks.

## Validation

Acceptance criteria evidence:

- Go pre-code gate: passed. Round 5 reviewers `glm`, `kimi`, `mimo`, `qwen`,
  `minimax`, and `deepseek` all voted `READY FOR IMPLEMENTATION` for the Go
  implementation plan.
- Go local implementation gate: passed on Linux tests, Windows target tests,
  macOS target tests, FreeBSD target tests, interoperability matrix, directory
  matrix, focused row-lifetime/window tests, focused race tests, and benchmark
  smoke.
- Go external `PRODUCTION GRADE` reviewer gate: passed in round 3 after reruns
  for two timed-out reviewer processes.

Tests or equivalent validation:

- Go validation commands passed:
  - `cd go && go test ./...`
  - `cd go && GOOS=windows GOARCH=amd64 go test ./...`
  - `cd go && GOOS=darwin GOARCH=arm64 go test -exec=true ./...`
  - `cd go && GOOS=freebsd GOARCH=amd64 go test -exec=true ./...`
  - `python3 tests/interoperability/run_matrix.py --writers go --readers go stock --entries 20`
  - `python3 tests/interoperability/run_directory_matrix.py --readers go stock`
- Post-review-fix validation repeated the same Go test/cross-target/
  interoperability gates and the reader-core real-journal smoke.
- Post-round-2-fix validation added focused bounded-window, cross-window,
  segmented row-arena, refresh rollback, row-pin clearing, `.journal.zst`
  cleanup, and race coverage.

Real-use evidence:

- Go reader-core smoke ran against a real Netdata raw journal:
  121,159 records and 5,028,947 fields read in each of `auto`, `mmap`, and
  `read-at` modes with 1 MiB windows before the round-2 test hardening.
  `auto` selected `mmap`; `mmap` used mapped windows only; `read-at` used read
  buffers only.
- The same smoke was repeated after fixing the round-1 compressed DATA row path
  finding, with identical counts and backend stats.
- After round-2 fixes, Go reader-core smoke ran against a larger
  201,326,592-byte Netdata raw journal: 123,210 records and 5,110,520 fields
  in each mode with identical checksum `3904379478172528663`; `auto`/`mmap`
  held 4 MiB mapped and `read-at` held 4 MiB read buffers.

Reviewer findings:

- 2026-06-14 Rust planning review round:
  - `glm`: `PLAN NOT READY`; blocker is unresolved `FileReader`/`ReaderCell`
    architecture around the `ouroboros` self-referencing `JournalFile<Mmap>`
    storage and missing `max_windows` / pread benchmark planning.
  - `kimi`: `PLAN NOT READY`; blockers are undefined `WindowAccessor`
    abstraction, missing positioned-read backend design, and missing public API
    transition from mmap strategy to access mode.
  - `mimo`: `PLAN NOT READY`; blockers are `MemoryMap`/pread representation,
    hash-table map policy, pread row-lifetime storage, and missing
    `JournalFile` generic/parallel-type plan.
  - `qwen`: `PLAN NOT READY`; blockers are missing `pread` backend trait,
    conflict between `ExperimentalMmapStrategy` and `Auto/Mmap/Pread`, and
    `JournalFile<M: MemoryMap>` being unable to host pread as currently
    shaped.
  - `minimax`: `PLAN NOT READY`; blockers are missing concrete SOW gate
    decisions, unspecified pread storage lifecycle, unresolved whole-file mode,
    undefined live/snapshot refresh under the new accessor, and missing
    benchmark baseline.
  - `deepseek`: `PLAN NOT READY`; blockers are `MemoryMap:
    Deref<Target=[u8]>` semantics, unresolved core architectural choice, and
    undefined zero-copy semantics for pread.
  - Disposition: these findings were valid for the abandoned Rust pread plan.
    They are not blockers for the current Rust scope because Rust pread is no
    longer being implemented in this SOW. They remain useful negative evidence:
    do not refactor Rust around pread unless a later SOW reopens that design
    with a concrete need.
- 2026-06-14 Go planning review rounds:
  - Rounds 1, 3, and 4 found valid `PLAN NOT READY` blockers around accessor
    construction, row lifetime, refresh, Windows mmap ownership, `ReadAt`
    pinning, row-arena limits, `Reader` struct migration, and DATA
    decompression routing.
  - The SOW was revised after each round with concrete mechanical decisions and
    validation requirements.
  - Round 5 result: all six reviewers voted `READY FOR IMPLEMENTATION`.
- 2026-06-14 Go production review round 1:
  - Five reviewers voted `PRODUCTION GRADE`.
  - `minimax` voted `NOT PRODUCTION GRADE` because compressed DATA raw bytes
    were read through the row-lifetime path before decompression.
  - Disposition: accepted and fixed.
- 2026-06-14 Go production review round 2:
  - `glm`, `kimi`, and `mimo` voted `PRODUCTION GRADE`.
  - `deepseek`, `minimax`, and `qwen` voted `NOT PRODUCTION GRADE`.
  - Disposition: accepted and fixed. Round 3 pending.
- 2026-06-14 Go production review round 3:
  - `glm`, `kimi`, `mimo`, `qwen`, `deepseek`, and `minimax` voted
    `PRODUCTION GRADE`.
  - Disposition: Go phase review gate passed.

Same-failure scan:

- Local same-failure search confirmed the removed `readOnlyMapping` reader path
  is no longer referenced by Go reader code. Writer `mappedArena` remains
  separate.

Sensitive data gate:

- This SOW contains no raw sensitive data. File paths are repository-local
  source paths or sanitized local validation targets only.

Artifact maintenance gate:

- Not complete. Go implementation changed public reader options/stats and
  benchmark semantics; specs/docs must be updated before this SOW can close
  after all language phases.

Specs update:

- Pending after Go production-grade review and before SOW close.

Project skills update:

- Pending after all language phases or if reviewer findings require workflow
  changes.

End-user/operator docs update:

- Pending after all language phases or before the next public release that
  exposes the new reader access options.

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
