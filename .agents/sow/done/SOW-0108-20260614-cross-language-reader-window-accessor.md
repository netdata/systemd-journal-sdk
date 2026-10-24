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

Regression reopened 2026-10-15: valid-input Go indexed Explorer reads retain an
evictable DATA value across nested posting reads. Both frozen baseline facet
and histogram commands crash. The user approved repairing this
prerequisite first, preserving current SDK work and measuring allocation costs.
SOW-0159 is paused; no concurrent implementation is authorized.
Earlier local placements pass forced-access and cross-language large-fixture
checks but flag a 386 small-open slowdown. The approved Linux native-reader
opener and cached constructor statistics pass full, race, 386, integrated,
compiler-floor and cross-platform checks. The current 118-case timing matrix
finishes with 79 improvements, 39 inconclusive results and no regression flags.
Inconclusive results do not establish equivalence. Earlier small-open timing
flags remain recorded; controlled same-executable and four-binary crossover
repeats show improvements. The timing cause is not established. Private storage
variants remain excluded. Local validation and two independent internal final
reviews pass. External review is adjudicated with no verified blocker: GLM,
Minimax and Deepseek return PRODUCTION GRADE; Mimo returns NEEDS CHANGES with
its sole P2 disproven by source and 18 executed checks. Qwen fails twice with
HTTP 429 and Kimi times out twice without a verdict; both coverage gaps remain
explicit. The approved hybrid header reads retain one fixed 16-byte reader
buffer, fitting native slices and cached readAt fallback. This original SOW
closes with the scoped repair and lifecycle in one signed commit after final
audit and independent exact-revision impact checks. SDK work resumes afterward.

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


## Regression - 2026-10-15

### What Broke

Go indexed facet counting retains a temporary DATA value while nested posting
reads can evict and unmap its window. Converting that value to an owned string
after traversal can crash. Indexed histogram counting retains the same value
across both posting and ENTRY reads. The original lifetime guarantee remains
required; no relaxation is proposed.

### Evidence

- A frozen executable built from SDK commit `dafcdac4b971` crashes on a valid
  regular 100,000-row, 32-field, approximately 176 MiB synthetic journal using
  default rolling mmap and an indexed filter/facet query. An independent direct
  invocation reproduced the fault in `runtime.slicebytetostring` from
  `indexedCountFacetGroup` after `visitDataEntryOffsets`.
- `go/journal/explorer.go`: `indexedCountFacetGroup` and
  `indexedCountHistogram` retain `splitRawPayload`'s borrowed value across
  accessor reads. `reader_access.go` temporary slices are evictable;
  `reader_access_mmap_unix.go` unmaps evicted windows.
- Commit `a8119457` changed these readers from stable whole-file access to
  temporary rolling slices. This SOW explicitly included Explorer's internal
  lifetime preservation. The defect therefore reopens this original SOW,
  rather than becoming a new SDK-hardening SOW.
- Both facet and histogram failures are reproduced with default mmap settings.
  Smaller-window and zero-row baseline controls can silently return wrong keys
  even when they do not crash; compare complete outputs before using a baseline
  for any timing claim.

### Why Previous Validation Missed It

Existing successful tests did not force eviction of a retained facet/histogram
DATA value during nested posting reads. Closed-file validity, small fixtures
and ordinary row-lifetime tests do not establish this temporary callback
lifetime. The prior accepted outcome is contradicted for these callers.

### Repair Plan And Scope Decision

The user approved prerequisite repair on 2026-10-15, the recommended
long-term-best option. Repair and validate this original SOW first, preserving
SOW-0159's working changes and measuring allocations, then resume SDK and CLI
work sequentially. The existing no-performance-regression constraint remains.

The smallest candidate owns a facet/histogram string key before nested reads.
Histogram can reuse one key per DATA object. This candidate can add allocations
for unmatched facet values, so the user's no-performance-regression constraint
still applies. A larger count-before-payload reorder is an alternative only if
measurement rejects the ownership candidate; it changes more error/accounting
paths and requires its own concrete analysis before implementation.

Do not use current-row pins across an entire FIELD walk: they persist until row
advance and can force unrelated reads through scratch or exceed row budgets.
No public API, memory-envelope or compiler-floor change is proposed.

### Pre-Implementation Gate For This Regression

Status: ready for the owned-key candidate and forced-eviction validation;
performance and review remain acceptance gates.

- Problem / root-cause model: facet and histogram callbacks retain an unowned
  value from a temporary accessor slice while nested posting/ENTRY reads may
  recycle positioned-read storage or unmap its mmap window. A later string
  conversion dereferences invalid storage.
- Evidence reviewed: frozen valid-input baseline facet crash at SDK commit
  `dafcdac4b971`; originating commit `a8119457`; both callback bodies in
  `go/journal/explorer.go`; accessor eviction/lifetime rules; existing Explorer
  parity, reader-window and borrowed-payload tests. Rust already owns the
  corresponding value before posting callbacks; no Rust runtime repair is needed.
- Affected contracts and surfaces: Go indexed facets and histograms, both
  compact/regular formats, mmap/ReadAt and compressed/uncompressed DATA. Public
  results, unset counts, stats, error propagation and memory bounds must remain
  equivalent. SDK-hardening and CLI behavior remain separately owned.
- Existing patterns to reuse: owned Go string map keys, existing native posting
  traversal and candidate sets, traversal-strategy parity oracle, current
  reader-window stats, retained matched-helper binaries and synthetic corpora.
- Risk and blast radius: a copy before nested reads fixes lifetime but can
  allocate for unmatched DATA. Histogram can reuse the owned key instead of
  copying once per matching row. Measure both dense and sparse/high-cardinality
  workloads; do not accept an allocation/timing regression or treat a crashing
  baseline as a faster implementation.
- Sensitive data handling plan: synthetic identities and fields only. Keep
  fixtures, crash logs and raw measurements under ignored `.local/`; durable
  evidence uses public commit IDs, relative source paths and aggregate results.
- Implementation plan: preserve SOW-0159 source changes. Own each facet or
  histogram key while the payload is still valid, then reuse it after nested
  access. Add forced-eviction and scratch-reuse coverage plus matching/unmatched
  allocation benchmarks. If measurement rejects this candidate, analyze the
  count-before-payload alternative before changing implementation strategy.
- Validation plan: reproduce the baseline histogram failure; exercise both
  layouts/backends and compressed DATA in child processes where an mmap fault
  would terminate the test runner. Compare indexed/traversal and Rust results,
  stats and unset behavior. Run the Go Explorer/window suites, real 176 MiB
  fixture, paired valid-control performance/allocation checks, same-failure
  search, authorized whole-regression review or explicit waiver, and clean audit.
- Artifact impact plan: preserve existing lifetime specs; add regression test
  procedures to the runtime compatibility skill, update docs only if usage or
  guarantees change, and update this SOW plus both status indexes. Stage only
  the repair's exact source hunks and artifacts; do not commit paused SDK work.
- Open decisions: scheduling and lifetime contracts are approved. The updated
  numbered header-allocation design choice below supersedes the earlier
  direct-slice proposal, which fails the cached-boundary performance constraint.
  No header strategy has been transferred into the prerequisite runtime.
  External review authorization or waiver is required after local validation. Performance
  failure requires a concrete new candidate, never a silently relaxed gate.

### Validation Plan

- Force mmap eviction and positioned-read scratch reuse in facet and histogram
  paths, including exact filters, matching/unmatched values and both layouts.
- Compare complete indexed results with the traversal strategy and the Rust
  sibling; preserve unset accounting, statistics and bounded memory.
- Measure allocations and timings for sparse/high-cardinality unmatched facets
  as well as matching values. A crash or incorrect baseline cannot certify a
  speedup. Do not weaken SOW-0159's performance gate.
- Keep the existing SOW-0159 working changes uncommitted and separately owned.
  Any prerequisite commit must contain only this regression's repair/artifacts;
  validation must disclose when both working surfaces are present.
- Obtain the required regression review disposition, update affected artifacts,
  run the complete-and-clean SOW audit and close this repair before resuming
  dependent implementation.

### Artifact And Follow-Up Mapping

The existing product/accessor lifetime specs already require the correct
behavior; update their evidence if repair changes the contract. Update runtime
compatibility guidance for forced-eviction indexed-query checks. Consumer docs
need a change only if public usage or guarantees change. Canonical/root status
indexes record this reopened, active regression. The original narrative and
prior verification-repair history remain intact above.

### Current Validation And Outcome

Source/lifetime investigation and both baseline crash reproductions are complete.
Prerequisite repair is authorized and active; SOW-0159 is paused. The previous
`hybrid-slots-1` placement passes 303 focused native/integrated and 32-bit
controls, the standalone full Go suite (1,115 leaves), 311 race cases and all
16 large-fixture language/strategy checks, but reproduces a 386 regular-small
open slowdown. Its initial/repeat timing evidence remains retained. Earlier
`hybrid-packed-1` keeps the same hybrid algorithm and packs two private booleans:
measured sizes/classes are amd64 624/640 and 386 524/576. Its five pure builds
and allocation-shape controls pass; full functional/performance revalidation
was not completed because its small-open timing flag persists. The user
subsequently approved the Linux native-reader opener; `native-open-1` passes
new amd64 descriptor controls and shared-file mmap timings, but its longer
original-layout 386 repeat flags a slowdown. Later controlled same-executable
and four-binary crossover repeats show improvements; the cause of the earlier
discrepancy remains unproven. The current native-open-stats-1 candidate passes
full/focused/race, 386, integrated, compiler-floor and non-Linux build checks.
Its complete performance matrix is running. Private storage variants did not
establish a cure and remain excluded from the root.
Earlier candidates' failed compact timings or increased allocation costs remain
recorded below. Full functional/performance acceptance of the current candidate
remains open; no earlier placement's results substitute for its complete gate.
No review approval or completed outcome is claimed. Current SDK work remains
preserved and separately owned.

### Repair Execution - 2026-10-15

- Reproduced both facet and histogram fatal faults with the frozen pre-change
  SDK on the valid 176 MiB corpus, default 32 MiB mmap windows and 100 returned
  rows. Zero-row and smaller-window controls can succeed; they do not establish
  lifetime safety and cannot replace the failing production settings.
- Added child-isolated indexed-versus-traversal checks across both layouts,
  uncompressed/Zstandard/XZ/LZ4 DATA and ReadAt/mmap/auto access. Before the fix,
  all twelve uncompressed facet/histogram cases fail with wrong results; the
  owned decompressed-payload controls pass. The test forces eviction or scratch
  reuse and checks rows, counts, histograms and memory bounds.
- Retained a pre-repair test executable and source snapshot with all paused
  SOW-0159 changes present, then implemented only early owned keys in facets and
  histograms. This isolates the prerequisite repair for before/after benchmarks.
  Matching/unmatched low/high-cardinality allocation benchmarks are included;
  no performance or review acceptance is claimed yet.

### Ownership Candidate Measurement And Revision - 2026-10-15

The early-copy candidate passes all 48 lifetime cases but is rejected by the
performance gate. Ten alternating measured pairs after two warmups, pinned to
one CPU, show sparse high-cardinality facets 54.0% slower and histograms 53.8%
slower. Facet allocations rise from 1,922 to 2,930 per operation and histogram
allocations from 1,935 to 2,933; approximately 645 KiB of unnecessary key copies
are added per operation. Dense histograms improve, but that does not compensate
for the filtered-query regressions. Raw results and immutable executable hashes
are retained under `.local/`; no acceptance is inferred from the passing tests.

The next candidate keeps payload validation before posting traversal and owns a
key only when a matching result needs it. This is an implementation refinement
within the approved lifetime repair; it changes no public contract:

- Facets acquire their key at the first matching posting. The inline posting
  callback precedes any nested read, so its original value remains valid.
  Compressed values are owned decoder results. For a later uncompressed posting,
  save the value's file offset/length before traversal, reacquire that byte range
  and immediately copy it. Reuse the key thereafter.
- Histograms acquire their key at the first actual bucket update, after ENTRY
  validation and time checks. Reacquire an uncompressed value by its saved
  offset/length because the ENTRY read can invalidate even the inline value.
  Compressed values need no second decode. A separate readiness flag preserves
  valid empty values and prevents repeated copies.
- Unmatched DATA and values with no bucket update allocate no key. Each matching
  uncompressed DATA requires at most one additional temporary accessor read;
  measure that cost against the valid, non-evicting baseline. Keep existing
  counters, unset tracking, payload-error precedence and posting error propagation.
- Do not reorder postings ahead of payload validation or add row pins, buffers,
  duplicate decompression or a new accessor API. The paused SDK chain guards
  must not become a dependency of this prerequisite repair.

Validation adds empty/binary values, time-excluded histogram values and malformed
posting/ENTRY errors to the forced-access coverage. Rebuild both benchmark
executables together if their shared test helper changes. Re-run the paired
allocation/timing checks, the default-window reproductions and the standalone
repair without SOW-0159 before claiming local completion.

The first lazy candidate passes all 78 expanded child cases, including complete
indexed statistics and actual compressed-DATA activity. Its facet allocations
match the baseline exactly. Histogram allocations decrease in every measured
case and dense histogram elapsed time improves by 35.6% at low cardinality and
5.5% at high cardinality. However, facets remain 2.6%-5.8% slower across the
four matched cases, so this candidate also fails acceptance. The added
first-match branch executes for every matching posting; moving ownership outside
that callback is the next measured refinement, not a relaxed performance gate.

For the next facet candidate, preserve the original posting callback exactly.
When every posting is a candidate, own the required key before traversal. For
filtered candidates, count first and acquire a key only if the count is nonzero;
reacquire uncompressed raw bytes only when there is more than the inline
posting. A single-inline traversal performs no accessor read; compressed values
already own their bytes. This preserves the same validation/error order and
zero unmatched-key allocations while removing the added per-posting branch.
Retain the histogram candidate, which already waits until an actual update.
Qualify the all-postings shortcut once using existing population metadata:
the visible ENTRY count must equal the declared header count. A recovered
prefix can suppress all of a DATA object's postings even in an all-candidates
query; those views retain count-first ownership without depending on paused
SDK recovery fields. Empty values need neither a reread nor an allocation.

The count-first candidate passes all 78 cases. Ten matched pairs show six
improvements (both high-cardinality facets and all histograms); the two
low-cardinality facets are inconclusive, not accepted. Their allocations match
the baseline. Assembly inspection establishes that the inner posting callback
has identical instructions and captures, with the same 40-byte frame. The outer
DATA callback grows from 248 to 288 stack bytes and computes/spills a raw value
offset even when it will never be used. The next local refinement computes that
offset only inside the reacquisition branch, using the validated uncompressed
DATA suffix (`object size - value length`), and postpones the compression flag
until after posting traversal. It changes no reads or result ordering.

Standalone HEAD-plus-repair validation initially ran before its read-only test
input links were ready: all Go packages except the journal package passed, and
that package reported only a missing FSPRG-vector input. The input exists in the
main repository; this is a private validation-workspace setup failure. Correct
that setup before rerunning the failed test and the final complete suite. Do not
report the initial run as passing.

### Independent Large-Fixture And Standalone Evidence - 2026-10-15

- A private source archive from HEAD contains only the two Explorer repair
  functions and the new regression tests; it contains none of SOW-0159's runtime
  changes. The failed FSPRG input test passes after the input-link setup is
  corrected. Final full-suite validation is still required after the last change.
- Its standalone Go reader helper passes the original default-window faulting
  query on the 176 MiB regular fixture and the 128 MiB compact fixture. Both
  indexed and traversal strategies agree with the frozen, unchanged Rust SDK:
  16 language/layout/strategy/operation observations, each with 100 returned
  rows, matching row-byte/checksum and facet/histogram checksums, and 6,250
  aggregation updates. The original default-window failure is therefore fixed
  independently of paused SDK work.
- The non-crashing frozen Go controls also produce wrong value checksums.
  Neither a successful exit nor the same aggregate total establishes their
  correctness. Use the small, non-evicting matched benchmark fixtures as valid
  baseline performance controls; do not count the crashing or incorrect large
  runs as speed comparisons.
- The large-fixture single-run timings are correctness/reference evidence only,
  not an accepted performance comparison. Rust supplies the sibling aggregation
  oracle. Stock journalctl has no equivalent facet-count/histogram operation;
  unique-field enumeration does not preserve multiplicities or time buckets.
- Source search found no additional internal instance of the same retained
  temporary DATA pattern. Netdata boot discovery owns the key before nested
  reads; row classification copies retained labels; snapshot and unique-value
  callbacks consume/copy values before posting access. Rust indexed aggregation
  already owns its value while the DATA guard is alive. No Rust runtime change
  is required to restore the existing paired lifetime contract.

### Final Facet Callback Refinement - 2026-10-15

Twenty alternating 500 ms pairs after the deferred-offset change still leave
low-cardinality facet timings inconclusive: dense median ratio 1.00013 with
interval 0.99373-1.01167; sparse ratio 0.98358 with interval 0.96892-1.01319.
Keep that gate open. Assembly confirms the original callback tests the same
all-candidates flag for every posting. Select the all-candidates or filtered
callback once per DATA instead: both retain the original reference/count/map
update order, while the filtered callback captures only the candidate map and
the dense callback needs no membership test. Hoist these inputs outside FIELD
traversal to avoid copying the complete candidate structure into closures.

This uses the same native posting visitor, maps, error propagation, public API
and ownership rules; it introduces no capacity guesses or additional indexes.
The selection uses candidate membership independently of the complete-population
ownership shortcut, so recovered views retain their existing clipping behavior.
Check escape analysis and allocations, all regression cases, standalone full
tests and the matched performance matrix before acceptance.

### Current Standalone Gate And Proposed Allocation Scope - 2026-10-15

- The final callback-specialized ownership candidate passes the complete
  pristine-HEAD-plus-repair Go suite with `CGO_ENABLED=0`: all five packages with
  tests pass; the journal package completes in 85.584 seconds. Tests, benchmarks
  and source archives remain repository-local. The earlier input-setup failure
  is corrected and its focused FSPRG retry also passes.
- The isolated before/after performance matrix has eight completed cases.
  Dense low/high-cardinality histograms improve (ratios 0.66786 and 0.95036).
  The other six cases are inconclusive; no performance acceptance is claimed.
  Facet allocation counts/bytes are unchanged in all four cases. Histogram key
  reuse reduces allocations/bytes in every case. Combined-SDK candidate timings
  cannot substitute for this isolated prerequisite evidence.
- Allocation profiling finds ENTRY-array header checks responsible for much of
  the remaining cost. Original `validEntryObjectOffset` creates a 16-byte buffer
  per posting; the paused SDK implementation already uses a temporary accessor
  slice and parses it immediately. Its wider checks/recovery rules remain owned
  by SOW-0159. The buffer-read optimization alone could be separated without
  changing those original validation rules.
- A numbered scope decision is pending: include that existing buffer-read
  optimization in this prerequisite and validate reader opening plus aggregation,
  or retain facet/histogram-only scope and continue profiling. The recommended
  long-term-best option keeps the performance gate and removes unnecessary
  allocation using an existing immediate-parse pattern. No scope expansion or
  additional header-reader implementation is authorized until the user answers.
- Independent race validation of the already approved ownership repair passes
  in 90.209 seconds. External review remains unauthorized; request its disposition after
  local validation and the performance gate are resolved.

### Audit Fixture False Positives - 2026-10-15

