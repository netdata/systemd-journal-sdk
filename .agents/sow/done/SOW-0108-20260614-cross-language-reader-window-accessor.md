# SOW-0108 - Cross-Language Reader Window Accessor Architecture

## Status

Status: completed

Sub-state: activated 2026-06-14 after the user explicitly chose to pause
SOW-0105 and prioritize this foundational reader memory architecture work.
Rust pread work was dropped by user decision after platform evidence showed
Rust already has mmap support on Linux, FreeBSD, macOS, and Windows through
`memmap2`. The Go phase passed the pre-code gate, implementation validation,
and external production review; commit `a811945` records the verified Go
window-accessor chunk. Python passed the pre-code reviewer gate in round 4,
has been implemented locally, passed local Python/interoperability validation,
and completed production review round 1 with two valid blocking findings. The
round-1 fixes were implemented, locally revalidated, and production review
round 2 passed with all six reviewers voting `PRODUCTION GRADE`. The Python
phase is accepted. The active language phase is Node.js implementation. The
Node.js pre-code reviewer gate passed in round 5 with all six reviewers voting
`READY FOR IMPLEMENTATION`. Node.js implementation is locally complete and
validated. Production review rounds 1 and 2 found valid blockers. The fixes
were implemented and locally revalidated. Production review round 3 passed
with all six reviewers voting `PRODUCTION GRADE`.

Regression reopened 2026-06-14: the completed SOW missed public verification
API parity. Rust `verify_file()` / `verify_file_with_key()` and Go
`VerifyFile()` / `VerifyFileWithKey()` still materialize a whole journal file
before object-graph and sealed-HMAC verification. Node.js and Python already
verify through bounded reader access. This is a regression against the
cross-language bounded access architecture because verification helpers were in
the declared blast radius.

Current regression sub-state: repaired, locally validated, externally reviewed,
and completed. Rust and Go public file-path verifiers now use bounded
reader-backed byte sources instead of materializing whole journal files.
Production review round 1 found one process blocker, round 2 found one real Go
mmap aliasing blocker, round 3 found one process-only ledger blocker, and round
4 resolved the final process gate. All six reviewers ultimately voted
`PRODUCTION GRADE` for the repaired regression surface.

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
  `llm-netdata-cloud/qwen3.7-plus`,
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

Status: locally implemented, validated, reviewed, and accepted. Production
review passed in round 3.

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

### Python Pre-Code Design Analysis - 2026-06-14

Status: Python analysis is ready for pre-code reviewer round 4 after addressing
the blockers from rounds 1, 2, and 3. Python code must not change until all six
reviewers vote `READY FOR IMPLEMENTATION`.

Ground-truth facts from the current Python implementation:

- `FileReader` stores one byte container in `self._buffer` and all parse paths
  index or slice that object directly: `python/journal/reader.py:35`,
  `python/journal/reader.py:36`.
- `FileReader.open()` maps the whole file with `mmap.mmap(fd, 0,
  access=mmap.ACCESS_READ)`: `python/journal/reader.py:72`,
  `python/journal/reader.py:83`, `python/journal/reader.py:890`,
  `python/journal/reader.py:896`.
- `.journal.zst` currently uses `decompress_zst_to_temp()`, which calls
  `decompress_zst_sync(input_path)` and writes the full decompressed result
  from memory: `python/journal/reader.py:79`, `python/journal/compress.py:124`,
  `python/journal/compress.py:125`, `python/journal/compress.py:129`.
- The reader refresh path remaps the whole file when file size changes:
  `python/journal/reader.py:232`, `python/journal/reader.py:235`,
  `python/journal/reader.py:238`.
- Normal entry materialization is an owned-result API. It copies field names,
  field values, and payloads into Python `bytes`: `python/journal/reader.py:429`,
  `python/journal/reader.py:454`, `python/journal/reader.py:455`,
  `python/journal/reader.py:456`.
- Current-row enumeration avoids pre-materializing a full entry, but currently
  routes through `_read_data_payload_at()`: `python/journal/reader.py:650`,
  `python/journal/reader.py:655`, `python/journal/reader.py:662`.
- `SdJournalEnumerateAvailableData()` converts the reader result to `bytes`:
  `python/journal/facade.py:323`, `python/journal/facade.py:325`,
  `python/journal/facade.py:329`. This keeps the Python facade compatibility
  contract but is not a zero-copy facade surface.
- `DirectoryReader` opens `FileReader` instances directly, so directory access
  inherits the file-reader memory model: `python/journal/directory_reader.py:31`,
  `python/journal/directory_reader.py:39`,
  `python/journal/directory_reader.py:47`,
  `python/journal/directory_reader.py:53`.
- Python Explorer reaches into FileReader internals and must be migrated with
  FileReader rather than left on raw `_buffer` access:
  `python/journal/explorer.py:1056`, `python/journal/explorer.py:1070`,
  `python/journal/explorer.py:1072`, `python/journal/explorer.py:1207`,
  `python/journal/explorer.py:1213`, `python/journal/explorer.py:1768`,
  `python/journal/explorer.py:1769`.
- Python verification helpers also reach into FileReader internals and must be
  migrated in this phase, not left as a separate whole-file/buffer bypass:
  `python/journal/verify.py:48`, `python/journal/verify.py:77`,
  `python/journal/verify.py:81`, `python/journal/verify.py:83`,
  `python/journal/verify.py:153`, `python/journal/verify.py:161`.
- The Netdata Python wrapper has a separate whole-file mmap header probe that
  must be removed or redirected through the new bounded accessor:
  `python/journal/netdata.py:2118`, `python/journal/netdata.py:2144`,
  `python/journal/netdata.py:2149`.
- Existing Python tests assert internal implementation identity that will no
  longer exist and must be migrated without weakening coverage:
  `python/test_reader_facade.py:370`, `python/test_reader_facade.py:388`,
  `python/test_reader_facade.py:399`, `python/test_reader_facade.py:402`.
- Existing `_platform_io.read_at()` already provides a positioned-read shim:
  it uses `os.pread` when available and otherwise serializes
  `lseek`/`read`/restore under a process-local lock:
  `python/journal/_platform_io.py:12`,
  `python/journal/_platform_io.py:15`,
  `python/journal/_platform_io.py:17`,
  `python/journal/_platform_io.py:18`,
  `python/journal/_platform_io.py:29`.
- Current Python docs/specs explicitly record the current limitation that
  facade DATA enumeration returns `bytes`, not borrowed mmap slices:
  `.agents/sow/specs/product-scope.md:982`,
  `.agents/sow/specs/product-scope.md:984`,
  `python/README.md:475`, `python/README.md:478`.

Official platform facts checked:

- Python standard-library `mmap` is available except on WASI and has separate
  Unix and Windows constructors. Both support read-only access through
  `ACCESS_READ`: https://docs.python.org/3/library/mmap.html.
- Python `mmap` supports mapping a subsection with an `offset`; the offset must
  be aligned to `ALLOCATIONGRANULARITY`. The docs state that
  `ALLOCATIONGRANULARITY` is equal to `PAGESIZE` on Unix:
  https://docs.python.org/3/library/mmap.html.
- Python `mmap` length `0` maps the whole file. That is exactly the current
  behavior to remove from production reader paths:
  https://docs.python.org/3/library/mmap.html.
- `os.pread()` is documented as Unix-only. Therefore positioned reads must be
  treated as an explicit fallback using the existing compatibility shim, not as
  the preferred cross-platform production backend:
  https://docs.python.org/3/library/os.html#os.pread.
- Python 3.14 `compression.zstd` includes a file interface (`open()` and
  `ZstdFile`) and incremental decompression classes, so `.journal.zst`
  decompression can stream into a temporary `.journal` instead of building the
  whole decompressed journal in memory:
  https://docs.python.org/3/library/compression.zstd.html.

Runtime facts checked on configured targets:

- Local Linux Python exposes `mmap.ACCESS_READ`,
  `mmap.ALLOCATIONGRANULARITY == 4096`, and `os.pread`.
- Configured macOS Python exposes `mmap.ACCESS_READ`,
  `mmap.ALLOCATIONGRANULARITY == 16384`, and `os.pread`.
- Configured Windows/MSYS Python exposes `mmap.ACCESS_READ`,
  `mmap.ALLOCATIONGRANULARITY == 65536`, and `os.pread`; official docs still
  require the implementation to tolerate Python builds where `os.pread` is not
  present on Windows.
- Local behavior check: slicing a Python `mmap` returns `bytes`, while slicing
  `memoryview(mmap_obj)` returns a `memoryview`. Closing an mmap with exported
  memoryviews raises `BufferError`. Therefore a Python zero-copy path must be
  intentionally implemented with memoryviews and explicit row/window ownership;
  the current `mmap` slice code is copy-on-slice.

Python reader surfaces and required lifetime class:

| Surface | Current behavior | Required accessor lifetime |
|---|---|---|
| `FileReader.get_entry()` | owned `dict` with copied `bytes` | temporary/internal views, then explicit owned copies |
| `FileReader.visit_entry_payloads()` | callback path, currently copied by mmap slicing | callback-scoped `memoryview` for uncompressed DATA where possible |
| `FileReader.collect_entry_payloads()` | list of payload objects | explicit owned-result API; copy after callback |
| `FileReader.get_entry_payload()`, `get_raw()`, `get_raw_values()` | owned `bytes` / lists | temporary/internal views, then explicit owned copies |
| `entry_data_restart()` + `enumerate_entry_payload()` | current-row enumeration | row-lifetime memoryview/arena-backed views until row advance/seek/file switch/close |
| `SdJournalEnumerateAvailableData()` | facade returns `bytes` | may continue returning `bytes` for Python facade compatibility; underlying reader row lifetime must still be correct |
| Explorer scan/index paths | internal classification | temporary views; no raw `_buffer` direct access |
| Verification helpers | strict offset/object checks with raw `_buffer` / whole-file `bytes` | accessor-backed strict reads; no raw reader buffer bypass |
| Netdata wrapper header probe | separate whole-file mmap | bounded header helper; no whole-file map/copy and no temporary full reader open |
| `DirectoryReader` / `SdJournalOpenDirectory` / `OpenFiles` | eager per-file readers | pass options through to every `FileReader` |

Python design decisions recorded before code:

1. Python uses rolling mmap as the production default on Linux, FreeBSD, macOS,
   and Windows.
   - Rationale: Python standard-library `mmap` exists on Unix and Windows, and
     the configured Linux/macOS/Windows runtimes expose it.
   - Implication: Python must not default to positioned reads on Windows merely
     because `os.pread` availability differs across Python builds.
   - Risk: Python mmap view lifetime is subtler than current `bytes` returns;
     tests must prove row-pinned windows cannot be evicted while views are live.

2. Python keeps positioned-read windows as an explicit fallback/diagnostic
   backend, not as the preferred path on the four target OS families.
   - Rationale: `os.pread` is officially Unix-only, while mmap is the
     documented cross-platform mechanism.
   - Implication: `Auto` may fall back to rolling positioned reads only if the
     initial mmap probe fails before public data is returned. Explicit `Mmap`
     fails clearly.
   - Risk: fallback reads on Python builds without `os.pread` use serialized
     `lseek`/`read` through `_platform_io.read_at()`; this is correct but not a
     high-throughput production path.

3. Python introduces explicit reader options while preserving existing call
   sites.
   - Add `ReaderOptions` with `access_mode`, `window_size`, `max_windows`,
     `max_row_arena_bytes`, `max_retired_windows`, and `bounds`.
   - Add constants or enum values for `auto`, `mmap`, and `read-at`.
   - Public signatures are keyword-compatible:
     `FileReader.open(path, *, options=None)`,
     `DirectoryReader.open(path, *, options=None)`,
     `DirectoryReader.open_files(paths, *, options=None)`, and
     `DirectoryReader.from_readers(path, readers, *, allow_empty=False,
     options=None)`.
   - `SdJournalOpen*` keep the existing `flags` parameter and add an optional
     keyword-only `options=None`; existing callers are unaffected.
   - `FileReader.selected_access_mode()` and `FileReader.access_stats()` expose
     the selected backend and stats. `DirectoryReader.access_stats()` aggregates
     per-file stats and exposes each file's selected backend.
   - Stats must include at least: selected backend, fallback reason, visible
     size, window size, max windows, mapped bytes, read-buffer bytes,
     row-pinned windows, retired windows, retired bytes, row-arena current/peak
     bytes, row-arena limit bytes, row-arena segment bytes, active row-arena
     segments, temp-copy count, and mmap/read-at window miss/eviction counts.
   - Benchmark CLI flags `--window-size`, `--bounds`, and `--mmap-strategy`
     must start configuring real reader options instead of being report-only.

4. Python low-level hot paths should use `memoryview` for uncompressed DATA
   where safe, but the Python facade may keep returning `bytes`.
   - Rationale: the existing facade API and docs promise easy retention of
     `bytes`; changing it to `memoryview` would be a public behavior change and
     would expose callers to mmap exported-view lifetime rules.
   - Implication: `FileReader.visit_entry_payloads()` and
     `FileReader.enumerate_entry_payload()` can be the zero-copy surfaces;
     `SdJournalEnumerateAvailableData()` remains compatibility-oriented by
     copying to `bytes`.
   - Risk: Python will still not match Rust/Go facade zero-copy behavior until
     a future explicit facade API decision changes this contract. This is an
     existing documented limitation and must be called out in the reviewer
     prompt.

5. Python `.journal.zst` support must stream into a temp `.journal`, matching
   Rust/Go repository-extension behavior.
   - Rationale: the current `decompress_zst_to_temp()` path loads the whole
     compressed input and decompressed output in memory.
   - Implication: add a streaming temp-file path that copies 1 MiB chunks by
     default, using `compression.zstd.open()` / `ZstdFile` when available or an
     incremental decompressor if that is the available streaming API. If zstd
     streaming support is unavailable, keep a clear unsupported error. The
     reader open path must not call the existing whole-input
     `decompress_zst_sync()` path.
   - Concrete API: add `stream_zst_to_temp(input_path, *, chunk_size=1 << 20,
     prefix='python-sdk-journal')` in `python/journal/compress.py`. Remove
     `decompress_zst_to_temp()` from production code; do not merely deprecate
     it. `stream_zst_to_temp()` is the only production file-level zstd API.
   - `decompress_zst_sync()` remains allowed only for DATA-payload-level
     decompression of an already-bounded compressed object buffer. It is not a
     file-opening API.
   - The normal SDK paths in `python/journal/reader.py:80`,
     `python/journal/verify.py:157`, and `python/journal/netdata.py:2142`
     must use `stream_zst_to_temp()` for `.journal.zst` files. If
     `verify.py` no longer opens `.journal.zst` directly after the verifier
     migration, this exact call site disappears instead of being left on the
     old helper.
   - Tests may use whole-byte decompression only for deliberately small
     synthetic expected-value fixtures with an explicit size cap in the test.
     SDK verification tests must exercise the streaming path.

