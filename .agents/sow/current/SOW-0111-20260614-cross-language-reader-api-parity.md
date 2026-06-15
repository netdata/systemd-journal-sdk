# SOW-0111 - Cross-Language Reader API And Memory-Access Parity

## Status

Status: in-progress

Sub-state: activated for implementation after recording user decisions for
Go/Python read-at policy and Node.js mmap packaging direction; local
implementation chunk validated, pending whole-SOW reviewer pass before close.

## Requirements

### Purpose

Make Rust the reader source of truth and bring Go, Python, and Node.js into a
clear, documented, test-proven parity contract for reader APIs, memory-access
behavior, row-level payload lifetime, and hot-path performance expectations.

This work exists because the SDK is a performance-critical journal backend for
Netdata and other consumers. A correct but confusing or slower public API is not
acceptable when it can steer users onto non-optimal paths.

### User Request

Create an SOW for language parity with these requirements:

- Rust is the source of truth.
- Rust currently has a whole-file mmap mode; it should be hidden from the
  public API somehow so users never get confused and use it.
- Go can fall back to / read via `ReadAt`; investigate whether this is really
  needed. If it is not needed, remove it or hide it from the public API.
- Go visitor API has weaker lifetime; keep it only if all documentation makes
  clear that it is callback-scoped and does not provide row-level guarantees.
- Python has rolling mmap plus read-at fallback; investigate whether read-at is
  needed. If it is not needed, remove it or hide it from the public API.
- Python can explicitly use read-at; treat this with the same policy as the
  previous point.
- Node.js should use `mmap-io` and abstract reader access so consumers can use
  the library with and without mmap.
- No production reader or verification path may whole-map a journal file or load
  an entire journal file into memory.

### Assistant Understanding

Facts:

- Rust default reader options use rolling-window mmap, but the public-facing
  Rust reader options still expose an experimental whole-file mmap strategy.
- Go supports mmap on Unix and Windows but also exposes `ReaderAccessReadAt` and
  can silently fall back to read-at when access mode is auto.
- Go's `EnumerateEntryPayload` documents row-level lifetime, but
  `VisitEntryPayloads` documents visitor-call lifetime only.
- Python supports rolling mmap and read-at fallback, and exposes read-at through
  `ReaderOptions`.
- Python also exposes `visit_entry_payloads`; today it passes owned `bytes`
  produced by `_read_data_payload_at`, so retaining callback payloads is safe
  but it is not the zero-copy row-level hot path.
- Python exposes row-level `enumerate_entry_payload`, backed by row-lifetime
  access through `_read_data_payload_row`.
- Node.js currently has no mmap backend and uses bounded positioned-read
  windows; project docs explicitly state this.
- Node.js exposes `visitEntryPayloads` and `enumerateEntryPayload`; both call
  `_readDataPayloadAt` with row-lifetime mode by default. This provides bounded
  row-lifetime behavior over its current read-at windows, but it still lacks an
  mmap backend.
- Node.js public TypeScript declarations expose `"mmap"` as an allowed
  `ReaderOptions.accessMode` even though runtime `openReaderAccessor` rejects
  explicit mmap with `UnsupportedAccessModeError`.
- SOW-0108 completed bounded reader access and bounded verification, but this
  SOW is stricter: it is about cross-language API parity and preventing public
  consumers from selecting confusing or weaker access modes.
- Netdata Explorer does not use Go `VisitEntryPayloads`; it uses Explorer
  DATA-offset traversal for scan-time facets/histograms and owned `GetEntry`
  payload expansion for returned rows. Therefore the weaker Go visitor lifetime
  is not a Netdata Explorer blocker, but it must be documented as callback
  scoped everywhere it is exposed.

Inferences:

- Rust whole-file mmap was useful for experiments and benchmarks, but it is not
  appropriate as a normal public consumer option under the current performance
  contract.