The expanded changed-file audit exits 2 for two address-pattern matches in
`go/adapter/main.go` and `rust/src/adapter/main.rs`. Inspection confirms both are
the same deliberately malformed, all-zero journal archive filename, already
present at HEAD, used to test filename error classification. They contain no
personal address or private data. Do not alter the compatibility fixtures or
mask genuine addresses to make the audit pass.

The active SOW's required audit-repair gate owns a minimal recognition fix:
remove only that exact double-quoted all-zero filename token from the email
candidate text before matching, as the audit already does for the canonical
public Git SSH remote. Actual address tokens on the same line must still be
reported. Validate the exact Go/Rust fixture shapes, adjacent addresses,
nonzero near-misses and suffix near-misses, then rerun the expanded audit.
This changes no SDK or CLI behavior and is independent of the pending header
allocation scope decision. Record the resulting clean verdict before closing.

Seven isolated pattern controls pass: exact Go/Rust filename fixtures are
excluded, while a synthetic address, an adjacent address and nonzero/suffix/
unquoted near-misses remain detected. Shell syntax validation passes. The
expanded changed-file audit then exits zero with the full verdict
`SOW initialization complete and clean`; the initial audit failure is resolved.

The standalone Explorer and reader-window/borrow race selection also passes
in 90.209 seconds. These checks use the ownership-only repair; the proposed
ENTRY-header allocation change remains unimplemented in this prerequisite
while its scope decision is pending.

A separate pristine-HEAD run of the new regression tests fails 20 of 78 parent
subcases: ten uncompressed lifetime cases and ten empty/binary value cases;
compressed and malformed-index controls pass. The repaired standalone suite
passes all 78. An initial rough log count included the nested child failure
report twice; the retained structured summary counts parent subcases only.
ReadAt facets happen to pass in the original copy-based header implementation;
the paused SDK's temporary-header path exposes scratch reuse there too. The
repair covers both without depending on that paused change.

### Additional Profiling While The Scope Decision Is Pending

Callgrind collection is limited to `indexedCountFacetGroup` and its callees,
using retained pristine before/after executables, one operation per process,
two warmup pairs and twenty alternating measured pairs on one CPU. This is an
instruction metric, not native elapsed-time evidence. Low-cardinality dense and
sparse median ratios are 0.99879 and 0.99860; their intervals still cross one
(0.99438-1.00032 and 0.99444-1.00357). It does not close the performance gate.
Raw profiles, commands, individual runs and executable hashes are retained.

The proposed direct header read is lifetime-safe because parsing copies scalar
fields immediately and posting-array buffers already own their bytes. Source
analysis identifies an access-pattern distinction to validate if approved:
cross-window headers currently assemble cached chunks, whereas a contiguous
temporary read can use scratch ReadAt. Warm boundary performance, tiny windows,
short/wrong/zero headers and borrowed-row pin pressure therefore join the
reader-open/scan validation plan. No original structural checks, recovery rules
or public API changes belong in this proposed allocation-only transfer.

### In-Progress Implementation Review

An independent read-only source review finds no new lifetime, count, error or
unset-accounting defect in the two-function repair. It confirms raw suffix
arithmetic, descriptor-only length reads after eviction, the singleton/no-reader-
access path, owned compression output, empty-key readiness and unchanged
membership semantics. The same-failure search finds no additional internal
borrowed DATA use spanning nested reads. This is ordinary implementation review,
not the required authorized whole-SOW acceptance review.

The review qualifies one future recovery interaction: equal visible/declared
ENTRY counts do not prove recovery is inactive when only unused ENTRY_ARRAY
capacity is missing. A recovered view can suppress all postings of a DATA even
with equal global counts. The prerequisite's ordinary walker always visits its
positive inline posting, so no recovery dependency belongs here. SOW-0159 must
handle that interaction when it resumes; the actual current SOW file records
the required optimization qualification and validation. No unmatched-allocation
claim for every recovered view is accepted by this prerequisite evidence.

The time-range value fixture is strengthened so an additional DATA value occurs
only outside the requested interval. This tests a value with no contributing
posting, in addition to empty/binary values. Benchmark fixture/functions are
unchanged; retained matched benchmark binaries remain the measured artifacts.
All twelve strengthened shape cases pass in the isolated repair (0.142 seconds).

### Larger Valid-Control Comparison - 2026-10-15

While the header-allocation scope decision is pending, validation continues
within the approved two-function repair. Build matched reader helpers from
pristine HEAD and HEAD plus that repair, with the same helper source and CGO
disabled. Compare the immutable 100,000-row regular/compact corpora using an
explicit 256 MiB window, which holds each fixture without eviction. This makes
the original baseline a valid control; its default-window crash/wrong output
cannot serve that purpose.

The retained driver checks every result against the unchanged Rust helper,
compares complete Go indexed counters between phases, and requires matching
returned-row and aggregation checksums. Cover low/high-cardinality facets and
histograms, filtered/unfiltered, with two warmup pairs plus ten alternating
measured pairs on one CPU. Retain every command, stdout/stderr, GNU time/RSS
record and fixture/executable hash. This supplements the small-fixture results;
it does not certify default-window baseline performance or erase inconclusive
cases. The pending header optimization remains outside the prerequisite.

All sixteen large controls finish with matching Rust semantics and complete Go
indexed counters. Seven timings regress, three improve and six are inconclusive.
The regressions include regular low-cardinality dense facets (ratio 1.0698),
regular high-cardinality sparse facets (1.0358), regular high-cardinality dense
and sparse histograms (1.0995 and 1.0357), compact low-cardinality dense facets
(1.0232), compact high-cardinality sparse facets (1.0313), and compact
high-cardinality dense histograms (1.0311). Each listed interval excludes one.
Retained commands, paired observations and intervals are in the private
`sow-0108-large-valid-control-performance` workspace. The candidate fails the
performance gate; small-fixture improvements do not override these results.

### Counter And Singleton Refinement Plan - 2026-10-15

The singleton histogram path currently rereads each contributing raw value
after its ENTRY read. That adds an accessor operation for every unique DATA,
where no reuse can repay the extra read. Keep the existing owned-string
approach, but acquire a singleton key before ENTRY access when candidate
membership and query/bucket bounds prove that it must contribute. Hoist the
proof once per histogram: no realtime query bounds, nonempty buckets, normalized
first-bucket width, and the maximum possible unsigned timestamp quotient fits
in `int`. The last condition preserves the existing bucket-index behavior for
extreme timestamps and 32-bit builds. A singleton posting invokes its callback
before any accessor read, so the raw value is still valid there. Filtered-out
postings, time-bounded queries and uncertain bucket ranges retain lazy ownership.
No key allocation is added to valid no-update paths; invalid ENTRY errors can
occur after the same required key was acquired. Compute the raw suffix offset
only when the fallback actually rereads the value.

Batch facet reference statistics after each DATA posting walk. Dense callbacks
already count every delivered posting; filtered callbacks keep a separate local
visited count. Add the actual count before checking the posting-walk error, so
partial error statistics and later reread-error precedence remain unchanged.
There are no progress callbacks inside these indexed walks and progress
snapshots copy statistics by value. This removes a per-posting result write
without changing native traversal, membership, maps, allocations or public APIs.

These are implementation refinements within the approved two-function repair;
the header allocation transfer remains pending. Recheck forced-access/error
cases and compiler captures, then measure both small allocation controls and
the larger valid-control matrix using fresh immutable candidate executables.

The singleton/scalar-capture candidate passes all 126 forced-access cases then
measured. The small allocation matrix has two improvements, one regression
(filtered high-cardinality facets, ratio 1.0400, interval 1.0051-1.0643), and five
inconclusive cases. Facet allocations are unchanged; all histogram allocation
counts decrease. The large valid-control matrix has one improvement, eight
regressions and seven inconclusive cases. Performance acceptance remains open.
The fresh private workspaces are `sow-0108-singleton-scalar-small-performance`
and `sow-0108-large-singleton-scalar-performance`; no unsuccessful result is
discarded or averaged against an unrelated improvement.

Native CPU profiles of 150 complete regular low-cardinality facet queries
retain matching complete results. Baseline elapsed time is 7.536 seconds and
candidate time is 7.870 seconds. Posting-header validation remains a major cost;
the sampled increase is concentrated in unchanged row-scan/header-access
callees, so the profile does not prove that one new ownership branch explains
the whole slowdown. These single profiles are diagnostic, not paired acceptance
evidence. Compiler analysis confirms all aggregation callbacks stay off the
heap and the histogram captures only the DATA size scalar, not a whole header.

The next narrow refinement reduces callback setup and captures. Select the
facet posting callback once per facet, outside DATA traversal, and reuse local
count/visited scalars reset for each DATA. Indexed walkers are synchronous and
non-reentrant, so this preserves the same counts and error flush. In histograms,
capture only the candidate all flag and offset map rather than copying the
56-byte candidate structure into both callbacks. These changes do not add
reads, allocations, caches or new public behavior. New direct-helper tests
cover partial facet counters and singleton bucket integer boundaries, zero
width and empty buckets; validate these along with the prior forced-access
matrix before accepting another measurement.

The hoisted-callback candidate passes all 142 current focused cases, including
the twelve explicit singleton bucket-boundary cases and four direct-helper
partial-counter cases. Those direct tests retain the private result on error,
since the public Explorer API correctly discards an unsuccessful result. No
partial-result public contract is added. Compiler analysis confirms the reused
facet closures do not escape and histogram callbacks capture the candidate map
and flag rather than the full candidate structure. Large valid-control and
small allocation comparisons remain required for this candidate.

The completed hoisted-callback small matrix has three improvements and five
inconclusive cases, with no measured regression in that matrix. Facet
allocations remain unchanged. Low-cardinality dense histogram allocations fall
from 3,694 to 1,912 per operation; high-cardinality dense histograms fall from
4,464 to 3,702. Sparse histograms also allocate less (1,973 to 1,948 and 2,728
to 2,718). These counts apply to the matched 2,048-row fixture only.

The large valid-control matrix still fails the gate: four regressions, two
improvements and ten inconclusive cases. Regressions are regular filtered
low-cardinality facets (ratio 1.02251, interval 1.00672-1.05379), regular dense
high-cardinality histograms (1.17639, 1.01987-1.36940), regular filtered
high-cardinality histograms (1.04482, 1.02804-1.05610), and compact dense
high-cardinality histograms (1.03099, 1.01082-1.09616). The wider interval in one
case is retained, not converted into a performance claim. Every paired run
still matches the unchanged Rust helper and complete Go indexed statistics.
Evidence workspaces are `sow-0108-hoisted-small-performance` and
`sow-0108-large-hoisted-performance`.

The latest standalone helper also passes the original default-32-MiB-window
large-file query: all sixteen language/layout/strategy/operation checks agree
on complete aggregation and 100 returned rows. Fresh 32-bit Go execution passes
the twelve singleton bucket-boundary cases (0.162 seconds), including the
platform-dependent integer conversion fallback. This is correctness evidence;
32-bit performance has not been measured.

A second read-only implementation inspection confirms that hoisted visitors
are synchronous and never retained; serialized Reader use remains required.
The existing accessor has no cheap validity probe for a temporary value.
Diagnostic counter snapshots scan windows and do not guarantee invalidation
detection after a failed scratch read. Do not replace ownership with a guessed
counter or backend-specific pointer check. The pending allocation-only header
scope decision remains separate; no transfer has been made.

The latest pristine-HEAD-plus-repair full Go suite passes with CGO disabled and
four Go execution threads; all five packages containing tests pass and the
journal package completes in 100.298 seconds. The expanded changed-file audit
also returns the complete-and-clean verdict, and `git diff --check` is clean.
The new candidate's race selection is still running. Performance acceptance,
the pending scope choice and authorized review/waiver remain unresolved; this
SOW is still in progress and neither requested SDK/CLI SOW has resumed.

### Histogram Callback Reuse Plan - 2026-10-15

The remaining large high-cardinality histogram failures exercise a new posting
closure for each DATA object. Compiler output shows that closure stores the
value/key state plus candidate/reader/result metadata repeatedly. Reuse one
synchronous posting visitor for the histogram query, as the latest facet path
already does. Reset value, compression, singleton eligibility, DATA end, key and
key-readiness before every valid DATA walk. Preserve all membership checks,
ENTRY access, timestamp/bucket checks and error propagation in their existing
order. Clear the borrowed/decoded value immediately after the posting walk,
before either returning its error or loading the next DATA; otherwise the
hoisted slice could unnecessarily retain a previous decompression buffer while
the next one is allocated. The map key remains owned.

This changes callback setup only, stays inside the two approved functions, and
introduces no new accessor interface, buffer policy, public API or native read.
Keep earlier candidate binaries/results immutable. Validate the complete
forced-access matrix and compiler escape behavior, then compare both timing
matrices again. The separate header-allocation transfer remains unapproved.

Clear the shared owned key after each walk as well as the value slice. This
avoids retaining an unused duplicate string while the next DATA is decoded;
initialization and the previous-walk clear establish the empty key before the
next valid DATA. No additional allocation or clearing pass is needed.

Measurement correction: earlier small comparisons reused an unchanged
benchmark fixture/function against a baseline binary with the older test
catalog. The added singleton/boundary/error tests can change linked code
placement even though they are not timed. Retain those runs as exploratory
observations; do not use them as the final matched timing gate. Fresh HEAD and
HEAD-plus-two-functions source archives now contain the exact same complete
142-case test file, verified by hash, and build both timed binaries with the
same flags. Diagnostic escape-analysis compilation is separate from the timed
executables. Large helper controls are also rebuilt with identical flags and
the same helper source. No prior successful timing gate is being withdrawn:
all earlier candidates already had an open or failed performance gate.

The preceding hoisted-facet candidate's race selection completes successfully
in 229.020 seconds. The new reused-histogram candidate separately passes all
142 focused cases. Its diagnostic compilation confirms both reused visitors
and shared value/key state remain on the stack; there is no new closure heap
allocation. Final timing/whole-suite evidence must name this new executable;
the preceding candidate's full-suite/race results do not certify the refinement.

The reused-histogram large single-query comparison retains six regressions,
one improvement and nine inconclusive cases, with all semantic/statistical
completeness checks passing. In particular, compact low-cardinality facets
change substantially even though their source path is unchanged by the latest
histogram-only refinement. That observation does not establish a cause. Before
making further code changes, run the same paired harness with the exact same
baseline executable in both phases. This qualifies measurement variability
without pretending an identical implementation has improved or regressed.
Retain the original candidate results; the performance gate stays open.

The identical-binary control completes with all sixteen intervals including one
and all results/counters matching. It finds no phase bias in that run. This
does not invalidate the candidate's regressions or establish equivalence of a
changed implementation. The fully matched small allocation comparison is now
running; no additional code refinement or header-scope transfer has begun.

The fully matched eight-case small comparison completes: two improvements,
five inconclusive cases and one regression. Filtered high-cardinality
histograms regress (ratio 1.06029, interval 1.01727-1.13308), despite allocating
less. All facet allocation counts/bytes remain the same; all histogram counts
decrease by the same amounts as the earlier observations. The new reuse
candidate therefore also fails the timing gate. Its matched source/binary
hashes, commands, per-process output and allocation samples are retained in
`sow-0108-reused-clear-small-performance`.

The scope question is presented again with the completed evidence: transfer
only the existing immediate header-buffer read optimization from paused
SOW-0159 and validate reader-opening/boundary behavior, or keep the two-function
scope and continue its performance investigation. Recommend the former as
long-term-best; the allocation profile identifies existing per-posting header
allocations as a material cost. No answer has been received, so no header
transfer or public Reader behavior change is implemented in this prerequisite.

The current reused-histogram standalone helper passes all sixteen original
default-window Go/Rust index/traversal checks. Its 32-bit singleton-boundary
selection also passes (8.568 seconds). The expanded changed-file audit reports
complete and clean, and whitespace validation is clean. All validation jobs
started for these checks have completed. The final chosen implementation still
needs its full-suite/race confirmation after the performance/scope choice is
resolved; the earlier whole-suite/race runs remain named candidate evidence.
Before acceptance, also qualify representative compressed-DATA aggregation
timings/allocation behavior: codec correctness alone does not establish the
performance of the changed shared visitor/key path. Keep that work in this
active regression gate rather than treating it as a separate optional follow-up.

### Filtered Histogram Wrapper And Codec Validation - 2026-10-15

The preceding goal turn made implementation and validation progress; the
header-allocation scope question remains unanswered. Continue only the approved
facet/histogram repair and its validation. Assembly from the frozen helpers
shows the original histogram posting callback has 122 instructions, 549 code
bytes and a 264-byte frame. The reused posting callback has 213 instructions,
986 bytes and a 296-byte frame. Its DATA callback shrinks from 280 to 200 stack
bytes, but rejected postings still enter the larger matched-work frame.

Select a filtered wrapper once per histogram query. Dense queries invoke the
existing matched body directly, with its all-candidates branch recording the
reference statistic. Filtered queries use a small wrapper that records the
same statistic, checks the candidate map and calls the matched body only for
accepted offsets. The matched body's false all-candidates flag prevents a
duplicate increment. Keep ownership, ENTRY/time/bucket processing, error order
and post-walk value/key clearing unchanged. An independent read-only inspection
confirms the source-level count/lifetime model; actual frame reduction, escape
behavior and benefit must be measured. No extra journal read or unmatched key
allocation is allowed.

For compressed performance controls, render the existing synthetic JSON writer
from `tests/interoperability/go_fixture_writer.py` into a fresh private builder
and compile it once against HEAD. Preserve the current benchmark's 64 DATA/16
FIELD buckets, compression threshold 32 and publication cadence zero. Generate
immutable 2,048-row regular/compact fixtures for Zstandard, XZ and LZ4, with
low/high-cardinality mixed values and a separate true-singleton shape. Reuse
the current VALUE/SELECT patterns and synthetic identities; no host data enters
the fixtures. Check native codec flags and successful query decompression.

Add an optional prebuilt-corpus benchmark to the ownership test helper. It
reads explicitly selected private fixture paths, skips when no corpus is
provided, and excludes fixture creation/encoding from the timer. Before timing,
compare complete index/traversal aggregation plus expected update totals:
mixed shapes 2,197 dense/34 sparse, true singletons 2,048/32. Copy the identical
complete helper into HEAD and repaired archives and build with matching flags.
Retain fixture/source/binary hashes, warmups, alternating measured pairs,
allocation counts/bytes and every result. Cover both layouts/codecs with dense
low-cardinality and sparse high/singleton queries for facets and histograms.
This is validation infrastructure within the active regression, not resumed
SDK or CLI implementation; the performance gate remains unchanged.

All eighteen immutable codec fixtures pass native structural inspection. Their
8 MiB reserved files fit the default 32 MiB accessor window. The baseline's
36 one-operation preflight cases agree with traversal output and expected
update totals, and report actual decompression; those observations are
correctness/setup checks rather than accepted timing samples.

The generated filtered wrapper has 32 instructions, 110 bytes and a 32-byte
frame. It contains one candidate map lookup and an indirect matched-body call,
with no ENTRY/string/bucket work. The matched body shrinks to 188 instructions,
847 bytes and a 288-byte frame, and dense traversal uses it directly. Compiler
escape inspection shows the wrapper/matched/DATA closures do not escape.
Conditional write barriers through captured pointer state remain visible in
assembly; stack placement alone does not prove their CPU cost is zero.

