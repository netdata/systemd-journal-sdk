# SOW-0159 - Rust/Go SDK Hardening And Reader Compatibility

## Status

Status: paused

Sub-state: implementation authorized on 2026-10-15; paused by the user's
instruction to repair the prerequisite SOW-0108 regression first. Preserve all
current SDK work, then resume this SOW before SOW-0121. Default recovery with
normal EOF and explicit status is approved. Public policy wiring, performance
acceptance, remaining validation and whole-SOW review remain open. Candidate
performance regressions currently prevent acceptance.

## Requirements

### Purpose

Improve the robustness and correctness of both SDKs while maintaining or
improving their existing performance and journal-native access paths.

### User Request

The user requested two separate SOW workstreams: SDK and journalctl. The user
explicitly requires performance to remain as-is or improve: do not hurt
performance. SOW-0159 owns SDK work; reopened SOW-0121 owns journalctl work.

### Assistant Understanding

Facts:

- Both SDKs can loop indefinitely during ordinary field/value enumeration on
  cyclic FIELD/DATA links. Strict verification has separate chain checks.
- Rust silently returns the first frame of a concatenated Zstandard DATA
  payload. A synthetic 1014-byte message was returned as 506 bytes with success;
  Rust verification rejected its DATA hash. Go returned all 1014 bytes and
  verified the same file successfully.
- Both SDKs already bound decompressed DATA output and read unknown-size single
  Zstandard frames. These are regression protections to preserve.
- Go rejects opening a file whose final ENTRY header was lost, although three
  earlier entries survive. Rust recovered those entries. With a missing later
  ENTRY array, Go returned no entries; Rust returned 350 entries before error.
- The binary journal schema, object layouts, and feature-bit assignments are
  unchanged between systemd v260 and checked upstream main. Sealing's OpenSSL
  migration preserves the encoded state/key/tag contract.
- Existing compatibility verification deliberately accepts an acyclic ascending
  FIELD DATA chain that strict index verification rejects. Unconditionally
  adopting newer upstream direction checks would change that contract.

Inferences:

- Shared traversal safeguards and complete frame decoding address concrete
  correctness risks without requiring a file-format redesign.
- Reader recovery can improve access to intact data, but its termination and
  diagnostics policy is a product decision rather than a validator cleanup.

Unknowns:

- Which bounded chain-termination mechanism preserves the measured hot paths
  across all affected reader, Explorer, facade, and writer callers.
- The approved recovery/error-reporting contract for ordinary readers.
- Performance of the eventual candidates; diagnostic debug runs are not
  release-performance baselines.

### Acceptance Criteria

- AC1: both languages terminate safely on self-links, multi-object cycles,
  invalid links, and lost chain tails in every reachable affected ordinary API.
  Audit hash lookup, FIELD enumeration, unique visitors/stateful enumeration,
  directory caches, Explorer/index callers, and writer lookup reuse.
- AC2: Rust consumes complete supported Zstandard streams in DATA reading and
  verification. Audit whole-file `.journal.zst` staging and every shipped,
  reachable decoder for the same failure. Go passes identical fixtures without
  unnecessary decoder changes.
- AC3: shared fixtures cover known/unknown-size single frames, concatenated and
  skippable frames, binary/repeated fields, malformed/truncated frames, trailing
  garbage, aggregate decompression bounds, and error-buffer cleanup.
- AC4: implement the approved paired crash-recovery contract and demonstrate
  forward/reverse iteration, time seeks, exact filters, file/directory input,
  and readable-prefix/error behavior on the same crash-truncation corpus.
- AC5: strict index verification, writer-excluded capture, append-open, retention
  inventory, and writer-failure guarantees remain strict. Rejected append-open
  leaves bytes and extent unchanged. Normal live-growth behavior remains valid.
- AC6: preserve borrowed payload lifetimes, mmap/window behavior, reusable DATA
  paths, indexed selection, existing acyclic compatibility allowances, and
  synthetic-identity/runtime-purity contracts in both languages.
- AC7: every affected valid-input performance surface meets the performance
  gate below. No reproducible regression is accepted, and one case's speedup
  cannot offset another case's slowdown.
- AC8: local conformance/interoperability, same-failure scans, artifact updates,
  external review or explicit waiver, and the complete-and-clean SOW audit pass
  before implementation close. Actual stock versions/features must be recorded.

## Analysis

Sources checked:

- SDK commit `dafcdac4b971d04e48768a1f831974f9252061e2` (`v0.10.0` source tree).
- `go/journal/reader_unique.go:18`, `reader_entry.go:330`, `reader.go:260`,
  `reader.go:606`, and `writer_compression.go:148`.
- `rust/src/crates/journal-core/src/file/file_iterators.rs:44`,
  `file/file.rs:506`, `file/file_payload.rs:451`, and
  `file/object_compression.rs:112`.
- `rust/src/journal/src/verify_graph/io.rs:12` and
  `reader_helpers.rs:266` for additional decoder surfaces.
- `go/journal/verify_index_test.go:394`,
  `.agents/sow/specs/product-scope.md`, and `rust-reader-performance.md`.
- `.local/sdk-reader-audit-l8whqk5m/sdk-reader-probes.json`: generated synthetic
  evidence only; durable summaries above do not depend on retaining this path.