- Go and Python read-at paths may remain useful internally for tests, exotic
  platforms, or explicit diagnostics, but public production API exposure needs
  evidence and a user decision.
- Node.js mmap support through `mmap-io` is a dependency and portability change,
  so it needs dependency review, fallback semantics, and platform validation.

Unknowns:

- Whether Go needs any production read-at fallback after confirming mmap works
  on Linux, FreeBSD, macOS, and Windows in the supported Go runtime matrix.
- Whether Python needs any production read-at fallback after confirming `mmap`
  works on Linux, FreeBSD, macOS, and Windows in the supported Python runtime
  matrix.
- Whether `mmap-io` satisfies the project's dependency, portability, native-code,
  packaging, and security constraints for Node.js, or whether it requires an
  optional dependency boundary.
- Whether Python `visit_entry_payloads` should stay as an owned-copy visitor,
  gain clearer documentation, or be redirected to row-level semantics where
  practical.
- Whether Node.js `visitEntryPayloads` should continue to return row-lifetime
  buffers by default once mmap exists, and how to document retention safety for
  Buffer-backed mmap/read-at windows.

### Acceptance Criteria

- Rust public consumer APIs no longer expose a normal whole-file mmap reader
  mode. Any whole-file mmap capability is removed, internal-only, test-only, or
  otherwise unavailable to normal SDK consumers, with docs/tests proving the
  intended boundary.
- Go public reader APIs match Rust's source-of-truth behavior for supported
  platforms: mmap-backed rolling windows by default, row-level pointer lifetime
  for all returned DATA payload surfaces, and no confusing public read-at path
  unless explicitly accepted by the user with evidence.
- Go `VisitEntryPayloads` remains explicitly callback-scoped if retained.
  Documentation must not describe it as row-level-safe, and row-level consumers
  must be directed to `EnumerateEntryPayload` or owned-copy helpers.
- Python public reader APIs follow the decided policy for read-at fallback and
  explicit read-at options; if retained, the API must make clear that it is not
  the Rust/Go performance path and cannot be selected accidentally.
- Python visitor and row-level payload APIs are documented distinctly:
  `visit_entry_payloads` currently returns owned bytes and is safe to retain but
  copies, while `enumerate_entry_payload` is the row-level API.
- Node.js default package keeps the current pure positioned-read backend and
  does not silently gain a native mmap runtime dependency. Native mmap support
  is split behind an optional package or equivalent explicit opt-in boundary,
  tracked as follow-up work before this SOW closes.
- Node.js TypeScript declarations and runtime behavior agree in the default
  package: public `mmap` selection is not exposed as an available mode unless it
  works through the optional backend boundary.
- Reader and verification tests prove no production code path maps a whole
  journal file or reads a whole journal file into memory.
- Cross-language parity tests cover file reader, directory reader, facade DATA
  enumeration, unique values, Explorer/Netdata paths where applicable,
  verification, `.journal.zst` temp-file behavior, small-window forced eviction,
  row-lifetime retention, and platform-specific mmap support.
- Benchmarks show the mmap hot path is not regressed in Rust, Go, or Python, and
  quantify Node.js positioned-read behavior after default-package API cleanup.

## Analysis

Sources checked:

- Python standard-library `mmap` documentation, Python 3.14.6:
  `https://docs.python.org/3/library/mmap.html`
- `rust/src/journal/src/lib.rs`
- `rust/src/crates/journal-core/src/file/mmap.rs`
- `rust/src/crates/journal-core/src/file/row_view.rs`
- `go/journal/reader.go`
- `go/journal/reader_access.go`
- `go/journal/reader_access_mmap_unix.go`
- `go/journal/reader_access_mmap_windows.go`
- `go/journal/reader_access_mmap_unsupported.go`
- `go/journal/reader_entry.go`
- `python/journal/reader_access.py`
- `python/journal/reader.py`
- `node/src/lib/reader-access.js`
- `node/src/lib/reader.js`
- `node/README.md`
- `.agents/sow/specs/product-scope.md`