Add four direct-helper histogram error cases (both layouts and all/filtered
candidate sets) to verify that a malformed matched ENTRY preserves the exact
one-reference partial count. The focused matrix is now 146 cases. Freeze the
updated complete helper identically in both archives before using either for
timing; earlier 142-case binaries remain immutable diagnostic evidence.

The filtered-wrapper candidate passes all 146 focused cases, including the new
partial histogram counters. All 36 codec preflight cases also pass complete
index/traversal output checks, expected update totals, actual decompression and
zero-eviction/temporary-copy controls. Those one-operation preflights are not a
timing gate. The next comparison first measures the previously regressing raw
filtered high-cardinality histogram using identically frozen complete helpers.

That matched ten-pair comparison reports a 1.01927 median ratio with a
1.01179-1.03797 interval. Allocations decrease from 2,728/63,656 bytes to
2,718/57,384 bytes, but the time regression remains. The wrapper refinement is
therefore not accepted. Preserve its executable/results and investigate the
captured DATA value state before spending time on the wider timing matrix.

Refine that state within the histogram function: retain the shared slice only
for independently decoded data or a qualified inline posting. Other raw values
need only a numeric offset/length until an accepted bucket actually needs the
key. Reacquire raw bytes into a callback-local slice and convert immediately;
do not store that temporary pointer back through the captured slice. Clear
shared slices only after a path that assigned them, and clear the key only when
it was made ready. Initial nil/empty state and these post-walk clears establish
the next DATA's state, including error returns. Empty values still become the
empty key. Time-filtered singleton and multi-posting reads still reacquire only
after their unchanged ENTRY/time/bucket checks. This reduces captured pointer
stores without adding an accessor API, changing reads or eagerly allocating
keys for unmatched DATA. Re-run the focused lifetime/error/boundary matrix and
measure; source reasoning does not establish the timing outcome.

The numeric-value refinement passes all 146 focused cases. Independent
read-only source analysis agrees with its empty/singleton/time-bound/error
state model. It also confirms that the existing accessor has no general borrow
generation token: LRU touches and post-copy counters cannot prove an old slice
survived. Inspecting concrete window internals would bypass the accessor
abstraction, so no such shortcut is added. The separate header scope choice
remains unanswered. A matched narrow timing run is now evaluating the changed
pointer stores before broader timing validation.

The narrow numeric-state comparison is inconclusive (median 1.01312, interval
0.98649-1.03247), with the same reduced allocation counts. It does not establish
performance acceptance. The complete eight-case raw matrix is running.

An independent read-only validation-infrastructure check finds two limits to
correct before final measurement: deferred reader close is currently included
once per benchmark calibration, and corpus hashes are compared only at run
start/end rather than against the generation manifest. Stop benchmark timers
before deferred close and require the recorded generation hashes as well.
Earlier runs remain useful diagnostic evidence with the close cost included;
none is accepted as the final timing gate. Full indexed counters are compared
for untimed preflight calls across phases/calibration; timed iterations check
the exact update total. The 146 focused cases independently compare full
indexed statistics and outputs. Do not describe timed-loop validation as a
comparison of every counter on every iteration.

The eight-case numeric-state raw comparison completes with three improvements,
four inconclusive cases and one regression. The remaining filtered
high-cardinality histogram has median ratio 1.00804 and interval
1.00160-1.07206. Facet allocation counts remain unchanged; histogram counts are
lower. Retain these results in `sow-0108-numeric-value-small-performance` with
their close-in-timer limitation. They do not close the performance gate. The
codec comparator also rejects truncated calibration output rather than
claiming comparison of statistics that Go's benchmark logger omitted.

The numeric-state standalone helper passes all sixteen default-window
Go/Rust index/traversal checks on the original large regular/compact corpus,
with exact row and aggregation checksums and 6,250 updates. Retain its
manifest and outputs in `sow-0108-numeric-value-large-validation`. These are
correctness checks; their incidental process timings are not a performance
comparison. Fresh before/candidate test executables contain the same updated
whole helper with timers stopped before reader close. The first compressed
timing selection covers singleton histograms for Zstandard/LZ4 and both layouts;
it is a four-case diagnostic selection, not the complete 36-case codec gate.

The four-case compressed selection completes with two inconclusive Zstandard
cases and two LZ4 singleton histogram regressions (regular median 1.00891,
interval 1.00570-1.01763; compact median 1.02683, interval
1.00591-1.03948). Allocations and complete logged preflight counters agree.
These inline-only postings do not exercise the proposed posting-array header
optimization, so that scope change alone cannot establish the full gate.

Measure a simpler histogram callback lifetime: reuse the prior per-DATA
visitor structure, but keep its payload slice immutable and reacquire raw bytes
only into a callback-local slice. Earlier versions reassigned the captured
payload, forcing pointer indirection even for already owned decoded data.
Without that reassignment, Go may capture the slice by value in the DATA-local
visitor; its key/readiness lifetime ends with that DATA callback, so query-wide
slice/key assignment and clear operations disappear. Preserve the qualified
inline ownership proof, candidate checks, ENTRY/time/bucket order, one key per
used DATA, all counters and error handling. Raw slices are dereferenced only
before nested reads or after immediate reacquisition; decoded slices remain
owned. This is a source refinement within the approved histogram function,
not an accessor/header expansion. Freeze it separately, inspect capture/escape
behavior and remeasure the failing raw/codec selections after focused checks.

The immutable DATA-local candidate passes all 146 focused cases. Compiler
diagnostics confirm `value` is captured by value (24 bytes, not reassigned),
both closures remain nonescaping and ownership strings enter only result maps.
Its DATA callback has no write-barrier checks, versus two in the former shared
state callback; the posting callback retains two for key assignments. Frame and
callback setup costs still need timing evidence, so this assembly observation
is not an acceptance claim. Its complete helper includes the corrected timer
boundary and is frozen identically into HEAD and candidate source archives.

Its first raw high-cardinality filtered histogram comparison is inconclusive:
median ratio 1.02848, interval 0.91412-1.15887, with about ten-percent variation
in both phases. Allocations retain the 2,728-to-2,718 decrease. This noisy run
cannot establish either a performance gain or equivalence. Measure the
compressed singleton selection next; preserve this run rather than treating
the compiler result as a timing win.

The immutable-local four-case compressed selection completes with all four
intervals including one and unchanged allocation counts. Median ratios range
from 0.99476 to 1.00755; the earlier LZ4 regressions are not reproduced by this
candidate. These remain inconclusive measurements, not established equivalence
or the full codec gate. Raw measurement noise and the existing scope choice
remain unresolved. Results are retained under
`sow-0108-immutable-local-singleton-codec-performance`.

A longer one-second-per-calibration run establishes a remaining regular LZ4
singleton regression: median 1.01996, interval 1.01176-1.03918, despite unchanged
allocations. Do not interpret the earlier shorter inconclusive run as parity.

For compressed singleton DATA only, preserve the original direct posting
visitor: the decoded payload is independently owned, and exactly one posting
cannot benefit from key reuse. Select that visitor before initializing any
raw-value/key-reuse state. Keep the native posting walker, candidate check,
ENTRY/time/bucket order, direct value conversion, partial counters and common
classification footer. Other DATA uses the immutable-local repair unchanged.
This avoids ownership bookkeeping where ownership already exists and reuse has
no work to save. It adds a guarded implementation path inside the same approved
histogram function, without a public contract or reader-access change. Extend
the direct malformed-ENTRY counter check to LZ4 singleton DATA and both layouts/
candidate sets; existing all-codec lifetime/singleton tests cover successful
behavior. Freeze matching helpers and measure the failing cases again.

The longer compact LZ4 run also regresses (median 1.02089, interval
1.00751-1.03923), confirming the need to measure the specialized decoded path.
The eight direct malformed-ENTRY/counter controls pass against unchanged HEAD,
including actual compressed singleton flags, so the extended checks do not
encode only the repair's behavior. The full focused matrix now contains 150
cases. Generated code for the decoded singleton posting callback is 549 bytes
with a 264-byte frame and zero write-barrier checks, matching the original
callback's size/frame; the general raw/reuse path remains separate. Its DATA
dispatcher is larger, so equal inner callback size is not a timing proof.

The decoded-singleton candidate passes all 150 focused cases. A one-second
calibration comparison is running for the same two LZ4 singleton histogram
cases that established the previous candidate's regression. The scope remains
the two Explorer functions; the header transfer is still unanswered and has
not been implemented. Whole-suite/race, full timing matrices, audit and review
remain final-candidate gates, rather than implied by prior candidate runs.

The decoded-singleton one-second comparison retains a regular LZ4 regression
(median 1.00521, interval 1.00340-1.02729); compact is inconclusive (median
1.00961, interval 0.99558-1.03941). The inner callback matches the original size,
but DATA dispatch and its shared footer still retain the compression flag
across the native walk to branch on it again. The candidate is not accepted.

Move the decompression counter update into each successful dispatch branch.
For the decoded-singleton branch it is unconditional, because that branch
already proves compression. The general branch retains its conditional update.
Check the walk error before either update, then execute the common classified/
loaded counters. No callback or fallible operation occurs between these three
counter writes, so their final and partial-error values are unchanged. This
removes a redundant known-true footer test and may shorten compression-flag
liveness on the fast branch. Preserve the existing two posting bodies and
native walker; no new reader scope or ownership rule is introduced. Confirm
with the current successful/error tests and matched timings rather than
assuming the branch removal is measurable.

The decoded-footer candidate passes all 150 focused cases, twenty selected
32-bit boundary/partial-counter controls and all sixteen original default-window
large Go/Rust/index/traversal checks. Its two-case long LZ4 timing comparison is
running. No full timing or whole-SOW acceptance is claimed.

Both aggregation callbacks currently call `splitRawPayload` and then compare
its field to an already known FIELD-chain name. `reader_unique.go` already uses
a direct known-prefix/boundary check. Investigate reusing that pattern within
these two functions to avoid a repeated delimiter scan and non-inlined split
call. Exact equivalence additionally requires checking that the requested field
contains no `=`; otherwise prefix matching could accept a name the existing
first-delimiter split rejects. Empty/binary names, missing/wrong delimiters and
mismatched FIELD DATA payloads must retain their current behavior. Keep the
native chain walk and its errors, even for an unmatchable field name. This is
an implementation investigation inside the approved scope; do not weaken
field validation to obtain a timing gain.

The byte-level equivalence is established: the old comparison succeeds exactly
when the first delimiter is at the requested field's length and all preceding
bytes equal that field. Therefore the field has no delimiter itself. Conversely,
a delimiter-free field, equal prefix and delimiter at that boundary guarantee
the same first delimiter and value suffix. This includes zero-length and binary
fields. Compute the field-delimiter flag once per field, keep the check inside
the DATA callback and reuse the existing prefix/boundary pattern there.

Add 24 direct-helper controls across both layouts and facet/histogram helpers:
wrong name, wrong boundary, early delimiter, empty parsed name, a requested FIELD
name containing `=`, and that unmatchable name pointing at a wrong-type object.
Use one FIELD hash bucket and same-length synthetic byte mutations, updating the
FIELD hash when renaming it; do not change file bounds or posting arrays. The
native lookup must actually find the mutated FIELD before testing the result.
All ordinary mismatches remain one unset update with no classified DATA;
wrong-type FIELD chains retain their error. The latter also proves that an
unmatchable name does not bypass native chain errors. Public Explorer rejects
delimiter-containing query names, so use the existing internal helpers for
that compatibility control. Independent read-only source inspection confirms
the fixture/hash strategy. Freeze the expanded identical helper in both phases
and run the new controls against unchanged HEAD as well as the repair.

The prefix candidate passes all 174 focused cases. All 24 newly added malformed
FIELD/DATA controls also pass against unchanged HEAD, including lookup of the
mutated FIELD and preservation of wrong-type chain errors. Its source archives
freeze the same complete helper in both phases. The earlier decoded-footer
long LZ4 comparison completes with both cases inconclusive and unchanged
allocations; regular median 1.00067 (interval 0.99198-1.09818), compact 0.98774
(0.94295-1.00076). Those noisy results do not establish acceptance. The current
prefix candidate still needs its own timing and larger-file qualification.

The current prefix candidate also passes 44 selected 32-bit boundary/error/
malformed-field controls and all sixteen original large-file Go/Rust strategy
checks. Both status indexes now name the 174-case focused matrix. The product
spec distinguishes the shared ownership guarantee from Go's contributing-key/
reuse implementation: unchanged Rust already owns a temporary value before
posting traversal, so no new claim that Rust delays that copy is introduced.
The matched eight-case raw timing/allocation matrix is running; SDK and CLI
implementation remain paused behind this prerequisite.

The current prefix candidate's eight-case matched raw comparison completes with
four improvements and four inconclusive cases, without a measured regression.
All facet allocation counts are unchanged. Histogram counts decrease by
1,782/25/762/10 for low-dense/low-sparse/high-dense/high-sparse respectively;
their byte totals also decrease. The formerly failing high-cardinality filtered
histogram is now inconclusive with median 0.98104 and interval
0.96607-1.01426. Low-cardinality filtered facets are also inconclusive (median
1.00997, interval 0.98605-1.03134), so no whole-gate acceptance is inferred from
the absence of a significant regression in this matrix. Raw commands, warmups,
all samples, allocation arrays and immutable hashes are retained in
`sow-0108-field-prefix-small-performance`. The full 36-case prebuilt codec
comparison is now running, followed by the matched large timing controls.

Contributor benchmark guidance now documents the focused test command, raw
allocation cases and optional prebuilt codec-corpus contract. It names the
exact expected shape/update totals and keeps fixture generation, validation,
opening and close outside measurement. No consumer SDK API/usage change is
introduced by this repair; the caller-facing wiki remains owned by paused
SOW-0159. The runtime compatibility skill records these validation procedures.

The full codec run finds a regular LZ4 singleton facet regression (median
1.01751, interval 1.00823-1.03757), with unchanged allocation counts. Other
cases continue to run against immutable executables; preserve the whole result.
The filtered facet path resets `visited` and flushes it to result statistics
for every DATA, even though a singleton has only one posting. Accumulate this
reference count for the FIELD walk and flush once immediately after that walk,
before propagating its error. Reset only the per-value matching `count` for
each DATA. Dense traversal retains its current count/flush path.

No fallible operation or external callback occurs between FIELD-walk return,
the flush and error propagation. The final successful and partial-error
reference totals are the same modular sum as per-DATA flushes, including errors
during posting reads or later value reacquisition. No result key, native read,
membership test or other counter changes. This removes redundant filtered
singleton bookkeeping inside the already approved facet function. Extend the
partial facet counter control with one successful preceding DATA object to
prove prior references/updates survive a later partial posting error, while
retaining the original no-prefix controls. Complete the current immutable
codec run before starting builds or timing the next candidate.

Also fold qualified raw singletons into the direct singleton histogram visitor.
The existing guard becomes `nEntries == 1 && (compressed || ownInlineValue)`.
After the unchanged reference/membership checks, own a raw callback-local key
before ENTRY access; decoded keys are owned only after successful time/bucket
checks. A singleton cannot reuse a key, and a local key needs no captured
key/readiness pointer. The native and paused recovery walkers invoke its inline
callback zero or one times with no prior accessor I/O. Keep the native walker.
General DATA retains lazy local reacquisition/key reuse; its old inline branch
is unreachable after this dispatch and is removed. Empty values remain the
empty key. The shared fast branch conditionally records decompression after
successful traversal because it now covers both storage forms.

Independent read-only source analysis confirms these lifetime/error/counter
conditions. Existing raw/codec singleton, time-bound, extreme bucket, malformed
ENTRY and 32-bit controls exercise the dispatch and fallback. This refinement
addresses the earlier raw high-cardinality histogram overhead without adding a
reader API or changing the bucket proof. Compressed timing must still be
remeasured: the merged callback gains compression branches even while raw
singleton bookkeeping decreases. Preserve the previous 36-case run as named
candidate evidence; do not mix its timing samples with the new executable.

The prefix candidate's complete 36-case codec comparison finishes: eleven
improvements, twenty-four inconclusive cases and the one regular LZ4 singleton
facet regression already recorded. Every preflight result/counter and expected
update total matches, and allocation counts do not increase (histogram reuse
reduces them where applicable). Its manifest verifies binary and fixture hashes
through the completed run. Keep this failed candidate's whole matrix under
`sow-0108-field-prefix-codec-performance`.

After that run ends, freeze the filtered FIELD-flush and merged singleton
candidate with the same complete 178-case helper in HEAD and candidate archives.
Build timing executables with identical flags, separately from any diagnostic
compiler build. The next validation includes the new partial-counter controls
against HEAD, the full focused matrix and the previously regressing cases.

The new successful-prefix facet control initially fails on unchanged HEAD:
the fixture assumed FIELD DATA insertion order. `writer_objects.go` prepends
new DATA to the FIELD chain, so the damaged DATA was visited first and its
one-reference/zero-update result was correct. Correct the synthetic fixture by
appending its successful unique DATA last, select that row for the filtered
case, and assert the native FIELD head before applying the posting mutation.
Keep the original expected two-reference/one-update result for that now-proven
walk order. This is a test-fixture correction, not a reader behavior change;
retain the failed logs and rerun the corrected controls against HEAD first.

The corrected 40 baseline partial-error/malformed-field controls pass, including
the successful-prefix facet cases and all zero classification/decompression
counters after failed singleton ENTRY reads. The FIELD-flush/singleton candidate
passes all 178 focused cases, 52 selected 32-bit controls and all sixteen
original large-file cross-language/strategy checks. The matched helper files
and source manifests are frozen under `sow-0108-field-flush-stats-178-*`.
The CLI validation helper uses the same Explorer source as this archive;
subsequent changes only corrected/strengthened tests. Fresh paired timing starts
with the sixteen valid large-file controls, then the raw and codec matrices.
Performance acceptance, complete-suite/race confirmation and review remain
open; earlier passing tests or inconclusive timing intervals do not close them.

The FIELD-flush/singleton candidate's sixteen-case valid large comparison
finishes with eight regressions, seven inconclusive cases and one improvement.
Regular dense low facets/histograms and dense/filtered high facets/histograms
include roughly 7-12% median slowdowns; compact dense low facets and filtered
high histograms also regress. All Go counters, row/aggregation checksums and
Rust-oracle results remain exact. This candidate fails the gate. Preserve all
samples and hashes in `sow-0108-large-field-flush-performance`.

Provenance checks confirm both CLI executables use Go 1.27.1, the same compiler/
architecture/CGO/build settings, identical dependency versions and the same
benchmark-helper source. Comparing every production Go file and module metadata
in their frozen archives finds only `journal/explorer.go` changed. Do not dismiss
the large regressions as a helper mismatch. Profile the exact non-evicting
large-case settings next to distinguish aggregation cost from surrounding work;
the earlier profiles used a different candidate and cannot explain this one.

The exact 256 MiB regular dense low-cardinality facet diagnostic repeats 200
complete helper loops: baseline 9.994624748 seconds, candidate 10.841001244
seconds, with identical results and final statistics. CPU samples attribute
0.16 seconds to unchanged `parseEntryHeader` in baseline and 1.26 seconds in
candidate; posting validation decreases from 3.77 to 3.40 cumulative seconds.
This locates an unexplained cost outside the repaired aggregation functions;
it does not establish its mechanism or excuse the failed timing gate. Inspect
the compiled parser and run unchanged-path controls before further source edits.