- `tests/benchmarks/README.md`, existing reader/writer benchmark runners,
  `tests/interoperability/run_compression_matrix.py`, and `run_verify_matrix.py`.

Current state:

- Fresh Rust/Go debug binaries reproduced both ordinary chain hangs under
  1.5-second child-process timeouts. Both passed the full-message control and
  unknown-size single-frame read/verify checks. These runs establish defects,
  not performance or full v262 interoperability certification.
- Available PATH stock journalctl is systemd 255.4. Source comparisons use
  pinned v260.1/v262/current revisions; a full-feature target-version runtime
  oracle remains required for implementation acceptance.

Risks:

- Adding full verification or entry scans to ordinary open/query paths would
  hide corruption at unacceptable cost and is outside this approved scope.
- Per-row visited sets, extra decompression passes, disabled native decoding,
  whole-file copies, or repeated metadata parsing can regress hot paths.
- Recovery can accidentally relax append/retention safety or silently conceal
  incomplete data unless its contract is explicit.

## Pre-Implementation Gate

Status: design decisions resolved for traversal, streams and default recovery
with normal EOF/status. Implementation is paused for prerequisite SOW-0108;
resume only after that repair passes its gates.

Problem / root-cause model:

- Ordinary index traversals trust link progression; separate strict-verifier
  protections do not make those calls terminate. Rust's first-frame sizing and
  single-frame fallback do not establish complete stream consumption. Go eager
  ENTRY-array loading discards a readable prefix when later references are lost.

Evidence reviewed:

- The source paths, synthetic probe results, existing compatibility tests,
  live/stable contracts, benchmark surfaces, and upstream references below.

Affected contracts and surfaces:

- Both public SDKs/facades, shared traversal consumers, compressed DATA,
  whole-file compression where reachable, verification, recovery, live reads,
  snapshots, writer lookup reuse, and consumer-visible error/lifetime behavior.
- CLI-only parsing, rendering, grep, and action dispatch belong to SOW-0121.

Existing patterns to reuse:

- Journal-native hash/FIELD/DATA chains and entry posting arrays; existing strict
  graph checks as evidence, without running that graph pass on ordinary reads.
- Native single-frame Zstandard fast paths, bounded streaming, reusable row
  arenas, cold/cached unique-value paths, and current mmap bounds separation.
- Traversal candidate: three-word lazy Brent guard, activated only after a link
  departs from normal monotonic progression. It accepts all acyclic orders,
  uses no extra reads/allocations, and preserves successful hash-hit early exits.
  An exhaustive scalar-model check covered 83,184 rooted functional graphs of
  one through five nodes in both directions against a visited-set oracle.
  This is correctness evidence only; retained release binaries precede edits,
  and per-path release measurements decide whether the candidate is accepted.
- Stream candidate: retain the known-size native attempt unchanged; replace
  single-frame fallback with bounded complete-stream decoding and per-frame
  checksum/size checks. Reuse one frame decoder. Expose a hidden core helper for
  verifier/staging reuse; whole-file staging remains streaming and has no DATA
  size cap. Unpublished legacy FFI crates are outside the shipped surface.
- Recovery implementation contract: copyable per-file status contains reason,
  first unavailable offset, declared entry count and surviving count. Directory
  and facade diagnostics identify files; collections allocate only on explicit
  diagnostic requests. Status records observed loss, not a full integrity proof.
  Preserve raw header counters. Go retains its existing loaded offset prefix;
  Rust derives effective counts/tail from ENTRY-array slots only after a proven
  missing-tail access, using native array slots and fixed ENTRY headers. Recovery
  views clamp partial final arrays and DATA posting tails; strict object parsing
  is unchanged. Refresh invalidates degraded bounds on growth/publication.
  On the error path, validate the readable native ENTRY-array slots and fixed
  ENTRY headers so an interior hole followed by a readable entry remains an
  error. Binary sampling alone cannot establish a contiguous intact prefix.
  This cold pass expands no DATA/rows, uses bounded scratch space, and adds no
  ordinary valid-file work. Measure its damaged-file cost separately.
  Recovery diagnostics report observed loss; they do not turn indexed queries
  into full integrity checks. Do not preflight unrelated arrays for every query.
  A damaged secondary posting index must return an error when its missing tail
  could still contain matching entries inside the recovered global population;
  do not silently certify an incomplete filtered result or scan/decode all rows.
  Apply the same completeness check before recovery is observed: a positive
  DATA posting count requires its inline posting and all declared array slots.
  Native array traversal must distinguish a completed count from a missing
  required next link. Unused DATA with zero count and zero inline pointer stays
  valid. These scalar/error-only checks add no global preflight or row scan.
  Exact-filter cursor construction may classify a tail error after its DATA
  guard drops and retry once after a changed global prefix is established.
  Refresh all retained filter leaves before consuming the final expression;
  earlier leaves can predate recovery discovered by a later leaf. Keep failure
  retryable. Do not replay an entire controlled Explorer query: emitted callbacks
  and shared sampling state cannot be undone. An unknown damaged secondary tail
  remains an error there until global loss is observed through reader access.
  Wrong types, cycles, misalignment, malformed nonzero sizes, compression errors
  and general I/O errors remain errors. Strict verify/snapshot/append/retention
  paths explicitly retain strict validation. Prefix computation/status work is
  independent of the pending terminal EOF/error choice; wire that policy only
  after its clarification.