Pre-implementation state reviewed:

- Rust defaults to `ExperimentalMmapStrategy::Windowed` through
  `ReaderOptions::default`, but `ReaderOptions::with_mmap_strategy` can still
  accept `ExperimentalMmapStrategy::WholeFile`.
- Rust `WindowManager` supports row-pinned rolling mmap windows and row-scoped
  overflow storage when hostile current-row access would exceed the pin budget.
- Go `ReaderOptions` exposes `ReaderAccessReadAt`, `ReaderAccessMmap`, and
  `ReaderAccessAuto`; auto can fall back to read-at if mmap fails.
- Go `EnumerateEntryPayload` documents row-level lifetime, while
  `VisitEntryPayloads` documents that consumers must not retain slices after
  the visitor returns.
- Python `ReaderOptions` defaults to auto, tries mmap, can fall back to read-at,
  and exposes explicit read-at / pread aliases.
- Python `visit_entry_payloads` invalidates entry DATA state, reads every
  current-row DATA payload through `_read_data_payload_at`, and passes owned
  bytes to the callback. `enumerate_entry_payload` uses `_read_data_payload_row`
  for row-lifetime access.
- Node.js currently rejects explicit mmap and records that auto selected the
  read-at backend because Node core has no portable mmap API.
- Node.js `visitEntryPayloads` and `enumerateEntryPayload` both use
  `_readDataPayloadAt` with the default row-lifetime mode, but this is backed
  by read-at windows today. TypeScript declarations still list `"mmap"` as a
  valid `ReaderOptions.accessMode`.

Risks:

- A public whole-file mmap option can be accidentally selected by a consumer and
  destroy memory-footprint expectations on large files.
- A silent read-at fallback can make production deployments look correct while
  losing the mmap performance characteristics expected from Rust/Go parity.
- A weaker Go visitor lifetime contract can cause consumers to retain invalid
  slices if they assume row-level behavior. This is a documentation/API clarity
  risk, not a Netdata Explorer blocker, because Netdata Explorer does not use
  the weak visitor path.
- Python's visitor is safe to retain today because it copies, but that makes it
  a different cost model from Rust/Go hot-path visitors. Documentation and
  benchmarks must not present it as equivalent without the copy cost.
- Node.js currently promises/selects a public mmap mode in declarations but
  rejects it at runtime. This is a public API parity and documentation problem
  even before mmap support is implemented.
- Adding Node.js `mmap-io` introduces a native dependency surface. If this is
  not optional, packaged, and tested correctly, it can violate the project's
  no-native-runtime-addon expectation for default Node.js consumers.
- Removing or hiding fallback APIs can break consumers that have already adopted
  them since the SDK is published. Because the SDK is still new, this is the
  right time to make the contract strict, but it still requires a versioning and
  documentation decision.

## Pre-Implementation Gate

Status: ready for activation

Problem / root-cause model:

- Reader memory-access options drifted from the intended Rust-source-of-truth
  contract. SOW-0108 correctly bounded memory access, but it did not fully
  enforce identical public API semantics or prevent users from selecting
  non-reference access modes.
- The main root cause is that implementation portability and testability added
  fallback/access-mode knobs that are now visible to consumers. The current
  product goal is stricter: optimal mmap-based reader behavior should be the
  obvious and default path, and weaker paths should be hidden, removed, or very
  explicitly opt-in.

Evidence reviewed:

- Rust exposes `ReaderOptions::with_mmap_strategy` and
  `ExperimentalMmapStrategy::WholeFile`.
- Go exposes `ReaderAccessReadAt` and documents a weaker `VisitEntryPayloads`
  lifetime.
- Python exposes read-at aliases and fallback, and its visitor currently passes
  owned bytes while row-level enumeration uses row lifetime access.
- Node.js documents that it has no mmap backend and uses read-at windows; its
  public types expose mmap while runtime rejects explicit mmap.