Clarify the helper timer: each `--loops` iteration opens a fresh reader, builds
the query, executes Explore, computes checksums and sorted summaries, and closes
the reader. `read_seconds` covers that whole pipeline. Final ExplorerStats are
the last loop's logical counts, not a work-cost measurement or an aggregate of
all loops. CPU profiling continues through post-timer output/heap-profile GC;
heap `alloc_space` is cumulative sampled allocation and `inuse_space` after
explicit GC/close is not peak query lifetime. Retain these profiles as
diagnostics only. Paired unchanged row-only Explorer and sequential SDK controls
will use the same executables, fixtures, explicit non-evicting window and timer,
without calling either repaired aggregation function. Any code-layout or GC
explanation remains a hypothesis until supported by those controls and assembly.

The first unchanged-path control, regular row-only Explorer, is reproducibly
slower (median ratio 1.14858, 95% interval 1.11392-1.16770). It requests neither
facets nor a histogram and calls neither changed aggregation function. An
independent read-only comparison checks eleven row/helper functions: normalized
instructions and stack-frame sizes match. The parser moves by 512 code bytes,
preserving its cache-line alignment; profile excess is attributed mainly to two
adjacent stack stores. Thus added instructions or a larger checked frame do not
explain this row-only result. Broader code-address/cache interactions remain
unproven. Preserve the measured regression rather than classifying it as noise.

Hardware-counter collection is unavailable under the existing process
permissions. The read-only `perf stat` availability check fails; no permission,
service, package or host configuration change is attempted. Continue using the
available Go profiles, assembly and matched diagnostic controls. The complete
four-case unchanged-path control is still running; this first case is not a
complete diagnostic outcome or performance acceptance.

Next bounded source refinement: use one reusable facet posting visitor for both
dense and filtered candidates. Increment one FIELD-local visited total on every
callback, apply the original all-candidates-or-map-membership test, then update
the DATA-local count and distinct-row set. Flush the visited total once after
FIELD traversal, before propagating any error, for both candidate forms. Remove
the duplicate dense visitor setup and per-DATA dense statistics flush. The
existing native traversal, key ownership, result maps and all other counters
remain unchanged. No caller callback or fallible work observes the internal
intermediate reference count; final and partial-error modular totals are exact.
Existing dense/filtered successful-prefix/error controls exercise the change.

This simplifies the approved two-function implementation and removes duplicated
setup/bookkeeping. It restores the baseline's per-posting all-candidate branch;
actual speed and text size must be measured rather than inferred. It is not
address padding or an asserted fix for the unexplained unchanged-path cost.
Keep the current frozen diagnostic running without changing its executables.
Source editing and separate-CPU local validation may proceed while it runs;
retain those later diagnostic samples with this concurrency qualification, and
do not use them as final acceptance. New acceptance timing waits until all such
jobs finish. No header allocation or reader contract transfer is authorized.

The single-visitor candidate passes all 178 focused cases and the complete
standalone Go suite (`CGO_ENABLED=0`, journal package 83.410 seconds). Its facet
parent is 64 compiled bytes smaller and its duplicate dense visitor is removed;
this is a source simplification, not established elapsed-time improvement.
Selected 32-bit controls pass; race validation is running on separate CPUs.

A one-loop Callgrind row-scan diagnostic on baseline and FIELD-flush binaries
returns identical outputs/counters. Simulated instruction totals differ by
less than 0.05%; modeled instruction-cache misses and branch misses do not
increase in candidate. This model does not explain the native slowdown and does
not emulate every processor address/prediction effect. Keep its raw traces and
do not infer native timing equivalence. Next, a short three-binary row-only
diagnostic on another CPU will compare baseline, FIELD-flush and single-visitor
placements. Concurrent work and the different CPU make it exploratory only;
final paired timing still waits for all jobs to finish.

The single-visitor candidate's race selection passes (216.406 seconds), its
52 selected 32-bit controls pass, and all sixteen original large-fixture
Go/Rust indexed/traversal checks pass with the default 32 MiB window. The new
correctness manifest also checks both fixture and binary hashes. Its first
CPU-4 launch was refused before any query because that CPU was outside the
caller's inherited affinity; explicit process pinning makes the intended
separate-CPU validation pass. Preserve that failed preflight log, not as a
reader failure. These checks do not certify performance.

The exploratory three-binary row-only probe has medians 0.59414 seconds
(baseline), 0.69535 (FIELD-flush) and 0.61083 (single visitor) for twenty loops.
Five observations per binary, another CPU and concurrent validation make these
diagnostic rather than equivalence evidence. The smaller candidate substantially
reduces the old unexplained cost in this probe; no mechanism is claimed.

The optional sequential control was overbatched: each process expands every
field over twenty complete scans and takes approximately 45-50 seconds. A
separate user question requests permission to interrupt only that private
diagnostic driver and verified descendants, retain completed evidence and use
shorter current-candidate checks. Until answered, do not stop those processes.
Otherwise let the run finish. Its first row-only case was measured before
concurrent validation; later cases carry the concurrency qualification already
recorded. Benchmark batching/timer lessons are added to the runtime compatibility
skill and benchmark README. No product/API contract changes result.

The current focused suite also passes in the combined working tree with paused
SOW-0159 changes present (22.745 seconds). The expanded changed-file audit again
reports `SOW initialization complete and clean`, and whitespace validation is
clean. An isolated code/artifact review patch applies cleanly to the unchanged
index under `git apply --check --cached`; nothing is staged. It contains only
the two Explorer functions, their complete new helper, the lifetime spec,
compatibility/benchmark instructions and the narrow audit correction. Paused SDK
runtime changes are excluded. Eventual lifecycle staging must also preserve the
paused dependency SOW records and coherent status indexes.

An ordinary independent source review finds no new lifetime, partial-error,
counter-wrap or recovery-walker issue in the single-visitor diff. Dense calls
skip map lookup; all emitted callbacks contribute to FIELD-level references;
the total is flushed before propagating errors. No caller callback can observe
intermediate statistics. Existing prefix/error controls cover the change. This
is implementation analysis, not authorized final acceptance review.

The previously failing regular LZ4 singleton facet is faster in a CPU-29
exploratory comparison (ratio 0.96341, interval 0.95712-0.96922), with exactly
2,161 allocations and 1,473,112 bytes per operation in both phases and identical
full counters. Concurrent diagnostic work prevents treating this as final timing
acceptance. Hold the source fixed for the fresh complete matrices. Any flagged
case needs independent reproduction using the same executables before another
source refinement; retain all samples rather than replacing an unfavorable run.

The remaining top-level timer/dispatch comparison also finds identical
normalized bodies and frames in all three binaries, including `main.main`,
`readBenchOnce`, `runBenchLoops`, query configuration and result processing.
Apparent disassembly differences resolve to identical constant bytes and
relocated symbol references. No propagated row-only frame growth or additional
stack-growth check is established. The cause of unchanged-path timing differences is
still unresolved; source-cost and native timing evidence remain separate.

The current candidate is frozen for acceptance measurement. Prepare a shorter
four-case unchanged-path harness with ten row-only loops and one full SDK scan
per process, matching complete counts/counters and preserving binary/fixture
hashes. It supports stopping normally between processes and a total run budget.
After the old diagnostic and all other jobs finish, run that control plus the
sixteen large, eight raw and thirty-six codec comparisons sequentially. Do not
substitute the concurrent exploratory measurements for those final matrices.

The final build-settings check catches an acquisition-command mismatch before
acceptance measurement: the initial single-visitor CLI used default CGO enabled,
whereas the retained baseline explicitly disabled it. Both exploratory test
binaries used the same default, but retain all of those runs as exploratory.
Rebuild the candidate CLI and both matched test executables explicitly with
`CGO_ENABLED=0`; preserve the initial executables under their original names.
The new pure CLI matches baseline's Go 1.27.1/compiler/architecture/dependency/
VCS settings. The parser and facet symbol positions/sizes are unchanged by this
rebuild, but the final gate must name the rebuilt executables. Their focused
178-case suite and all sixteen original large-fixture checks pass again. The
earlier full Go suite already used CGO disabled on these same source files;
the race run intentionally uses its required instrumentation environment.

The old 96-process unchanged-path diagnostic finishes normally with a complete
hash manifest; no process was interrupted and the related approval question is
obsolete. FIELD-flush row-only queries regress on both layouts (regular ratio
1.14858; compact 1.03819), while full sequential scans are inconclusive
(1.01686 regular; 0.99907 compact). These remain named diagnostic evidence for
that superseded candidate, with the concurrency qualification above.

All other validation jobs have finished. Start the current pure single-visitor
candidate's final four-case unchanged-path comparison on CPU 28, followed by
large/raw/codec matrices sequentially. Both core CLI build settings and complete
test-helper provenance are matched; record outcomes before deciding whether
another source change is justified. No performance acceptance is claimed yet.

The current pure candidate's four unchanged-path controls complete with exact
results/counters and clean hashes. Regular row-only queries regress (ratio
1.02838, interval 1.01657-1.04083), as do compact row-only queries (1.02087,
1.00597-1.02863). Full sequential reads are inconclusive on regular (1.00674,
0.99556-1.02617) and compact (0.99830, 0.98792-1.02198). Preserve these results
in `sow-0108-single-facet-final-unchanged-controls`.

An independent same-binary repeat increases row-only batches from ten to forty
fresh-reader loops, keeping two warmup and ten alternating measured pairs.
Regular remains slower by its median but is inconclusive (1.02379,
0.98648-1.03495). Compact independently reproduces the slowdown (1.01916,
1.00681-1.02606). Retain both complete runs; the favorable regular interval
does not erase the initial result, and compact fails the current gate. Neither
query calls a repaired aggregation function. Continue the frozen candidate's
large/raw/codec accounting before deciding the next implementation/scope step;
do not claim unchanged-path equivalence or tune code addresses as a substitute
for a concrete improvement.

The sixteen-case large comparison completes with two flagged regressions, two
improvements and twelve inconclusive cases. All checksums, complete Go counters,
Rust-oracle results and hashes match. The flagged cases are regular filtered
low-cardinality histograms (1.04016, interval 1.00158-1.04438) and compact dense
low-cardinality facets (1.02268, 1.00126-1.04295). Preserve the full matrix under
`sow-0108-single-facet-final-large-performance`; independently repeat those cases
with the same executables and averages of multiple complete fresh processes per
phase. Retain every process and block mean, not only favorable samples.

The raw eight-case allocation/timing matrix is running before the codec matrix.
No source change is made in response to a single flagged small difference.
The current performance gate remains failed/open because compact row-only
slowdown independently reproduced. Complete the approved allocation accounting
and remaining frozen-source measurements before requesting the next scope
decision. The separate header-allocation proposal still has no user answer.

The eight raw cases finish with four improvements, three inconclusive results
and one flagged low-cardinality filtered facet (ratio 1.00940, interval
1.00033-1.01989). All facet allocation counts and bytes exactly match baseline.
Histogram allocations decrease by 1,782/25/762/10 for low dense/low filtered/
high dense/high filtered respectively; corresponding byte reductions are
1,121,536/15,872/468,736/6,272. The complete hash manifest and all samples are
retained in `sow-0108-single-facet-final-raw-performance`. Independently repeat
the single flagged raw case with the same executables. The full thirty-six-case
codec matrix now runs sequentially, using the rebuilt matched pure executables.

The complete codec matrix finishes: six improvements, twenty-nine inconclusive
cases and one flagged regular XZ high-cardinality facet (ratio 1.04097, interval
1.00574-1.07138). Full preflight statistics, update/decompression totals, fixture
and executable hashes match. Detailed allocation medians include small positive
differences in seven Zstandard/XZ cases (0.5-24 bytes and up to 0.5 allocations);
retain and investigate them rather than asserting allocation equivalence. Retain the
complete run in `sow-0108-single-facet-final-codec-performance` and independently
repeat its flagged case with longer measured batches using these same binaries.
The fixed-candidate large repeats start first, using ten complete processes per
phase and retaining both individual measurements and paired block means.

The larger-process large-case repeats preserve all semantics and hashes.
Compact dense low-cardinality facets independently regress (ratio 1.02975,
interval 1.02253-1.03826). Regular filtered histograms are inconclusive with
substantial variation (1.00196, 0.99696-1.18830); neither this wide interval nor
its near-one median establishes equivalence. The raw filtered low facet's
independent one-second repeat is inconclusive (1.00230, 0.97834-1.01583), with
allocation counts/bytes unchanged. Retain all initial and repeat measurements.

Repeat the seven compressed cases with positive allocation medians using
one-second batches and fixed binaries; this includes the XZ timing flag. In
parallel, prepare only an isolated HEAD-based test helper for the existing
header-read primitives. It compares the allocating readAt check and the proposed
immediate readSlice/parse check within/crossing warmed windows on both backends.
Its purpose is to quantify proposal tradeoffs before a scope decision, not to
transfer the header optimization into the prerequisite. No product runtime
source, recovery behavior or approved two-function implementation changes.
Compile/run that analysis only after the current performance job finishes.

All seven longer codec repeats finish with matching median allocation counts,
overlapping byte ranges and inconclusive timing intervals. The initial XZ time
flag is not independently reproduced (regular high facet ratio 0.99903,
interval 0.92040-1.03192); retain both runs. Byte medians differ by -4 to +1 bytes
against baseline totals from about 20 MB to 17 GB per query. This does not
establish exact timing equivalence or justify erasing the independently
reproduced compact row/facet regressions. Unchanged XZ dictionary allocation is
an opportunity outside the prerequisite repair; no decoder/pooling work is
implemented or accepted into this scope.

### Updated Header-Allocation Proposal And Decision - 2026-10-15

The original direct-slice-only recommendation is withdrawn. Isolated existing-
primitive measurements show a cached crossing header needs one new positioned
file read under `readSlice`, while the existing `readAt` copies two cached
chunks. In the four-strategy comparison, mmap cached header medians are:

| Strategy | Within window | Crossing window | Bytes / allocations per check |
|---|---:|---:|---:|
| Existing allocating readAt | 38.54 ns | 58.06 ns | 16 / 1 |
| Direct slice | 17.71 ns | 740.90 ns | 0 / 0 |
| Reusable owned buffer | 23.33 ns | 42.05 ns | 0 / 0 |
| Hybrid slices/buffer | 19.07 ns | 42.87 ns | 0 / 0 |

The hybrid uses direct slices for fitting headers in power-of-two windows and
owned-buffer readAt otherwise. Non-power-of-two windows use the buffer path;
both layouts of the public journal format still share the same 16-byte object
header. The analysis checks 64 correctness cases on mmap/ReadAt, window sizes
64/63/8/default, zero/flagged-zero/wrong-type/short/overflow headers and retained
rows. All pass. Thirty-two microbenchmark cases with three measurements each
finish, asserting actual temporary-read and cache-work counts. Hybrid and buffer
paths allocate zero per check and introduce no temporary reads in warmed cases.
The runtime source in both analysis archives is byte-identical to HEAD; only
private test helpers differ. This is proposal evidence, not full-reader or
prerequisite acceptance. Reader construction/size-class costs remain to be
measured if a design is approved.

Updated user decision 1 was resolved on 2026-10-15: the user answered `1a`,
selecting option A below. Preserve B/C as the alternatives considered.

- A, recommended long-term-best: include hybrid native slices plus one fixed
  16-byte owned buffer per reader. Preserve cached boundary reads and measure
  the complete affected reader surface.
- B, surgical: reuse the owned buffer for every header. Simpler acquisition,
  preserving the existing read method; common cached headers are slower than A
  in the isolated comparison, while both improve on the original.
- C: retain the two-function scope and continue its unresolved timing work.

No answer to an earlier proposal is assumed to approve this changed design.
The scope decision is reserved to the user by `AGENTS.md:142`; no runtime
implementation of A or B starts before the corresponding choice is recorded.

### Pre-Implementation Gate For The Proposed Header Scope

Status: analysis complete; user approved option A on 2026-10-15.

- Problem / root-cause model: the two-function lifetime repair is correct but
  independently reproduces slower compact row/facet queries. Unchanged row
  functions have matching instructions/frames; no causal layout mechanism is
  established. Separately, the existing native ENTRY-offset validator allocates
  a 16-byte header buffer per check. Removing this measured allocation is a
  concrete reader-work reduction; it is not claimed to explain the slowdown.
- Evidence reviewed: HEAD `reader.go:680`, escape/CPU/allocation profiles,
  accessor cached-window and temporary-copy paths, complete fixed-candidate
  timing matrices and repeats, and the isolated primitive source/hash manifests.
- Affected contracts and surfaces: Go ordinary open, live ENTRY-array reload,
  reverse posting arrays, Explorer filters/facets/histograms, directory/facade
  readers and boot-array fallback. Preserve current validation, zero-header and
  error behavior. Transfer no SOW-0159 recovery/bounds changes. Rust already uses
  its native header-view path; no new public feature or parity exception applies.
- Existing patterns to reuse: native `readSlice`, existing cached `readAt`,
  immediate scalar parsing, normalized default window size and private Reader
  state. The measured placement refinement below supersedes the initial tail
  placement: reuse the verified unused cursor/position slots for the buffer.
- Risk and blast radius: one fixed 16-byte buffer may change Reader allocation
  class. Validate empty/small/large construction as well as hot checks. Preserve
  bounds/error classes, cached-crossing behavior, retained rows and closed
  readers; no header borrow may span another accessor operation.
- Sensitive data handling plan: synthetic header bytes, fields and identities
  only; raw fixtures/profiles stay under ignored `.local/`. Durable evidence
  contains only relative paths, public source commits and aggregate measurements.
- Implementation plan: after choice A or B, add only the private acquisition
  helper and fixed buffer, replace the allocating acquisition in the original
  validator, and retain its remaining body unchanged. A uses fitting native
  slices for power-of-two windows and cached readAt fallback; B always uses
  readAt. Preserve all unrelated paused SDK work. C authorizes no header change.
- Validation plan: turn the approved strategy's primitive controls into normal
  tests; exercise both backends/layouts, tiny/default/non-power-of-two windows,
  pinned rows, partial errors, closed readers and live refresh. Re-run full Go,
  race, selected 32-bit and cross-language large-file checks. Freeze matched
  helpers/build flags for opening, unchanged-path and raw/codec timing/allocation
  gates. Require review disposition and a complete-and-clean audit before close.
- Artifact impact plan: maintain the lifetime spec, compatibility skill,
  benchmark procedures, regression evidence and both status indexes. Internal
  acquisition adds no consumer API or usage change; document constant memory
  and operational details where measurements require it. Keep SOW-0159 paused
  and track its direct-slice boundary issue there without implementing it yet.
- Open decisions: final technical review authorization or explicit waiver
  after local validation. The user selected A; no performance gate is relaxed.

### Independent Proposal Analysis And Current Checkpoint - 2026-10-15

Read-only internal analysis found no correctness defect in the private header
prototype. Production constructors normalize options once, pass those same
options to the accessor and store them in Reader. There is one production
Reader literal in HEAD and one in the paused SDK implementation; no production
clone, options mutation or accessor replacement invalidates the window-size
pairing. Backend fallback preserves the normalized window size. The hybrid
arithmetic admits only fitting headers in power-of-two windows; short and
non-power-of-two windows use cached readAt. The independent owned buffer is
parsed synchronously into scalars, and failed reads return before parsing.