6. Python row-lifetime behavior is a state machine, not an incidental cache.
   - `entry_data_restart()` and end-of-enumeration reset only enumeration
     cursors. They must not clear row-pinned windows or the row arena while the
     row is still current.
   - Owned-result APIs such as `get_entry()`, `get_entry_payload()`,
     `get_raw()`, `get_raw_values()`, `collect_entry_payloads()`, and
     `visit_entry_payloads()` may copy from temp/callback views, but they must
     not invalidate row-lifetime views previously returned for the same current
     row.
   - `clear_row()` is called only when the reader leaves the row: `next()`,
     `previous()`, successful seek to another row, file switch, directory reader
     switch, and close. Direct `refresh()` while staying on the same row must
     preserve current-row views.
   - Views kept by callers after the row is left are outside the contract. The
     implementation may keep them alive temporarily because Python exported
     memoryviews can block `mmap.close()`, but this must be bounded and visible
     in stats.

Proposed Python internal files and APIs:

- Add `python/journal/reader_access.py` for the common accessor, window,
  options, stats, row arena, and backend-selection logic.
- Keep `python/journal/reader.py` as the journal parser/iterator owner, but
  replace direct `self._buffer` reads with narrow helper methods over the
  accessor.
- Accessor methods:
  - `size()`;
  - `temp_view(offset, size)` for parse/callback-scoped memory;
  - `row_view(offset, size)` for current-row returned data;
  - `read_bytes(offset, size)` only for explicit owned-result APIs and tests;
  - `u8(offset)`, `u32(offset)`, `u64(offset)`;
  - `clear_row()`;
  - `snapshot_visible_bounds()` / `restore_visible_bounds(snapshot)`;
  - `refresh_visible_bounds()`;
  - `stats()`;
  - `close()`.
- Rolling mmap backend:
  - align window base down to `mmap.ALLOCATIONGRANULARITY`;
  - map at most `window_size` plus alignment slack, clipped to visible size;
  - store each mmap object and a base memoryview owner in a window record;
  - return sliced memoryviews for temp and row access;
  - row-pinned windows are not evicted until row clear;
  - closing a window releases internal memoryviews before closing the mmap;
  - if external code violates the lifetime contract and exported memoryviews
    make `mmap.close()` raise `BufferError`, move the window to a bounded
    retired list and report retired windows/bytes in stats. The default
    `max_retired_windows` is `max_windows`. Before mapping a new window and on
    close, retry closing retired windows. If retiring another window would
    exceed the cap, raise a controlled `RuntimeError` explaining that current
    row views were retained past row lifetime.
- Rolling read-at backend:
  - reuse fixed bytearray window buffers;
  - fill buffers through `_platform_io.read_at()`;
  - return memoryviews over those buffers;
  - row-pinned read buffers cannot be overwritten until row clear;
  - if all windows are row-pinned, temp reads use scratch bytes and record a
    temp-copy stat instead of exceeding the window budget.
- Row arena:
  - fixed-size bytearray segments allocated once and never resized while any
    memoryview can exist;
  - default `row_arena_segment_bytes` is 1 MiB;
  - writes use slice assignment into pre-sized segments, so later row-arena
    allocations cannot resize a bytearray backing an exported memoryview;
  - used for decompressed DATA and row-returned ranges that cross windows or
    exceed one window;
  - a single allocation larger than `row_arena_segment_bytes` uses a larger
    fixed-size segment, still subject to `max_row_arena_bytes`;
  - bounded by `max_row_arena_bytes`, default 256 MiB, with a controlled error.

Parser migration plan:

- Open path:
  1. If input is `.journal.zst`, stream-decompress to a temp `.journal`.
  2. Open the temp/normal file descriptor with `os.O_RDONLY` plus
     `os.O_BINARY` where present.
  3. Build the accessor through one constructor for `Auto`, `Mmap`, or
     `ReadAt`.
  4. Parse the header from a bounded header view, not a whole-file map.
  5. Load current entry-array state through accessor helpers.
- Replace direct buffer helpers:
  - `parse_object_header(self._buffer, offset)` becomes a reader helper that
    reads `OBJECT_HEADER_SIZE` through the accessor.
  - `_UNPACK_U64_FROM(self._buffer, offset)` and `_UNPACK_U32_FROM(...)`
    become accessor `u64()` / `u32()` calls.
  - `len(self._buffer)` becomes `self._accessor.size()`.
  - direct byte indexing becomes `self._accessor.u8(offset)`.
- Same-failure migration requirement:
  - After implementation, `rg 'self\._buffer|reader\._buffer' python/journal`
    must return no direct production reader/parser/explorer/netdata/verification
    bypasses. Any remaining match must be a test-only assertion or an explicit
    compatibility shim that raises on direct slicing.
  - Exact helper replacements:
    - `len(self._buffer)` and `len(reader._buffer)` become
      `self._accessor.size()` inside `FileReader` or `reader._visible_size()`
      for helper modules such as Explorer.
    - `self._buffer[offset]` becomes `self._accessor.u8(offset)`.
    - `_UNPACK_U32_FROM(self._buffer, offset)` and
      `_UNPACK_U64_FROM(self._buffer, offset)` become accessor `u32()` and
      `u64()` calls inside `FileReader`; helper modules call existing
      `reader._UNPACK_U64(offset)` only after that helper delegates to the
      accessor.
    - `parse_object_header(self._buffer, offset)` becomes
      `self._object_header_at(offset)` backed by `temp_view()`.
    - Verification code must not receive `reader._buffer`. It uses
      `reader._parse_entry_object_at(offset)`,
      `reader._parse_data_object_at(offset)`, or bounded
      `reader._verify_temp_view(offset, size)` helpers.
  - `python/journal/explorer.py:1070` specifically becomes
    `reader._visible_size() < bucket_offset + HASH_ITEM_SIZE`.
  - `python/journal/verify.py` is in scope. `_verify_reader_entry_offsets()`
    and strict entry/DATA parsing must use accessor-backed temp reads.
  - Object-graph and sealed verification are in scope. Add
    `python/journal/_verify_adapter.py` with `_AccessorBytesAdapter`, an
    accessor-backed bytes-like verification adapter used by
    `verify_object_graph()` and `_SealedVerifier`.
  - `_AccessorBytesAdapter` must support the existing verifier access shape
    without materializing the whole journal file:
    - `__len__()` delegates to accessor visible size;
    - `__getitem__(int)` delegates to accessor `u8()`;
    - `__getitem__(slice)` returns `bytes` for the bounded requested range,
      because existing verification code uses `.decode()`, `int.from_bytes`,
      `hmac`, and `secrets.compare_digest`-compatible byte inputs.
  - Large sealed-verification/HMAC ranges must not call
    `adapter[offset:offset + huge_size]` and build one huge bytes object.
    `_AccessorBytesAdapter` must provide `update_hmac(hm, offset, size, *,
    chunk_size=1 << 20)` or an equivalent chunked helper that feeds bounded
    chunks to the HMAC object. If a chunk crosses windows, only that bounded
    chunk may be stitched/copied. Stats must report verification temp-copy
    ranges/chunks; verification adapter reads do not create row-lifetime pins.
    The `_SealedVerifier` migration must route `_hmac_object()` payload-body
    ranges such as DATA, FIELD, and ENTRY payload ranges through
    `adapter.update_hmac()` instead of single-slice `hm.update(data[start:end])`
    calls; tiny fixed header/tag slices may remain normal bounded slices.
  - Object-graph verification call shape after migration is
    `verify_object_graph(_AccessorBytesAdapter(reader._accessor))` or an
    equivalent adapter-backed reader object. It must not pass whole-file
    `bytes`.
  - `verify_file()` and `verify_file_with_key()` must route through
    `FileReader.open(path, *, options=...)` plus `_AccessorBytesAdapter`.
    `_read_journal_file_bytes()` must be removed from
    `python/journal/verify.py`. A whole-file bytes helper may exist only in
    tests with an explicit small-size cap; it must not be reachable from SDK
    verification entry points.
- Split DATA reads:
  - `_read_data_payload_temp(offset)` for internal parse, filters, Explorer,
    unique values, and owned APIs;
  - `_read_data_payload_row(offset)` for `enumerate_entry_payload()`;
  - keep `_read_data_payload_at(offset)` as a temp-path compatibility wrapper
    while migrating callers. This compatibility wrapper must preserve existing
    `bytes` behavior for callers that use `bytes` methods such as `.find()`,
    `.startswith()`, or `.decode()`. New zero-copy internal callers use
    `_read_data_payload_temp()` directly.
- For uncompressed DATA:
  - temp path returns a temp memoryview or scratch memoryview;
  - row path returns a row-pinned memoryview or row-arena memoryview.
- For compressed DATA:
  - read compressed bytes through the temp path;
  - decompress into a bounded row arena only for row-returned APIs;
  - internal/owned APIs may use temporary decompressed `bytes` and then copy
    only where the caller explicitly owns results.
- Migrate Explorer private uses to reader helper methods and temp payload path;
  do not leave `reader._buffer` as a bypass.
- Replace the Netdata wrapper's whole-file header probe with a bounded header
  helper, not a temporary whole `FileReader` open. The helper opens the file,
  reads only the header bytes with `_platform_io.read_at()` or the accessor's
  bounded header read, parses realtime bounds, closes the descriptor, and never
  maps or copies the whole file.
- Refresh:
  - snapshot logical reader state and accessor visible bounds before reading a
    fresh header;
  - refresh visible size/header only at current explicit live points;
  - reload entry arrays into temporary data and commit only on success;
  - restore prior visible bounds on failure;
  - do not clear row-pinned windows or row arena during direct `refresh()` while
    staying on the same row.

Tests required before the Python phase can pass:

- Default `FileReader.open()` selects rolling mmap on Linux, macOS, and
  Windows where tested; stats expose selected backend.
- Explicit `mmap` fails clearly when an injected mmap constructor fails.
- `Auto` falls back to rolling read-at only when an injected initial mmap probe
  fails before public data is returned, and stats expose fallback reason.
- Explicit `read-at` never creates mmap windows.
- Read-at backend tests must verify fixed bytearray window reuse. The fallback
  `lseek`/`read` path may serialize through the existing process-local lock,
  and this limitation must be recorded in stats/docs rather than hidden.
- Small-window tests force multiple mmap/read-at windows and prove correct
  entry traversal.
- Large sparse-file bounded-memory tests use a file at least 64 MiB larger than
  the configured total window budget and prove mapped/read-buffer bytes remain
  inside the accessor budget.
- Cross-window DATA and object-larger-than-window tests prove row-returned
  uncompressed data is copied into row arena and remains valid for the current
  row.
- Compressed DATA tests prove row-returned decompressed data is row-arena
  backed and remains valid until row advance.
- Row-arena limit test returns a controlled error without invalidating earlier
  current-row results.
- Row-arena fixed-segment test proves exported memoryviews are backed by
  fixed-size bytearray segments that are not resized by later same-row arena
  allocations. Required test name: `test_reader_row_arena_segments_are_fixed_size`
  or an equivalent focused test.
- Row pin/eviction pressure test retains current-row `memoryview` payloads,
  forces other window reads, and verifies the retained views remain valid until
  row advance.
- Row-advance tests verify row pins and row-arena current bytes are cleared
  after `next()` / `previous()` without relying on unsafe use after invalidation.
- DATA restart/end-of-enumeration test proves row views remain valid while the
  row is still current.
- Same-row mixed-API tests prove `entry_data_restart()`, end-of-enumeration,
  `get_entry()`, `get_entry_payload()`, `get_raw()`,
  `visit_entry_payloads()`, and `collect_entry_payloads()` do not clear
  row-pinned views while the row is current.
- `visit_entry_payloads()` callback lifetime test covers callback-scoped views.
- Direct refresh success and failure tests preserve current-row views and
  restore visible bounds on failure.
- Snapshot-bounds test proves appended rows remain invisible when snapshot mode
  is selected.
- Directory integration test proves options reach every file reader and no
  directory path opens a whole-file map/copy.
- `.journal.zst` test proves streaming temp-file decode in 1 MiB chunks,
  cleanup on close and open-error paths, bounded accessor open of the temporary
  `.journal`, and no whole-file decompression.
- Verification tests prove `verify.py` strict entry/DATA and object-graph paths
  work without `reader._buffer` or `_read_journal_file_bytes()` whole-file
  reads.
- Sealed verification tests prove `verify_file_with_key()` also uses the
  accessor-backed verification adapter and does not materialize the whole file.
- Verification adapter tests prove `_AccessorBytesAdapter` supports integer
  indexing, bounded slice reads returning `bytes`, and chunked HMAC updates.
  Required coverage includes tests equivalent to
  `test_verify_adapter_does_not_materialize_whole_file` and
  `test_verify_adapter_sealed_whole_file_uses_chunks`.
- Verification removal test proves `_read_journal_file_bytes()` does not exist
  in `python/journal/verify.py` and cannot be called by `verify_file()` or
  `verify_file_with_key()`.
- Tests that currently inspect `_fd`, `_mmap`, and `_buffer` must be migrated to
  accessor identity/stats or fixture-based corruption checks. The migration must
  preserve refresh rollback and boundary-rejection coverage.
- A bypass detection test must fail if production Python reader, Explorer,
  Netdata, or verification code directly reaches into `reader._buffer`.
  Mechanism: a test or audit helper must scan `python/journal/` for
  `reader._buffer`, `reader._mmap`, `reader._fd`, `self._buffer`, and
  `self._mmap`, with an explicit allowlist for `reader_access.py` internals if
  needed. The allowlist must not include Explorer, Netdata, verification, or
  facade code. It may allow writer-only files such as `writer_arena.py`, because
  writer mmap internals are not part of the reader-accessor bypass contract.
  The same bypass scan must include `decompress_zst_to_temp` so it cannot
  remain in production file-opening paths.
- Existing compact, compressed DATA, sealed/FSS, historical-header,
  conformance, facade, Explorer, Netdata, directory, and journalctl tests must
  keep passing.
- Benchmark smoke must report selected backend, window size, max windows,
  mapped/read-buffer bytes, row-arena peak, and temp-copy counts. The mmap path
  must not show a meaningful regression versus the pre-change Python baseline
  on the same large file and mode.
- Benchmark-only process-self RSS reads such as `/proc/self/status` are allowed
  as diagnostics outside the core reader path. They must remain absent from
  runtime reader code.

Reviewer gate for Python:

- Run the full reviewer pool read-only against this SOW, the current Python
  reader code, and the Python plan above.
- Required vote string before implementation may begin:
  `READY FOR IMPLEMENTATION`.
- Explicitly ask reviewers to verify:
  - Python mmap availability reasoning for Linux, FreeBSD, macOS, and Windows;
  - whether keeping facade `bytes` while adding lower-level memoryview hot paths
    is acceptable or whether it violates the SOW contract;
  - exported-memoryview / `BufferError` risks;
  - row-pinned mmap/read-at window lifetime;
  - live refresh rollback;
  - `.journal.zst` streaming behavior;
  - Explorer, Netdata, and verification raw `_buffer` bypass removal;
  - exact `ReaderOptions`, selected-backend, and stats API shape;
  - bounded retired-window behavior and row-lifetime state machine;
  - whether the validation list is sufficient before code starts.

Python pre-code reviewer round 1:

- Result: gate failed; do not implement Python yet.
- Votes:
  - `glm`: `PLAN NOT READY`.
  - `kimi`: `PLAN NOT READY`.
  - `mimo`: `READY FOR IMPLEMENTATION`.
  - `qwen`: `READY FOR IMPLEMENTATION`.
  - `minimax`: `PLAN NOT READY`.
  - `deepseek`: `READY FOR IMPLEMENTATION`.