Risk and blast radius:

- High for hostile-file termination, silent payload truncation, and recovery
  semantics. Changes must cover the actual shared callers in both languages
  without absorbing unrelated refactors or writer optimization projects.

Sensitive data handling plan:

- Use synthetic identities and repository-local fixtures only. Do not inspect
  host journals or private corpora. Keep generated journals, profiles, logs,
  dependency caches, and oracle builds under this task's `.local/` directory.
  Durable evidence contains sanitized counts, relative paths, and public commits.

Implementation plan:

1. Resolve the decisions below, re-ground against the current source, enumerate
   shared call sites, and retain release baseline binaries/corpus before edits.
2. Add paired corruption and stream fixtures and missing benchmark cases.
3. Implement the approved bounded chain protection and Rust stream completion
   while preserving existing fast paths; cover reachable sibling decoders.
4. Implement the approved ordinary-reader recovery behavior separately from
   strict validation. Keep corruption handling bounded and observable.
5. Run correctness, performance, profiling, and cross-language gates; update
   artifacts and obtain the required whole-SOW review disposition before close.

Validation plan:

- Run `go -C go test ./journal` and the affected Rust workspace package suites,
  including public SDK, core, facade, and verifier tests. Use existing local
  toolchains and redirect every cache/temp output inside the repository.
- Extend and run compression/verify, directory/mixed-directory, and relevant
  live/growth matrices. Add shared ordinary-reader corruption cases rather
  than assuming verifier tests cover those APIs.
- Validate sealed output/read/verify against full-feature stock targets. Retain
  historical unkeyed, compact/regular, XZ/LZ4/Zstd, and FSS fixtures.
- Apply the performance gate; inspect same-patterns in every reachable shared
  caller, then run `.agents/sow/audit.sh` and inspect its complete verdict.

Artifact impact plan:

- AGENTS.md: existing performance, language-parity, and purity rules suffice
  unless the user explicitly changes a project-wide contract.
- Runtime project skills: update compatibility guidance for adopted recovery,
  stream-completion, and corruption-validation procedures when implemented.
- Specs: update product scope and Rust performance evidence for shipped behavior;
  planning must not describe unimplemented recovery as current reality.
- End-user/operator docs: update affected reader/recovery/error guides and any
  changed performance guarantees after implementation; use docs-authoring skill.
- End-user/operator skills: none currently exist; verify at close.
- SOW lifecycle: keep this record pending until decisions and execution order
  are resolved; do not execute it alongside SOW-0121.