The evidence remains deliberately limited: primitive timing reuses warmed
windows and excludes Reader construction, cold mapping, eviction pressure and
Reader size-class effects. Methods run in grouped order with three samples;
small differences between hybrid and buffer-only strategies require paired
whole-reader validation after authorization. Correctness strategies also share
one Reader sequentially. These observations do not establish production
performance or satisfy the final technical review gate.

All owned build and measurement jobs have finished normally; no process was
stopped. No header production implementation, staging or commit has occurred.
SOW-0159 and SOW-0121 remain paused. The expanded changed-artifact audit reports
`SOW initialization complete and clean` in
`sow-0108-decision-checkpoint-audit.log`; whitespace checks pass. User choice
A/B/C and, after successful
local validation, the final review disposition remain required before close.

### Approved Hybrid Header Implementation - 2026-10-15

The user selected `1a` after receiving the crash context, measured compact
row/facet slowdowns, allocation evidence, options, memory/layout risks and
whole-reader acceptance conditions. Implement hybrid acquisition in the shared
Go ENTRY-slot validator: use a native slice only when the 16-byte header fits
inside a power-of-two cache window, and otherwise use cached readAt into one
private fixed 16-byte Reader buffer. Keep the buffer at the cold tail of Reader
state and parse immediately without exposing a borrowed header. Preserve the
validator's original validation/error behavior in the isolated SOW-0108 build;
preserve paused SOW-0159 validation when applying the acquisition helper in the
working tree. Rust's existing native header path needs no equivalent code change.

This approval expands the two-function Explorer repair only by the private
Reader acquisition helper/buffer and the required tests, benchmarks and artifact
updates. Measure opening costs, actual Reader allocation class, retained memory,
cached/crossing/eviction work, unchanged queries and raw/compressed indexed
aggregation with matched helpers and pure builds. The isolated microbenchmarks
do not establish that this change resolves the compact slowdown. Keep SOW-0159
and SOW-0121 paused until this complete repair passes local validation, review
disposition and the complete-and-clean audit.

### Hybrid Implementation Validation And Allocation Measurement - 2026-10-15

The approved hybrid helper is implemented with the original candidate's buffer
at Reader's tail. Native slices parse immediately; boundary/non-power-of-two
headers use cached readAt. The frozen prerequisite transfers only the two
Explorer functions, the helper/buffer and header acquisition into HEAD. Paused
SOW-0159 bounds/shape/recovery rules remain absent from that standalone build.
Both timing binaries include identical complete Explorer and header benchmark
helpers; the direct-helper correctness-only file is excluded from both timing
builds and included in separate correctness builds. Runtime hashes and matched
benchmark hashes are identical between the two correctness test revisions.

- Full standalone pure Go suite passes: journal 98.896 seconds; 1,091 leaf
  cases across all passing packages, including the initial 100 header cases
  and 178 Explorer lifetime/error/boundary cases.
- Expanded focused validation passes 303 leaves on native and 386 builds:
  124 header cases, 178 Explorer cases and the allocation-shape diagnostic.
- Race selection passes 287 leaves in 216.556 seconds; the added 24 raw-header
  vectors also pass a separate race run. Working-tree header checks pass all
  124 cases with the paused SDK validation still present.
- All sixteen large-file Go/Rust indexed/traversal checks pass with matching
  results, immutable fixtures and default 32 MiB windows. The first harness
  launch rejected its requested CPU before reading files; retain that failed
  CPU-availability guard log and the successful explicitly pinned retry.
- The fixed 16-byte tail field changes standalone Reader size from 632 to 648
  bytes, and Go 1.27.1 linux/amd64 heap class from 640 to 704 bytes. A retained
  4,096-object allocation diagnostic confirms both classes. Empty-reader opens
  allocate 64 additional bytes; five-row opens save five header allocations
  and net 16 bytes. These are measured fixed costs, not only raw sizeof.
- Ten paired allocation-only measurements flag a slowdown: 86.790 to 91.825 ns,
  ratio 1.05805, 95% interval 1.03400-1.08427. Continue the complete retained
  opening/header matrix before refining placement; no performance acceptance
  is claimed. Investigate whether verified unused Reader fields can hold the
  same approved buffer without changing the hybrid algorithm or adding memory.

The initial live header test exposed a separate healthy-positioning defect:
opening an empty reader, appending then explicitly refreshing skips the first
row unless the caller first SeekHead. A diagnostic against unchanged HEAD
confirms the same failure after populated EOF and unique-triggered refresh.
Original SOW-0045 is reopened/paused with all four reproductions and the full
position/caller validation plan. The header test explicitly positions at head;
no positioning repair is mixed into this SOW. This valid separate finding is
tracked, not silently discarded or claimed fixed.

### Buffer Placement Refinement Within Option A - 2026-10-15

Keep the approved hybrid algorithm and fixed Reader-owned buffer. The initial
tail placement increases Go's allocation class and allocation-only latency, so
reuse the two private fields `cursor uint64` and `position int` instead. Complete
HEAD/current source searches find no reads, writes or literal initializers for
either Reader field. All Reader literals are keyed; no reflection lookup,
unsafe cast, field-offset constant, serialization or assembly dependency was
found. Other cursor references belong to distinct Explorer or CLI types.

On gc/amd64 the unused fields occupy bytes 400-415. A fixed [16]byte buffer in
that span preserves direction at offset 416, every surviving later field and
the existing GC pointer layout; removing the new tail buffer restores size
632. On gc/386 the old span is 12 bytes, so replacement shifts subsequent
fields by four bytes and predicts size 524 -> 528. Both are predicted to fit
the checked runtime's 576-byte allocation class; measure rather than claim
that prediction as a result. Private field names/count change in both builds.

This is a storage-placement refinement of the selected option A, with the same
header routes, ownership, validation and public behavior. No additional reader
algorithm or unrelated cleanup is introduced. Record this plan before code;
then re-run functional/race/386 checks, retained allocation diagnostics and the
complete matched performance gates. Retain the first tail-buffer executable,
source manifest and full matrix as rejected/diagnostic evidence. No performance
acceptance follows from a static layout argument.

The original tail-buffer matrix finishes all 27 cases: seventeen improvements,
nine inconclusive results and the allocation-only regression above. All four
large opens improve; empty-reader latency is inconclusive while retaining the
measured 64-byte allocation increase. Preserve all cases and the complete
manifest in `sow-0108-hybrid-1-header-construction-performance`; do not accept
the tail placement by averaging its unfavorable allocation case away.

The revised native allocation diagnostic measures size 632 and heap class 640
for both baseline and candidate. The 386 diagnostic measures size 524 -> 528
but heap class 576 for both. Each uses 4,096 escaping Reader allocations. These
results confirm the predicted class preservation; full revised latency/query
comparisons remain required. No additional public API or format change occurs.

### Revised Placement Validation Checkpoint - 2026-10-15

The frozen `hybrid-slots-1` runtime differs from HEAD only in the two Explorer
functions and the approved Reader acquisition/storage changes. All six native/
386 test and CLI artifacts report CGO_ENABLED=0; source, helper and executable
hashes are retained in `sow-0108-hybrid-slots-1-validation-build-manifest.json`.
The separately built race executable uses CGO only for race instrumentation.

- The full standalone Go suite passes serially, 1,115 leaves and journal
  package 106.386 seconds. The first run overlapped heavy validation and failed
  the stock live-reader stress harness because its writer exceeded 45 seconds.
  Preserve that failure. Unchanged baseline passes the same live case alone in
  22.65 seconds; the complete candidate suite then passes without other private
  validation jobs. An initial direct-binary diagnostic used the wrong working
  directory and skipped its C oracle build; that skipped run is not evidence.
- Race selection passes all 311 leaves in 261.348 seconds. Native/integrated
  and 386 focused controls each pass 303 leaves: 124 header cases, 178 Explorer
  cases and one allocation-shape diagnostic. The 386 run takes 96.911 seconds;
  the working-tree integrated run takes 88.704 seconds.
- Default 32 MiB-window validation completes all sixteen large Go/Rust indexed
  and traversal commands with exact results/counters and clean fixture/binary
  hashes. Keep the successful new-placement manifest separate from prior
  candidates; no stale executable is used for this result.
- Expanded audit reports complete-and-clean before performance; whitespace
  checks pass. The isolated code/artifact review patch checks cleanly against
  the index without staging it; paused SDK runtime changes remain excluded.

All correctness jobs have finished before final timings start. Run constructor/
header, unchanged-query, large aggregation, raw and codec matrices sequentially
with fixed pure builds on CPU 28. Preserve every case, allocation observation,
semantic counter and initial/repeat result. Large-open controls already remove
100,000 allocations/1.6 MB per open; this does not establish the complete query
performance gate. Final technical review remains unauthorized and unwaived.

### Revised Placement Paired Performance Checkpoint - 2026-10-15

The frozen `hybrid-slots-1` pair completes four matrices sequentially on CPU
28 with two warmup pairs and ten alternating measured pairs per case. Runtime,
helper, executable and fixture hashes remain fixed. The retained driver source
manifest records the exact measurement programs. The 95% paired bootstrap
intervals describe observed timing uncertainty; an inconclusive interval is
not evidence of equivalence and no slowdown tolerance is introduced.

- Constructor/header, 27 cases: eighteen timing improvements, nine
  inconclusive, no flagged slowdown. Every one of the 324 paired allocation
  observations, including warmups, has non-positive candidate byte/allocation
  deltas. Reader allocation remains 640 bytes/one allocation; its paired time
  ratio is 0.99015, interval 0.98139-1.03736, inconclusive.
- Each 100,000-entry open removes exactly 100,000 allocations and approximately
  1.6 MB. Regular mmap time is 21.75 -> 19.11 ms, ratio 0.8835, interval
  0.8593-0.9230; compact mmap 17.26 -> 14.77 ms, ratio 0.8716, interval
  0.8430-0.8847; regular ReadAt 68.70 -> 66.02 ms, ratio 0.9592, interval
  0.8801-0.9835; compact ReadAt 46.41 -> 40.28 ms, ratio 0.8906, interval
  0.8333-0.9803. All four are improvements. Empty-open allocation counts and
  bytes are unchanged; five-entry opens save five allocations/80 bytes.
- Every direct header case saves one allocation/16 bytes. Cached fitting
  checks have paired time ratios about 0.466-0.473; cached crossing checks
  0.7452 mmap and 0.7434 ReadAt. Eviction still allocates cache metadata:
  candidate mmap 130 bytes/one allocation and ReadAt 205 bytes/two allocations.
  The spec explicitly distinguishes reusable header storage from cache-window
  allocation rather than implying that all reader operations allocate zero.
- Unchanged row/entry controls, four cases: compact rows improve, ratio
  0.91640, interval 0.91027-0.96146. Regular rows are inconclusive at 0.96822,
  interval 0.91723-1.00102; regular entry iteration 0.98578,
  0.94822-1.01318; compact entry iteration 0.97715, 0.95089-1.01229.
  Exact result/checksum/work-counter preflights pass. This new compact-row
  result does not erase the rejected two-function candidate's reproduced
  slowdown.
- Large aggregation, sixteen cases: twelve improvements, four inconclusive,
  no flagged slowdown; Go/Rust output and work counters agree exactly. The
  prior problematic compact low-cardinality dense facet is now inconclusive:
  ratio 0.93848, interval 0.92203-1.03772. Other inconclusive dense cases are
  regular high-cardinality facet, compact high-cardinality facet and compact
  high-cardinality histogram. The latter has ratio 1.00525, interval
  0.99507-1.06816, so it needs a retained longer-batch confirmation before
  accepting that path. All comparisons use the valid 256 MiB control window;
  separate default-window correctness remains the sixteen passing commands.
- Raw aggregation, eight cases: all improve, with paired ratios 0.60258 to
  0.95338. Every case lowers both bytes and allocation counts; low-cardinality
  dense histogram falls from 3,694 to 130 allocations and 1,259,536 to 109,488
  bytes per query. High-cardinality dense facet falls from 3,699 to 2,937
  allocations and 879,288 to 867,096 bytes. Raw timing logs check errors and
  nonzero updates; they do not retain complete maps or indexed counters.
  Semantic/counter equality comes from focused correctness and large-matrix
  validation rather than from those raw timing observations alone.

Evidence directories are
`sow-0108-hybrid-slots-1-header-construction-performance`,
`sow-0108-hybrid-slots-1-unchanged-controls`,
`sow-0108-hybrid-slots-1-large-performance` and
`sow-0108-hybrid-slots-1-raw-performance`, each retaining its complete manifest,
per-process observations and comparisons under ignored task scratch.

The 36-case compressed matrix is running. After it finishes, measure selected
386 constructor/header cases against the frozen 386 baseline, because the
storage replacement shifts later fields by four bytes on that architecture
despite preserving its measured 576-byte allocation class. Repeat the four
inconclusive dense large aggregations with longer timed batches and the same
executables. Preserve all initial/repeated evidence; investigate any reproduced
slowdown instead of averaging it into other cases. No source optimization,
scope change, final performance acceptance or review approval follows from
this checkpoint.

The unchanged XZ dictionary-allocation opportunity is now explicitly tracked
by pending SOW-0160. No decoder work or execution dependency is added here.
This settles the earlier outside-scope note without promising untracked work.

Complete the sibling performance reference with the frozen unchanged Rust
helper and this Go candidate on the four regular/compact low-cardinality dense
facet/histogram cases. Both timers include reader open, query/result processing
and close. Use the same fixture, explicit 256 MiB window, output checks and
alternating paired procedure; interpret cross-language differences as reference
costs, not Go before/after regressions. Stock journalctl has no equivalent
facet-count/time-bucket operation, as recorded above. This additional reference
does not replace the full Go baseline/candidate performance gate.

### Current Artifact Maintenance And Follow-Up Ledger - 2026-10-15

- AGENTS.md: unchanged; the existing lifetime, one-SOW, review and pure-runtime
  rules already govern this repair. No workflow or product-scope exception is
  added beyond the user's recorded header-acquisition approval.
- Runtime project skills: the compatibility skill now requires forced DATA
  invalidation, error/counter/bucket controls, matched helper and timer
  boundaries, constructor/header checks and measured allocation classes.
- Specs: `product-scope.md` records owned internal aggregation keys and
  synchronous header views/reusable fixed storage. These restore internal
  lifetime guarantees without adding an API. Cache metadata allocation remains
  explicitly allowed by the existing bounded accessor contract.
- End-user/operator docs: `tests/benchmarks/README.md` documents the focused
  lifetime/header/codec controls, immutable fixtures, timer boundaries and
  allocation diagnostics. SDK usage, CLI syntax and published API docs do not
  change in this prerequisite; their paused SDK edits are not included here.
- End-user/operator skills: no output/reference skills exist or are affected.
- SOW lifecycle: keep this SOW in-progress/current until the complete gate is
  satisfied. Preserve paused SOW-0159 and SOW-0121. Original SOW-0045 is reopened
  and paused for the independently reproduced explicit-refresh positioning
  regression; it does not reorder this chain. Pending SOW-0160 tracks the
  unchanged XZ allocation opportunity without authorizing implementation.
- Status indexes: canonical and root indexes reflect the active prerequisite,
  the paused work and those concrete tracked findings.
- Sensitive data: all new tests use synthetic journal values/identities. Raw
  fixtures, profiles, build/measurement manifests and diagnostic executables
  remain in ignored repository-local scratch. Durable evidence contains source
  pointers, public commit identity and aggregate observations only.
- Review disposition: the internal source/data investigations are ordinary
  implementation analysis, not external acceptance. External review remains
  unauthorized and unwaived; request that decision after all local validation.
- Audit: the expanded pre-performance audit was complete-and-clean. Repeat it
  after this final evidence/tracking work; no close is claimed from an earlier
  audit or an exit code without its full verdict.

Lessons retained in the compatibility skill and benchmark procedures: an
incorrect default-window baseline cannot certify a speedup; per-payload header
allocation and Reader heap-class growth must be measured separately; a fitting
native slice and a crossing temporary slice can have different file-read costs;
and incomplete timing intervals cannot be reported as equivalence. Deferred
recovery interaction checks remain explicitly in SOW-0159. The positioning and
XZ opportunities now have their own concrete lifecycle destinations above.

### Revised Placement Complete Codec Matrix - 2026-10-15

All 36 compressed cases finish: seventeen improvements, nineteen inconclusive
and no timing slowdown flags. Complete-stream/output/indexed preflights and
fixture/executable checks pass. All 360 measured baseline/candidate allocation
pairs lower both bytes and allocation counts; retain the separate warmup data
as well. The complete matrix is
`sow-0108-hybrid-slots-1-codec-performance`; unchanged decoder costs are not
presented as introduced by the repair.

Three cases warrant longer one-second batches using the same frozen pair:

| Case | Paired time ratio | 95% interval | Reason |
|---|---:|---:|---|
| regular/xz/low/facet | 0.96926 | 0.90170-1.27119 | Widely varying initial timings |
| regular/xz/high/facet | 1.00816 | 0.96065-1.07747 | Positive initial point estimate |
| compact/lz4/high/facet | 1.02479 | 0.76851-1.25093 | Positive estimate and wide interval |

This is confirmation of measured uncertainty, not acceptance of a slowdown or
a request to redesign a decoder. Preserve initial and repeated results rather
than selecting only favorable observations. The targeted 386 constructor/header
matrix runs first; longer large-case confirmations and the Rust reference remain
in the sequential measurement plan. Final performance assessment and technical
review disposition remain open.

### Targeted 386 Initial Flag - 2026-10-15

The selected 386 matrix flags regular five-entry mmap open in its initial
ten paired measurements: median 24,136.5 -> 28,073.5 ns, paired ratio 1.16724,
95% interval 1.15516-1.37576. Bytes fall from 1,648 to 1,568 and allocations
from 22 to 17, so allocation reduction alone cannot establish acceptance.
The retained 386 Reader diagnostic still measures the same 576-byte heap
class. No root cause is established for this timing result.

Finish all thirteen selected cases, then independently repeat every flagged
case using one-second timed batches and the same frozen binaries. Preserve the
initial result and repeat rather than substituting larger-file gains. Do not
change storage, header routes or the approved design in response to a single
initial flag. A reproduced slowdown must be resolved before closing this repair.

The complete selected 386 matrix finishes thirteen cases: eight improvements,
four inconclusive and the single regular-small-open flag above. Its retained
manifest is complete. The one-second repeat now runs first, before the queued
large/codec/sibling comparisons; no source or executable changes are made.

Independent read-only data analysis of the completed compressed matrix checks
all 864 process outputs, 2,020 preflight counter records and fixture/executable
hashes. All 432 pairs reduce both bytes and allocations, including all 72
warmup pairs; the 360 measured pairs alone populate the comparison arrays.
This confirms measurement bookkeeping, not timing equivalence or final
acceptance. Initial intervals and selected repeats remain recorded above.

### Reproduced 386 Flag And Large Confirmations - 2026-10-15

The same regular-small-open 386 case independently regresses in one-second
batches: 23,719.5 -> 26,824.5 ns, ratio 1.12920, 95% interval
1.07498-1.15086. Bytes remain 1,648 -> 1,568 and allocations 22 -> 17. Retain
both initial and repeated observations in
`sow-0108-hybrid-slots-1-386-small-open-repeat`; this is a failed performance
gate, not an acceptable tradeoff for faster large journals. Do not claim
readiness or run external acceptance review while this failure is unresolved.
Investigate the frozen binaries, matched benchmark setup and 386 layout before
changing implementation or requesting an irreducible design decision.