- Accepted blockers and dispositions:
  - `verify.py` raw `_buffer` / whole-file verification path was missing from
    the migration plan. Disposition: verification is now explicitly in scope
    and must use accessor-backed bounded reads.
  - Tests depended on disappearing `_fd`, `_mmap`, and `_buffer` internals.
    Disposition: tests must migrate to accessor identity/stats or fixture-based
    corruption without reducing coverage.
  - Netdata header probe was an unresolved fork. Disposition: use a bounded
    header helper, not a temporary whole-reader open.
  - Retired mmap windows had no cap. Disposition: add
    `max_retired_windows`, retry closure, expose stats, and raise a controlled
    error if callers retain row views beyond lifetime enough to exceed the cap.
  - Public options/stats shape was not pinned. Disposition: exact public
    signatures and diagnostics are now recorded.
  - Row-lifetime state machine was under-specified. Disposition: exact clear
    points and same-row non-clear behavior are now recorded.
  - `.journal.zst` streaming details were vague. Disposition: default 1 MiB
    streaming chunks, no reader-open use of whole-input `decompress_zst_sync()`,
    and focused tests are now required.

Python pre-code reviewer round 2:

- Result: gate failed; do not implement Python yet.
- Votes:
  - `glm`: `READY FOR IMPLEMENTATION`.
  - `kimi`: `READY FOR IMPLEMENTATION`.
  - `mimo`: `READY FOR IMPLEMENTATION`.
  - `qwen`: `READY FOR IMPLEMENTATION`.
  - `minimax`: `PLAN NOT READY`.
  - `deepseek`: `READY FOR IMPLEMENTATION`.
- Accepted blockers and dispositions:
  - Object-graph and sealed verification migration was still not concrete
    enough. Disposition: verification now requires a bytes-like accessor
    adapter used by `verify_object_graph()` and `_SealedVerifier`; normal SDK
    `verify_file()` and `verify_file_with_key()` must not use
    `_read_journal_file_bytes()`.
  - Explorer/verification/test internal replacement mechanics were too vague.
    Disposition: exact helper replacements for `len(reader._buffer)`,
    `reader._buffer`, strict verification parsing, and bypass detection are now
    recorded.
  - Python zstd streaming surface did not say what remains of
    `decompress_zst_to_temp()` and `decompress_zst_sync()`. Disposition:
    `stream_zst_to_temp()` is now the file-level API; `decompress_zst_sync()`
    remains only for bounded DATA object payloads, not file opening.

Python pre-code reviewer round 3:

- Result: gate failed; do not implement Python yet.
- Votes:
  - `glm`: `READY FOR IMPLEMENTATION`.
  - `kimi`: `READY FOR IMPLEMENTATION`.
  - `mimo`: `READY FOR IMPLEMENTATION`.
  - `qwen`: `READY FOR IMPLEMENTATION`.
  - `minimax`: `PLAN NOT READY`.
  - `deepseek`: `READY FOR IMPLEMENTATION`.
- Accepted blockers and dispositions:
  - Verification bytes adapter contract was still too thin. Disposition:
    `_AccessorBytesAdapter` is now named in `python/journal/_verify_adapter.py`
    and its `__len__`, integer indexing, bounded byte-slice indexing, and
    chunked HMAC responsibilities are explicit.
  - `_read_journal_file_bytes()` removal was not mechanical. Disposition:
    `verify_file()` and `verify_file_with_key()` must use `FileReader.open()`
    plus `_AccessorBytesAdapter`; `_read_journal_file_bytes()` must be removed
    from `python/journal/verify.py`, and tests must prove it cannot be called
    by SDK verification entry points.
  - zstd file-level API remained ambiguous. Disposition:
    `decompress_zst_to_temp()` must be removed from production code, not
    deprecated. `stream_zst_to_temp()` is the only production file-level zstd
    API, and exact migration points are recorded for reader, verifier, and
    Netdata paths.
  - Row arena wording allowed a growable `bytearray` behind exported
    memoryviews. Disposition: row arena now requires fixed-size bytearray
    segments, never resized while memoryviews can exist, with segment stats and
    a focused fixed-segment test.

Python pre-code reviewer round 4:

- Result: gate passed; Python implementation may start.
- Reviewer outputs:
  `.local/agent-reviews/sow-0108-python-precode-round4/`.
- Votes:
  - `glm`: `READY FOR IMPLEMENTATION`.
  - `kimi`: `READY FOR IMPLEMENTATION`.
  - `mimo`: `READY FOR IMPLEMENTATION`.
  - `qwen`: `READY FOR IMPLEMENTATION`.
  - `minimax`: `READY FOR IMPLEMENTATION`.
  - `deepseek`: `READY FOR IMPLEMENTATION`.
- Accepted non-blocking watchpoints carried into implementation:
  - Bypass detection must scope or allowlist writer-only mmap internals such
    as `writer_arena.py`; reader, Explorer, Netdata, verification, facade, and
    directory code remain forbidden bypass surfaces.
  - `_read_data_payload_at()` remains a bytes-compatible wrapper for existing
    Python callers that use bytes-only methods, while new lower-level hot paths
    use `_read_data_payload_temp()` / `_read_data_payload_row()`.
  - `_hmac_object()` payload-body HMAC updates must use the adapter's chunked
    `update_hmac()` path instead of materializing large DATA/FIELD/ENTRY
    payload slices.
  - `verify_file_with_key()` should avoid avoidable double-open /
    double-decompress behavior when practical in the implementation.
  - Documentation and stats should make retired-window memory visible because
    callers that retain views beyond row lifetime can temporarily extend the
    practical memory envelope until the bounded retired-window cleanup succeeds.

### Python Implementation Evidence - 2026-06-14

Status: local Python implementation complete and accepted. Production review
round 1 found two valid hot-path issues; both were fixed and locally
revalidated. Production review round 2 passed with all six reviewers voting
`PRODUCTION GRADE`.

Implemented shape:

- Added `python/journal/reader_access.py` as the bounded reader accessor layer.
  It provides `ReaderOptions`, `auto`, explicit `mmap`, explicit `read-at`,
  rolling window caches, row-pinned windows, bounded retired mmap windows,
  selected-backend stats, and a fixed-segment row arena for compressed DATA or
  oversized row-lifetime fallbacks.
- Replaced Python `FileReader` whole-file mmap ownership with accessor-backed
  reads in `python/journal/reader.py`.
  - `FileReader.open(..., options=None)` now opens through the accessor.
  - Uncompressed DATA can be returned as a row-lifetime memoryview.
  - Existing byte-returning methods keep compatibility through
    `_read_data_payload_at()`.
  - Row-scoped pins and arenas clear on row movement, seek, position switch, or
    close, not while the caller is still on the same row.
- Added `python/journal/_verify_adapter.py` and migrated
  `python/journal/verify.py` away from whole-file byte reads. Object-graph
  verification and sealed TAG/HMAC verification now read bounded ranges through
  the accessor adapter; HMAC body ranges are fed in chunks.
- Replaced file-level zstd decompression in `python/journal/compress.py` with
  `stream_zst_to_temp()`. DATA-level `decompress_zst_sync()` now accepts only
  bytes-like compressed DATA payloads.
- Migrated directory reader, facade, Explorer, and Netdata header probing to the
  accessor-compatible API surface. Netdata header metadata reads only bounded
  header bytes and does not mmap/read a whole journal file.
- Exported `ReaderOptions`, `READER_ACCESS_AUTO`, `READER_ACCESS_MMAP`, and
  `READER_ACCESS_READ_AT` from `python/journal/__init__.py`.
- Extended `python/cmd/reader_core_bench.py` so benchmark options now construct
  real `ReaderOptions` and report `access_stats`, including selected backend,
  mapped bytes, read-buffer bytes, window misses, evictions, visible size,
  window size, and row-arena metrics. This prevents benchmark reports from
  treating backend flags as cosmetic metadata.

Focused tests added or migrated:

- Existing refresh rollback test now proves failed refresh restores visible
  bounds and preserves same-row returned payload data.
- Existing corrupt-entry test now proves oversized ENTRY rejection through the
  accessor path, not a synthetic `_buffer`.
- New explicit mmap test proves forced `mmap` mode reads entries.
- New explicit `read-at` test proves current-row memoryview data survives
  eviction pressure with `window_size=64` and `max_windows=1`.
- New `.journal.zst` test proves whole-file zstd input is stream-expanded to a
  temporary journal and then read through the bounded accessor.
- New default-mode test proves a normal Python reader selects rolling mmap when
  mmap is available on the local Linux host.
- New auto-fallback test forces the initial mmap probe to fail, proves `auto`
  falls back to rolling `read-at`, proves explicit `read-at` still works, and
  proves explicit `mmap` fails clearly instead of silently changing backend.
- New snapshot-bounds test proves a snapshot reader does not observe appended
  entries after query/open start, while the live reader test continues to prove
  controlled refresh sees published appends.
- New same-base growth test proves a row-pinned window is not replaced when a
  larger same-base temporary view is needed; the temporary path uses scratch
  instead.
- New large sparse-file test proves the accessor stays inside
  `window_size * max_windows` mapped/read-buffer budget even when file size is
  much larger.
- New fixed-segment row-arena test proves same-row compressed/oversized data
  storage does not resize already-returned buffers.
- New oversized-payload test proves DATA larger than a window is served through
  the row arena and remains valid for the row.
- New directory-options test proves directory readers propagate `ReaderOptions`
  to every file reader.
- New verification-adapter test proves sealed/HMAC verification can feed
  bounded chunks without whole-file reads.
- New bypass-scan test fails if production Python reader, facade, Explorer,
  Netdata wrapper, or verifier paths reintroduce whole-file reader bypasses such
  as `_buffer`, `_mmap`, direct `mmap.mmap`, `_read_journal_file_bytes()`, or
  removed `decompress_zst_to_temp()`.

Local validation:

- `python -m py_compile python/journal/reader_access.py python/journal/reader.py python/journal/directory_reader.py python/journal/facade.py python/journal/verify.py python/journal/netdata.py python/journal/compress.py python/journal/_verify_adapter.py python/cmd/reader_core_bench.py python/test_reader_facade.py python/test_all.py`: passed.
- `.local/python-venv/bin/python python/test_all.py`: passed.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed; verdict
  `SOW initialization complete and clean`.
- Bypass scan:
  `rg -n "decompress_zst_to_temp|_read_journal_file_bytes|_buffer|_mmap|mmap\\.mmap|f\\.read\\(|read\\(\\)" python/journal`
  found no old reader/verify/netdata whole-file bypass. Remaining hits are the
  new reader accessor, writer-only mmap code, optional platform helper reads, or
  directory-writer header reads.
- `python3 tests/interoperability/run_matrix.py --writers python --readers python stock --entries 10`:
  passed, 11/11 checks, systemd `260 (260.1-2-manjaro)`.
- `python3 tests/interoperability/run_directory_matrix.py --readers python stock`:
  passed.
- `python3 tests/interoperability/run_verify_matrix.py --skip-build`: passed,
  0 failures across stock, Go, Rust, Node, and Python readers; positive count
  9, negative count 12, systemd `260 (260.1-2-manjaro)`.
- `.local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py --readers python stock`:
  passed, 27/27 checks, including `.journal.zst` directory and sealed verify
  cases.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: passed,
  including Python file/directory query parity and follow/tail cases against
  stock, Rust, Go, and Node journalctl rewrites.
- `python3 tests/interoperability/run_live_matrix.py --entries 10 --readers python stock --features regular zstd compact sealed`:
  passed, 16/16 feature/writer combinations across Go, Rust, Node, and Python
  writers with Python and stock readers.
- Benchmark smoke:
  `.local/python-venv/bin/python python/cmd/reader_core_bench.py --input .local/sow0108-python-bench/smoke.journal --mode sdk-payloads --surface file --mmap-strategy mmap --window-size 65536 --bounds snapshot`
  read 100 records and 3,200 fields; `access_stats.selected_backend=mmap`,
  `mapped_bytes=266816`, `read_buffer_bytes=0`, `max_windows=4`.
- Benchmark smoke:
  `.local/python-venv/bin/python python/cmd/reader_core_bench.py --input .local/sow0108-python-bench/smoke.journal --mode sdk-payloads --surface file --mmap-strategy read-at --window-size 65536 --bounds snapshot`
  read the same 100 records and 3,200 fields;
  `access_stats.selected_backend=read-at`, `mapped_bytes=0`,
  `read_buffer_bytes=262144`, `max_windows=4`.

Python production review round 1:

- Reviewer outputs:
  `.local/agent-reviews/sow-0108-python-prod-round1/`.
- Votes:
  - `glm`: `PRODUCTION GRADE`.
  - `mimo`: `PRODUCTION GRADE`.
  - `qwen`: `PRODUCTION GRADE`.
  - `deepseek`: `PRODUCTION GRADE`.
  - `kimi`: `NOT PRODUCTION GRADE`.
  - `minimax`: `NOT PRODUCTION GRADE`.
- `kimi` blocker:
  `_ReadAtAccessor._read_window()` allocated a zero-filled `bytearray(length)`,
  then copied bytes returned by `read_at()` into it. This violated the hot-path
  no-extra-copy intent for the rolling positioned-read fallback.
  - Disposition: accepted and fixed. `_ReadAtAccessor._read_window()` now
    stores the `bytes` returned by `read_at()` directly in `_ReadAtWindow` after
    checking for short reads. This removes the redundant allocation and copy.
- `minimax` blocker:
  scalar helpers `u8()`, `u32()`, and `u64()` called `read_bytes()`, forcing
  temporary `bytes` allocation for every scalar object/header read.
  - Disposition: accepted and fixed. The scalar helpers now use
    `temp_view()` directly and parse from the memoryview.
- Non-blocking visibility improvement:
  `access_stats()` now exposes whether the Python runtime is using true
  `os.pread` for the read-at backend, so fallback diagnostics show whether the
  explicit `read-at` path is a positioned read or the locked seek/read
  compatibility fallback.
- Reviewer-process caveat:
  the `kimi` run emitted a final `NOT PRODUCTION GRADE` review and then spawned
  another nested reviewer command despite the read-only no-recursion prompt.
  The exact process tree was stopped after preserving the final output. The
  preserved final verdict above is treated as the authoritative `kimi` vote for
  round 1.

Post-round-1-fix validation:

- `python -m py_compile python/journal/_platform_io.py python/journal/reader_access.py python/journal/reader.py python/journal/facade.py python/cmd/reader_core_bench.py python/test_reader_facade.py python/test_all.py`:
  passed.
- `.local/python-venv/bin/python python/test_all.py`: passed after replacing
  the cached `os.pread` availability constant with dynamic detection, so
  existing monkeypatch tests for the locked seek/read fallback still work.
- `git diff --check`: passed.
- Reader-core benchmark smoke after fixes:
  - explicit `mmap`: 100 records, 3,200 fields,
    `access_stats.selected_backend=mmap`, `mapped_bytes=266816`,
    `read_buffer_bytes=0`, `read_at_uses_pread=true`.
  - explicit `read-at`: 100 records, 3,200 fields,
    `access_stats.selected_backend=read-at`, `mapped_bytes=0`,
    `read_buffer_bytes=262144`, `read_at_uses_pread=true`.
- `python3 tests/interoperability/run_matrix.py --writers python --readers python stock --entries 10`:
  passed, 11/11 checks.
- `python3 tests/interoperability/run_directory_matrix.py --readers python stock`:
  passed.