- SOW-status.md: update canonical and root indexes at preparation and each state
  change; commit implementation/lifecycle completion together at final close.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced` (`v260.1`):
  `docs/JOURNAL_FILE_FORMAT.md`, `src/libsystemd/sd-journal/journal-def.h`.
- `systemd/systemd @ 7f073ff56f14e9b6ff7052998b640df88a359358` (`263~devel`):
  `src/libsystemd/sd-journal/journal-file.c:583`, `:1004`;
  `src/basic/compress.c:841`; `src/test/test-fsprg.c:303`; `NEWS:422`.
- `systemd/systemd @ 62c5c888f434067683e72f678b1bb995873f275b`:
  `src/basic/compress.c`, unknown-size/streaming Zstandard decoding.

Open decisions:

1. Resolved on 2026-10-16: the user authorized read-only external review after
   complete local validation, including later rounds after material fixes.
   This SOW remains paused for the SOW-0108 prerequisite; review has not run here.

Resolved decisions: preserve current acyclic compatibility and strict-verifier
separation; execute SDK first, then the CLI SOW with complete v262 behavior.
Recover intact prefixes by default, end with normal EOF and expose status.
Repair prerequisite SOW-0108 first while preserving this working change set.
The selected traversal mechanism still requires performance acceptance.

## Implications And Decisions

1. Approved on 2026-10-14: two separate workstreams, both Rust and Go, with SDK
   scope here and journalctl scope in originating SOW-0121.
2. Approved hard constraint: preserve or improve performance. This overrides
   any generic permission to accept a documented slower path for this work.
3. Recommendation classification: long-term-best, implemented minimal-complete.
   Fix demonstrated gaps and their reachable consequences; broader performance
   redesign, dependency refresh, format redesign, and refactoring are separate.
4. Existing baseline remains v260.1; newer source/fixtures supplement it. No
   compiler-floor increase, CGO, system journal linkage, implicit host identity,
   automatic writer lock, or core subprocess execution is authorized.
5. Sealing backend replacement is not required by the observed format changes.
   Preserve interoperability and test it rather than porting an OS dependency.
6. Approved on 2026-10-15: complete both workstreams sequentially, SDK first.
   Recover intact entries by default and expose recovery status. Iteration's
   terminal signal is the remaining recovery contract clarification.
7. Approved on 2026-10-15 for SOW-0121: advance the complete CLI contract to
   systemd v262. SDK binary-format baseline remains v260.1, with newer stream
   and crash-recovery behavior covered here.
8. Approved on 2026-10-15: recovered SDK iteration ends normally after returning
   the intact prefix; callers inspect recovery status for observed tail loss.
9. Approved on 2026-10-15 for SOW-0121: native PCRE2 in the CLIs only, including
   a Go CGO exception, subject to the unchanged performance gate. SDK runtime
   dependencies and purity remain unchanged.
10. Approved on 2026-10-15: pause this SOW, repair the original SOW-0108 Explorer
    lifetime regression first, preserve current work and measure allocation
    costs, then resume this SOW and SOW-0121 sequentially.

## Plan

1. Approve contracts and measured candidate designs; freeze baseline evidence.
2. Close chain/stream gaps with shared tests and per-path performance evidence.
3. Deliver the selected recovery outcome and cross-language validation.
4. Review, maintain artifacts, audit, and close only after all gates pass.

## Performance Gate

- Capture baseline before runtime edits using release binaries, pinned compiler
  and dependency versions, an immutable synthetic corpus, identical output
  checksums, format/compression/cardinality, mmap/window strategy, bounds,
  publication cadence, and host conditions. Record baseline variance.
- Run baseline/candidate in alternating order with warmups and repeated samples;
  start with ten measured repetitions and investigate noise when necessary.
  Require unchanged or improved results on each affected valid-input case.
  There is no permitted slowdown percentage or cross-case averaging exemption.
- Measure elapsed/CPU time, throughput, allocations, peak RSS, page faults,
  and syscalls when supported. Profile uncertain or adverse changes. An
  inconclusive comparison cannot be reported as a passed performance gate.
- Cover FIELD catalogs, low/high-cardinality unique visitors and cached restart,
  exact indexed filters, forward/reverse payload scans, reused compressed DATA,
  live/snapshot and file/directory inputs, snapshot visits, and verification.
  Add writer/Explorer cases when shared lookup or decoder changes reach them.
- Preserve zero uncompressed Rust row allocations after warmup, Go allocation
  baselines, borrowed slices, native single-frame decoding, reusable buffers,
  and requested-scope decompression. Do not add full ordinary-open verification,
  entry-scan substitutes, per-row visited allocations, or whole-file buffers.
- Reuse `tests/benchmarks/run_reader_core_benchmarks.py` for regular/compact,
  forward/backward, file/open-files production surfaces. Extend existing drivers
  for field/unique enumeration and compressed fixtures: those surfaces are not
  all measured by the current runner.
- Reuse `go -C go test ./journal -run '^$' -bench
  'Benchmark(IndexedSnapshot|VerifyIndexRepeatedValue|DirectoryReaderUniqueHighCardinality)'
  -benchmem -count 10`, and paired Rust cases. If writers change, run
  `run_writer_core_benchmarks.py` with matching publication settings.
- Compare equivalent work against the fastest sibling and stock where practical;
  record versions and reasons for unavailable/non-equivalent comparisons. A
  previously truncated payload or omitted row is not a valid faster baseline.
- Render available before/after reports through
  `tests/benchmarks/report_benchmarks.py`. Every measurable regression blocks
  acceptance; redesign and rerun instead of silently relaxing the requirement.

## Implementation And Review Plan

Implementation:

- One project-manager writer, sequential edits, paired language outcomes, and
  shared fixtures. The 2026-10-15 instruction authorizes implementation; recovery
  termination is separately gated until the requested clarification arrives.
- SOW-0156 retains the broader fixed-header audit, SOW-0155 the unconfirmed
  large-writer-offset concern, SOW-0137 residual writer optimization, and
  SOW-0098 legacy-core consolidation. Avoid unrelated changes to those scopes.

Reviewers:

- External review is not authorized by the SOW-preparation request. After full
  local implementation/validation, recommend whole-SOW review; obtain explicit
  authorization or waiver and follow the system-wide reviewer skill.

Repository boundary block for every external-reviewer prompt:

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

- Record correctness, performance, review, and audit failures. Do not close,
  weaken strict paths, narrow compatibility, or change the approved design
  without resolving the evidence and any required user decision.

## Execution Log

### 2026-10-15

- The user authorized implementation of both SOWs as one goal, sequentially.
- Recorded default recovery/status and CLI v262 decisions. Started release
  baseline capture before runtime changes; no compiler-floor or native-library
  policy changes are authorized.
- Recovery validation exposed a Rust seek discrepancy in `file/cursor.rs`:
  reverse realtime seeks used the forward partition, and forward seeks beyond
  the last row returned the tail. Repair the shared native-array partition in
  this scope so recovered and intact files both select the first timestamp at
  or above the request forward, or the last at or below it backward. Test both
  filtered/unfiltered directions and out-of-range requests; no row scan is added.
- Initial release A/B discovery used retained baseline/candidate binaries on
  identical 100,000-row regular/compact corpora, alternating order, one warmup
  and three measured pairs. Checksums matched, but Rust core paths flagged
  roughly 5-13% slower cases and some SDK/Go cases also flagged slowdowns or
  noise. These are rejected discovery results, not performance acceptance.
  `Node` had grown from four to seven words, copying chain state on every row.
  Restore its original size; only a non-increasing ENTRY-array link invokes a
  cold stack-local bounded chain check. Normal arrays still follow their direct
  native path. Keep lazy guards on FIELD/DATA iterators, and move unordered
  bookkeeping/recovery retries off the common successful path. Remeasure every
  affected case; do not offset one slowdown against another improvement.
- Built an isolated full-feature `systemd/systemd @
  8cc40e0c5e9234bf45084751ac53b1fbfe70b492` v262 oracle. Its journalctl reports
  `+OPENSSL +PCRE2 +LZ4 +XZ +ZSTD`. Sources, private header extraction, builds,
  dependency downloads and logs remain under the task's ignored `.local/`
  directory. No system installation or service change occurred.
- Added matching public SDK `inspect-reader` probes to the existing conformance
  adapters, plus `tests/interoperability/run_reader_hardening.py`. Its first
  chain/stream run passed 120 SDK observations over the same regular/compact
  corpus. It exercised self/multi-object cycles, ascending acyclic FIELD DATA,
  DATA-hash lookup, ENTRY arrays, known/unknown/concatenated/skippable/empty-first
  Zstandard frames, corrupt later checksums, truncation, trailing bytes, output
  caps, binary/repeated fields and whole-file staging. All 20 applicable stock
  v262 verification checks agreed. Public recovery cases remain gated until
  terminal policy is selected and default recovery is enabled.
- Focused private recovery-enabled tests pass forward/reverse/time/exact reads,
  raw counter preservation, same-size and physical repair, pending directory
  merge rows, malformed/interior holes and arena bounds. Rust additionally
  retained the historical no-RTC population of 10,757 entries. Full suites are
  not accepted: the current deliberately unwired public recovery policy leaves
  one historical Rust drain failure and one Go directory-fixture failure.
- Retained alternating before/after measurements and standard reports. The
  ten-pair CPU-pinned diagnostic still flagged Rust core-next and several
  compact/unique cases; candidate acceptance remains failed. Kernel policy
  rejected unprivileged hardware profiling; no kernel setting was changed.
  Installed Callgrind profiling found 1.213B baseline versus 1.163B candidate
  instructions for core payloads, with similar simulated cache misses. These
  counters do not explain native timings or waive the performance gate.
  Removed an avoidable per-row population check from `JournalCursor`; empty
  population checks now occur when resolving positions. Remeasure this change.
- DWARF layout inspection found that the new cold recovery state shifted Rust's
  header map from byte 376 to 456 and window manager from 392 to 472. Move the
  cold state after existing mappings and recheck layout/timings. This is a layout
  candidate, not a claim that it explains the regression.
- Same-pattern investigation added sparse posting coverage: a known lost ENTRY
  beyond the recovered population is safe to omit even when that DATA value
  does not occur in the final surviving row. An unreadable posting segment with
  unknown targets remains an error. Share the recovery-aware DATA-header cursor
  construction between exact filtering and Explorer facets/histograms so a
  missing array pointer cannot be silently converted into an empty result.
- Paired sparse-value and missing-array tests now pass, including strict index,
  snapshot capture, and append rejection. The Rust cold-state layout candidate
  restored the original header/window-manager offsets in emitted DWARF. Run
  alternating measurements before attributing any timing change to layout.
- Preserve the approved traversal checks while removing duplicate work in the
  measured unique path: return the matched FIELD's DATA head from the existing
  bucket visitor, and reset only the cycle guard's inactive marker. Initialize
  the remaining guard words on its cold activation path. These implementation
  candidates still require functional and before/after performance validation.
- Restored layout alone did not remove the measured regressions. Disassembly
  found a new out-of-line DATA constructor staging a 48-byte object on the stack
  and copying it into the return guard, whereas the baseline constructs its
  return directly. Inline that constructor without removing type, size, arena,
  alignment, or zerocopy validation; measure the resulting binary separately.
- DATA inlining removed its out-of-line temporary and improved measured core
  payload scans by about 5% regular and 13% compact. Some cursor-only cases
  still regressed. The remaining 75-byte ENTRY and ENTRY_ARRAY constructors
  stage analogous 40-byte temporaries. Apply the same targeted inlining there,
  keep their checks, and remeasure rather than accepting the residual slowdown.
- Cursor-only timing remains unresolved after constructor inlining. Emitted
  code showed that `OffsetArrayObject::capacity` still stages an array and
  repeats variant/alignment/length extraction already performed to read its
  slot. Inline this small getter so the compiler can share that work. Retain
  recovery dispatch and zero-slot checks; inspect and measure the candidate.
- The capacity candidate removes reproducible regressions in the current
  targeted ten-pair run, but several confidence intervals remain inconclusive.
  Recheck those cases with twenty pairs/twenty child measurements per phase;
  do not treat absence of a flag as a performance pass.
- Allocation probes compare the same warmed payload scans on both immutable
  corpora: Rust baseline/candidate each recorded zero allocations in all three
  measured passes. Go matched its baseline at five allocations per complete
  regular scan and zero per compact scan. Each scan returned 100,000 rows,
  3,200,000 fields and 81,700,000 payload bytes. This is allocation evidence,
  not a timing or general performance waiver.
- Updated the product/performance specs and reader/language/snapshot wiki pages
  for complete streams and native-index errors. The wiki structure validator
  passed all 16 pages. Public recovery docs and executable-example validation
  remain required after the terminal contract is selected.
- Mid-implementation `.agents/sow/audit.sh` reported complete and clean. Repeat
  after final artifacts, review disposition and lifecycle updates before close.
- Followed disassembly evidence for two remaining duplicated-work candidates:
  inline the new FIELD-head visitor, and read each traversed next-link once
  before both cycle checking and iterator advancement. This avoids repeated
  zerocopy header validation. Internal cursor materialization uses a required
  nonzero-offset getter to remove an emitted second zero test; the public
  Option-returning getter and the actual zero-slot validation remain intact.
  Functional and retained-binary timing validation are still required.
- Added directory-catalog benchmark coverage to the Rust helper, matching Go's
  existing directory surface. Retained comparison tooling also supports
  directory inputs and content/name fingerprints, including hardlinked fixture
  files without repeatedly hashing the same inode. Rebuild matching helper
  sources against both frozen baseline and candidate SDKs for that comparison;
  retain the existing file-run binaries unchanged.
- Public API impact check found that the new `JournalObject::EXPECTED_TYPE`
  member must have a default to preserve downstream custom implementations.
  Make the hint optional/default absent, and set it for every built-in object.
  Built-in typed reads keep their wire-type check; custom implementations keep
  their previous contract. The new public `CyclicObjectChain` error variant
  needs an unreleased migration note for exhaustive Rust enum matches.
- Expanded public chain/stream/facade/directory tests passed 232 observations
  and 20 stock v262 verification checks. Fresh-open malformed posting tests
  also pass in both SDKs. Added cold exact-filter recovery after releasing the
  DATA guard and refreshed retained filter leaves before consuming them;
  all ten focused Rust recovery tests and Go's paired suite pass.
- A full release comparison stopped at a reproducible crash in the unchanged
  Go Explorer baseline. Its completed SOW-0108 lifetime claim is contradicted;
  the prerequisite scope question remains open. The benchmark runner now saves
  raw output and completed samples/comparisons incrementally, so later failures
  do not lose the earlier evidence.
- Compact diagnostics flag several forward Rust payload/facade cases 2-4%
  slower. The candidate helper also adds directory-catalog code, so rebuild
  matching helper sources for the controlled comparison. Read-only disassembly
  found no extra runtime Option/type-hint test, but forward `Cursor::next` now
  saves two more registers than the baseline. Reverse cursor and payload
  visitor instructions are unchanged. Outline the rare array transition to
  isolate its checks from same-array movement, then inspect emitted code and
  measure it. Preserve every missing-link, cycle and bounds error.
- Go's compact high-cardinality visitor also regressed in the diagnostic run.
  Retained CPU profiles show time concentrated in DATA-header/accessor parsing;
  compiler diagnostics confirm the guard is inlined. Recovery fields had been
  inserted before the reader's cached layout/row state. Move that cold state to
  the struct tail, restoring the existing hot-field positions. This is a layout
  candidate, not a proven cause. Measure it on both corpora with matched helpers.
- A traced Rust compact snapshot payload scan returned identical complete
  counters and exactly 124 syscalls before/after, including 22 mmap calls,
  11 munmaps and 7 statx calls. This supports unchanged syscall behavior for
  that case only; traced timings are not latency acceptance evidence.
- Rebuilt private SDK-backed CLI/writer integration tools and target-version
  C readers. Stock v262 closed-file checks pass: 32 general, 72 DATA-compression,
  20 compact and 18 binary-field checks. All 18 live writer/feature cases pass
  across both writers and stock/Rust/Go polling readers, including active
  libsystemd readers, all 100 final entries, structure and sealed-key checks.
  These are SDK integration checks, not full v262 CLI acceptance. Final public
  recovery wiring still requires a fresh complete validation pass.
- Directory/mixed-directory matrices also pass against stock v262. Whole-file
  compressed source fixtures were retained outside the watched directories
  rather than deleted; fixture generation and SDK reader paths are unchanged.
- Matched-helper measurements accept neither residual small Rust SDK slowdowns
  nor inconclusive cases. Inspection found a full `Location` checkpoint copied
  before every SDK row, solely for cold selected-ENTRY failure handling. Keep a
  scalar prior resolved offset on successive rows; copy a full anchor only for
  nonresolved seek positions. Reconstruct the exact prior Location on error.
  This preserves filtered EOF getters, direction changes and live repair.
  Rejected the alternative of anchoring recovery at the global surviving tail:
  a sparse filter can end on a different matching row, so that shortcut changes
  post-EOF getters and can return a duplicate when reversing. Add that concrete
  counterexample to the focused tests before measuring the compact checkpoint.
- The compact-checkpoint test passes with all eleven focused Rust hardening
  tests. Independent read-only inspection confirms that every Location variant
  reconstructs the same pre-error anchor as the full checkpoint; this is source
  correctness evidence, not performance acceptance.
- The completed matched-helper run has 36 comparisons: 14 improvements,
  18 inconclusive results and four compact Rust regressions. The flagged
  payload/facade cases are approximately 0.9-2.3% slower. Both Go high-cardinality
  visitor cases improve after restoring the reader's hot field layout. The
  compact-checkpoint candidate is not included in those measurements.
- A subsequent current Rust build failed because the archived baseline and
  current checkout shared a Cargo target directory; Cargo reused baseline core
  artifacts while rebuilding the current SDK. Keep the retained baseline
  binaries immutable and use a separate target directory for current builds.
  The failed build is not validation evidence and did not alter source or
  dependencies. Rebuild current tools in the isolated target before measuring.

### 2026-10-14

- Prepared the SDK scope after source inspection and isolated synthetic probes;
  recorded the user's language-parity and performance requirements.
- No SDK, dependency, public spec, or runtime implementation changed.

## Validation

### Required On Resume: Explorer Recovery Ownership Qualification

SOW-0108's prerequisite repair has an early-owned-key shortcut for ordinary
all-candidate views whose visible ENTRY count equals the declared count. Its
standalone walker always visits a positive inline posting. This SDK SOW adds
recovered posting clipping, so equal counts alone cease to prove contribution:
missing unused ENTRY_ARRAY capacity can leave every declared ENTRY visible while
recovery still suppresses a DATA object's postings.

When this SOW resumes, qualify the Go facet shortcut using actual recovery state,
not only global count equality. Keep that dependency outside the standalone
SOW-0108 patch. Retain count-first ownership in every degraded view and test the
equal-count/incomplete-array case with DATA postings outside the recovered
population. Results/error semantics remain unchanged; the issue is avoiding
unnecessary cold-path key allocations. Record allocation evidence before making
an unconditional zero-unmatched-allocation claim for recovered reads.

Acceptance criteria evidence:

- AC1-AC3: paired traversal guards and complete Rust stream decoding are
  implemented locally. The expanded shared chain/stream corpus passed 232 SDK
  observations and 20 applicable stock v262 verification checks. Exhaustive
  small-graph tests also cover reset/reuse of the lazy cycle detector.
  Added fresh-open exact/Explorer checks for positive posting counts with lost
  inline/head/next pointers, plus missing required global-array links, facade
  calls and directory catalog paths. Both regular and compact layouts pass.
- AC4: recovery-prefix/status internals and focused private-policy tests pass
  in both languages, including sparse posting tails and unknown-target errors.
  Public default recovery and its terminal signal await the requested user
  clarification; public recovery acceptance is not claimed.
- AC5-AC6: focused tests retain raw counters, reject malformed/interior holes,
  and reject strict verification, capture and append-open without byte changes.
  Complete live-growth, retention, lifetime and cross-language validation remain
  required before close.
- AC7: failed. Retained alternating release measurements contain reproducible
  regressions. Layout and code-generation candidates are being measured; no
  tolerance, offsetting speedup, or noisy result has been accepted as a pass.
- AC8: incomplete. Final docs/specs, full local matrices, review disposition,
  artifact maintenance and complete-and-clean audit remain delivery gates.

Tests or equivalent validation:

- Rust core: 94 tests passed, including downstream custom `JournalObject`
  implementations that omit the additive optional type hint. The latest public
  Rust suite passes 175 tests and retains the one expected historical no-RTC
  failure while default recovery is disabled. Focused private recovery tests pass, including
  regular/compact forward/reverse/time/exact/sparse/directory repair paths.
  Go's focused recovery suite passed, including strict capture and append-open.
- Full-suite attempts found the two expected historical-fixture failures while
  default recovery is intentionally disabled. These attempts are not green
  acceptance runs and must be rerun after the terminal policy is implemented.
- Release A/B runs use immutable 100,000-row regular/compact journals, frozen
  pre-change SDK binaries, alternating order, warmups, exact row/field/byte
  checksums, ten measured pairs and retained raw samples. CPU-pinned diagnostic
  runs use five child measurements per phase. Standard benchmark summaries and
  reports are retained under the task's ignored `.local/` directory.
- Preparation validation on 2026-10-14: `git diff --check`, explicit whitespace
  checks including new files, gate/state/index checks, and the original CLI
  narrative-preservation comparison passed. `.agents/sow/audit.sh` reported
  `SOW initialization complete and clean`; its durable-artifact scan found no
  sensitive-data patterns.

Real-use evidence:

- Public SDK reader/verifier paths were exercised through file-backed commands
  on synthetic fixtures. The isolated stock v262 oracle has OpenSSL, PCRE2, LZ4,
  XZ and Zstandard enabled. No host journal or production service was used.

Reviewer findings:

- Read-only investigations confirmed traversal/decoder gaps, recovery-policy
  isolation, sparse-posting boundaries and code-generation differences. Fixed
  the confirmed sparse-tail rejection and missing DATA-array cursor ambiguity;
  paired regression tests pass. These investigations are not final whole-SOW
  implementation review approval.
- An independent read-only planning consistency check found no blocking scope,
  performance-gate, lifecycle, or decision-recording flaws. Runtime design and
  implementation readiness remain unapproved. External review has not been
  authorized or waived; recommend it after complete local validation.
- The frozen pre-change Go Explorer benchmark crashes on the valid 100,000-row
  regular corpus. Independent inspection traced a borrowed facet value across
  posting reads that can unmap its storage; histogram has the same pattern.
  `go/journal/explorer.go` copies the value only after those reads. Commit
  `a8119457` introduced rolling temporary slices under completed SOW-0108.
  The original SOW explicitly claimed Explorer lifetime preservation. A scope
  clarification asks whether to repair that prerequisite in reopened SOW-0108
  first or retain it as a separately scheduled regression. No repair or
  acceptance of this crashing performance case is claimed here.

Same-failure scan:

- Guards cover FIELD/DATA hash lookup, FIELD/DATA iteration, global/posting
  arrays, Explorer and shared writer lookups. Recovery-aware DATA cursor
  construction is shared by exact filtering and Explorer facets/histograms.
  Strict constructors were inspected independently in both languages. Final
  shared-caller/lifetime/live-growth scans and coverage mapping remain required.

Sensitive data gate:

- Changed source, tests and tracking text use synthetic identities, counts,
  relative evidence and public commits. Generated journals, logs and profiles
  remain ignored under `.local/`. Repeat the durable-artifact scan before commit.

Artifact maintenance gate:

- AGENTS.md: no workflow or responsibility change is needed. Existing purity,
  performance and review rules apply.
- Runtime compatibility skill: update with the approved recovery workflow and
  normal EOF/status contract when implementation resumes and final evidence is
  available. Paired chain/stream/fresh-query, callback-lifetime and
  retained-performance procedures are already updated; final recovery-policy
  guidance remains part of this paused implementation.
- Specs and end-user/operator docs: impact inventory is complete. Update reader,
  compression and native-index error contracts are updated and labelled
  unreleased where applicable. All 35 verified examples and 16 wiki page checks
  pass. Final public recovery/status documentation must describe the approved
  normal EOF/status policy once implemented; do not describe the currently
  disabled default as shipped behavior.
- End-user/operator skills: none are installed for this project.
- SOW lifecycle: current/paused for prerequisite SOW-0108, which is reopened
  and in progress for the proven pre-existing Explorer lifetime regression.
  The user selected repair-first execution. Preserve this SDK work until the
  repair passes its performance, review and audit gates. SOW-0121 remains
  separately paused until this SDK SOW is completed, reviewed, audited and
  committed; keep runtime changes attributable to their owning SOWs.
- SOW-status.md: canonical and root summaries reflect SOW-0108, then SOW-0159,
  then SOW-0121, with only the prerequisite repair active.

Specs update:

- Required: product reader contract and Rust hot-path/decompression contract.
  CLI v262 specs remain owned by SOW-0121.

Project skills update:

- Required: record the paired hostile-file corpus, bounded cycle checks and
  ordinary-recovery versus strict-capture validation workflow.

End-user/operator docs update:

- Required: Reader APIs, language API pages, Indexed Snapshots, reader options
  and native-index error propagation. Status examples must pass both wiki
  validators before close.

End-user/operator skills update:

- No output/reference skills exist; verify that fact again at implementation close.

Lessons:

- Safe strict verification does not prove safe ordinary index traversal.
- Single-frame decoder success does not prove complete logical DATA consumption.

Follow-up mapping:

- SDK defects, reachable consequences, and performance acceptance belong here.
- CLI defects belong to SOW-0121; general CLI scalability remains SOW-0122.
- Existing unrelated pending scopes retain their ownership as listed above.

## Outcome

Traversal/stream repairs and recovery internals are implemented locally. Public
recovery termination, performance acceptance, full validation, artifact updates
and review remain open. This SOW is not ready to close or publish.

## Lessons Extracted

Measure the real production path and verify complete output before interpreting
performance. Keep ordinary compatibility and strict integrity policies separate.

## Followup

No additional SOW is created for this work. All identified work is
owned by this record, SOW-0121, or the specifically listed existing records.

## Regression Log

No completed outcome exists in this new scope. Append dated regressions only
after an accepted outcome is later contradicted.

## Prerequisite Measurement - 2026-10-15

A separate healthy-positioning finding is tracked in reopened/paused original
SOW-0045. Unchanged HEAD skips an append after empty/EOF Refresh or a unique
query that refreshes offsets. The approved normal EOF/status decision here
concerns recovery termination; do not silently absorb that independent state
machine repair. Preserve this SOW's recovery-specific positioning validation
and cross-check interactions when implementation resumes.

SOW-0108's isolated header analysis identifies a required performance correction
in this paused implementation: `go/journal/reader.go` currently acquires the
16-byte ENTRY-slot header with unconditional `readSlice`. A header crossing two
already-cached windows then takes an extra positioned file read, unlike the
previous chunk-copying path. In one matched primitive comparison, mmap crossing
headers take 740.90 ns versus 58.06 ns for the original path. Removing heap
allocation alone does not satisfy the performance contract.

The user selected option `1a`, hybrid native slices plus an owned buffer, for
the active prerequisite on 2026-10-15. Its complete reader performance gate
remains required. When this SOW resumes, use the approved acquisition strategy while
preserving this SOW's separate bounds/ENTRY-shape/zero-header/recovery checks.
Do not transfer those additional checks into the prerequisite repair.

Add warmed crossing headers, non-power-of-two/tiny/default windows, retained
rows, closed readers and empty/small/large opening allocation costs to this
SOW's Go performance validation. The private primitive checks pass 64 cases;
they do not certify the full paused SDK or its reader-construction cost. This
note tracks dependent work only; SOW-0159 remains paused and no runtime source
is changed by this update.

## Review Authorization - 2026-10-16

The user selected option A, granting standing read-only external-review
authorization for SOW-0108, this SDK SOW and SOW-0121 at their complete local
validation milestones, including further rounds after material fixes. Use the
six named reviewers and the system-wide reviewer skill. Earlier unauthorized
review notes describe the prior preparation checkpoint and are superseded by
this decision. Keep the approved one-SOW sequence and performance gate; this
authorization does not resume SDK implementation before SOW-0108 closes.