Longer large-case confirmations complete with ten fresh processes per phase
per paired block, immutable binaries/fixtures and exact semantics/counters:

| Case | Paired time ratio | 95% interval | Assessment |
|---|---:|---:|---|
| regular/high/facet/dense | 0.95082 | 0.91040-0.96213 | Improvement |
| compact/low/facet/dense | 0.96712 | 0.92380-0.99642 | Improvement |
| compact/high/facet/dense | 1.00735 | 0.99724-1.03804 | Inconclusive |
| compact/high/histogram/dense | 1.01645 | 0.96837-1.06635 | Inconclusive |

All records remain in `sow-0108-hybrid-slots-1-large-dense-repeat`. The prior
problematic compact low facet now improves in this longer run. Neither that
improvement nor the two remaining uncertain large cases can cancel the confirmed
386 failure. The queued three-case codec confirmation runs next, unchanged.

The three one-second codec confirmations complete with unchanged counters,
fixtures and executable hashes. All are inconclusive: regular XZ low facet
ratio 0.98470, interval 0.96383-1.02891; regular XZ high facet 1.00500,
0.97499-1.07477; compact LZ4 high facet 0.98725, 0.97381-1.01790. None confirms
a slowdown, and none proves equivalence. Retain the complete original matrix
and `sow-0108-hybrid-slots-1-codec-repeat` together.

Read-only 386 source analysis establishes that Reader options, accessor and
file-size fields precede the four-byte shift. Accessor/backend/window objects
are unchanged separate allocations; both fitting header paths reach the same
window lookup. The changed Explorer functions do not run during ordinary open.
Warmed 386 fitting-header checks improve about 82 -> 50 ns, which does not
explain the positive complete-open delta. Later-field/GC/link placement remains
an unproven hypothesis. Process CPU totals include untimed setup/calibration;
their larger system-time delta is only a diagnostic lead.

After the bounded Rust reference finishes, profile the existing 386 binaries
with an equal fixed number of small opens, then inspect syscall counts/timing
and existing generated-fixture/backend controls. Profiles are diagnostic and
cannot replace uninstrumented paired acceptance. No production source change
is authorized by an unproven attribution; record any necessary helper extension
before implementing it, using identical helpers for both builds.

### Frozen 386 Diagnostic Evidence - 2026-10-15

Equal-count CPU profiles complete 150,000 regular-small opens per binary:
baseline 24,244 ns and candidate 24,295 ns. Profiling changes execution and
these two runs are not paired acceptance. Top samples remain dominated by
syscalls; they do not identify a causal added reader operation. Separate
3,000-iteration syscall diagnostics report exactly matching principal counts:
3,014 openat/close, 3,003 fstat64, 3,023 mmap2, 3,005 munmap, 12,034 fcntl and
3,008 epoll_ctl, including setup/calibration. Traced elapsed time is not an
uninstrumented speed comparison. No additional window mapping or file read is
shown by these counts.

The complete diagnostic manifest, profiles, syscall tables and opening
disassembly remain in `sow-0108-hybrid-slots-1-386-small-open-diagnostics`.
Neither diagnostic establishes why the independently paired one-second case
regresses. Test that calibration/iteration-count lead next with ten alternating
uninstrumented pairs of exactly 150,000 opens using the original frozen
binaries. Preserve every earlier result. No production or helper source changes
have been made for this investigation; performance acceptance remains failed.

The bounded sibling reference also completes. At the same 256 MiB window and
with matching 100-row output/100,000 updates, Go candidate/Rust baseline paired
ratios are 0.79062 regular facets, 0.79604 regular histogram, 0.76150 compact
facets and 0.77443 compact histogram, each interval below one. This is only the
four measured low-cardinality dense cases, not a broad cross-language speed
claim. Go process peak RSS is about 4.4-5.3 MiB higher in this reference; retain
both phases' RSS observations. The fixed Go baseline/candidate matrices, not
these sibling ratios, decide regression acceptance. Evidence is retained in
`sow-0108-hybrid-slots-1-sibling-reference`.

### Shared Small-Fixture Measurement Plan - 2026-10-15

Uninstrumented fixed-count confirmation is inconclusive: paired ratio 1.09857,
95% interval 0.99806-1.15678, phase medians 23,772 -> 26,155 ns. This retains
the positive estimate and does not clear the initial/repeated slowdown.

The current small benchmark creates a deterministic-content file per process
and calibration, but does not hash/freeze the actual inode/path across phases.
Add an optional `JOURNAL_SDK_READER_OPEN_SMALL_CORPUS` benchmark-only input,
with the same regular/compact path shape and five-entry preflight as the
generated path. Keep generation as the default. Prepare one synthetic regular
and compact file using the same writer options/rows, verify bytes and counts,
then hash both before and after every comparison.

Rebuild both 386 timing binaries with the identical extended helper and
byte-identical original runtime sources. Compare generated versus immutable
small fixtures with those same rebuilt binaries, and mmap versus ReadAt with
the shared corpus. Retain old binaries/results: new test linking can itself
change code placement, so disappearance in both fixture modes would not prove
that fixture generation caused the original flag. No SDK runtime, approved
header routing or Reader storage changes belong to this measurement refinement.

### Shared-Fixture Outcome And Private Storage Refinement - 2026-10-15

The extended-helper builds have byte-identical runtime source to the original
pair. Their generated regular-small mmap control is inconclusive at ratio
0.99363, interval 0.97127-1.00925. All four shared five-entry cases are also
inconclusive: regular mmap 1.00737 [0.98035,1.08793], regular ReadAt 0.99699
[0.98264,1.01835], compact mmap 0.99790 [0.88568,1.03366], compact ReadAt
1.00365 [0.98141,1.02437]. Fixture hashes/count preflights pass. These new test
binaries do not establish why the older pair independently slowed; retain
both build contexts and all results. No performance acceptance follows.

Refine only private storage within approved option A: move the existing
`entryDataActive bool` beside `currentHeaderValid bool`, consuming padding,
and remove its old padded slot. Keep the fixed 16-byte owned header buffer,
hybrid routes and all scalar/error policy unchanged. Source analysis predicts
standalone Reader size 528 -> 524 on 386, restoring original size, and
632 -> 624 on amd64; measure these predictions and actual heap classes.
This eliminates known unnecessary struct growth. It is not claimed to explain
the earlier timing result or to guarantee a latency improvement.

Evidence/risk gate before code: all five boolean uses are named reads/writes;
Reader literals are keyed; no Reader unsafe cast, field-address/offset,
reflection, serialization or assembly dependency was found. Reader remains
non-concurrent. Earlier fields and the header algorithm remain unchanged;
unique-state field offsets and GC metadata move. Preserve every unrelated
paused SDK field/change. This is a field-placement refinement of A, not a
new API, cache algorithm, architecture exception or accepted slowdown.

Freeze new before/candidate/correctness sources and matched helpers. First
verify native/386 sizes/classes and the previously problematic generated/shared
small-open cases, retaining an old-helper comparison as an independent link
context. If those warrant continuing, complete focused/full/race/386 and
default-window large-file correctness, then the complete affected performance
matrices using the new fixed pair. Record every unfavorable observation;
do not infer performance acceptance from packing or allocation counts alone.
All data remains synthetic under ignored scratch. Existing lifetime/header
specs and controls apply; record measured layout/evidence and update the review
bundle/status indexes without transferring paused SDK implementation.

### Packed Storage Build And Layout Measurements - 2026-10-15

Five frozen pure builds complete: before/candidate amd64 and 386 with the
extended matched helper, plus a 386 candidate with the original matched helper.
All report CGO_ENABLED=0. Runtime manifests verify the only change from the
previous candidate is the private boolean move in `reader.go`; the Explorer
repair and hybrid acquisition algorithm are byte-identical.

Retained 4,096-object diagnostics measure baseline/candidate sizes/classes:

| Architecture | Reader size before -> packed | Allocation class before -> packed |
|---|---:|---:|
| amd64 | 632 -> 624 bytes | 640 -> 640 bytes |
| 386 | 524 -> 524 bytes | 576 -> 576 bytes |

The old-helper 386 packed build independently measures 524/576 as well. These
results confirm the source layout calculation, not a timing fix. Evidence is
`sow-0108-hybrid-packed-1-build-manifest.json` and its per-build shape logs.
First run the formerly problematic regular-small mmap case against the original
frozen baseline with identical original benchmark helpers. Preserve that link
context separately from the extended-helper/shared-file controls. Full
functional and performance revalidation of this packed candidate remains open.
The earlier review patch targets the previous candidate and is superseded;
regenerate it from the new frozen source only after local gates are assessed.

### Packed Old-Helper Flag And Unchanged-SDK Control - 2026-10-15

The packed candidate still flags regular-small mmap open against the original
old-helper baseline: 23,411 -> 26,639 ns, paired ratio 1.14553, interval
1.12112-1.15893. Allocations remain 22 -> 17 and bytes 1,648 -> 1,568.
Restoring 386 sizeof did not clear this old-link-context timing gap. Retain
`sow-0108-hybrid-packed-old-helper-1-small-open`; stop broad acceptance reruns
until this path is understood. Do not claim a causal fix from boolean packing.

The newer before build also has an observed approximately 26 us small-open
cost even though every SDK runtime source hash matches the original before
build. Test that lead directly: compare the original before executable with
the extended-helper before executable, using alternating one-second pairs,
no profiling and no shared-corpus override. These have byte-identical SDK
source and different benchmark-helper/build contexts. This negative control
can establish whether the timing gap requires an SDK change; it is not a
replacement baseline/candidate acceptance comparison. Keep all prior evidence
and do not infer fixture or linker causality without that discriminating result.

The unchanged-SDK negative control completes: original-before 23,196 ns versus
extended-helper-before 23,285 ns, ratio 0.99803, interval 0.96564-1.02146,
inconclusive, with exactly identical 1,648 bytes/22 allocations. This does not
reproduce the old SDK candidate's gap or establish its cause. Build metadata
matches across original before/candidate, extended before and packed old-helper
candidate, including compiler/runtime settings; no flag mismatch is found.
Retain `sow-0108-386-small-open-unchanged-sdk-control` without changing the
earlier failure disposition. Next measure the packed candidate against its own
matched baseline using the immutable small corpus in both layouts/backends.

### Same-Process Header Strategy Diagnostic Plan - 2026-10-15

The packed shared regular-small mmap case initially flags ratio 1.03865,
interval 1.01015-1.08464; regular ReadAt is inconclusive at 0.98728,
0.96302-1.03191. Finish this matrix before further jobs. Neither packing nor
changing test linking clears the full uncertainty.

Prepare a private analysis-only benchmark in a separate source archive. Open
the existing five-entry fixture through the normal catalog constructor, wrap
its accessor, then invoke the unchanged ENTRY-array loader and normal close.
For the five known ENTRY-header offsets only, the wrapper selects native
tempSlice, cached readAt into a reusable owned 16-byte buffer, or the original
allocating 16-byte readAt. Other reads delegate unchanged. Every strategy uses
the same wrapper, constructor, fixture, count checks and close path; assert
that exactly five headers were intercepted. Rotate strategy order across
processes and include mmap/ReadAt controls.

This exercises existing primitives without changing SDK runtime code, reader
options or the approved product strategy. The wrapper and constructor split
add common overhead and alter call/link context; it is diagnostic evidence,
not a public API implementation or an acceptance speed comparison. Keep it
out of all final timing binaries and committed regression tests. No unsafe
memory access, native dependency, live host journal or non-synthetic data is
needed. Preserve full outputs, fixture hashes and algorithm/counter preflights.

The packed shared-file matrix finishes one regular mmap timing flag and three
inconclusive cases; all save five allocations/80 bytes. Compact mmap ratio is
1.00203, interval 0.88213-1.08504; compact ReadAt 0.99800,
0.97044-1.01871. Preserve the complete matrix and existing failure disposition.

The same-process strategy diagnostic completes fourteen processes, including
twelve measured processes with balanced rotating orders. Native/allocating
paired ratio is 0.94950 [0.90079,0.99107]; owned-buffer/allocating is 0.98108
[0.90499,0.99205]. Both save the expected five allocations/80 bytes. Every
strategy intercepts exactly five ENTRY headers; inputs/hashes remain fixed.
These observations support the approved primitive choice but do not explain
the separately linked full-open flag or satisfy acceptance. The generic
comparison helper retained median key names containing `seconds`; their actual
input/output numeric unit in this diagnostic is ns/op, as recorded in each
benchmark row. Ratios and intervals are dimensionless.

Next use a separate private executable to compare full construction with the
same metadata-only constructor followed immediately by the unchanged ENTRY
loader. No accessor wrapper or alternative header strategy is used. Both paths
must return five identical offsets, metadata and close outcomes. This isolates
the constructor call/frame context using one linked program. Source inspection
shows the original metadata constructor does nothing after optional ENTRY
loading except return the Reader. The split is diagnostic only: no SDK source
or constructor contract changes until it provides evidence for a concrete
implementation refinement. Retain both layouts/heap classes and every prior
failure; benchmark strategy order remains rotated rather than grouped.

The same-process constructor split is inconclusive: ratio 0.99930, interval
0.98739-1.01298, with identical allocation counts/bytes and valid five-row
results. Reject it as an implementation change; it provides no demonstrated
benefit and does not explain the flagged complete-open comparisons.

### Read-Only File-Open Scope Lead - 2026-10-15

No new file-opener runtime implementation is authorized. Source investigation
identifies a concrete separate work reduction to measure before requesting any
scope decision: Unix `openReaderFile` calls `os.Open`; installed Go's Unix
`newFile` attempts poller setup for regular files and restores blocking mode
after registration fails. The frozen Linux traces record four fcntl calls and
one failed epoll_ctl per reader open. `os.NewFile` given an already blocking
descriptor follows the non-pollable path. This is independent of, and not
claimed as the cause of, the header candidate's timing gap.

Changing the shared opener would also affect whole-file compressed input,
writer/log metadata and retention inventory, so it is not a routine header
placement change. Analyze a separate native-journal-read helper that leaves
those callers and non-Linux behavior intact. Preserve Go/Rust public behavior,
pure runtime, compiler floors, close-on-exec, interrupted opens, typed path
errors and descriptor cleanup. User approval is required before any runtime
scope expansion.

Measure only existing standard-library primitives in an isolated Linux
benchmark module: os.Open versus blocking syscall.Open with O_CLOEXEC followed
by os.NewFile, each with identical stat/close checks. Use synthetic shared
files, both local amd64/386 builds, matched single-program rotating order and
retained syscall/allocation observations. This estimates the available work
reduction for a concrete proposal; it does not implement an SDK opener or
establish full-reader performance. Read-only platform/contract analysis runs
in parallel; no installs, services or live journals are involved.

### File-Open Primitive Evidence And New Scope Decision - 2026-10-15

The isolated standard-library module completes both pure-Go architectures,
two warmups and twelve measured processes per architecture, alternating the
two strategy orders on CPU 28. Both strategies open the same immutable
five-row synthetic file, stat it, obtain its descriptor and close it. Neither
uses or changes an SDK opener.

| Architecture | os.Open median | Blocking open/NewFile median | Paired time ratio and interval | Bytes / allocations |
|---|---:|---:|---:|---:|
| amd64 | 6,154.5 ns | 4,795.5 ns | 0.77795 [0.77368,0.78505] | 440 / 4 both |
| 386 | 7,835.5 ns | 5,627.5 ns | 0.71409 [0.70542,0.73667] | 344 / 4 both |

Fixed-count 3,000-open traces confirm the work reduction: fcntl counts drop
12,006 -> 3,006 and epoll_ctl 3,001 -> 1, with identical open/close/stat counts.
The extra six fcntl calls and single epoll_ctl are process setup. Thus the
primitive removes four calls per open, including the failed regular-file poller
registration. Full source, fixture/executable hashes, per-process outputs,
allocation observations and traces are retained in
`sow-0108-file-open-primitives` under the task's ignored scratch directory.

This is not a demonstrated cure for the complete-reader regression. The current
packed candidate still has a regular-small mmap paired time flag of 1.03865
[1.01015,1.08464] with the shared fixture, and 1.14553 [1.12112,1.15893] in the
original matched-helper control. Both save five allocations/80 bytes. No
allocation saving compensates for a failed timing gate.

Read-only platform and contract checks establish:

- Existing `openReaderFile` in `go/journal/file_open_unix.go:15` is shared by
  native reads, `.zst` sources, log metadata and root retention. Do not change
  that shared helper for the narrow proposal.
- A separate helper called only by the native branch in
  `go/journal/reader.go:305-307` confines the change. Other platforms can
  delegate to their existing helper; compressed sources continue at line 310.
- Linux standard `syscall.Open` supplies large-file handling on 386. Preserve
  read-only access, atomic close-on-exec, normal symlink following, interrupted
  open retries, typed `os.PathError` values and descriptor ownership/cleanup.
  No dependency, compiler-floor or CGO change is needed.
- Public native readers require sized random access and expose neither file
  deadlines nor a concurrent-close guarantee. Explicit paths currently have
  no regular-file rejection. Do not add such a rejection or a stat/reopen race.
  Keep compressed sequential-source scheduling unchanged.
- Direct descriptor opening bypasses Go's internal `testlog.Open` hook.
  Fixture-dependent validation must run uncached and retain input hashes;
  importing internal hooks or using linkname is not an acceptable workaround.
- Go/Rust parity concerns observable behavior and guarantees, not identical OS
  calls. Rust needs no matching runtime change for this internal Linux cost.

New user decision 1 remains open; the earlier `1a` approved hybrid header
acquisition only and does not authorize this additional scope:

- A, recommended long-term-best: add a Linux-only native-reader opener with
  the preserved contract above, retaining the hybrid algorithm. Benefit:
  removes independently measured work on both architectures without new
  allocation. Cost/risk: new platform-specific code and descriptor-contract
  coverage; full-reader improvement is not established by primitive results.
- B, surgical: retain the existing opener and restore the original allocating
  header acquisition on 386 while retaining the approved hybrid on amd64.
  Benefit: limits the change to header acquisition and preserves the original
  386 header costs. Cost/risk: gives up five small-open or 100,000 large-open
  allocation savings on 386 and still requires the complete performance gate;
  the independently linked timing cause remains unproven.

Recommend A because it eliminates verified unnecessary work rather than adding
an architecture-specific allocation fallback. No runtime opener or 386 routing
change starts before the user chooses. Either option preserves the full
correctness, allocation and per-path timing gates and the approved sequential
SOW-0108 -> SOW-0159 -> SOW-0121 execution order.

The explicit numbered scope question has been submitted to the user and is
pending; no dependent implementation starts meanwhile. The expanded changed-
artifact audit passes with `SOW initialization complete and clean` in
`sow-0108-file-open-proposal-audit.log`; `git diff --check` is clean. Independent
allocation-result inspection confirms exact equality in all 56 primitive
observations, including warmups. All owned measurement processes completed
normally. No files are staged or committed and the two requested SOWs remain
paused behind this active prerequisite.

### Approved Native-Reader Opener And Pre-Implementation Gate - 2026-10-15

The user answered `A` to the additional opener question. This authorizes the
recommended long-term-best Linux native-journal-only helper, while retaining
the approved hybrid header algorithm and the full performance gate. The
original shared opener, compressed-source path and other platforms retain
their existing behavior. No CGO, dependency or compiler-floor exception is
introduced. SOW-0108 remains the sole active implementation.