- SOW-0108 closed bounded access, not complete public API parity.

Affected contracts and surfaces:

- Rust `ReaderOptions`, facade options, internal benchmark/test commands, docs,
  and verified examples.
- Go `ReaderOptions`, `WithAccessMode`, `WithMmap`, facade wrappers,
  `VisitEntryPayloads`, `EnumerateEntryPayload`, docs, tests, benchmarks, and
  Netdata wrapper paths.
- Python `ReaderOptions`, facade payload enumeration, Explorer/Netdata reader
  paths, docs, tests, and benchmark command options.
- Node.js `reader-access.js`, TypeScript declarations, `FileReader`, facade,
  Explorer/Netdata paths, package dependency metadata, docs, and tests.
- Shared specs, consumer docs, SOW status, and release notes.

Existing patterns to reuse:

- Rust `CurrentRowView` and `WindowManager` row-pinned window semantics.
- Go `rollingReaderAccessor` bounded window and row arena architecture.
- Python `_MmapAccessor` / `_ReadAtAccessor` abstraction from SOW-0108.
- Node.js `ReadAtAccessor` bounded window and row arena abstraction from
  SOW-0108.
- Shared verify matrix, reader facade tests, Explorer/Netdata comparator tests,
  and benchmark tools.

Risk and blast radius:

- High API risk: public types and options may change in Rust, Go, Python, and
  Node.js.
- High performance risk: reader hot paths are performance-critical and must be
  benchmarked before and after.
- Medium compatibility risk: existing early consumers may use access-mode knobs.
- Medium packaging risk: Node.js `mmap-io` may introduce platform install
  failures or native runtime dependency concerns.
- Low data-loss risk: reader-only behavior, but verifier and facade paths must
  remain correct for corrupt and hostile files.

Sensitive data handling plan:

- This work uses synthetic fixtures and repository-local benchmark inputs.
- Do not record real journal payloads, host paths, usernames, tokens, cookies,
  SNMP communities, customer data, or private endpoints in durable artifacts.
- Benchmark reports committed to the repository must contain sanitized aggregate
  metrics only.

Implementation plan:

1. Rust: hide or remove the public whole-file mmap option from consumer-facing
   APIs, preserve only internal/test/benchmark access if explicitly justified,
   and add tests/docs that prove normal consumers cannot select it.
2. Go: keep read-at for tests, diagnostics, constrained-platform investigation,
   and controlled fallback evidence, but make it prominent that it is not a
   production reader mode; keep `VisitEntryPayloads` only if docs and tests
   clearly distinguish its callback-scoped lifetime from row-level
   `EnumerateEntryPayload`.
3. Python: keep read-at for tests, diagnostics, constrained-platform
   investigation, and controlled fallback evidence, hide it from the top-level
   package where practical, and align docs/tests with the decided public
   contract, including explicit visitor-vs-row-level payload lifetime and
   copy-cost documentation.
4. Node.js: keep the default package pure and positioned-read based, remove or
   hide unsupported public `mmap` selection from default-package types/docs, and
   create a separate pending SOW for optional native mmap package/API support.
5. Add cross-language no-whole-file memory-access guards and performance
   benchmarks for mmap paths.
6. Run local validation, then the full reviewer pool until production-grade.

Validation plan:

- Static searches proving no production reader/verify path uses whole-file
  `read`, `ReadFile`, `ReadAll`, `readFileSync`, whole-file mmap, or equivalent.
- Rust tests for default/windowed mmap and absence of public whole-file consumer
  selection.
- Go tests on Linux plus target checks for Windows, macOS, and FreeBSD; native
  macOS/Windows smoke where available.
- Python tests for mmap-selected defaults and explicit non-mmap policy behavior.
- Node.js tests for default-package access-mode agreement, positioned-read row
  lifetime, small windows, forced eviction, and unsupported mmap selection
  handling.