- `.local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py --readers python stock`:
  passed, 27/27 checks.
- `python3 tests/interoperability/run_verify_matrix.py --skip-build`: passed,
  status `PASS`, 0 failures, positive count 9, negative count 12.
- `python3 tests/interoperability/run_live_matrix.py --entries 10 --readers python stock --features regular zstd compact sealed`:
  passed, 16/16 feature/writer combinations.
- `python3 tests/interoperability/run_journalctl_query_matrix.py`: first
  parallel attempt failed before validation with `Text file busy` while another
  matrix was rebuilding `.local/interoperability/bin/rust-journalctl`; the
  serialized rerun passed with status `PASS` and 0 failures.

Python production review round 2:

- Reviewer outputs:
  `.local/agent-reviews/sow-0108-python-prod-round2/`.
- The reviewer models for this round were read from `~/.AGENTS.md` before the
  run because the user explicitly asked for the latest global reviewer model
  list:
  - `glm`: `llm-netdata-cloud/glm-5.2-max`
  - `kimi`: `llm-netdata-cloud/kimi-k2.7-code`
  - `mimo`: `llm-netdata-cloud/mimo-v2.5-pro`
  - `qwen`: `llm-netdata-cloud/qwen3.7-plus`
  - `minimax`: `llm-netdata-cloud/minimax-m3-coder`
  - `deepseek`: `llm-netdata-cloud/deepseek-v4-pro`
- Votes:
  - `glm`: `PRODUCTION GRADE`.
  - `kimi`: `PRODUCTION GRADE`.
  - `mimo`: `PRODUCTION GRADE`.
  - `qwen`: `PRODUCTION GRADE`.
  - `minimax`: `PRODUCTION GRADE`.
  - `deepseek`: `PRODUCTION GRADE`.
- Gate result: passed. The Python phase is accepted for SOW-0108.
- Non-blocking observations carried forward as watchpoints:
  - Read-at window misses allocate a fresh bounded `bytes` buffer per window
    replacement. This is correct after the redundant-copy fix and remains
    bounded by `max_windows`, but a future buffer pool could reduce GC pressure
    if profiling shows this fallback path matters.
  - Existing Python owned-result APIs and facade compatibility still copy to
    `bytes` by contract. This is accepted for Python compatibility and is not
    the low-level zero-copy row-view path.
  - Extra focused tests could be added later for compressed DATA under
    eviction pressure, retired-window cap overflow, direct mmap row-pinning
    with `max_windows=1`, and Windows-specific mmap failure modes. Existing
    tests and matrices were sufficient for this phase.
  - The Python live reader refreshes visible file size at controlled points but
    does not yet implement a separate preallocation-aware stale-header
    invalidation path. Existing live matrix coverage passed for supported live
    append behavior. Treat this as a watchpoint if real Python live readers see
    preallocated-file stale-header behavior.
  - Some `lz4` builds may prefer `bytes` over `memoryview`; local validation
    accepted `memoryview`, and this is bounded DATA-level decompression, not a
    whole-file path.

Validation caveat:

- Running `python3 tests/interoperability/run_mixed_directory_matrix.py
  --readers python stock` with system Python failed before reader validation
  because that interpreter lacked `lz4.block`. The same command passed with the
  repository Python venv, which contains the Python compression dependencies.

### Node.js Pre-Code Design Analysis - 2026-06-14

Status: Node.js pre-code reviewer round 5 passed the implementation-readiness
gate. Node.js implementation may start from the plan below.

Ground-truth facts from the current Node.js implementation:

- `FileReader` owns a whole-file resident Buffer:
  `node/src/lib/reader.js:52`, `node/src/lib/reader.js:54`.
- `FileReader.open()` reads normal journals with `safeReadFileSync()` and
  `.journal.zst` temporary journals with `safeReadFileSync()` after
  decompression: `node/src/lib/reader.js:78`, `node/src/lib/reader.js:83`,
  `node/src/lib/reader.js:85`, `node/src/lib/reader.js:87`.
- Reader refresh still reloads the whole file into a new Buffer:
  `node/src/lib/reader.js:218`, `node/src/lib/reader.js:222`,
  `node/src/lib/reader.js:226`.
- Entry-array, ENTRY, DATA, FIELD, and hash-table paths read directly from
  `this.buffer`: `node/src/lib/reader.js:140`, `node/src/lib/reader.js:154`,
  `node/src/lib/reader.js:163`, `node/src/lib/reader.js:272`,
  `node/src/lib/reader.js:386`, `node/src/lib/reader.js:543`,
  `node/src/lib/reader.js:768`, `node/src/lib/reader.js:775`,
  `node/src/lib/reader.js:779`, `node/src/lib/reader.js:798`.
- `entry.js` parser helpers assume a whole Buffer with `buf.length`,
  `buf.read*()`, and `buf.slice()/subarray()`:
  `node/src/lib/entry.js:16`, `node/src/lib/entry.js:28`,
  `node/src/lib/entry.js:73`, `node/src/lib/entry.js:88`,
  `node/src/lib/entry.js:92`.
- Directory readers eagerly open per-file readers and do not accept or pass
  reader options: `node/src/lib/directory-reader.js:22`,
  `node/src/lib/directory-reader.js:27`, `node/src/lib/directory-reader.js:34`,
  `node/src/lib/directory-reader.js:40`.
- Facade open paths do not accept reader options and delegate to
  `FileReader.open()` / `DirectoryReader.open()`:
  `node/src/facade.js:224`, `node/src/facade.js:231`,
  `node/src/facade.js:235`, `node/src/facade.js:239`.
- Explorer has at least one direct `reader.buffer` probe:
  `node/src/lib/explorer.js:1036`, `node/src/lib/explorer.js:1038`,
  `node/src/lib/explorer.js:1039`.
- The Netdata wrapper has a `readerOptions` config field but does not pass it
  to `FileReader.open()`:
  `node/src/lib/netdata.js:199`, `node/src/lib/netdata.js:207`,
  `node/src/lib/netdata.js:216`, `node/src/lib/netdata.js:1257`,
  `node/src/lib/netdata.js:2306`.
- Verification reads the whole file and then verifies whole Buffers:
  `node/src/lib/verify.js:43`, `node/src/lib/verify.js:45`,
  `node/src/lib/verify.js:67`, `node/src/lib/verify.js:116`,
  `node/src/lib/verify.js:118`, `node/src/lib/verify.js:150`,
  `node/src/lib/verify.js:155`, `node/src/lib/verify.js:157`.
- Sealed verification HMAC currently walks object bodies by slicing the
  whole-file Buffer:
  `node/src/lib/verify.js:455`, `node/src/lib/verify.js:457`,
  `node/src/lib/verify.js:478`, `node/src/lib/verify.js:482`,
  `node/src/lib/verify.js:552`, `node/src/lib/verify.js:554`,
  `node/src/lib/verify.js:563`, `node/src/lib/verify.js:570`,
  `node/src/lib/verify.js:575`.
- Object-graph verification is whole-Buffer based:
  `node/src/lib/verify-graph.js:62`, `node/src/lib/verify-graph.js:67`,
  `node/src/lib/verify-graph.js:92`, `node/src/lib/verify-graph.js:103`,
  `node/src/lib/verify-graph.js:173`, `node/src/lib/verify-graph.js:319`.
- The Node reader benchmark accepts window/access arguments but ignores them:
  `node/cmd/reader_core_bench.js:58`, `node/cmd/reader_core_bench.js:61`,
  `node/cmd/reader_core_bench.js:144`, `node/cmd/reader_core_bench.js:160`,
  `node/cmd/reader_core_bench.js:206`.
- TypeScript definitions expose no reader options today:
  `node/index.d.ts:79`, `node/index.d.ts:118`, `node/index.d.ts:653`,
  `node/index.d.ts:702`.
- The Node conformance adapter opens readers directly and must either use
  default reader options intentionally or participate in option propagation
  where the adapter CLI exposes reader options:
  `node/adapter/index.js:175`, `node/adapter/index.js:257`,
  `node/adapter/index.js:300`, `node/adapter/index.js:386`,
  `node/adapter/index.js:398`, `node/adapter/index.js:426`,
  `node/adapter/index.js:437`, `node/adapter/index.js:471`.

Official/runtime API evidence:

- Node.js v26.3.0 official `fs` docs expose positioned synchronous reads via
  `fs.readSync(fd, buffer, offset, length[, position])`; `position` can be a
  number, bigint, or null, and the caller provides the destination Buffer.
  Source checked: `https://nodejs.org/api/fs.html#fsreadsyncfd-buffer-offset-length-position`.
- Node.js v26.3.0 official `zlib` docs say compression/decompression is built
  around Streams and include Zstd support.
  Source checked: `https://nodejs.org/api/zlib.html`.
- Node.js v26.3.0 official `Buffer` docs state `Buffer.subarray()` returns a
  new Buffer referencing the same memory as the original.
  Source checked: `https://nodejs.org/api/buffer.html#bufsubarraystart-end`.
- Local Node runtime for this SOW is `v26.2.0`. It exposes
  `createZstdDecompress` and `zstdDecompressSync` in `node:zlib`.
- Local Node runtime has `fs.readSync()` but no `fs.mmap` function. The
  official `fs` API index also has no mmap entry. Therefore Node core does not
  provide a production mmap API for this SOW. Verified locally with
  `node -e "console.log(Object.keys(require('fs')).filter(k => /mmap/i.test(k)))"`,
  which returned an empty list on Node `v26.2.0`.

Node.js design decisions recorded before code:

1. Node.js implements a rolling positioned-read accessor, not mmap.
   - Rationale: Node core has positioned reads, Buffer views, and Zstd streams,
     but no mmap API. The project forbids native runtime addon loading unless
     the user explicitly changes that policy.
   - Implication: Node cannot match Rust/Go mmap mechanics, but it can match
     the bounded-window, row-lifetime, and snapshot/live semantics.
   - Risk: positioned reads pay syscall/copy cost on window misses. This is
     unavoidable without accepting a native mmap dependency.

2. Node.js explicit `accessMode: "mmap"` fails clearly.
   - Rationale: silently mapping `"mmap"` to read-at would violate the user's
     requirement that mmap be mandatory when available and explicit modes not
     silently downgrade.
   - Implication: `accessMode: "auto"` and `accessMode: "read-at"` work;
     `accessMode: "mmap"` throws an unsupported-backend error in Node.js core.
   - Error contract: add `UnsupportedAccessModeError extends Error` from
     `node/src/lib/reader-access.js`. `FileReader.open()`,
     `DirectoryReader.open()`, `DirectoryReader.openFiles()`, facade open
     helpers, benchmark opens, adapter opens, and Netdata opens must surface the
     same error class when the normalized options explicitly request
     `accessMode: "mmap"`.
   - Risk: callers expecting a fake mmap mode will need to use `auto` or
     `read-at`. This is correct because Node currently has no real core mmap.

3. Node.js default `accessMode: "auto"` selects rolling positioned reads.
   - Rationale: this is the only core runtime backend that can meet the
     bounded-memory contract.
   - Implication: benchmark and stats must report selected backend
     `read-at`, not `buffer`, so no report can pretend a whole Buffer path is
     still valid.
   - Risk: existing benchmark CLI values such as `--mmap-strategy buffer` need
     compatibility handling. The value may be accepted as a deprecated alias
     for `auto`, but the reported selected backend must be `read-at`.

4. Node.js exposes an idiomatic `ReaderOptions` object.
- Proposed fields:
  - `accessMode: "auto" | "read-at" | "mmap"`.
  - `bounds: "live" | "snapshot"`.
  - `windowSizeBytes`.
  - `maxWindows`.
  - `maxRowArenaBytes`.
  - `rowArenaSegmentBytes`.
   - Defaults should mirror the accepted Python/Go values unless local Node
     benchmarking proves a different value is needed before production review:
     32 MiB windows, four active windows, and 256 MiB row arena.
   - Public open APIs accept options in a backward-compatible optional
     parameter:
     `FileReader.open(path, options = {})`,
     `DirectoryReader.open(path, options = {})`,
     `DirectoryReader.openFiles(paths, options = {})`,
     `SdJournal.open*(_, flags?, options?)`, and benchmark/Netdata paths.
   - TypeScript definitions must document the options and the returned access
     stats.

5. Node.js introduces `node/src/lib/reader-access.js`.
   - Responsibilities:
     - open and close the file descriptor;
     - record visible size at open for snapshot bounds;
     - refresh visible size only at controlled live points;
   - perform positioned `readSync()` into bounded window Buffers;
   - align windows to configured boundaries;
   - track LRU windows and eviction;
     - pin windows that back current-row returned Buffer views;
     - maintain a row-scoped append-only arena for compressed DATA and
       cross-window/oversized row-returned payloads;
   - expose scalar helpers (`u8`, `u32`, `u64`), temporary views, row views,
     copied bytes, chunk iterators for HMAC, and access stats.
   - The accessor must never call `safeReadFileSync()` for production reads.
   - The accessor must never allocate per scalar read. Scalar reads use an
     internal fixed scratch Buffer or a window view.
   - Window bases are offset-relative and rounded down to the configured
     window-size boundary. No OS page alignment is required because Node uses
     positioned `readSync()`, not mmap.
   - A short read at the visible file end is normal and produces a truncated
     final window. A short read before the accessor-visible size is a
     controlled corruption/short-read error.
   - Window misses allocate or reuse one bounded window Buffer. They do not
     allocate a second copy of the same bytes. Evicted unpinned window Buffers
     may be reused after row advance; pinned current-row windows must never be
     overwritten or reused.
   - Window-reuse invariant: a window Buffer may be overwritten only when the
     LRU candidate has no current-row pin and no returned same-row `Buffer`
     view can reference its underlying memory. This is the Node equivalent of
     Rust's row-pinned window guarantee. A temporary parse view that does not
     escape the immediate parse may use scratch memory, but a row-returned view
     must pin the backing window until row advance.
   - Temporary reads may return a borrowed window slice only when the requested
     range fits inside one loaded window. Cross-window, oversized, or
     all-windows-pinned temporary reads must use bounded scratch/copy memory and
     must not increase `maxWindows`.
   - If a temporary parse read needs a new window while all configured windows
     are row-pinned, the accessor uses bounded scratch/copy memory for that
     temporary read instead of exceeding `maxWindows` or overwriting pinned
     row views.

6. Row lifetime is explicit state.
   - `next()`, `previous()`, seek operations, file switch, and `close()` clear
     row pins, row arena segments, and current-row DATA enumeration state.
   - `enumerateEntryPayload()`, `visitEntryPayloads()`, `getEntryPayload()`,
     and Explorer row scanning return Buffer views valid until the next row
     advance or close.
   - Uncompressed DATA returns a Buffer `subarray()` from a pinned window when
     the full payload fits one window.
   - Compressed DATA expands into row arena memory and returns a row-arena
     Buffer view.
   - DATA that crosses a window boundary or is larger than one window is copied
     into row arena memory before returning.
   - Owned `getEntry()` may keep returning owned `Buffer.from()` copies for
     compatibility; it must not be used to prove low-level zero-copy behavior.
   - Same-row operations must not clear row pins or row arena memory. This
     includes `entryDataRestart()`, `clearEntryDataState()`,
     end-of-enumeration, `getEntry()`, `visitEntryPayloads()`,
     `collectEntryPayloads()`, and `getEntryPayload()`. These operations may
     reset only enumeration cursors or owned-result caches.
   - Row arena storage uses fixed-size Buffer segments, default 1 MiB, that are
     allocated once and never resized while any returned view may exist.
     Appending compressed DATA or cross-window payload copies writes into the
     current segment or allocates a new fixed segment. Returned arena
     `Buffer.subarray()` views therefore keep stable backing memory until row
     advance or close.
   - `maxRowArenaBytes` limits total live row-arena segment bytes. If one row
     exceeds the limit, the reader returns a controlled row-arena-limit error
     instead of exhausting memory.
   - Node.js does not use Python-style retired mmap windows. `Buffer.subarray()`
     keeps the underlying ArrayBuffer alive through V8 GC, so there is no
     `mmap.close()` / `BufferError` equivalent. Post-row retained Buffer views
     are outside the documented contract and may observe stale data after row
     advance.