- Problem / root-cause model: small 386 opens still fail timing acceptance.
  Independently, standard Linux regular-file opening performs four avoidable
  poller-setup syscalls. Removing that measured work is approved; it is not
  claimed to explain the header timing difference.
- Evidence reviewed: primitive module and all 56 allocation observations,
  syscall traces, existing platform openers and their callers, local Go 1.26.2
  and 1.27.1 descriptor construction and open semantics, Reader ownership and
  public concurrency/deadline contracts, current packed candidate matrices.
- Affected contracts and surfaces: native Go journal opening on Linux,
  including mmap/ReadAt, regular/compact, facade/directory/query and live reads.
  Preserve read-only access, atomic close-on-exec, 386 large-file support,
  symlinks, EINTR retry, typed path errors, descriptor cleanup and malformed-
  file outcomes. No explicit native regular-file rejection is added.
- Existing patterns to reuse: standard syscall.Open plus os.NewFile; existing
  platform opener delegation, Reader constructor error cleanup and immutable
  fixture/paired benchmark infrastructure. The descriptor stays private.
- Risk and blast radius: blocking descriptors skip poller setup. Restrict the
  new helper to native sized random-access files; retain compressed sequential
  source opening. Direct open bypasses Go test-cache input observation, so run
  fixture-dependent validation uncached and retain fixture hashes. No runtime
  hook, host probing or new API is needed.
- Sensitive data handling plan: synthetic paths, identities and journal fields
  only; temporary files, builds, logs and traces stay in ignored repository
  scratch. Durable evidence uses relative paths and aggregate measurements.
- Implementation plan: add a Linux helper that opens read-only with O_CLOEXEC,
  retries EINTR, wraps open failures as os.PathError, transfers ownership to
  os.File and closes the raw descriptor if wrapping unexpectedly fails. Add a
  non-Linux delegator. Call it only in openJournalFile's non-.zst branch.
- Validation plan: focused read-only/blocking/CLOEXEC, typed-error, symlink,
  sparse >2 GiB and integration controls; retain current compressed-input,
  malformed, cleanup and live tests. Freeze new HEAD-based baseline/candidate
  archives with only approved repair changes and identical timing helpers.
  First repeat small-open controls, then full/focused/race/386/large functional
  checks and affected performance matrices without overlapping private jobs.
  Verify syscall reduction through the real Reader and build non-Linux targets.
- Artifact impact plan: update compatibility spec, runtime validation guidance,
  benchmark procedures, this SOW and both status indexes. Consumer APIs and
  usage do not change; explain any operational/testing implications in the
  appropriate existing artifacts. Preserve all paused SDK implementation hunks.
- Open decisions: this opener design is approved. External review authorization
  or explicit waiver remains necessary after the complete local gate; no other
  product choice is needed to implement this design.

### Native Opener Execution And Storage Refinement - 2026-10-15

The Linux helper and non-Linux delegator are implemented. Only the native
openJournalFile branch changes; the shared opener and compressed sources do
not. Root amd64 descriptor checks pass eleven leaves, including sparse >2 GiB
reads. Five frozen pure builds and their allocation-class diagnostics pass.
Timing excludes both candidate-only direct-header and opener contract tests.

The first complete-reader comparison on immutable shared small files records
two mmap improvements and two inconclusive ReadAt cases, all saving five
allocations/80 bytes. Regular mmap ratio is 0.82745 [0.75815,0.92560]; compact
mmap 0.85482 [0.84128,0.91294]. The original matched-helper setup is initially
inconclusive at 1.04544 [0.99676,1.05503]. Its longer sixteen-pair repeat
confirms a remaining slowdown: 1.03227 [1.01946,1.04638], medians 23,915.5 ->
24,739.5 ns. Preserve both complete results. Opener savings do not clear this
failure and full functional validation has not started for this candidate.

Reflection-only layout diagnostics show the existing fixed buffer replaces
twelve unused bytes on 386 with sixteen bytes. Although packing preserves the
524-byte Reader and 576-byte allocation class, direction moves 372 -> 376,
entryOffsets 376 -> 380, entryIndex 388 -> 392 and currentHeader 396 -> 400.
No cache or compiler cause is established by these offsets.

Next test a private storage-placement refinement within the approved design:
remove the fixed buffer from the old cursor/position location, keep direction
there, and place the same sixteen-byte buffer after entryDataIndex, using the
space recovered by the existing boolean packing. This changes no algorithm,
public API, bounds/error policy or amount of buffer state. It may keep the
ENTRY-offset slice together and align currentHeader on 386; measure rather
than assuming a performance benefit. Freeze new source/build/hash manifests,
check both architecture sizes/classes, and repeat the original and shared
small-open comparisons before transferring a winning placement to the root.
Retain every earlier candidate/failure. This is an internal placement choice,
not an additional opener or architecture-specific header-routing design.

The late-buffer original-helper repeat also flags a slowdown: ratio 1.02144
[1.00831,1.03184], with unchanged allocation savings. Reject field placement
as an established cure; do not transfer the private late layout to the root.

### Internal Constructor Work Refinement - 2026-10-15

Source inspection identifies duplicate accessor statistics recomputation.
`newRollingReaderAccessor` initializes every statistics field; a successful
`probeInitialWindow` updates counters/windows and refreshes the snapshot again.
Each successful return in `newReaderAccessor` then calls `stats()`, recomputing
that already-fresh snapshot. No intervening operation can change it, and all
production callers discard this constructor return value.

Within the approved accessor/performance repair, replace only those three
returns with the cached `statsValue`. Preserve the existing three-result
internal signature and every consumer-visible statistic, backend selection,
fallback reason, error and ownership rule. This is routine removal of duplicate
work, with no additional product/design choice, state, API or dependency. It is
not claimed as the cause of the separately linked timing difference.

Validate initial returned counters against explicit empty/nonempty mmap/ReadAt
expectations, a fresh statistics query and Auto's initial-mmap-failure fallback.
Use synthetic files and the existing constructor/backend patterns. Exclude the
new correctness-only tests from both timing builds, retaining identical complete
benchmark helpers and the original matched-helper comparison. Freeze this
three-line change separately from paused SDK accessor changes. Retain original
Reader placement; check native and 386 allocation classes and repeat the failed
open comparison before full validation. Existing artifact, purity, data handling
and per-path performance/review gates continue to apply unchanged.

The cached-statistics candidate's sixteen-pair original-helper comparison still
flags ratio 1.02597 [1.01353,1.04283]. Seven new initial-statistics controls pass;
this duplicate-work removal is not established as a timing cure. No full
functional acceptance is claimed for this candidate.

Real-Reader tracing verifies 3,003 opens with identical open/close/stat/map/unmap
counts. Candidate fcntl calls fall 12,018 -> 3,009 and epoll_ctl 3,004 -> 1:
four calls saved per open. A controlled openat EINTR injection is followed by
a successful retry and passing one-iteration Reader benchmark. The first trace
checker expected adjacent EINTR/INJECTED markers; strace inserts its errno
explanation between them. Preserve that checker failure and raw output; the
corrected checker validates the retained successful commands and input hashes.
Hardware-counter profiling is unavailable in the installed perf invocation;
retain its error without changing tools, permissions or host settings.

### Private Lazy Buffer Representation Plan - 2026-10-15

Test a representation refinement of the same approved single fixed-size owned
buffer: store a pointer to a sixteen-byte array and allocate that array once
only if a header needs cached readAt fallback. Fitting native headers keep the
existing zero-allocation path. This retains one buffer per Reader, constant
storage, immediate scalar consumption and all acquisition/error/bounds rules.
It adds no public option or architecture-specific routing. The first fallback
allocates one sixteen-byte object, matching the original validator's allocation
for that read; subsequent fallbacks reuse it. Measure cold fallback costs as
well as warmed checks and whole construction; do not assume a timing win.

The motivation is concrete layout pressure: replacing twelve unused bytes with
an inline sixteen-byte array displaces 386 fields. A pointer is four bytes on
386 and eight on amd64. Keep existing boolean packing, measure rather than
assume the resulting size/allocation class, and retain every failed inline
placement. Review Go lifetime/escape behavior: Reader owns the array, borrowed
header slices never survive another accessor operation, and array allocation
is skipped for every fitting-header path. No unsafe conversion or shared global
scratch is permitted. Freeze this private prototype independently, preserving
the native opener, three cached-statistics returns and matched helper sources.
Repeat the original-layout opening failure before transferring it to the root.

The lazy representation builds but misses the allocation-class objective:
amd64 size/class is 616/640 and 386 is 512/576. Raw size alone does not determine
class; retain the measured result rather than assuming 512-byte allocation.
It introduces a first-fallback allocation without demonstrating the intended
class reduction. Do not transfer it to the root or claim performance success.

### Bounded Layout-Size Storage Prototype - 2026-10-15

Source evidence provides a smaller representation change while keeping the
approved inline array: the three private cached layout sizes are uint64, but
`configureLayout` assigns only constants 4, 8, 16, 64 and 72 from
`go/journal/format.go:16-23`. Repository search finds only those six constant
assignments and the three uint64-returning getters. No parsed file offset or
size is assigned to these caches and no outside-package caller can access them.

Prototype uint8 storage for those three caches, with explicit widening in
their existing uint64 getters. Constant assignments remain compile-time range
checked, so a future larger layout constant cannot silently truncate. Preserve
the inline sixteen-byte buffer, existing boolean packing, native opener and
cached-statistics returns. This changes only private storage and load widths;
format widths, offset arithmetic, APIs, validation and acquisition algorithms
stay unchanged. The same historical-format, compact/regular, cached-header and
retained-row gates apply. Measure actual classes on both architectures and
repeat the original matched-helper failure before transferring the prototype.
No new consumer decision is involved in representing proven bounded internal
constants; no unsafe conversion, dependency, additional state or fixture-helper
change is allowed. Retain all unsuccessful placement/representation evidence.

The narrower cache prototype achieves a 504-byte / 512-byte-class 386 Reader,
but its sixteen-pair original-helper repeat still flags a slowdown: ratio
1.04889 [1.01636,1.05987], medians 24,517.5 -> 25,528 ns, 1,696 -> 1,552 B/op
and 22 -> 17 allocations/op. Do not transfer it to the root. The root retains
the approved inline buffer, original cache widths, native opener and cached
constructor statistics. Preserve all private candidate sources and results.

### Bounded Measurement-Context Diagnosis - 2026-10-15

Two read-only source/harness investigations find no extra entry-array traversal
or timer/parser error explaining the remaining small-open regression. The
original helper validates exactly five ENTRY headers in both builds. Current
runtime work removes those five allocations, one duplicate statistics refresh
and four descriptor syscalls. These reductions do not establish end-to-end
timing acceptance. Earlier 386 disassembly shows an extra helper frame, but no
causal timing attribution is established.

The shared and original-helper measurements change both executable/helper
link context and fixture/setup context. The current complete benchmark helper
can select an immutable small corpus; otherwise it creates and closes a Writer
before resetting timing counters. That reset does not reset process allocator,
GC or kernel cache history. Their timing contribution remains a hypothesis.

Run a bounded discriminator without runtime changes or rebuilds: use the same
frozen native-open-stats-1 386 baseline/candidate new-helper binaries in both
generated and immutable shared-file contexts. Alternate context order and
baseline/candidate order; use two warmups plus sixteen measured pairs per
context at 1.5 seconds. Retain every process result, binary/fixture hashes and
separate within-context comparisons. Do not average contexts, erase earlier
failures, or treat an inconclusive result as equivalent performance.

The fixed-work private narrow-cache diagnostic completed four measured pairs
at 200,000 opens each, retaining wall/user/kernel CPU, page faults and allocation
counts in `sow-0108-native-open-sized-1-fixed-work-diagnostic`. Timings vary
substantially across individual pairs. Whole-process counters include untimed
fixture generation and cleanup; they cannot attribute the timed API difference
to a constructor stage. This diagnostic is not performance acceptance.

The same-executable discriminator completes with improvements in both contexts:
generated ratio 0.90502 [0.89126,0.92952], medians 26,721.5 -> 23,959 ns;
shared ratio 0.89099 [0.87765,0.90295], medians 23,559 -> 21,032 ns. Both remove
five allocations / 80 bytes. Retain all 72 process observations and checked
hashes in `sow-0108-native-open-stats-1-context-diagnostic`. Fixture generation
alone does not reproduce the timing reversal in this executable pair. The
original-helper failure is not superseded or excused by this result.

Proceed with the already-planned full, focused, race, 386, integrated,
compiler-floor and cross-platform functional validation of the frozen root
candidate, serially without benchmark overlap. This completes necessary
correctness evidence while the bounded read-only machine-code investigation
compares the separately linked helpers. Performance acceptance and external
review remain open; no SOW advances on this diagnostic result.

Next, prepare identical private constructor-stage benchmark helpers in new
copies of the original-helper frozen sources. Keep all runtime source and
compiler flags unchanged. After serial functional validation completes, measure
header-only open/close, accessor creation/close on an already-open descriptor,
ENTRY-array loading on an existing cached accessor and raw header parsing.
The array stage clears only entryOffsets before each load to retain its normal
allocation behavior. Validate counts before timing and retain allocations,
binary/source hashes and exact stage boundaries. These diagnostic timings do
not replace the original complete-open control or the full performance gate.

### Test-Cache Invocation Consequence - 2026-10-15

The approved direct Linux opener bypasses Go's internal fixture-input tracking.
The documented uncached validation rule must therefore also reach the existing
repository test entrypoints. Source search finds two shell test invocations:
`.github/workflows/release.yml:119` and
`tests/coverage/run_go_coverage.sh:19`. Both currently omit `-count=1`.

Add that flag to those two existing commands, preserving test selection,
coverage instrumentation, compiler floors, build caches and release behavior.
This is a necessary validation consequence of the approved opener, not an
additional SDK or release design choice. Validate shell syntax and workflow
parsing/command preservation; the complete functional run already executes the
same uncached Go command successfully. Do not install tools, run a release or
repeat the full suite merely to validate the invocation-only edit. Include the
two files in the scoped review patch and artifact ledger.

### Current Candidate Functional And Crossover Evidence - 2026-10-15

`sow-0108-native-open-stats-1-functional/manifest.json` records all eleven
serial steps passing without benchmark overlap:

| Gate | Evidence |
| --- | --- |
| Uncached complete pure-Go suite | 1,133 leaves; 106.475 s |
| Focused race instrumentation | 329 leaves; 220.688 s |
| 386 focused pure-Go suite | 321 leaves; 31.020 s |
| Preserved SDK workspace integration | 321 leaves; 24.238 s |
| Go 1.26.2 floor, amd64 and 386 | 143 leaves each; 15.221 / 14.840 s |
| Non-Linux compilation | Windows, Darwin and FreeBSD amd64, CGO disabled |
| Complete-reader helpers | Frozen before/candidate pure-Go executables built |

The CI/coverage invocation edits pass `bash -n` and workflow parsing; the Test
Go step retains its source checkout directory and exact test selection.

The four-binary crossover completes two warmups and sixteen measured pairs per
helper family, using the original private temporary directory, identical CPU,
benchtime and inherited process environment. Original-helper ratio is 0.90752
[0.89207,0.91455], medians 26,599 -> 23,954 ns. New-helper ratio is 0.89669
[0.89254,0.90511], medians 26,733.5 -> 24,116 ns. Both report 1,648 -> 1,568 B/op
and 22 -> 17 allocations/op. Hashes of all four executables are unchanged.
Retain all 72 observations in `sow-0108-native-open-stats-1-helper-crossover`.

Earlier original-helper runs report 1,696 -> 1,616 B/op. That absolute-byte
discrepancy is unexplained, so the crossover does not demonstrate an identical
allocation context or identify the cause of earlier timing flags. It does
establish that the old executable pair does not consistently reproduce them
under the controlled crossover. No failed evidence is discarded.

Actual four-binary disassembly finds the same SDK instruction shapes and
function addresses between helper families. The timed benchmark closures have
the same instructions and move by 256 bytes, preserving 64-byte alignment.
Relocated data references and fixture/setup state remain possible influences,
not established causes. The prepared constructor-stage helper is retained as
a diagnostic option; no additional runtime optimization is justified by this
investigation alone.

Run the already-planned current-candidate matrix serially: default-window
large-file Go/Rust/index/traversal correctness; native and 386 construction and
header controls; all eight raw-value and 36 compressed-value cases; sixteen
large aggregation cases with a valid non-evicting baseline; four unchanged
complete-reader controls. Retain allocation bytes/counts separately from
timing and stop at any failed command or flagged per-case regression. Earlier
full matrices remain algorithm evidence, not substitute current-build results.

### Correctness-Test Windows Empty-File Expectation - 2026-10-15

Read-only implementation inspection identifies a portability defect in the new
initial-statistics test. The existing Windows backend creates a file-mapping
handle in its constructor (`go/journal/reader_access_mmap_windows.go:35-48`,
`:145-153`). A zero-byte raw file therefore fails explicit Mmap construction;
Auto legitimately selects ReadAt with a fallback reason. The new test assumed
successful Mmap and no fallback for that file on every platform. Cross-
compilation checks syntax but cannot validate this runtime expectation.

Keep all backend/runtime behavior unchanged. Correct only the test: assert
explicit Mmap failure for a Windows zero-byte raw file; assert Auto's ReadAt
selection and nonempty reason there; retain success, exact initial counters,
fresh-snapshot equality and expected mode for all other cases. This follows
existing failure/fallback contracts and needs no new product choice. Freeze a
new correctness-only source snapshot preserving the prior snapshot and timing
executables. After timing completes, run the focused statistics controls on
native/386/compiler-floor targets and cross-compile Windows. No production or
matched timing source changes, so ongoing measurements remain applicable.

Microsoft's [file-mapping documentation](https://learn.microsoft.com/en-us/windows/win32/memory/creating-a-file-mapping-object)
confirms that mapping a zero-length file with zero maximum size fails with
ERROR_FILE_INVALID. The platform expectation follows that existing behavior;
Windows runtime execution has not been performed. The same inspection also
finds the compatibility skill's Explorer test example missing `-count=1`;
align that command with the already-approved uncached validation policy.
No additional runtime defect is verified by this ordinary implementation
inspection; it does not replace the whole-SOW review gate.

### Current Construction And Raw-Aggregation Matrix - 2026-10-15

Both complete construction/header matrices finish without a flagged case:
amd64 has 19 improvements and eight inconclusive results; 386 also has 19
improvements and eight inconclusive results. Each retains 27 cases, ten paired
measurements after two warmups, complete allocation metrics and checked input
hashes. Inconclusive timing is not classified as equivalent performance.
Current 100,000-row mmap opening ratios are 0.8886 regular / 0.8688 compact on
amd64 and 0.8823 / 0.8547 on 386. Measured Reader allocation classes remain
640 bytes on amd64 and 576 bytes on 386; private smaller-cache experiments are
not part of this candidate.

All eight raw aggregation cases improve, with paired median ratios ranging
0.6048 to 0.9532. Every case also lowers median allocation bytes and counts.
For the unfiltered low-cardinality histogram, allocation bytes fall
1,259,536 -> 109,488 and allocation count 3,694 -> 130 per query. This includes
the approved ownership repair and header acquisition together; it does not
attribute the complete gain solely to descriptor opening.

Evidence directories are `sow-0108-native-open-stats-1-header-performance`,
`sow-0108-native-open-stats-1-386-header-performance` and
`sow-0108-native-open-stats-1-raw-performance`. The compressed and large-reader
matrices are still running; no final performance or review acceptance is
claimed from these completed subsets.