- Shared facade tests proving row-level DATA payload retention after field
  enumeration and until next row.
- Shared verify matrix and `.journal.zst` tests.
- Reader benchmarks before/after for Rust and Go; bounded comparative
  benchmarks for Python and Node.js.
- `git diff --check`, `.agents/sow/audit.sh`, docs validators, and relevant
  language test suites.
- External reviewers: glm, kimi, mimo, qwen, minimax, and deepseek, excluding
  any model used as implementer if delegation is enabled.

Artifact impact plan:

- AGENTS.md: likely update if the public reader access contract becomes a
  project-wide guardrail.
- Runtime project skills: likely update `project-journal-compatibility` if
  reader access/lifetime rules need to be enforced in future work.
- Specs: update `.agents/sow/specs/product-scope.md` with the final parity
  contract.
- End-user/operator docs: update Rust, Go, Python, and Node.js docs plus wiki
  pages affected by public reader options.
- End-user/operator skills: no output/reference skills are currently expected;
  verify at close.
- SOW lifecycle: keep this SOW pending until decisions are resolved; activate
  only one implementation SOW at a time.
- SOW-status.md: update both canonical and root convenience ledgers when status
  changes.

Open-source reference evidence:

- Not checked during SOW creation. Implementation should research official Go,
  Python, Node.js, and `mmap-io` documentation, and inspect `mmap-io` package
  source/version metadata before deciding the Node.js path.

Open decisions:

1. Go read-at policy:
   - A. Remove public read-at access from production APIs if mmap is available
     on all supported Go platforms.
   - B. Keep read-at internal/test-only and hide it from consumer docs/types
     where practical.
   - C. Keep explicit read-at public, but document it as diagnostic/non-optimal.
2. Python read-at policy:
   - A. Remove public read-at access from production APIs if mmap is available
     on all supported Python platforms.
   - B. Keep read-at internal/test-only and hide it from consumer docs/types
     where practical.
   - C. Keep explicit read-at public, but document it as diagnostic/non-optimal.
3. Node.js mmap dependency policy:
   - A. Make `mmap-io` the default backend when installed/supported, with
     positioned-read fallback explicitly available.
   - B. Make `mmap-io` an optional opt-in backend and keep positioned-read as
     the default.
   - C. Split Node.js mmap support into an optional package if the dependency
     conflicts with the pure default package contract.

Resolved decisions:

1. Go read-at policy:
   - User decision: B. Keep read-at internal/test-only and hide it from
     consumer-facing production docs and normal public guidance where practical.
   - Additional requirement: make it prominent that read-at mode is not for
     production use.
   - Implication: Go should keep read-at available for tests, diagnostics, and
     platform fallback investigation, but production examples and docs must steer
     users to mmap-backed rolling access.
   - Risk: if a platform-specific mmap failure exists, implementation must record
     it and ask before exposing read-at as a production fallback.

2. Python read-at policy:
   - User decision: B. Keep read-at internal/test-only and hide it from
     consumer-facing production docs and normal public guidance where practical.
   - Additional requirement: make it prominent that read-at mode is not for
     production use.
   - User expectation: Python should support mmap on all supported production
     platforms; implementation must verify this rather than assume it.
   - Implication: Python should keep read-at available for tests, diagnostics,
     and fallback investigation, but production examples and docs must steer
     users to mmap-backed rolling access.
   - Risk: if Python mmap is unavailable on a supported platform, implementation
     must record evidence and ask before exposing read-at as a production path.

3. Node.js mmap dependency policy:
   - User decision: C, interpreted as a split optional mmap package/SOW if that
     is the route that lets consumers choose while the SDK supports both
     positioned-read and mmap options.
   - Required product intent: consumers of the API should be able to decide which
     backend they use, and the project should support both choices.
   - Implication: the default Node.js SDK package should keep the current pure
     positioned-read backend and should not silently gain a native mmap runtime
     dependency. Native mmap support should be designed as an optional package or
     equivalent explicit opt-in boundary, with API shape, dependency review,
     platform support, and packaging handled separately.
   - Risk: Node.js public typings must not promise mmap support in the default
     package until that optional boundary exists and is validated.