7. Parser migration should be accessor-backed, not whole-buffer emulation.
   - Keep existing `entry.js` exported whole-Buffer helpers for tests and
     direct callers that already provide an owned Buffer.
   - Add accessor-backed parser helpers, either in a new internal module or as
     private `FileReader` methods:
     - `_readObjectHeaderAt(offset)`;
     - `_readEntryObjectAt(offset, includeItems)`;
     - `_readDataHeaderAt(offset)`;
     - `_readDataPayloadTemp(offset)` for compressed-hash or temporary parsing;
     - `_readDataPayloadRow(offset)` for row-lifetime returned payloads;
     - `_readFieldObjectAt(offset)`;
     - `_readEntryArrayObject(offset)`;
     - `_readEntryArrayItemOffset(byteOffset)`;
     - `_readHashBucketOffset(tableOffset, bucket)`.
   - Existing exported `parseEntryObject(buf, offset, compact)`,
     `parseDataObject(buf, offset, compact)`, and
     `parseDataPayload(buf, offset, compact)` remain source-compatible for
     callers that already own a Buffer.
   - Do not create a fake object with `.length` and `.read*()` that silently
     pages in arbitrary ranges and hides whole-file behavior from tests.

8. Verification must be migrated with the reader, not left as a whole-file
   exception.
   - Add `node/src/lib/verify-adapter.js` with an accessor-backed byte source.
     Required interface:
     - `get length()`;
     - `u8(offset)`;
     - `u32(offset)`;
     - `u64(offset)`;
     - `bytes(offset, length)` returning a bounded Buffer copy for small
       metadata or hash payloads;
     - `view(offset, length)` returning a temporary bounded view when safe;
     - `updateHmac(hmac, offset, length, chunkSize = 1 << 20)` updating the
       HMAC in bounded chunks without whole-file slices.
   - `verifyObjectGraph()` may keep accepting owned Buffers for compatibility,
     but `verifyFile()` / `verifyFileWithKey()` must verify through bounded
     accessor reads.
   - The byte-source object is the canonical verifier input. A direct Buffer
     adapter is allowed for `verifyObjectGraph(data)` compatibility and tests,
     but file-path verification must construct the byte source from the
     bounded accessor. `GraphVerifier` must not retain production direct
     `this.data.read*()`, `this.data.slice()`, or `this.data.subarray()`
     whole-Buffer paths after migration.
   - `node/src/lib/verify-adapter.js` is the single file-based verification
     input contract. `verifyFile()` and `verifyFileWithKey()` construct one
     accessor-backed byte source and pass it to graph verification, sealed HMAC
     verification, and strict ENTRY/DATA verification. The strict verification
     path must accept this byte source directly; it must not open a separate
     `FileReader` and must not read or retain a whole-file Buffer.
   - `verifyFile()` must be rewritten over the same accessor-backed byte-source
     helper as `verifyFileWithKey()`. It must remove the current
     `FileReader.open(path); const buf = r.buffer` strict-verification path
     and must not access a reader-owned whole-file Buffer.
   - Seal/HMAC verification updates HMAC state in bounded chunks from the
     accessor, not by slicing the whole file.
   - `verifyTagHmac()`, `hmacSealedObjectRange()`, and `hmacObject()` must be
     migrated explicitly. Their current `data.slice(...); hm.update(...)`
     object-body paths must become byte-source `updateHmac()` calls, including
     DATA, FIELD, ENTRY, and TAG object body ranges.
   - Strict entry/DATA verification uses accessor-backed parse helpers.
   - `verifyFileWithKey()` must not do the current double whole-file read. It
     should create one verification byte source for graph verification, sealed
     HMAC verification when needed, and strict entry/DATA verification.
   - `verify-graph.js` must be refactored so `GraphVerifier` reads through the
     byte-source methods. Direct `Buffer` callers can be supported by wrapping
     the Buffer in the same byte-source interface.

9. `.journal.zst` whole-file support must stream to a temp `.journal`.
   - The current public Node.js reader/verify/header APIs are synchronous, and
     Node core Zstd streaming is stream/async based. Do not silently make
     `FileReader.open()`, `verifyFile()`, or `readFileHeader()` async in this
     SOW.
   - Add `node/src/lib/zst-stream.js` and replace `decompressZstToTemp()` for
     reader-open/header/verify use with a synchronous wrapper around a
     worker-thread streaming pipeline:
     - main thread creates the temp directory and output path;
     - worker thread runs `pipeline(createReadStream(input),
       createZstdDecompress(), createWriteStream(output))`;
     - a `SharedArrayBuffer` status word plus `Atomics.wait()` / notify lets
       the synchronous caller wait without converting the public API to async;
     - worker writes only a bounded sanitized error message to a temp-sidecar
       path on failure;
     - main thread owns temp cleanup on success and failure.
   - Required helper shape:
     - `streamZstToTempSync(inputPath, options = {})` returns
       `{ path, cleanup }`, where `path` is the decompressed temporary journal
       path and `cleanup()` is idempotent.
     - The worker never deletes the temp directory. It only writes the output
       file and optional bounded sidecar error. The main thread wrapper and
       callers use one `finally` path to call `cleanup()`, avoiding cleanup
       ownership races.
     - `options.timeoutMs` is bounded and documented. The default may be high
       enough for large files, but the timeout must be finite and surfaced in
       accessor/header/verify errors as a sanitized class of failure.
   - `Atomics.wait()` must use a bounded timeout and the main thread must
     handle worker startup failure, worker `error`, worker early `exit`, and
     timeout by terminating the worker where needed and cleaning temp paths.
   - The worker-thread path uses Node core only. It does not execute external
     programs, load native addons, or probe host state.
   - If worker threads or Zstd streaming are unavailable in a Node runtime,
     `.journal.zst` open/verify/header calls fail clearly with an unsupported
     whole-file-zstd-streaming error. Normal `.journal` readers are unaffected.
   - After streaming, open the temporary `.journal` through the same bounded
     accessor using the caller's normalized `ReaderOptions`; the temporary
     accessor's visible size is fixed at streaming completion and uses
     `bounds: "snapshot"` regardless of the caller's live-tail preference,
     because a decompressed temporary file is immutable for that query.
   - Remove `decompressZstToTemp()` from production code rather than merely
     deprecating it. Reader open, verification, directory-reader discovery,
     Netdata header probing, journalctl rewrite, adapter paths, and
     `readFileHeader()` must use the streaming helper for whole-file zstd
     inputs.
   - Keep `decompressZstSync()` only as an explicit bytes helper if needed by
     direct callers/tests; it must not be used by production reader open,
     directory reader, Netdata wrapper, journalctl rewrite, or verification.
   - `readFileHeader(path)` is an exported production helper and must be
     migrated too. For `.journal` input it reads bounded header bytes through
     the accessor or direct bounded header read. For `.journal.zst` input it
     uses the same sync worker streaming helper to produce a temp journal, then
     reads only the header from that temp and cleans up.
   - Empty input, truncated zstd frames, trailing-garbage decode failures, and
     I/O failures must produce controlled sanitized open/verify/header errors.
     Worker errors must delete partial temp output and temp directories before
     returning. Concurrent opens of the same `.journal.zst` must use distinct
     temp directories.

10. Directory/facade/Explorer/Netdata/journalctl all use the same accessor.
    - Directory readers pass options into every `FileReader.open()`.
    - Facade open methods accept optional options and preserve old calls.
    - Explorer removes direct `reader.buffer` access; compression-state probes
      use a reader method backed by accessor scalar reads.
    - Netdata passes `config.readerOptions` to every file reader. The existing
      `FileReader.open(pathStr)` call sites in `node/src/lib/netdata.js` must
      become `FileReader.open(pathStr, config.readerOptions ?? {})` or an
      equivalent helper that preserves the configured options.
    - `node/cmd/journalctl/index.js` must pass options where it constructs
      readers if command-line options are added; otherwise it uses defaults.
    - `node/adapter/index.js` must keep journal-file access through
      `FileReader.open(path, options)` / `DirectoryReader.open(path, options)`.
      If the adapter CLI exposes no reader-option flags in this SOW, it must
      still use a single normalized default options object so the adapter
      exercises the same accessor path as production readers.
    - `node/cmd/reader_core_bench.js` must translate its parsed
      `--window-size`, `--max-windows`, `--mmap-strategy` / `--access-mode`,
      and `--bounds` arguments into real `ReaderOptions`, pass them to file,
      directory, SDK, and facade reader opens, and report the resulting
      accessor stats.
    - `getEntry()` may keep returning owned `Buffer.from()` copies for
      compatibility, but its owned results must survive row advance and must
      not be implemented as row-lifetime views.
    - `_resetCachedEntryDataState()` and `_invalidateEntryDataState()` must
      reset only DATA enumeration cursors / caches. They must not call the
      accessor row-clear operation. Row pins and row arena are cleared only by
      row advance, successful seek, file switch, and close.
    - `_enumerateFieldsIndexed()`, `_findFieldHeadDataOffset()`,
      `_readFieldObjectAt()`, `_readDataHeaderAt()`, `_readEntryArrayObject()`,
      `_readEntryArrayItemOffset()`, and the Explorer compressed-object flag
      probe must use accessor-backed scalar/object helpers. No indexed-field
      path may keep `this.buffer` or `reader.buffer`.

11. Access stats are part of validation evidence.
    - Expose at least:
      `requestedAccessMode`, `selectedAccessMode`, `selectedBackend`,
      `fallbackReason`, `bounds`, `visibleSize`, `windowSizeBytes`,
      `maxWindows`, `windowsCreated`, `windowHits`, `windowMisses`,
      `evictions`, `pinnedWindows`, `readBufferBytes`,
      `rowArenaPeakBytes`, `tempCopyBytes`, `shortReads`, and
      `readSyncUsesPosition`.
    - Benchmark output must include these stats for SDK and facade surfaces.

12. Direct refresh has explicit rollback semantics.
    - Before any refresh reads a new header or visible size, snapshot logical
      reader state, entry offsets, current index, layout cache, current-row
      pins, row arena, and accessor visible bounds.
    - Read the current header through bounded accessor/direct header reads.
    - If the header/size is unchanged, update only safe header metadata and
      preserve current-row pins.
    - If changed, reload entry-array offsets into temporary arrays using the
      accessor's refreshed visible bounds.
    - Commit new header/layout/entry arrays only after all temporary work
      succeeds.
    - On failure, restore prior visible bounds and logical reader state without
      clearing row pins or row arena for the current row.
    - The snapshot must include accessor visible bounds before temporary header
      and entry-array reload work begins. Failure restoration must restore
      those accessor bounds as well as logical reader fields.
    - Direct refresh on the same row must preserve current DATA enumeration
      cursors (`entryDataOffsets`, `entryDataIndex`, and active state) in
      addition to preserving returned row views.
    - Current-row returned Buffer views must survive a direct `refresh()` while
      the reader remains on the same row.

13. Bypass detection is mechanical.
    - Add a Node test that scans production reader surfaces and fails on
      forbidden whole-file reader patterns.
    - Forbidden production patterns include:
      `safeReadFileSync`, `readFileSync`, `decompressZstToTemp`,
      `readJournalFileForVerify`, `r.buffer`, `reader.buffer`,
      `this.buffer` inside reader-like classes, `this.data.read`,
      `this.data.slice`, `this.data.subarray`, and
      `parseEntryObject(this.buffer` / `parseDataObject(this.buffer` /
      `parseDataPayload(this.buffer`.
    - The bypass scan must be precise, not a blind repo-wide `.buffer` grep.
      Legitimate non-reader Buffer conversions such as
      `Buffer.from(value.buffer, value.byteOffset, value.byteLength)`, WASM
      bytes handling, and explicit owned bytes helper APIs are allowlisted only
      when the surrounding code is not reading a journal file through a
      reader-owned whole-file Buffer.
    - Forbidden surfaces include `node/src/lib/reader.js`,
      `node/src/lib/directory-reader.js`, `node/src/facade.js`,
      `node/src/lib/explorer.js`, `node/src/lib/netdata.js`,
      `node/src/lib/verify.js`, `node/src/lib/verify-graph.js`,
      `node/src/lib/compress.js` import/caller sites,
      `node/cmd/journalctl/index.js`, `node/cmd/reader_core_bench.js`, and
      `node/adapter/index.js`.
    - Allowlisted non-reader uses include tests, writer code, `fs-safe.js`,
      `platform.js`, `lock.js`, `xz-block.js` WASM loading, benchmark process
      status reads, and explicit bytes helper APIs such as direct
      `decompressZstSync()` when they are not used by production reader paths.
      Adapter stdin/request payload reads may stay allowlisted, but adapter
      journal-file reads must go through the reader APIs and must not access a
      reader-owned whole-file Buffer.

Implementation plan for Node.js after reviewer approval:

1. Add `reader-access.js` with option normalization, explicit unsupported mmap
   error, rolling positioned-read windows, row pinning, row arena, live/snapshot
   bounds, stats, and close/error cleanup.
2. Add streaming `.journal.zst` temp-file decompression and switch reader open,
   header read, directory read, and verification to the streaming helper.
3. Refactor `FileReader` to own an accessor instead of `this.buffer`; migrate
   every object/entry/DATA/FIELD/hash-table read to accessor-backed helpers.
4. Preserve existing owned `getEntry()` results while making low-level payload
   traversal return row-lifetime Buffer views.
5. Propagate reader options through `DirectoryReader`, facade, Netdata, CLI
   benchmark, and TypeScript definitions.
6. Migrate `verify.js` and `verify-graph.js` file-based verification to the
   bounded byte source while keeping direct Buffer verification helpers
   source-compatible.
7. Replace direct refresh with accessor-bounds snapshot/commit/restore logic
   that preserves current-row pins on same-row refresh.
8. Add bypass/same-failure tests that fail if production Node reader,
   directory reader, Explorer, Netdata wrapper, journalctl rewrite, or
   verification reintroduces `safeReadFileSync()` / whole-file Buffer reads.
9. Add focused row-lifetime, eviction, cross-window, large-object, compressed
   DATA, `.journal.zst`, live/snapshot, verification, directory, facade,
   Netdata, and benchmark tests.
10. Run local validation and benchmark smokes before production reviewer round 1.

Tests required before the Node.js phase can pass:

- Node package tests:
  - `npm_config_cache=.local/npm-cache npm test` or the repository equivalent
    that uses `.local/` cache.