### Complete Current-Candidate Performance Evidence - 2026-10-15

The serial `sow-0108-native-open-stats-1-performance-pipeline` completes all
seven stages. All six executable hashes and all driver hashes match their
starting manifests. A separate result check verifies 2,832 paired-process
records, two warmup pairs and ten measured pairs per case, plus sixteen Rust
oracle records for large aggregation. Every current-build process succeeds.

| Matrix | Cases | Improvement | Inconclusive | Regression flag |
| --- | ---: | ---: | ---: | ---: |
| amd64 construction and headers | 27 | 19 | 8 | 0 |
| 386 construction and headers | 27 | 19 | 8 | 0 |
| Raw indexed aggregation | 8 | 8 | 0 | 0 |
| Compressed indexed aggregation | 36 | 17 | 19 | 0 |
| Large indexed aggregation | 16 | 15 | 1 | 0 |
| Complete-reader controls | 4 | 1 | 3 | 0 |
| Total | 118 | 79 | 39 | 0 |

All 36 compressed cases lower median allocation bytes and allocation counts,
including the nineteen timing-inconclusive cases. Together with the eight raw
cases, all 44 measured aggregation allocation comparisons improve. Compressed
preflights preserve exact indexed counters, outputs, stream checks and all
eighteen immutable fixture hashes. The large matrix preserves Go/Rust output
and full Go counter agreement. Its before/candidate timing uses the recorded
256 MiB non-evicting window because the unchanged default-window baseline
crashes; it is not default-window before/after timing evidence. Sixteen separate
default-window large-file correctness commands pass for the repaired Go path
and Rust indexed/traversal references.

The complete-reader controls retain twenty loops / two million rows per SDK
entry process. Their final paired median candidate/baseline ratios are:

| Case | Ratio | Paired bootstrap 95% interval | Assessment |
| --- | ---: | --- | --- |
| Regular Explorer query | 0.96280 | [0.91827, 1.00127] | Inconclusive |
| Regular SDK entry | 1.00019 | [0.98393, 1.02891] | Inconclusive |
| Compact Explorer query | 0.95180 | [0.92072, 0.96964] | Improvement |
| Compact SDK entry | 0.99606 | [0.99402, 1.00752] | Inconclusive |

No tolerance or cross-case average is applied. These measurements do not prove
equivalence for any inconclusive case and do not erase earlier flags. The
unexplained historical timing and 48-byte allocation-context discrepancies
remain explicit limitations for whole-SOW review. The current matrix has no
reproduced flagged case requiring another runtime candidate. Raw observations,
allocation metrics, commands and checked manifests remain under ignored local
evidence; `sow-0108-native-open-stats-1-performance-verification.json` records
the independent count/hash/allocation checks.

### Correctness-Test Correction Validation - 2026-10-15

The platform-aware initial-statistics correction passes seven leaf tests each
on current Go amd64/386, Go 1.26.2 amd64/386 and the preserved integrated SDK
workspace. Windows amd64 cross-compilation also passes. No Windows runtime
execution is claimed. These six steps are recorded in
`sow-0108-native-open-stats-1-test-fix-validation/manifest.json`.

The corrected correctness-only v2 snapshot changes one test file; every runtime
source hash remains identical to the previously validated and timed candidate.
The prior full 1,133-leaf suite, focused race and other functional results remain
runtime evidence; the five focused runs above validate the final test edit.
The ordinary read-only implementation inspection found this test expectation
and one missing uncached-test flag in the compatibility skill. Both are fixed.
That inspection is not the required independent whole-SOW acceptance review.

### Diagnostic And Opportunity Disposition - 2026-10-15

- The prepared constructor-stage helper was not built or run. Further stage
  measurements are rejected as required repair work: the controlled crossover
  and complete current-candidate matrix do not reproduce the old flag, and
  another isolated stage experiment would not establish its historical cause.
  Retain the helper and all unsuccessful results without a delivery promise.
- Read-only accessor analysis finds that `touchWindow` recomputes unchanged
  statistics on an ordinary cache hit unless it newly pins a window. The clock
  and last-used updates still affect eviction. No correctness failure or timing
  significance is established. A new shared-accessor optimization is rejected
  from this repair and its required follow-up scope: it is unnecessary to the
  approved ownership/header/opener outcome and expands the blast radius to all
  cache hits. No runtime change or performance claim follows from this finding.
  Existing public `AccessStats()` tests refresh the snapshot and would mask
  stale cached fields; this limits what those tests could prove for such a
  separate optimization.
- Recovery interaction checks remain part of paused SOW-0159; CLI v262/PCRE2
  work remains in paused SOW-0121. The independently reproduced explicit-refresh
  positioning regression remains in reopened SOW-0045. Unchanged XZ dictionary
  allocation remains tracked by pending SOW-0160. None is silently accepted or
  implemented in this prerequisite.

### Final Local Artifact Gate Checkpoint - 2026-10-15

AGENTS.md and the runtime-purity architecture remain unchanged. The compatibility
skill, product-scope spec and benchmark guide now cover the approved Linux
opener, fixed header storage, ownership checks, measured allocation classes,
initial statistics, cross-platform boundaries and uncached test invocations.
The release workflow and coverage runner add only `-count=1` to their existing
Go test commands. Their syntax/command-preservation checks pass; no release,
installation or host configuration operation was performed.

Published SDK usage and API contracts are unchanged by this prerequisite, so
the paused SDK wiki edits are excluded. No output/operator skill exists to
update. Tests use synthetic identities and values; raw fixtures and diagnostic
artifacts remain ignored. No new dependency, compiler minimum or external
journal-library linkage is introduced. Canonical/root status indexes must show
this SOW in-progress, local measurements complete and review disposition open.
The expanded changed-artifact audit reports `SOW initialization complete and
clean` in `sow-0108-native-open-stats-1-final-local-audit.log`, including 158 SOW
records and 99 scanned durable artifacts. `git diff --check` passes. The scoped
sixteen-file code/artifact v3 patch applies cleanly to HEAD using a separate
scratch index; the actual staging index is untouched. Ordinary same-failure
analysis covers both aggregation callers, nested posting/ENTRY reads,
constructor statistics returns, native/crossing header callers and native-file
cleanup, including the broader backend/layout/codec/error tests above.

Two independent internal final reviews now inspect the whole repair, with
runtime and measurement emphasis respectively. Their verdicts and any required
dispositions remain open. External review is still unauthorized and unwaived;
request that decision after the local review has no unresolved required fix.
No claim of completed SOW, final performance equivalence or publication follows
from this local checkpoint.

### Independent Internal Final Review And Disposition - 2026-10-15

Both read-only reviewers assess the complete approved repair and its direct
and indirect consequences. Both return PASS with no unresolved P0-P2 defect.
Their verdicts are internal technical assessments; neither authorizes external
review, waives that gate, establishes timing equivalence or completes this SOW.

Reviewed HEAD is `dafcdac4b971d04e48768a1f831974f9252061e2`. Both reconstruct
the sixteen-file scoped v3 patch against HEAD and verify its Go files against
the frozen correctness-v2 source. The patch SHA256 is
`f9c98fbbe2e4118037ab2a3297ab911f73504ac7bf5976ddfdd38844a39d726d`.
All 89 frozen runtime-file hashes match. The preserved paused SDK changes are
excluded. Neither reviewer builds, tests, profiles, benchmarks or writes files.

The runtime review verifies aggregation ownership/reacquisition, immediate
header consumption, the fixed-buffer lifetime, constructor statistics,
descriptor/error semantics, platform delegation and uncached test entrypoints.
It independently recounts all retained functional events: 1,133 full-suite
leaves, 329 race leaves, 321 each for 386 and integration, 143 per compiler-floor
target and seven per corrected-statistics execution target. No failure events
are found.

The measurement review verifies all 2,848 retained process outputs: 2,832 paired
baseline/candidate records and sixteen Rust oracle records. It independently
recomputes all 118 paired medians, intervals, classifications, phase medians and
variation values. Across all 1,176 benchmark allocation pairs, including
warmups, neither bytes/op nor allocations/op increases. Large and complete-
reader CLI matrices do not expose allocation counts; RSS observations do not
establish allocation equivalence.

One verified P2 evidence-preservation finding is resolved. The pinned drivers
imported an untracked comparison module owned by paused SOW-0159. A later edit
there could prevent reproducing calculations from the retained package alone.
The resolution preserves that module and the unchanged HEAD reader-benchmark
module byte-for-byte in `sow-0108-native-open-stats-1-calculation-provenance`,
with hashes and tracked status. The parent independently replays all 118
recorded ratios, intervals and classifications using the captured source; all
match. The measurement reviewer
verifies the captured bytes and replay coverage. No runtime change, benchmark
rerun or transfer of paused SDK implementation scope occurs. The manifest
explicitly labels these hashes post-measurement; no historical starting hash is
asserted.

The comparison module SHA256 is
`4b6b93f8cabf08923ed4438e26dd01ba583539ebff5642a92a6e53639ccc9b1e`;
the calculation-provenance manifest SHA256 is
`0e45946735c6368758d124e98821e6fdba7ea68b240b8588c3156e1a0d0f1703`.
This resolves reproducibility without erasing the original finding.

Both reviews retain these limits: 39 timing-inconclusive cases are uncertain;
historical timing and 48-byte allocation-context differences remain unexplained;
large before/after timing needs the valid non-evicting baseline; and non-Linux
execution was not performed. Codec complete-counter checks occur in preflights
and calibration, while timed iterations check update totals. Raw timing helpers
also have narrower assertions than the correctness suites. No broader assertion
about every timed iteration or platform follows from their PASS verdicts.

The separate `touchWindow` optimization is not required for this approved
outcome. Deferred implementation destinations remain SOW-0159, SOW-0121,
SOW-0045 and SOW-0160 as mapped above; no new untracked delivery promise is
created. Final record-only changes and evidence preservation do not alter the
reviewed runtime/test patch. Repeat the expanded audit and whitespace check,
pin the final handoff records, and request external-review authorization or an
explicit user waiver. The SOW remains in-progress/current until that gate and
the required combined implementation/lifecycle commit are complete.

The post-review expanded audit reports `SOW initialization complete and clean`
in `sow-0108-native-open-stats-1-post-review-audit.log`; the whitespace check
passes. A final audit after this result-only record and an independent impact
check of the final ledger changes are captured with the handoff evidence.
External-review authorization or waiver is the remaining user gate; neither
internal PASS changes the SOW status or authorizes starting the paused SOWs.

### External Review Authorization - 2026-10-16

The user selected option A: authorize read-only external review for SOW-0108
now, then SOW-0159 and SOW-0121 after each complete local validation milestone.
Standing authorization includes further rounds after material fixes within
those three SOWs. Run the six reviewers named in the approved question through
the system-wide external-reviewers skill. Do not expand implementation scope,
relax the performance contract or activate multiple SOWs concurrently.

The reviewed runtime/test patch and measurements remain unchanged. Record this
authorization and refresh handoff hashes before invocation; display the full
neutral prompt and review the complete regression repair with its direct and
indirect consequences. SDK and CLI work remain paused until this prerequisite
passes review, audit and the combined implementation/lifecycle commit.

### External Whole-Repair Review And Adjudication - 2026-10-16

The user-authorized six-reviewer round uses the same complete approved repair
prompt and frozen sixteen-file v3 patch for every reviewer. The runtime/test
patch SHA256 remains
`f9c98fbbe2e4118037ab2a3297ab911f73504ac7bf5976ddfdd38844a39d726d`.
The full prompt SHA256 is
`c8b8f130ecee9014a036f2629918dfc455bb9c52b6feb50e2b34f0745b5e183e`.
Review inputs and each original report, invocation result and adjudication are
retained privately with the regression evidence. No SDK implementation is
accepted by this prerequisite's external review.

- GLM: PRODUCTION GRADE, no in-scope finding. Its reconstruction checks all
  sixteen scoped files, 89 frozen runtime hashes, functional records, the
  118-case calculations and allocation evidence. Existing SOW-0045/SOW-0160
  issues remain separately owned.
- Minimax: PRODUCTION GRADE with three P3 observations. Its claimed unnecessary
  audit exception is disproven using the exact Perl matcher: the trailing
  tilde creates a word boundary after the archive filename's `.journal`
  portion, which still matches the email-like pattern. Retain the narrow
  all-zero quoted filename exception. Its timing/platform observations are
  existing limitations. A hypothetical cross-call retention test for a private
  synchronously consumed header slice is not required by the approved
  contract. References to recovery internals concern paused SOW-0159; they
  are not public SDK acceptance evidence. Frozen sources match their manifest;
  the root working tree also preserves deliberately excluded paused SDK edits.
- Deepseek: PRODUCTION GRADE with three P3 observations. Constructor snapshot
  freshness is already proved by source and tests; no stale return is shown.
  Byte offsets in mutation fixtures intentionally target the journal layout
  independently of parser implementation. Dense full-value assertions reside
  in retained preflights and Go/Rust comparisons rather than every timed
  iteration; no failed dense behavior or broken approved gate is shown. Extra
  comments, unrelated offset refactoring and duplicate testing are rejected
  from required repair scope. Its comparison-module observation is resolved
  by the retained source capture and complete replay described above.
- Mimo: original verdict NEEDS CHANGES with one P2, rejected as a false
  positive. Frozen `go/journal/explorer.go:1215` begins the facet loop;
  `var count, visited uint64` at :1223 is inside it. The counter is flushed
  at :1273 before an error returns at :1274-1275. Therefore prior facets are
  not added repeatedly. An independent read-only check verifies this scope
  and the reproducer's runtime hashes. Eighteen executed cases cover one and
  three rows, two facets and Traversal/Index/Compare on current amd64, current
  386 and minimum-compiler amd64. All return exactly two or six DATA refs.
  Valid indexed queries explicitly select AllValues; the default FirstValue
  query in the report is rejected by the existing contract. Healthy cases
  were executed; partial-error scope is established by source and existing
  interrupted-posting tests, without claiming a new multi-facet fault-injection
  execution. No production correction or material code change follows.
- Qwen: no technical verdict. The invocation and its one permitted retry both
  fail with HTTP 429. Preserve both failures and the resulting coverage gap;
  no substitute or third attempt is made.
- Kimi: no technical verdict. The first attempt and its one permitted
  continuation reach their supervised 1,800-second limits (recorded durations
  1,800.457 and 1,801.112 seconds). Neither returns a complete review block.
  Preserve both attempts and the resulting coverage gap; no third attempt
  or substituted verdict is made.

The separate runtime-purity guard observation belongs to original SOW-0071:
its completed criteria explicitly promise rejection of subprocess APIs, but
its literal patterns miss standard Go variants. Static matching proves the
misses without executing any example. This is not evidence that current core
SDK code launches a process. Reopen the original SOW for paused tracking,
preserving its original narrative and the active 0108 -> 0159 -> 0121 order;
no scanner implementation, new prerequisite or additional runtime scope is
introduced. SOW-0045 and SOW-0160 remain the other separately tracked findings.

The current matrix has 79 improvements, 39 inconclusive results and no flagged
slowdown across 118 cases. Inconclusive intervals do not establish equivalence.
All 1,176 benchmark allocation pairs avoid an increase; CLI matrices do not
expose allocation counts. Historical timing and absolute-byte discrepancies,
the non-evicting large-baseline requirement and non-Linux runtime coverage
limits remain unchanged and are not erased by reviewer verdicts.

The master adjudication finds no unresolved blocking repair finding. An
independent read-only check also supports the received-report dispositions
and preserved verdicts. The round is complete with the two explicit model
availability gaps; those gaps are not positive verdicts. The runtime/test
patch remains unchanged. Finish the artifact/lifecycle audit and exact-revision
impact check before the required combined signed commit.

### Final Regression Artifact And Follow-Up Gate - 2026-10-16

The final runtime/test patch is unchanged by review disposition and tracking
updates. The accepted repair restores the existing ownership guarantee for
Go indexed facets and histograms, uses the approved bounded hybrid header
acquisition, and retains the approved native-reader opener and constructor
snapshot optimization. Public SDK/CLI behavior changes remain outside this
prerequisite. The preceding validation sections record the exact functional,
platform, performance and review evidence and their limits.

- Same-failure search: both aggregation callers, singleton and multi-posting
  paths, nested posting/ENTRY reads, partial errors and counter flushes,
  compressed/raw value ownership, synchronous header consumers, constructor
  returns and descriptor cleanup are covered by source inspection and the
  retained focused/large/integrated checks. Mimo's additional multi-facet
  allegation is disproven independently by source and 18 executed cases.
- AGENTS.md: no workflow or responsibility change; its existing ownership,
  sequential implementation, review and pure SDK contracts continue to apply.
- Runtime project skills: the scoped compatibility changes document forced
  invalidation, allocation classes, matched timers/helpers, hybrid acquisition,
  native opening, cached initial statistics and uncached Go checks.
- Specs: the scoped product-scope changes document the restored internal
  lifetime and header-use guarantees. Paused SDK behavior/spec edits remain
  excluded from this repair.
- End-user/operator docs: the benchmark guide records the actual methods and
  limits. Published SDK API and CLI usage do not change in this prerequisite;
  their paused wiki edits remain excluded.
- End-user/operator skills: none exist or are affected by this repair.
- Sensitive-data handling: tests use synthetic values and identities. Raw
  journals, logs, profiles, binary/source manifests and reviewer reports remain
  ignored; durable records contain relative pointers and sanitized aggregates.
- SOW lifecycle/status indexes: close this original reopened SOW with its
  implementation in one commit. Preserve paused SOW-0159/SOW-0121 and the
  0108 -> 0159 -> 0121 sequence. Original SOW-0045 remains paused for healthy
  EOF/refresh positioning; original SOW-0071 is reopened for paused guard-gap
  tracking; pending SOW-0160 owns unchanged XZ dictionary allocation. None adds
  an execution prerequisite or runtime implementation here. SOW-0122 keeps its
  separate CLI scalability scope and references the current CLI SOW location.
- Lessons: invalid default-window results cannot certify timing; native and
  crossing header access need separate controls; allocation-class effects and
  instruction/syscall evidence do not establish latency causation; retain
  conflicting measurements and uncertainty. The compatibility skill and
  benchmark guide hold the resulting procedures.
- Follow-up scan: historical pending design/measurement notes above are
  superseded by the recorded decisions and completed checks. Remaining SDK
  recovery interactions are owned by SOW-0159, CLI v262/PCRE2 work by SOW-0121,
  positioning by SOW-0045, guard enforcement by SOW-0071, and XZ allocation by
  SOW-0160. Additional constructor-stage diagnosis, a shared touchWindow
  optimization and the non-blocking duplicate/comment/refactoring suggestions
  are explicitly rejected as required work for this approved outcome. No
  untracked implementation promise remains.

The post-external tracking audit is complete and clean, scanning 100 durable
artifacts with no sensitive-data patterns. Whitespace checks pass. Repeat the
full audit after this final lifecycle move, then independently check the exact
closing revision and commit the implementation/lifecycle together. Completion
is not reported to the user before that combined signed commit succeeds.

The post-move lifecycle audit reports `SOW initialization complete and clean`,
including 100 scanned durable artifacts with no sensitive-data patterns.
Whitespace checks pass. The final closing revision is pinned for an independent
impact check before the combined signed commit. Runtime/test bytes remain the
reviewed v3 patch; preserve the paused SDK working changes throughout staging.