## Implications And Decisions

User decision on 2026-06-14: keep Go `VisitEntryPayloads`, but make all
documentation explain that it is callback-scoped and does not provide
row-level guarantees. Row-level consumers must use `EnumerateEntryPayload` or
owned-copy helpers.

User decisions on 2026-06-15:

1. Go read-at stays internal/test-only where practical. Production docs must make
   it prominent that read-at is not for production use.
2. Python read-at follows the same policy. Implementation must verify Python mmap
   support on supported production platforms.
3. Node.js should support consumer choice for positioned-read and mmap backends,
   but the default package must not silently take a native mmap dependency.
   Native mmap support should be split behind an optional package or equivalent
   explicit opt-in boundary, with the exact package/API design validated before
   implementation.

## Plan

1. Complete evidence pass and decision memo for Go/Python read-at and Node.js
   mmap dependency policy.
2. Record user decisions in this SOW before implementation.
3. Implement Rust public whole-file mmap hiding/removal.
4. Implement Go parity and visitor lifetime documentation/API clarity fixes.
5. Implement Python access-mode parity.
6. Clean up Node.js default-package access-mode declarations/runtime/docs and
   create follow-up tracking for optional native mmap support.
7. Add tests, docs, specs, benchmarks, and reviewer evidence.

## Delegation Plan

Implementer:

- Pending user activation and routing decision. Current project default is the
  external implementer model recorded in `AGENTS.md`, but this SOW is currently
  only created and not active.

Reviewers:

- At implementation close, run the approved reviewer pool in read-only mode:
  glm, kimi, mimo, qwen, minimax, and deepseek, with the implementer excluded if
  an external implementer is used.

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

- Any implementer failure, reviewer failure, unavailable model, failed audit,
  failed benchmark, or unsupported platform result must be recorded in this SOW
  before continuing.

## Execution Log

### 2026-06-14

- Created pending SOW from user-requested cross-language reader parity review.
- Recorded user decision that Go `VisitEntryPayloads` is not a Netdata
  Explorer blocker and may remain only as a callback-scoped visitor documented
  separately from row-level APIs.
- Added precise Python and Node.js parity findings: Python visitor is safe but
  copy-based, Python row-level enumeration exists, Node visitor/enumerator use
  row-lifetime read-at windows today, and Node TypeScript exposes mmap while
  runtime rejects it.

### 2026-06-15

- Recorded user decisions for Go read-at, Python read-at, and Node.js optional
  mmap packaging direction.
- Activated the SOW for implementation.
- Verified Go has rolling mmap reader backends for `unix` and `windows`, with
  only `!unix && !windows` using the unsupported mmap constructor. Verified
  Python's standard-library `mmap` module documents both Unix and Windows
  constructors and marks only WebAssembly/WASI unavailable.
- Hid Rust whole-file mmap from normal `journal` public imports/options while
  preserving hidden internal/test/benchmark hooks.
- Made Go read-at prominent as non-production in Go API comments and public
  docs, and changed the Go reader benchmark helper default from `read-at` to
  `auto`.
- Removed Python read-at from the top-level package export while retaining the
  internal diagnostic constant under `journal.reader_access`.
- Removed Node.js default-package TypeScript mmap mode advertisement while
  keeping runtime explicit-mmap rejection.

## Validation

Acceptance criteria evidence:

- Rust normal `journal` public imports no longer re-export
  `ExperimentalMmapStrategy`; `ReaderOptions` keeps the mmap strategy field
  private and exposes only a doc-hidden internal/test/benchmark strategy hook.