- Focused accessor tests:
  - default `auto` selects `read-at`;
  - explicit `read-at` selects `read-at`;
  - explicit `mmap` fails clearly;
  - configured window size and max windows are enforced;
  - scalar reads and temporary views do not allocate per access;
  - cross-window object headers and DATA payloads work;
  - oversized or cross-window returned payloads go to row arena;
  - uncompressed same-window DATA returns a Buffer view backed by a pinned
    window;
  - compressed DATA returns row-arena memory;
  - row data remains valid after unrelated window eviction and until next row;
  - row-arena growth across multiple fixed segments does not invalidate earlier
    same-row compressed/cross-window returned Buffer views;
  - a hostile row that would exceed `maxRowArenaBytes` fails with the
    controlled row-arena-limit error without invalidating prior same-row views;
  - same-row operations reset only enumeration state and do not clear row pins;
  - `visitEntryPayloads()` callback payload views remain valid for the
    callback duration, and tests document that callers must copy data retained
    beyond the callback / current row;
  - `getEntry()` owned payload Buffers survive row advance independently of
    row-lifetime window/arena views;
  - end-of-enumeration after `entryDataRestart()` and
    `enumerateEntryPayload()` does not invalidate row-lifetime views returned
    earlier in the same row;
  - advancing clears pins and arenas;
  - snapshot readers never read beyond open size;
  - live readers refresh only at controlled tail/refresh points;
  - direct `refresh()` preserves current-row returned Buffer views and rolls
    back visible bounds on failure;
  - direct `refresh()` failure restores accessor visible bounds and preserves
    current DATA enumeration cursors when the reader remains on the same row;
  - short positioned reads and invalid offsets fail predictably;
  - close-after-error releases descriptors and temp paths.
- `.journal.zst` tests:
  - reader open streams to temp and reads through accessor;
  - verify path streams to temp and reads through accessor;
  - `readFileHeader()` on `.journal.zst` uses streaming temp decompression,
    reads only bounded header bytes from the temporary journal, and cleans up;
  - temp cleanup occurs on success and failure.
  - empty input, truncated zstd input, worker startup failure, worker timeout,
    and worker early exit/error produce controlled errors and remove partial
    temp output.
- Verification tests:
  - positive regular, compact, compressed DATA, and sealed/FSS cases pass;
  - existing negative corruption cases still fail;
  - HMAC verification does not materialize whole-file slices.
  - `verifyFile()` uses the accessor-backed byte source and has no `r.buffer`
    strict-verification path.
  - `verifyFileWithKey()` uses one accessor-backed byte source for graph
    verification, sealed HMAC verification, and strict entry/DATA verification;
    it must not reopen the same journal for a second whole-file or
    verification pass.
  - `verifyObjectGraph()` remains source-compatible for direct Buffer callers,
    while file-based graph verification uses the byte-source adapter and does
    not keep production direct `this.data.*` Buffer access.
- Bypass detection:
  - fail if production reader/verify/explorer/netdata/journalctl code paths
    call `safeReadFileSync()` or keep `reader.buffer` as a public/internal file
    content source. Test helper code may still use file reads to build fixtures.
  - fail if `node/adapter/index.js` accesses a reader-owned whole-file Buffer
    or performs journal-file reads outside `FileReader`/`DirectoryReader`;
    stdin/request payload reads may remain explicit allowlisted file reads.
  - fail if `node/src/lib/compress.js` or production import/caller sites expose
    `decompressZstToTemp()` for journal file inputs after the streaming helper
    is added.
  - fail if `node/src/lib/netdata.js` calls `FileReader.open()` without
    passing `config.readerOptions` or the normalized equivalent.
  - fail if `node/cmd/reader_core_bench.js` parses access/window options but
    does not pass them to reader opens.
- Interoperability/matrix validation:
  - `python3 tests/interoperability/run_matrix.py --writers node --readers node stock --entries 10`
  - `python3 tests/interoperability/run_directory_matrix.py --readers node stock`
  - `python3 tests/interoperability/run_verify_matrix.py --skip-build`
  - `python3 tests/interoperability/run_mixed_directory_matrix.py --readers node stock`
  - `python3 tests/interoperability/run_journalctl_query_matrix.py`
  - `python3 tests/interoperability/run_live_matrix.py --entries 10 --readers node stock --features regular zstd compact sealed`
- Benchmark smoke:
  - `node/cmd/reader_core_bench.js` in `sdk-payloads`, `sdk-entry`,
    `facade-data`, and `facade-next` modes, with explicit window size and
    snapshot bounds, reporting `selectedBackend: read-at` and bounded
    `readBufferBytes`.

Reviewer gate for Node.js:

- Run the full reviewer pool read-only against this SOW, the current Node.js
  reader/verification code, and the Node.js plan above.
- Required vote text: `READY FOR IMPLEMENTATION` or `PLAN NOT READY`.
- Reviewers must specifically check:
  - whether Node core mmap has been correctly rejected without native addons;
  - whether positioned reads can provide row-level Buffer lifetime safely;
  - whether every whole-file reader, verification, Explorer, Netdata,
    journalctl, and benchmark path is covered by the plan;
  - whether `.journal.zst` streaming is sufficiently specified;
  - whether tests are strong enough to catch accidental whole-file reads;
  - whether the plan preserves public API compatibility while adding options.

Node.js pre-code reviewer round 1:

- Gate result: failed. Node.js implementation must not start.
- Votes captured:
  - `mimo`: `READY FOR IMPLEMENTATION`.
  - `glm`: `PLAN NOT READY`.
  - `kimi`: `PLAN NOT READY`.
  - `minimax`: `PLAN NOT READY`.
  - `qwen`: output not captured due reviewer session handle loss; rerun
    required.
  - `deepseek`: output not captured due reviewer session handle loss; rerun
    required.
- Accepted blocker dispositions already incorporated into the Node.js plan:
  - Synchronous `.journal.zst` public APIs now use a worker-thread streaming
    pipeline to a temporary `.journal` instead of whole-file decompression or
    silently changing `FileReader.open()`, `verifyFile()`, or
    `readFileHeader()` to async APIs.
  - Row arena storage is now specified as fixed-size append-only Buffer
    segments, so same-row returned Buffer views cannot be invalidated by
    segment growth.
  - Same-row operations are separated from row-advance operations, so
    enumeration reset, owned `getEntry()`, visitor paths, and end-of-entry do
    not clear row pins or row arena memory.
  - Verification now has a concrete accessor-backed byte-source adapter,
    including chunked HMAC update and a migration path for `verify-graph.js`.
  - Parser migration now names concrete accessor-backed helper methods instead
    of relying on a fake whole-Buffer emulation layer.
  - Direct `refresh()` now has snapshot/temporary reload/commit/rollback
    semantics that preserve current-row returned Buffer views on failure.
  - `readFileHeader()` and `.journal.zst` header probing are explicitly in
    scope.
  - Mechanical bypass detection now names forbidden patterns, production
    surfaces, and allowlisted non-reader paths.
  - Node-specific stats use `readSyncUsesPosition`; the Node-inappropriate
    retired-window setting was removed in favor of `rowArenaSegmentBytes`.

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
- Python pre-code gate: passed in round 4. Reviewers `glm`, `kimi`, `mimo`,
  `qwen`, `minimax`, and `deepseek` all voted `READY FOR IMPLEMENTATION`.
- Python local implementation gate: passed on Python package tests, compile
  checks, reader bypass scan, writer/reader interoperability smoke, directory
  matrix, verification matrix, mixed-directory matrix, journalctl query matrix,
  live matrix, and benchmark smoke. Production review round 1 found two valid
  hot-path blockers; fixes are implemented and post-fix validation passed.
  Production review round 2 passed with all six reviewers voting
  `PRODUCTION GRADE`.
- Node.js pre-code gate: passed in round 5. Reviewers `glm`, `kimi`, `mimo`,
  `qwen`, `minimax`, and `deepseek` all voted `READY FOR IMPLEMENTATION`.
- Node.js local implementation gate: passed on syntax checks, TypeScript
  declaration checks, package tests, focused SOW-0108 tests, corrupted-fixture
  hang checks, conformance manifest loop, interoperability matrix, directory
  matrix, verification matrix, mixed-directory matrix, live matrix, journalctl
  query matrix, bypass scan, and benchmark smoke. Production review round 1
  and round 2 found valid blockers; fixes are implemented and locally
  revalidated. Production review round 3 passed with all six reviewers voting
  `PRODUCTION GRADE`.

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
- Python validation commands passed:
  - `python -m py_compile python/journal/reader_access.py python/journal/reader.py python/journal/directory_reader.py python/journal/facade.py python/journal/verify.py python/journal/netdata.py python/journal/compress.py python/journal/_verify_adapter.py python/cmd/reader_core_bench.py python/test_reader_facade.py python/test_all.py`
  - `.local/python-venv/bin/python python/test_all.py`
  - `git diff --check`
  - `.agents/sow/audit.sh`
  - `python3 tests/interoperability/run_matrix.py --writers python --readers python stock --entries 10`
  - `python3 tests/interoperability/run_directory_matrix.py --readers python stock`
  - `python3 tests/interoperability/run_verify_matrix.py --skip-build`
  - `.local/python-venv/bin/python tests/interoperability/run_mixed_directory_matrix.py --readers python stock`
  - `python3 tests/interoperability/run_journalctl_query_matrix.py`
  - `python3 tests/interoperability/run_live_matrix.py --entries 10 --readers python stock --features regular zstd compact sealed`
  - Python reader-core benchmark smoke in explicit `mmap` and explicit
    `read-at` modes with `--window-size 65536 --bounds snapshot`
- Node.js validation commands passed:
  - `node --check` on changed Node runtime and test files
  - `npm_config_cache=../.local/npm-cache npm run typecheck`
  - `npm_config_cache=../.local/npm-cache npm test`
  - full Node conformance manifest adapter loop with a 15 second per-case
    timeout
  - `python3 tests/interoperability/run_matrix.py --writers node --readers node stock --entries 10`
  - `python3 tests/interoperability/run_directory_matrix.py --readers node stock`
  - `python3 tests/interoperability/run_verify_matrix.py --skip-build`
  - `PYTHONPATH=.local/python-deps python3 tests/interoperability/run_mixed_directory_matrix.py --readers node stock`
  - `python3 tests/interoperability/run_live_matrix.py --entries 10 --readers node stock --features regular zstd compact sealed`
  - `python3 tests/interoperability/run_journalctl_query_matrix.py`
  - Node `reader_core_bench.js` smokes for `sdk-payloads`, `sdk-entry`,
    `facade-data`, and `facade-next` with 128-byte windows and snapshot bounds
  - `git diff --check`
  - `.agents/sow/audit.sh`

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
- Node.js reader-core smoke used 128-byte windows and one active window on a
  generated compact Node journal, forcing bounded positioned-read windows in
  all tested reader surfaces. `sdk-payloads`, `sdk-entry`, and `facade-data`
  read 10 records and 44 fields with identical checksum
  `11982092242414586274`; `facade-next` read 10 records. Access stats reported
  `readBufferBytes: 128` and `readSyncUsesPosition: true`.

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
- 2026-06-14 Python pre-code review rounds:
  - Rounds 1, 2, and 3 found valid `PLAN NOT READY` blockers around
    verification migration, test migration, row arena segment lifetime,
    file-level zstd streaming, Netdata header probing, and bypass detection.
  - Round 4 result: all six reviewers voted `READY FOR IMPLEMENTATION`.
- 2026-06-14 Python production review:
  - Round 1: `glm`, `mimo`, `qwen`, and `deepseek` voted
    `PRODUCTION GRADE`; `kimi` and `minimax` voted `NOT PRODUCTION GRADE`.
  - `kimi` found a redundant read-at fallback allocation/copy in
    `_ReadAtAccessor._read_window()`.
  - `minimax` found avoidable scalar-read allocation through
    `u8()`/`u32()`/`u64()` calling `read_bytes()`.
  - Disposition: both findings were accepted and fixed. Post-fix local
    validation passed.
  - Round 2: `glm`, `kimi`, `mimo`, `qwen`, `deepseek`, and `minimax` all voted
    `PRODUCTION GRADE` using the latest model names from `~/.AGENTS.md`.
  - Disposition: Python phase review gate passed.
- 2026-06-14 Node.js pre-code review round 1:
  - `mimo` voted `READY FOR IMPLEMENTATION`.
  - `glm`, `kimi`, and `minimax` voted `PLAN NOT READY`.
  - `qwen` and `deepseek` outputs were not captured due reviewer session
    handle loss.
  - Disposition: valid blockers were accepted and folded into the Node.js
    plan before round 2.
- 2026-06-14 Node.js pre-code review round 2:
  - Reviewer outputs are stored under
    `.local/agent-reviews/sow-0108-node-precode-round2/`.
  - Votes:
    - `glm`: `READY FOR IMPLEMENTATION`.
    - `kimi`: `READY FOR IMPLEMENTATION`.
    - `mimo`: `READY FOR IMPLEMENTATION`.
    - `qwen`: `READY FOR IMPLEMENTATION`.
    - `deepseek`: `READY FOR IMPLEMENTATION`.
    - `minimax`: `PLAN NOT READY`.
  - Gate result: failed. Node.js implementation must not start.
  - `minimax` blockers accepted:
    - `node/adapter/index.js` was not explicitly listed in the production
      bypass/reader-surface scope.
    - The sealed verification HMAC body path in
      `node/src/lib/verify.js:455-475` and `node/src/lib/verify.js:552-585`
      needed explicit migration wording to byte-source `updateHmac()`.
    - The Node accessor short-read and window-alignment policy needed to be
      specified.
    - `verifyFileWithKey()` needed a focused single-byte-source / no double
      reopen test requirement.
  - Disposition: accepted and fixed in the SOW. Round 3 pending with the full
    reviewer pool and the same whole-scope prompt.
- 2026-06-14 Node.js pre-code review round 3:
  - Reviewer outputs are stored under
    `.local/agent-reviews/sow-0108-node-precode-round3/`.
  - Votes:
    - `glm`: `READY FOR IMPLEMENTATION`.
    - `kimi`: `READY FOR IMPLEMENTATION`.
    - `qwen`: `READY FOR IMPLEMENTATION`.
    - `deepseek`: `READY FOR IMPLEMENTATION`.
    - `minimax`: `PLAN NOT READY`.
    - `mimo`: no final vote; the process stalled after source reads and was
      stopped by exact PID after the round was already invalidated by the
      `minimax` blocking vote.
  - Gate result: failed. Node.js implementation must not start.
  - `minimax` blockers accepted:
    - `verifyFile()` itself needed an explicit accessor-backed byte-source
      rewrite and focused test, not only `verifyFileWithKey()`.
    - `decompressZstToTemp()` needed an explicit remove/forbid requirement
      for Node production paths.
    - `readFileHeader()` needed explicit `.journal.zst` streaming and cleanup
      validation.
    - `node/adapter/index.js` and `node/src/lib/netdata.js` option propagation
      and bypass scope needed to be mechanically testable.
    - `verify-graph.js` needed the byte-source API to be canonical for file
      verification, with direct Buffer support only as a compatibility adapter.
    - Worker-thread `.journal.zst` error cases needed truncated/empty/concurrent
      temp cleanup requirements.
    - Benchmark CLI options needed to be wired to real `ReaderOptions`, not
      just parsed/reported.
    - `getEntry()` owned-copy lifetime, DATA-enumeration cursor preservation,
      indexed-field helper migration, and direct refresh accessor-bound
      rollback needed explicit implementation/test requirements.
  - Disposition: accepted and fixed in the SOW. Round 4 pending with the full
    reviewer pool and the same whole-scope prompt.
- 2026-06-14 Node.js pre-code review round 4:
  - Reviewer outputs are stored under
    `.local/agent-reviews/sow-0108-node-precode-round4/`.
  - Votes:
    - `glm`: `READY FOR IMPLEMENTATION`.
    - `kimi`: `READY FOR IMPLEMENTATION`.
    - `mimo`: `READY FOR IMPLEMENTATION`.
    - `qwen`: `READY FOR IMPLEMENTATION`.
    - `deepseek`: `READY FOR IMPLEMENTATION`.
    - `minimax`: `PLAN NOT READY`.
  - Gate result: failed. Node.js implementation must not start.
  - `minimax` blockers accepted:
    - The SOW status and round ledger were stale after round 3 and did not
      record round 4 as the current gate state.
    - The Node plan needed a stable unsupported-mmap error contract shared by
      `FileReader`, `DirectoryReader`, facade, adapter, benchmark, and Netdata
      opens.
    - The rolling window plan needed an explicit no-overwrite invariant for
      current-row returned `Buffer` views, plus a cross-window temporary read
      policy.
    - The verification plan needed to name the exact
      `node/src/lib/verify-adapter.js` byte-source contract as the single
      canonical input for `verifyFile()`, `verifyFileWithKey()`, graph
      verification, sealed HMAC verification, and strict ENTRY/DATA
      verification.
    - The `.journal.zst` plan needed a concrete `node/src/lib/zst-stream.js`
      helper API, a single temp-cleanup owner, a bounded timeout option, and
      explicit immutable snapshot bounds for temporary decompressed journals.
    - The bypass scan needed to be file-scoped and precise so legitimate
      `Buffer.from(value.buffer, ...)` conversions do not become false
      positives while reader-owned whole-file Buffer reads remain forbidden.
    - The Node no-mmap evidence needed the literal local `fs` export check.
  - Disposition: accepted and fixed in the SOW. Round 5 pending with the full
    reviewer pool and the same whole-scope prompt.
- 2026-06-14 Node.js pre-code review round 5:
  - Reviewer outputs are stored under
    `.local/agent-reviews/sow-0108-node-precode-round5/`.
  - Votes:
    - `glm`: `READY FOR IMPLEMENTATION`.
    - `kimi`: `READY FOR IMPLEMENTATION`.
    - `mimo`: `READY FOR IMPLEMENTATION`.
    - `qwen`: `READY FOR IMPLEMENTATION`.
    - `deepseek`: `READY FOR IMPLEMENTATION`.
    - `minimax`: `READY FOR IMPLEMENTATION`.
  - Gate result: passed. Node.js implementation may start.
  - Non-blocking watchpoints to carry into implementation:
    - Keep Netdata, adapter, journalctl, and benchmark option propagation
      mechanically testable at every open call site.
    - Preserve the `UnsupportedAccessModeError` contract and test it through
      all public open surfaces.
    - Ensure `verifyFileWithKey()` does not call `verifyFile(path)` as a
      subroutine and uses one accessor-backed byte source for graph,
      seal/HMAC, and strict ENTRY/DATA verification.
    - Keep `.journal.zst` cleanup ownership in the main thread and force
      snapshot bounds for decompressed temporary journals.
    - Keep bypass detection precise enough to allow legitimate
      `Buffer.from(value.buffer, ...)` conversions while rejecting reader-owned
      whole-file access.
    - Document the minimum supported Node runtime for the worker-thread Zstd
      path after implementation.
  - Disposition: pre-code gate passed. Node.js code changes can start.
- 2026-06-14 Node.js production review round 1:
  - Reviewer outputs are stored under
    `.local/agent-reviews/sow-0108-node-prod-round1/`.
  - Votes:
    - `glm`: `NOT PRODUCTION GRADE`.
    - `kimi`: `PRODUCTION GRADE`.
    - `mimo`: `PRODUCTION GRADE`.
    - `qwen`: `PRODUCTION GRADE`.
    - `deepseek`: `PRODUCTION GRADE`.
    - `minimax`: `PRODUCTION GRADE`.
  - Gate result: failed. Node.js phase cannot close until the full reviewer
    pool votes `PRODUCTION GRADE` after fixes.
  - `glm` blockers accepted:
    - Live-refresh rollback restored post-refresh accessor visible bounds
      instead of the pre-refresh bounds required by the SOW.
    - `queryUnique()` used row-lifetime DATA reads for FIELD-chain traversal
      where the SOW requires temporary/internal reads.
    - Focused Node accessor tests were below the SOW-required coverage and did
      not guard the refresh and FIELD-chain bugs.
  - Non-blocking `glm` watchpoint accepted:
    - Netdata `CombinedResult` zero-count facet backfill read
      `this.readerOptions` but the constructor never set it, so caller reader
      options were not propagated to those backfill readers.
  - Disposition: blockers and the option-propagation watchpoint are fixed.
    Focused tests and local validation passed. Round 2 pending with the full
    reviewer pool and the same whole-scope prompt.
- 2026-06-14 Node.js production review round 2:
  - Reviewer outputs are stored under
    `.local/agent-reviews/sow-0108-node-prod-round2/`.
  - Votes:
    - `glm`: `PRODUCTION GRADE`.
    - `kimi`: `PRODUCTION GRADE`.
    - `mimo`: `PRODUCTION GRADE`.
    - `deepseek`: `PRODUCTION GRADE`.
    - `minimax`: `PRODUCTION GRADE`.
    - `qwen`: `NOT PRODUCTION GRADE`.
  - Gate result: failed. Node.js phase cannot close until the full reviewer
    pool votes `PRODUCTION GRADE` after fixes.
  - `qwen` blocker accepted:
    - Explorer indexed facet/histogram FIELD-chain traversal used
      row-lifetime DATA reads in `node/src/lib/explorer.js`, the same class of
      bug as the round-1 `queryUnique()` issue.
  - Non-blocking `minimax` watchpoint accepted:
    - `CombinedResult.readerOptions` existed at runtime but was missing from
      `node/index.d.ts`.
  - Disposition: the blocker and type-surface gap are fixed. Focused test and
    full Node package validation passed. Round 3 pending with the full reviewer
    pool and the same whole-scope prompt.
- 2026-06-14 Node.js production review round 3:
  - Reviewer outputs are stored under
    `.local/agent-reviews/sow-0108-node-prod-round3/`.
  - Votes:
    - `glm`: `PRODUCTION GRADE`.
    - `kimi`: `PRODUCTION GRADE`.
    - `mimo`: `PRODUCTION GRADE`.
    - `qwen`: `PRODUCTION GRADE`.
    - `deepseek`: `PRODUCTION GRADE`.
    - `minimax`: `PRODUCTION GRADE`.
  - Gate result: passed. Node.js phase review gate passed.
  - Non-blocking watchpoints were reviewed and accepted as future-hardening or
    rejected as non-actionable for this SOW because they do not violate
    correctness, bounded-memory, row-lifetime, runtime-purity, or production
    surface contracts.

### Node.js Implementation Evidence - 2026-06-14

Status: local Node.js implementation complete, validated, reviewed, and
accepted. Production review passed in round 3.

Implemented reader-memory changes:

- Added `node/src/lib/reader-access.js`, which provides `ReaderOptions`,
  `accessMode` normalization, explicit `UnsupportedAccessModeError` for
  `mmap`, bounded rolling positioned-read windows, live/snapshot bounds,
  row-pinned windows, a row-scoped arena for compressed and cross-window DATA,
  row clearing, HMAC chunk reads, and access stats.
- Refactored `node/src/lib/reader.js` so `FileReader` owns a reader accessor
  rather than a whole-file resident Buffer. Normal `.journal` files and
  temporary decompressed `.journal.zst` files now open through the same bounded
  accessor.
- Added `node/src/lib/zst-stream.js`, which streams whole-file `.journal.zst`
  inputs to a temporary `.journal` through a synchronous worker-thread wrapper
  around Node core streams and zstd decompression. The main thread owns temp
  cleanup and terminates the worker after completion or error.
- Added `node/src/lib/verify-adapter.js` and migrated file-based verification
  in `node/src/lib/verify.js` and `node/src/lib/verify-graph.js` to a
  byte-source abstraction. File verification no longer needs a whole-file
  Buffer; direct Buffer graph verification remains available through an
  adapter.
- Migrated ENTRY, ENTRY_ARRAY, DATA, FIELD, hash-table, unique-value, Explorer
  compression-flag, and strict verification reads to accessor-backed helpers.
- Preserved owned `getEntry()` behavior through owned Buffer copies while
  current-row payload iteration returns row-lifetime Buffer views backed by
  pinned windows or the row arena.
- Propagated reader options through `DirectoryReader`, facade open methods,
  C-style facade helpers, the conformance adapter, Netdata function wrapper,
  journalctl rewrite, and `reader_core_bench`.
- Fixed Netdata combined-result zero-count facet backfill so it retains caller
  reader options instead of silently falling back to defaults.
- Updated `node/index.d.ts` and `node/README.md` with reader access options,
  stats, Node's explicit no-core-mmap contract, and the bounded memory
  envelope.
- Added focused Node tests in `node/test/chunks/sow0108-reader-access.js` for
  explicit mmap rejection, `auto` selecting `read-at`, row views surviving
  window eviction pressure, owned `getEntry()` data surviving row advance,
  option propagation through directory/facade surfaces, `.journal.zst`
  accessor use, and mechanical bypass detection.

Validation bugs found and fixed during local implementation:

- Active live readers initially reused stale cached accessor windows after a
  writer published new header/entry-array bytes without growing file size.
  Fix: `ReadAtAccessor.refreshVisibleBounds()` now drops unpinned windows at
  controlled live refresh points, while preserving row-pinned current-row
  windows.
- One corrupted systemd AFL fixture could hang in ENTRY_ARRAY chain traversal.
  Fix: Node ENTRY_ARRAY traversal now detects chain cycles and zero-progress
  segments and returns a controlled corruption error instead of spinning.
- Snapshot readers initially refreshed appended header/entry-array metadata
  when `refresh()` was called. Fix: snapshot-bounds readers now no-op refresh
  before reading current header bytes, preserving the open-time row set.

Production review round 1 fixes:

- `glm` found that failed live refresh rollback restored post-refresh visible
  bounds, not pre-refresh bounds. Fix: `_refreshEntryOffsets()` now snapshots
  logical reader/accessor state before reading refreshed header bytes and
  passes that state into reload rollback.
- `glm` found that `queryUnique()` used row-lifetime DATA reads while walking
  FIELD chains. Fix: `queryUnique()` now uses `_readDataPayloadAt(offset,
  false)` so FIELD-chain traversal uses temporary/internal reads.
- `glm` found focused coverage below the SOW-required Node accessor coverage.
  Fix: `node/test/chunks/sow0108-reader-access.js` now covers explicit
  `read-at`, `auto`, mmap rejection, temporary unique reads, row-lifetime
  preservation under window pressure, same-row enumeration restart, oversized
  and compressed row-arena paths, owned `getEntry()` lifetime, snapshot bounds,
  refresh rollback, short reads, invalid offsets, corrupted ENTRY_ARRAY cycles,
  directory/facade option propagation, `.journal.zst` bounded accessor use,
  production bypass scanning, and Netdata combined-result option retention.
- `glm` found that Netdata zero-count facet backfill did not retain caller
  reader options. Fix: `CombinedResult` now stores `readerOptions`, and a
  focused test guards the option-retention contract.

Production review round 2 fixes:

- `qwen` found that the Explorer indexed strategy still used row-lifetime DATA
  reads while walking FIELD chains for facet and histogram counting. Fix:
  `node/src/lib/explorer.js` now passes `false` to both FIELD-chain
  `_readDataPayloadAt()` calls, matching the `queryUnique()` temporary-read
  convention.
- `minimax` found a non-blocking TypeScript surface gap for
  `CombinedResult.readerOptions`. Fix: `node/index.d.ts` now declares the
  constructor option and `readerOptions` field.
- Added a focused regression test proving `ExplorerStrategy.Index` uses
  temporary DATA reads for FIELD-chain facet and histogram collection.

Local Node validation commands passed:

- `node --check` on changed Node runtime/test files.
- Direct `node/test/chunks/sow0108-reader-access.js` focused test run.
- `npm_config_cache=../.local/npm-cache npm run typecheck`.
- `npm_config_cache=../.local/npm-cache npm test`.
- Direct corrupted-fixture open checks for zstd-truncated and AFL corrupted
  fixtures, including the ENTRY_ARRAY cycle fixture that previously hung.
- Full Node conformance manifest adapter loop with a 15 second per-case
  timeout.
- `python3 tests/interoperability/run_matrix.py --writers node --readers node stock --entries 10`
  passed with 11/11 checks.
- `python3 tests/interoperability/run_directory_matrix.py --readers node stock`
  passed.
- `python3 tests/interoperability/run_verify_matrix.py --skip-build` passed.
- `PYTHONPATH=.local/python-deps python3 tests/interoperability/run_mixed_directory_matrix.py --readers node stock`
  passed with 27/27 checks.
- `python3 tests/interoperability/run_live_matrix.py --entries 10 --readers node stock --features regular zstd compact sealed`
  passed with 16/16 checks.
- `python3 tests/interoperability/run_journalctl_query_matrix.py` passed.
- `git diff --check` passed.
- `.agents/sow/audit.sh` passed.

Node benchmark/accessor smoke:

- `node node/cmd/reader_core_bench.js --input .local/interoperability/compact/none/node/node.journal --mode sdk-payloads --surface file --access-mode read-at --window-size 128 --max-windows 1 --bounds snapshot`
  read 10 records and 44 fields with checksum `11982092242414586274`,
  `readBufferBytes: 128`, and `readSyncUsesPosition: true`.
- The same bounded 128-byte-window smoke passed for `sdk-entry`,
  `facade-data`, and `facade-next`.

Bypass scan result:

- The production reader, directory reader, facade, Explorer, Netdata wrapper,
  verification, journalctl, and benchmark surfaces no longer contain
  `safeReadFileSync`, `decompressZstToTemp`, `this.buffer`, `reader.buffer`,
  or `r.buffer` journal-file bypasses.
- The remaining `readFileSync` hits in `node/adapter/index.js` are adapter
  stdin/text-fixture reads, not journal-file reader paths. Wider scans also
  found explicit helper/non-reader paths such as WASM loading, status reads,
  optional lock/platform helpers, and direct bytes helper APIs.

Same-failure scan:

- Local same-failure search confirmed the removed `readOnlyMapping` reader path
  is no longer referenced by Go reader code. Writer `mappedArena` remains
  separate.
- Node.js production reader-surface scan confirmed the migrated reader,
  directory reader, facade, Explorer, Netdata wrapper, verification,
  journalctl, benchmark, and adapter journal-file paths no longer use
  reader-owned whole-file Buffer access.

Sensitive data gate:

- This SOW contains no raw sensitive data. File paths are repository-local
  source paths or sanitized local validation targets only.

Artifact maintenance gate:

- Completed. Go, Python, and Node.js implementations changed public reader
  options/stats and benchmark semantics. Language README/API updates exist for
  the implemented phases. The product-scope spec is updated for the current
  reader-memory architecture. Status ledgers are updated during closeout.

Specs update:

- `.agents/sow/specs/product-scope.md` updated for bounded rolling reader
  access across Go, Python, and Node.js, including Node's no-core-mmap
  contract.

Project skills update:

- No project-skill update required. The existing journal-compatibility and
  orchestration skills already require bounded reader hot paths, row-lifetime
  compatibility, whole-SOW review batches, and reviewer consensus; this SOW did
  not change the repository workflow.

End-user/operator docs update:

- Language docs/API files have been updated for implemented public reader
  option surfaces. Consumer wiki expansion for Python and Node remains tracked
  by SOW-0106, which already owns Python/Node verified examples and published
  wiki pages.

End-user/operator skills update:

- No end-user/operator skill update required. No external operator skill
  currently consumes these reader internals.

Lessons:

- Small test fixtures hid reader-memory architecture risks. Large-file bounded
  memory tests are mandatory for every reader backend, not only Node.js.

Follow-up mapping:

- Directory-level shared reader window budgets remain rejected for this SOW:
  each file reader is bounded, and no reviewer found correctness or production
  readiness risk requiring a shared directory budget now.
- Optional native mmap for Node remains rejected for this SOW because Node core
  has no mmap API and native addon use is outside the project policy.
- Reviewer watchpoints around scratch-buffer comments, additional `.journal.zst`
  header-only tests, `getEntry()` temporary-read micro-optimization,
  `visitEntryPayloads()` callback-scope optimization, and indexed-query pin
  pressure are accepted as future-hardening ideas but rejected as required
  follow-up SOWs because they do not violate correctness, bounded-memory,
  row-lifetime, compatibility, or runtime-purity contracts and all reviewers
  still voted `PRODUCTION GRADE`.

## Outcome

Completed. SOW-0108 delivered the cross-language bounded reader access
architecture for Go, Python, and Node.js while preserving Rust as the mmap
reference implementation. Go and Python now have bounded rolling accessors with
mmap/read-at selection, and Node.js no longer has production whole-file reader
Buffer paths; it uses bounded positioned-read windows, row-pinned current-row
views, row-arena memory for compressed/cross-window payloads, and streaming
`.journal.zst` temp files.

## Lessons Extracted

- Boolean lifetime switches are easy to misuse in non-row FIELD-chain helpers.
  Focused tests must explicitly assert temporary/internal read use for unique
  value and indexed Explorer paths.
- Small fixtures are not enough evidence for memory architecture changes.
  Focused bounded-window, forced-eviction, row-arena, refresh-rollback, and
  bypass-scan tests are mandatory for reader-access work.
- Whole-file `.journal.zst` handling is a reader-memory risk even when normal
  `.journal` access is bounded; streaming-to-temp must be part of the same
  accessor contract.

## Followup

No mandatory follow-up SOW is required from this SOW. Existing pending SOW-0106
continues to own Python/Node consumer docs and verified examples.

## Regression - 2026-06-14

### What Broke

Public verification APIs in Rust and Go still use whole-file resident byte
buffers:

- Rust `verify_file()` and `verify_file_with_key()` call
  `read_journal_file_for_verify()`, which uses whole-file `std::fs::read()` for
  normal journals and `.journal.zst` `read_to_end()` for repository compressed
  files.
- Go `VerifyFile()` and `VerifyFileWithKey()` call `readJournalFileBytes()`,
  which uses `io.ReadAll()`.

Node.js and Python verification helpers were already migrated to bounded
reader-backed byte sources, so the completed SOW has a cross-language public API
parity gap. The ordinary row readers are still bounded; the regression is in the
verification surface.

### Evidence

- `rust/src/journal/src/sealed_verify.rs`: `verify_file()` loads a full `Vec<u8>`
  before object-graph verification; `read_journal_file_for_verify()` uses
  `std::fs::read()` / streaming decoder `read_to_end()`.
- `go/journal/verify.go`: `VerifyFile()` / `VerifyFileWithKey()` load a full
  `[]byte`; `readJournalFileBytes()` returns `io.ReadAll(f)`.
- `node/src/lib/verify.js` and `node/src/lib/verify-adapter.js`: file-path
  verification uses `FileReader` plus a bounded byte source.
- `python/journal/verify.py` and `python/journal/_verify_adapter.py`: file-path
  verification uses `FileReader` plus `_AccessorBytesAdapter`.
- This SOW's original risk/blast-radius list included verification helpers.

### Why Previous Validation Missed It

The completed SOW focused on production reader paths and explicitly preserved
Rust as the mmap/windowing reference. The Rust and Go verification helpers were
not rechecked as public reader-adjacent APIs after Node.js and Python moved
verification onto bounded byte sources.

### Repair Plan

- Reuse the existing verifier semantics; do not weaken object-graph, strict
  entry/DATA, TAG, or HMAC validation.
- Replace Rust and Go verifier inputs with bounded byte-source abstractions
  backed by the existing reader/window accessor.
- Keep compatibility test helpers that verify an in-memory buffer, but ensure
  public file-path verification is bounded.
- Avoid whole-file resident reads in Rust/Go public verification paths,
  including `.journal.zst` handling and sealed HMAC verification.
- Preserve ordinary reader hot-path performance; verification changes must not
  change row traversal behavior.

### Regression Repair Implementation

Rust:

- `rust/src/crates/journal-core/src/file/file.rs` exposes crate-hidden,
  unaligned bounded byte reads on top of the existing `WindowManager` so the
  verifier can read header bytes, object headers, TAG payloads, and HMAC ranges
  without bypassing the reader window architecture.
- `rust/src/journal/src/verify_graph/` now verifies object graphs through a
  bounded `VerifyByteSource` instead of a whole-file `&[u8]`.
- `rust/src/journal/src/sealed_verify.rs` opens a snapshot `FileReader`, wraps
  the underlying `JournalFile<Mmap>` as a verifier byte source, performs
  object-graph verification through that source, and reads sealed TAG/HMAC
  ranges in bounded chunks. The public `verify_file()` and
  `verify_file_with_key()` APIs no longer call a whole-file read helper.
- `rust/src/journal/src/tests/verification.rs` adds a sealed-file regression
  test with a 4 KiB reader window on a file larger than the window, covering
  both structural and keyed verification.

Go:

- `go/journal/verify_source.go` adds the verifier byte-source abstraction and a
  `Reader`-backed implementation. The source returns owned bytes read through
  `Reader.readAt()` rather than borrowed temporary window slices, so verifier
  code can safely hold a payload while later scalar reads map or evict other
  windows.
- `go/journal/verify_graph.go` now verifies object graphs through that bounded
  byte source instead of a whole-file `[]byte`.
- `go/journal/verify.go` opens a snapshot reader for `VerifyFile()` and
  `VerifyFileWithKey()`, runs object-graph verification and sealed TAG/HMAC
  validation through bounded source slices, and then runs strict reader
  traversal on the same reader. The old `io.ReadAll()` helper is removed.
- `go/journal/verify_test.go` adds a sealed-file regression test with explicit
  `ReaderAccessReadAt` and explicit `ReaderAccessMmap`, 4 KiB windows, and one
  maximum window on a file larger than the window.

Same-failure search:

- A focused source search of the Rust and Go verification surfaces found no
  remaining `read_journal_file_for_verify`, `std::fs::read()`,
  `read_to_end()`, `io.ReadAll`, or `readJournalFileBytes` use in the public
  verification path.

### Validation Plan

- Add focused tests proving Rust and Go public verification can run through very
  small windows on files larger than one window.
- Add or update tests that fail if public Rust/Go verification calls the old
  whole-file helper path.
- Run Rust verification tests and affected Rust package tests.
- Run Go verification tests and `go test ./...`.
- Run `tests/interoperability/run_verify_matrix.py`.
- Run `git diff --check` and `.agents/sow/audit.sh`.
- After local validation, run the reviewer pool against the reopened SOW and
  changed Rust/Go verification surfaces, and iterate until all reviewers vote
  `PRODUCTION GRADE`.

### Regression Repair Local Validation

Passed:

- `go test ./journal -run TestVerifyFileAndKeyWorkWithTinyReaderWindows -count=1`
  from `go/`: passed after the round-2 mmap aliasing repair, covering both
  explicit `ReaderAccessReadAt` and explicit `ReaderAccessMmap` with 4 KiB
  windows and one maximum window.
- `cargo test -p systemd-journal-sdk-core -p systemd-journal-sdk`
  from `rust/`: 73 `journal-core` tests and 118 `journal` tests passed,
  including `tests::verification::verify_file_and_key_work_with_tiny_reader_windows`.
- `go test ./...` from `go/`: all Go packages passed, including
  `TestVerifyFileAndKeyWorkWithTinyReaderWindows`.
- `python3 tests/interoperability/run_verify_matrix.py`: `status: PASS`,
  `failures: []`, 9 positive fixture classes and 12 negative corruption
  classes passed across stock `journalctl`, Rust, Go, Node.js, and Python on
  `systemd 260 (260.1-2-manjaro)`.
- `python3 tests/docs/check_wiki_docs.py`: validated 15 wiki markdown files.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed; SOW initialization complete and clean.

### Regression Repair Production Review Round 1

Reviewer outputs are stored under
`.local/agent-reviews/sow-0108-regression-round1/`.

Votes:

- `llm-netdata-cloud/glm-5.2-max`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/kimi-k2.7-code`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/qwen3.7-plus`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/minimax-m3-coder`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/deepseek-v4-pro`: `NOT PRODUCTION GRADE`.

Blocking finding:

- Deepseek reported that `go/journal/verify_source.go` was untracked, so a
  final commit that forgot it would not compile from a clean checkout.

Disposition:

- Accepted as a real process blocker, not a verifier-code defect. The file is
  staged explicitly before round 2 and before the final SOW close commit.

Non-blocking findings and dispositions:

- Rust `reader_file_size()` and `read_unaligned_bytes_at()` are `pub` and
  `#[doc(hidden)]` instead of `pub(crate)`. Disposition: accepted as necessary
  cross-crate verifier plumbing from `systemd-journal-sdk` into
  `systemd-journal-sdk-core`; they remain hidden from docs and are only used by
  the verifier.
- Rust and Go verifier byte sources allocate per small scalar read.
  Disposition: accepted for verification, which is not the query hot path. The
  replacement removes file-sized resident buffers; a later optimization can add
  scratch-buffer reads if verifier CPU/allocation cost matters.
- Go `readerVerifySource.Len()` relied on the snapshot-reader invariant.
  Disposition: accepted and clarified with a code comment because
  `openVerifyReader()` forces `WithSnapshot(true)`.
- Dedicated `.journal.zst` tiny-window verification tests and isolated
  `readerVerifySource` bounds tests were suggested as hardening. Disposition:
  not required for this regression because the interoperability verify matrix
  covers `.journal.zst` and corrupted verifier cases across all readers, and
  the new tiny-window tests prove the bounded source path for both structural
  and keyed verification.

### Regression Repair Production Review Round 2

Reviewer outputs are stored under
`.local/agent-reviews/sow-0108-regression-round2/`.

Votes:

- `llm-netdata-cloud/glm-5.2-max`: `NOT PRODUCTION GRADE`.
- `llm-netdata-cloud/kimi-k2.7-code`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/qwen3.7-plus`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/minimax-m3-coder`: `NOT PRODUCTION GRADE`.
- `llm-netdata-cloud/deepseek-v4-pro`: `PRODUCTION GRADE`.

Blocking findings:

- GLM reported a real Go verifier memory-safety bug: `readerVerifySource.Slice`
  returned a borrowed window slice via `Reader.readSlice()` / `tempSlice()`.
  `parseData()` and `parseField()` could hold that borrowed payload while later
  verifier scalar reads mapped or evicted another window. In mmap mode that can
  leave a slice pointing at unmapped memory.
- Minimax reported a process blocker: the SOW still recorded round 2 as pending
  and had not captured round-2 findings and dispositions.

Disposition:

- GLM finding accepted. `readerVerifySource.Slice()` now allocates an owned
  buffer, fills it through `Reader.readAt()`, and returns that owned slice. This
  mirrors the Rust verifier source contract and removes future aliasing hazards
  from all `source.Slice()` call sites.
- The Go focused test now exercises both explicit `ReaderAccessReadAt` and
  explicit `ReaderAccessMmap` with 4 KiB windows and one maximum window.
- Minimax finding accepted. Round-2 votes, blockers, and dispositions are
  recorded here before round 3.

Non-blocking findings and dispositions:

- Rust hidden-public verifier helpers remain accepted cross-crate plumbing.
- Per-scalar verifier allocations remain accepted because verification is an
  integrity-check path, not the query hot path.
- Dedicated `.journal.zst` tiny-window verification tests remain optional
  hardening because `.journal.zst` verification is already covered by existing
  sealed verification tests and the shared verify matrix.

### Regression Repair Production Review Round 3

Reviewer outputs are stored under
`.local/agent-reviews/sow-0108-regression-round3/`.

Votes:

- `llm-netdata-cloud/glm-5.2-max`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/kimi-k2.7-code`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/mimo-v2.5-pro`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/qwen3.7-plus`: `PRODUCTION GRADE`.
- `llm-netdata-cloud/minimax-m3-coder`: `NOT PRODUCTION GRADE`.
- `llm-netdata-cloud/deepseek-v4-pro`: `PRODUCTION GRADE`.

Blocking findings:

- Minimax reported a process-only blocker: this SOW still named round 3 as the
  current gate and had not yet recorded round-3 reviewer votes while round 3
  was still running.

Disposition:

- Accepted as a SOW ledger blocker, not a verifier-code defect. Round-3 votes
  and dispositions are now recorded before rerunning Minimax with the same full
  review scope.

Non-blocking findings and dispositions:

- Rust hidden-public verifier helpers remain accepted cross-crate plumbing.
- Per-scalar verifier allocations remain accepted because verification is an
  integrity-check path, not the query hot path.
- Dedicated `.journal.zst` tiny-window verification tests remain optional
  hardening because `.journal.zst` verification is already covered by existing
  sealed verification tests and the shared verify matrix.

### Regression Repair Production Review Round 4 - Minimax Process Rerun

Reviewer output is stored under
`.local/agent-reviews/sow-0108-regression-round4-minimax/`.

Vote:

- `llm-netdata-cloud/minimax-m3-coder`: `PRODUCTION GRADE`.

Disposition:

- The round-3 process-only blocker is resolved. The round-4 review rechecked
  the full Rust/Go verifier regression scope and found no blocking code,
  documentation, SOW, or security issues.

### Artifact Updates

- Specs: `product-scope.md` now includes public file-path verification in the
  bounded reader-memory contract and documents the accepted per-object scratch
  allocation shape.
- End-user docs: `docs/Reader-APIs.md`, `docs/Options-Reference.md`,
  `docs/Go-API.md`, and `docs/Rust-API.md` describe verifier APIs as bounded
  integrity-check paths, not query hot paths.
- Project skills: no project skill change was needed. Existing compatibility
  skill verifier guidance already requires `run_verify_matrix.py`; the missed
  regression was an API-surface gap, now closed by specs and tests.
- SOW status: root and canonical SOW status files record the reopened
  regression repair and are updated again when this SOW moves back to `done/`.
- Follow-up mapping: no new follow-up SOW is required for this regression.
  Per-scalar verifier allocation and per-DATA-object scratch allocation remain
  accepted verifier behavior because verification is an integrity-check path,
  not a query hot path.