- Go keeps `ReaderAccessReadAt` for compatibility and tests, but Go comments,
  API docs, README, wiki docs, and product scope now say it is not a production
  reader mode. The Go reader benchmark helper now defaults to `auto`, not
  `read-at`.
- Python read-at remains available under `journal.reader_access` for tests,
  diagnostics, constrained-platform investigation, and fallback evidence, but
  it is no longer exported from the top-level `journal` package. Python README
  now states that read-at is not a production reader mode.
- Node.js default-package TypeScript declarations no longer advertise mmap as
  an available access mode. Runtime explicit mmap rejection remains tested, and
  optional native mmap support is tracked by SOW-0113.
- Product scope, Go/Python/Node docs, and SOW ledgers now record the updated
  reader access contract.

Tests or equivalent validation:

- `cargo test -q -p systemd-journal-sdk default_reader_options_use_production_window_size`:
  passed.
- `cargo check -q --workspace`: passed after adding the explicit internal
  `corpus_digest` dependency on `journal-core`.
- `go test ./...`: passed.
- `npm run typecheck`: passed.
- `npm test`: passed.
- Targeted Python reader-access validation passed:
  top-level `journal.READER_ACCESS_READ_AT` absent, internal
  `journal.reader_access.READER_ACCESS_READ_AT` present, and affected mmap,
  read-at, fallback, row-arena, sparse-file, and directory reader tests passed.
- `python3 test_all.py`: blocked by missing optional environment dependency
  `lz4` before reaching the touched reader-access tests.
- `python3 tests/docs/check_wiki_docs.py`: passed.
- `python3 tests/docs/verify_examples.py`: passed, 31/31 examples.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed.

Real-use evidence:

- Local Rust, Go, Python, and Node reader/package tests exercised synthetic
  repository journal fixtures only. No live host journal was probed.

Reviewer findings:

- Pending whole-SOW external reviewer pass before SOW close.

Same-failure scan:

- `rg` scans for stale `with_mmap_strategy`, Node TypeScript mmap unions,
  top-level Python read-at export, production read-at wording, and whole-file
  mmap guidance found no unexpected public/default surfaces. Remaining matches
  are intentional internal/test hooks, runtime explicit-mmap rejection tests,
  Python internal diagnostic constants, or pre-implementation evidence text in
  this SOW.

Sensitive data gate:

- Passed. Durable artifacts contain repository paths, public documentation URLs,
  command names, and sanitized technical summaries only. `.agents/sow/audit.sh`
  sensitive-data scan passed.

Artifact maintenance gate:

- AGENTS.md: pending implementation decision.
- Runtime project skills: pending implementation decision.
- Specs: updated `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: updated Go wiki docs, Go README/API, Python README,
  Node README, options reference, and production profiles.
- End-user/operator skills: pending implementation decision.
- SOW lifecycle: activated in `.agents/sow/current/` with
  `Status: in-progress`; SOW-0113 created in pending for optional Node.js
  native mmap support.
- SOW-status.md: updated canonical and root ledgers.

Specs update:

- Updated `.agents/sow/specs/product-scope.md` with the final local reader
  access contract for Rust, Go, Python, and Node.js default package behavior.

Project skills update:

- Pending close decision. No durable workflow rule has changed yet; current
  project skills already cover reader compatibility and docs authoring.

End-user/operator docs update:

- Updated `docs/Go-API.md`, `docs/Options-Reference.md`,
  `docs/Production-Profiles.md`, `go/API.md`, `go/README.md`,
  `python/README.md`, and `node/README.md`.

End-user/operator skills update:

- Pending close decision. No output/reference skill is currently affected.

Lessons:

- Hiding a Rust re-export can expose internal test tools that were implicitly
  depending on the public facade; workspace checks are required for this class
  of change, not only public crate tests.

Follow-up mapping:

- Optional native Node.js mmap support is tracked by
  `.agents/sow/pending/SOW-0113-20260615-nodejs-optional-native-mmap-reader.md`.

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
