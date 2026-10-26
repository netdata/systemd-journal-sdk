# SOW-0159 - Rust/Go SDK Hardening And Reader Compatibility

## Status

Status: completed

Sub-state: SDK regression work completed 2026-10-26. Signed checkpoint
`a1144dcbf537` contains the approved EOF contract, read-only recovery scope and
I/O classification corrections. Independent SDK final review passes that head.
The Go invocation consumer regression and other confirmed CLI/qualification
findings are owned by reopened SOW-0121. Historical completion evidence remains
below; appended 2026-10-25/26 records govern this regression close.

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
- Final public recovery/status acceptance and cost; the policy itself is
  resolved by the recorded user decisions.
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

Initial investigation state:

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
with normal EOF/status. Implementation resumed on 2026-10-16 after SOW-0108
passed its gates and signed combined commit; the remaining work below uses
that repaired baseline and preserves strict-path policy.

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
  independent of the approved terminal policy. Enable ordinary-reader recovery
  with normal EOF and explicit status; preserve strict callers' false policy.

Risk and blast radius:

- High for damaged-file termination, silent payload truncation, and recovery
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

- AGENTS.md: retain native-index, language-parity and purity rules; record
  decision 11's approved representative performance-evidence guidance.
- Runtime project skills: update compatibility guidance for adopted recovery,
  stream-completion, and corruption-validation procedures when implemented.
- Specs: update product scope and Rust performance evidence for shipped behavior;
  planning must not describe unimplemented recovery as current reality.
- End-user/operator docs: update affected reader/recovery/error guides and any
  changed performance guarantees after implementation; use docs-authoring skill.
- End-user/operator skills: none currently exist; verify at close.
- SOW lifecycle: this is the sole current/in-progress implementation after
  prerequisite SOW-0108; keep SOW-0121 paused until this SDK SOW closes.
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
   The prerequisite and subsequent whole-SOW reviews are complete, with reviewer
   coverage gaps explicitly recorded below.
2. Resolved on 2026-10-16: the user requires Go to have complete parity with
   Rust for the ordinary-filter decision. Implement native indexed Go filtering
   with equivalent Boolean, positioning, refresh and failure behavior; no
   recovered-only workaround or language-specific error exception is selected.
   Preserve the existing performance gate and language-idiomatic API surfaces.
3. Resolved on 2026-10-17: repair reverse-filter skipped matches in both SDKs,
   returning the nearest preceding match after filter installation, forward
   EOF and direction changes. The user approved the long-term-best outcome.
4. Resolved on 2026-10-17: retain the last successful position in both SDKs
   after a nonrecoverable selected-entry load error. Repair followed by
   iteration must retry the failed candidate instead of skipping it.
5. Resolved on 2026-10-17: use binary posting search for explicit filtered
   realtime seeks in both SDKs, reporting errors from headers/slots actually
   visited. Ordinary sequential posting traversal remains unchanged apart
   from the separately approved reverse-position correction. Performance
   acceptance is still required; no slowdown exception was authorized.
6. Resolved on 2026-10-17: performance evidence uses engineering judgment,
   representative high-level tests and careful review, with targeted
   measurements for credible costs. Do not mechanically expand a large
   framework or block every uncertain timing interval. See decision 11.
7. Resolved on 2026-10-17: user selects 1A for Rust counted-array history.
   Keep Node at four words (32 bytes on 64-bit targets), replace cached next
   with the root and read next at array transitions. See decision 12.
8. Resolved by the existing product-scope cache policy: apply the required
   journal-engine cache generation bump for changed native index results.
   This introduces no new cache architecture or performance waiver; the
   one-time migration and unchanged hit path are recorded below.

Resolved decisions: preserve current acyclic compatibility and strict-verifier
separation; execute SDK first, then the CLI SOW with complete v262 behavior.
Recover intact prefixes by default, end with normal EOF and expose status.
Repair prerequisite SOW-0108 first while preserving this working change set.
Local performance acceptance is recorded below under decisions 11 and 12;
independent whole-SOW review is adjudicated below, including coverage gaps.

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
   terminal signal was clarified by decision 8 below.
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
11. Approved on 2026-10-17: use engineering judgment for performance acceptance.
    Representative high-level tests and careful code review may establish that
    a change does not negatively affect performance. Do not build or expand a
    large measurement framework to prove every small detail. Preserve or improve
    performance, investigate credible observed regressions, and use targeted
    measurements where code review or existing evidence leaves a real concern.
    The earlier self-authored requirement to block every inconclusive timing
    comparison is superseded; statistical uncertainty is not proof of equality
    and is also not, by itself, evidence of a regression.
12. Approved on 2026-10-17: keep Rust Node at 32 bytes on 64-bit targets,
    replacing cached next with the starting array offset. Accept the current
    header lookup at array transitions; preserve within-array row movement
    and validate representative performance. Root ordering state uses a spare
    alignment bit, avoiding per-row history allocation or a larger cursor.

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
- Require unchanged or improved affected valid-input behavior. There is no
  permitted reproducible slowdown percentage or cross-case averaging exemption.
  Apply decision 11: combine careful code review, representative high-level
  controls and retained measurements; do not require every possible case or
  every uncertain timing interval to demonstrate a statistically significant
  speedup before accepting the change.
- When measurement is needed, reuse existing runners, alternate baseline and
  candidate with warmups, and retain samples and semantic counters. Measure the
  quantities relevant to the actual change, such as elapsed cost, allocations,
  work counts or syscalls. Investigate credible adverse observations with
  targeted controls or profiles. Do not expand a framework mechanically or
  claim exact equality from an interval that crosses one.
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
- Reuse existing Go/Rust benchmarks when they answer a concrete remaining
  question. Shared writer changes need a blast-radius review; run targeted
  writer controls with matching publication settings when a credible cost
  remains unresolved.
- Compare equivalent work against the fastest sibling and stock where practical;
  record versions and reasons for unavailable/non-equivalent comparisons. A
  previously truncated payload or omitted row is not a valid faster baseline.
- Render available before/after reports through
  `tests/benchmarks/report_benchmarks.py`. Every measurable regression blocks
  acceptance; redesign and rerun instead of silently relaxing the requirement.

## Implementation And Review Plan

Implementation:

- One project-manager writer, sequential edits, paired language outcomes, and
  shared fixtures. Approved public default recovery and normal EOF/status are
  enabled and paired validation is complete as recorded below.
- SOW-0156 retains the broader fixed-header audit, SOW-0155 the unconfirmed
  large-writer-offset concern, SOW-0137 residual writer optimization, and
  SOW-0098 legacy-core consolidation. Avoid unrelated changes to those scopes.

Reviewers:

- Standing read-only external review is authorized on 2026-10-16 after complete
  local implementation/validation, including rounds after material fixes. Use
  the approved six reviewers and system-wide reviewer skill; do not infer
  acceptance here from the separate prerequisite's reports.

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

### Explorer Recovery Ownership Qualification

SOW-0108's prerequisite repair has an early-owned-key shortcut for ordinary
all-candidate views whose visible ENTRY count equals the declared count. Its
standalone walker always visits a positive inline posting. This SDK SOW adds
recovered posting clipping, so equal counts alone cease to prove contribution:
missing unused ENTRY_ARRAY capacity can leave every declared ENTRY visible while
recovery still suppresses a DATA object's postings.

The implemented Go facet shortcut checks actual recovery state, rather than
global count equality alone. Degraded views retain count-first ownership;
reader_recovery_facet_test.go covers the equal-count/incomplete-array case
with postings outside the recovered population. This qualification belongs
here rather than the prerequisite SOW-0108 patch. Results/error semantics are
unchanged. No unconditional zero-unmatched-allocation claim is made for
recovered reads.

This section records current acceptance evidence. Dated execution sections
below retain earlier failures, open questions and rejected experiments as
history; the final dispositions here and at the end supersede those states.

Acceptance criteria evidence:

- AC1-AC3: paired traversal guards and complete Rust stream decoding are
  implemented locally. The expanded shared chain/stream corpus passed 232 SDK
  observations and 20 applicable stock v262 verification checks. Exhaustive
  small-graph tests also cover reset/reuse of the lazy cycle detector.
  Added fresh-open exact/Explorer checks for positive posting counts with lost
  inline/head/next pointers, plus missing required global-array links, facade
  calls and directory catalog paths. Short counted-array controls also cover
  descending-prefix revisits, public Rust Nodes, reconstructed cursors, valid
  backward links, unused terminal links and writer rejection before mutation.
  Both regular and compact layouts pass.
- AC4: default intact-prefix recovery, normal EOF and explicit status are
  enabled in both public SDKs. Shared probes cover file/directory SDK/facade
  reads, sparse posting tails, repair/retry and unknown-target errors. Complete
  nearest-match and error-position decisions are approved and implemented,
  including directory filter mutation, native sequential discovery and repair/retry. Private-policy-only wording in the dated
  history is historical. Whole-SOW independent review is adjudicated below.
- AC5-AC6: full language suites retain strict verification/capture/append-open,
  raw counters, malformed/interior-hole rejection, lifetime, retention and live
  growth protections. Both compiler floors and Go's additional targets pass.
  The final v262 verifier/directory/mixed checks pass 63/34/42 observations.
  Earlier 32 general, 72 compression, 20 compact, 18 binary, 18 live-feature
  and 552 shared hardening observations retain their original provenance;
  later equivalent position/constructor/guard edits do not change their
  writer, codec, FSS or publication algorithms. They are not relabelled as
  runs of a later binary.
- AC7: accepted locally under decision 11, including the final counted-array
  controls for decision 12. Eight final Rust cases give three improvements and
  five inconclusive intervals, with no regression flag. Both focused final Go
  writer cases are inconclusive with no regression flag. Four-word Nodes,
  unchanged within-array work and reviewed allocation/native-I/O paths provide
  the accompanying engineering evidence. This is not a claim of exact equality.
  Earlier evidence retains its original source provenance.
  All 32 explicit filtered realtime comparisons improve on the original SDK
  baseline; the intentionally interrupted broad stage retains 28 completed
  comparisons without an adverse flag. Representative filter setup,
  ordinary materialization/visitation, unique enumeration, directory,
  compression/staging and writer controls are combined with native-path,
  allocation/work-counter and generated-code inspection. Credible Rust
  setup/ordinary and Go entry/writer costs prompted equivalent cost removals;
  the final targeted repeats do not reproduce those flags. Earlier adverse
  blocks remain recorded. No case's speedup offsets another's slowdown, and
  uncertain intervals are not treated as proof of equality. Exact writer
  observations, final source provenance and limits are recorded below.
- AC8: local validation, docs/specs, artifact reconciliation and whole-SOW review
  disposition are complete. The closing complete-and-clean audit and signed
  combined implementation/lifecycle commit accompany this completed record.

Tests or equivalent validation:

- Rust's supported 1.91 floor workspace passes, including 200 public SDK and
  98 core tests; the allocative-feature core also passes 98 tests. Final-source
  evidence is external-findings-fatcvrlo/rust-floor-validation. Existing
  ignored documentation examples remain recorded. This covers default recovery,
  frame streams, strict paths, lifetimes, counted cycles and repair/retry.
- Go's supported 1.26.2 all-package suites pass 1,995 amd64, 1,996 386 and 1,995
  race test events (parent/subtest nodes, not distinct leaves), excluding only
  the unchanged TestGoWriterLiveStockReadersStress. The retained isolated
  1,200-entry/fsync-per-entry test passes in 38.03 seconds with its original
  45-second deadline; an earlier full run's deadline failure remains a failure,
  with contention only an unproven working theory. After the final internal
  directory call correction, 170 directory/recovery/cycle events pass on each
  of amd64, 386 and race. The final directory mutation and lazy-discovery
  controls are included in all three final all-package runs; focused final
  Go directory checks also pass 306 events. Evidence: external-findings-fatcvrlo/
  go-floor-validation. Earlier Windows/macOS/FreeBSD builds retain their
  provenance; subsequent common-code edits are checked on 386 as well.
- Final release-build-8 tools pass 552 shared SDK observations and stock-v262
  verification/directory/mixed matrices (63/34/42). The exact source/binary
  hashes, commands and results are retained in internal-review-go-floor,
  internal-review-rust-floor-2, directory-transition-floor, release-build-8,
  shared-validation-8 and final-integration-8 under the task's ignored workspace.
- The required generation-4 engine migration passes Rust 1.91 index/engine
  package suites and existing cache-key migration tests. The new archived
  indexer test covers regular/compact missing links and short cycles with a
  healthy 13-row control. The original SDK executable instead succeeds with
  four rows for the regular missing-link case; it stops at that assertion,
  so no stronger baseline-execution claim is made. Evidence: cache-migration.
  The final requested-field test covers 20 native-chain/pointer failures plus
  two absent-map facet targets, with actual engine cache omission and a healthy
  sibling. Before repair, its first cycle case incorrectly succeeds; the
  added absent-map control separately reproduces the inherited panic. After
  repair all cases pass, including the complete final index/engine suites.
- After the unreleased cache upgrade note, all 35 verified examples pass again,
  and the static wiki validator passes all 16 pages (docs-validation-8).
- Earlier pre-enablement historical-fixture failures and the isolated retention
  precondition failure remain retained with their dispositions. Passing later
  tests do not erase them or the final amd64 stress deadline failure.
- Release A/B runs use immutable 100,000-row regular/compact journals, frozen
  pre-change SDK binaries, alternating order, warmups, exact row/field/byte
  checksums, retained raw samples and case-specific repetitions recorded in
  each manifest. The final targeted reader block uses seven measured pairs;
  writer controls use ten. CPU-pinned diagnostic
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
  performance-gate, lifecycle, or decision-recording flaws. Runtime design
  choices are approved as recorded. Standing whole-SOW external review was
  executed after complete local validation as recorded below. The prerequisite's
  separate reviewer reports do not substitute for this SDK review.
- The first whole-SOW SDK round returns two positive reports and one
  NEEDS CHANGES report; three reviewers have no verdict after permitted
  retries. Both verified blockers are reproduced and repaired, with the full
  primary audit and final floor/integration/docs checks passing again. Its
  verdicts do not accept the materially repaired candidate. The fresh complete
  round returns Minimax PRODUCTION GRADE; GLM, MiMo, Kimi and DeepSeek reach
  their execution limits without verdicts, and Qwen returns HTTP 429 twice.
  All notes are independently adjudicated; no verified blocker remains. These
  five coverage gaps are retained, without inventing consensus or approval.
  Exact dispositions and the decision against another long retry batch are below.
- Historical prerequisite, now resolved by signed commit 4c49205: the frozen
  pre-repair Go Explorer benchmark crashed
  on the valid 100,000-row regular corpus. Independent inspection traced a borrowed facet value across
  posting reads that can unmap its storage; histogram has the same pattern.
  `go/journal/explorer.go` copies the value only after those reads. Commit
  `a8119457` introduced rolling temporary slices under completed SOW-0108.
  The original SOW explicitly claimed Explorer lifetime preservation. A scope
  clarification was answered with repair-first execution. SOW-0108's combined
  implementation/review/validation/lifecycle commit precedes this resumption;
  its reports are not substituted for this SDK SOW's final review.

Same-failure scan:

- Guards cover FIELD/DATA hash lookup, FIELD/DATA iteration, global/posting
  arrays, Explorer and shared writer lookups. Recovery-aware DATA cursor
  construction is shared by exact filtering and Explorer facets/histograms.
  Strict constructors were inspected independently in both languages. Final
  shared-caller scans cover strict constructors, decoder callers, directory
  merging and retry, copied-window invalidation, post-Close access and both
  ordinary and explicit-time posting paths. Directory mutations and Go mode
  changes preserve proven sequential state; representative later-file damage
  controls prove nearer rows and repair/retry remain usable. Strict index
  consumers propagate encountered required chain/posting failures; an absent
  global-map target returns an error on its existing branch. Inherited source
  map membership/captured-tail analysis is separately tracked in SOW-0161.
  Existing lifetime/live-growth
  suites and shared fixtures validate those surfaces. Unfiltered Go refresh
  and realtime-probe issues remain separately tracked in SOW-0045.

Sensitive data gate:

- Changed source, tests and tracking text use synthetic identities, counts,
  relative evidence and public commits. Generated journals, logs and profiles
  remain ignored under `.local/`. The final changed-artifact scan is clean;
  closure text and the moved SOW are included in the closing audit.

Artifact maintenance gate:

- AGENTS.md: performance-evidence guidance now records decision 11's use of
  careful review, representative tests and targeted measurement. Existing
  purity, native-index, no-regression and review requirements remain in force.
- Runtime compatibility skill: updated with default recovery, normal EOF/status,
  fresh-query, callback-lifetime, filter position/time and directory retry
  controls and decision 11's representative performance-evidence workflow.
- Specs and end-user/operator docs: reader, compression and native-index error
  contracts are updated and labelled unreleased where applicable. Default
  recovery/status and normal EOF are documented. All 35 verified examples and
  16 wiki page checks pass. The final cold guard changes no documented API.
- End-user/operator skills: none are installed for this project.
- SOW lifecycle: completed and moved to done with the implementation in the
  same signed commit, after prerequisite SOW-0108's signed commit 4c49205.
  SOW-0121 remains separately paused until this commit succeeds; its activation
  and runtime changes belong to its own sequential implementation scope.
- SOW-status.md: canonical and root summaries reflect completed SOW-0108 and
  SOW-0159, then paused SOW-0121, with decision 11 and final review coverage.

Specs update:

- Updated: product reader contract, Rust hot-path/decompression contract,
  snapshot byte-extent wording and required engine cache generation 4.
  CLI v262 specs remain owned by SOW-0121.

Project skills update:

- Updated: paired damaged-file corpus, bounded cycle checks and
  ordinary-recovery versus strict-capture validation workflow.

End-user/operator docs update:

- Updated: Reader APIs, language API pages, Indexed Snapshots, reader options
  and native-index error propagation, plus the unreleased generation-4 cache
  migration. All 35 examples and 16 wiki page checks pass; the final equivalent internal cost removals change no example/API.

End-user/operator skills update:

- No output/reference skills exist; the final skill catalog confirms that no
  external consumer skill is affected.

Lessons:

- Safe strict verification does not prove safe ordinary index traversal.
- Single-frame decoder success does not prove complete logical DATA consumption.
- File-level position tests do not establish directory mutation semantics; a
  replay expectation can encode the original bug. Preserve native sequential
  discovery as well as the returned row sequence.
- New low-level errors require auditing consumer catches and cache insertion.

Follow-up mapping:

- SDK defects, reachable consequences, and performance acceptance belong here.
- CLI defects belong to SOW-0121; general CLI scalability remains SOW-0122.
- Pre-existing healthy EOF/refresh, directory seek/getter state and unfiltered
  realtime-probe error suppression remain in paused SOW-0045.
- Unchanged Go XZ dictionary allocation remains in pending SOW-0160.
- An inherited source-map membership/captured-tail lead is tracked in pending
  SOW-0161. It generates no new traversal error to propagate; source predictions
  remain unexecuted and do not justify adding certification to healthy indexing.
- The narrow Rust constructor identity optimization is implemented here.
  Broader cursor/getter experiments, decoder outlining and the private Go
  hash-lookup terminal-zero relocation are rejected from this delivery;
  their evidence is retained. No unimplemented optimization is required for
  the accepted outcome or left as an untracked promise.

## Outcome

Traversal/stream repairs, default public recovery/status and native filtering
are implemented and validated locally in both languages, including approved
position/time and directory retry corrections. Performance acceptance uses
the user's approved representative evidence and careful review, with final
targeted controls after equivalent cost removals. The final whole-SOW report
is positive, every note is adjudicated and five reviewer coverage gaps remain
explicit. Local validation and artifact maintenance pass. This completed record
and implementation are committed together after the closing audit. Publication
and the separately approved journalctl implementation are outside this SDK commit.

## Lessons Extracted

Measure the real production path and verify complete output before interpreting
performance. Keep ordinary compatibility and strict integrity policies separate.

## Followup

No additional SOW is required to close this work. All valid remaining items map to
SOW-0121, SOW-0122, SOW-0045, SOW-0160 or SOW-0161 as recorded in Validation. Rejected
optional experiments are not implementation requirements.

## Regression Log

No regression against this completed outcome has been reported. Reopen this
SOW and append a dated regression if later evidence contradicts its outcome.

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

## Resumption - 2026-10-16

The prerequisite is complete in signed commit `4c49205393c7`. Its exact
reviewed tree, signature verification and all 80 preserved working-file hashes
pass parent and independent post-commit checks. The preceding paused execution
log remains historical evidence; clarification-pending wording in the current
gate/acceptance summary is reconciled with the approved normal EOF/status policy.
No new product/design fork is identified by the paired read-only preparation.

### Approved Remaining Implementation And Risk Review

- Ordinary entry points still disable recovery: `go/journal/reader.go:230` and
  `rust/src/journal/src/lib.rs:437`. Enable these existing policy switches.
  File, directory and facade status APIs already exist; use their current
  language-native contracts rather than adding another status model.
- Preserve strict explicit policies in Go verification/capture and Rust sealed
  verification/IndexedSnapshot foundation. Ordinary bounded snapshot reading
  remains distinct from excluded-writer IndexedSnapshot capture. Append-open,
  retention and strict index validation retain their existing rejection rules.
- Apply the already recorded recovered-view qualification to Go's all-candidate
  facet key shortcut. Equal visible/declared counts do not prove every posting
  contributes after incomplete unused array capacity. Check actual recovery
  state, preserve count-first ownership for degraded views, and cover the
  equal-count/no-contributing-posting allocation case.
- Preserve the prerequisite's hybrid header acquisition and this SOW's separate
  alignment/arena/entirely-zero-header/ENTRY-shape/physical-extent validation.
  No additional header-acquisition redesign is authorized or needed.
- Promote appropriate focused recovery tests to public constructors and run the
  shared adapter corpus through files, directories, facades and whole-file
  compressed input. Check normal EOF/status, lazy detection, counts/raw header,
  reverse/time/exact reads and repair/refresh. Retain strict rejection and
  byte/extent preservation controls. Prior private-policy passes do not certify
  public defaults.
- Risks remain semantic relaxation of strict paths, recovered cursor/filter
  completeness, temporary DATA lifetimes, live refresh and added hot-path work.
  Reuse existing internals/tests and keep all changes within the approved scope.

### Baseline And Validation Plan On Resume

Freeze repaired HEAD before further runtime edits, retaining separate baseline
and current build targets and matching complete helper sources. The repaired Go
Explorer is now a valid default-window control. Preserve every older baseline,
failed run and historical context limit; do not overwrite or reinterpret them.
Rust runtime has no prerequisite change, but use the same final helper source
against baseline and candidate to avoid benchmark-link context differences.

Run focused public recovery suites first, then complete language, stock-v262,
shared corruption/stream/recovery, live/growth, strict capture/append, historical
and docs/example gates. Measure damaged-file recovery cost separately from
valid-input performance. Re-run every affected native/index/catalog/payload/
facade/directory/snapshot/verifier/writer surface with allocation evidence and
matched retained binaries. Current reproduced SDK regressions remain blocking;
no tolerance or aggregate offset is introduced. Investigate inconclusive cases
before any performance acceptance claim.

Maintain product/performance specs, the compatibility skill and affected wiki
pages for observed status, default prefix recovery, normal EOF and remaining
errors. Existing user decisions and standing external-review authorization apply.
All fixtures, caches, reports and profiles remain repository-local and synthetic;
no installation, host journal, service or out-of-repository write is authorized.

### Resumed Public Defaults And Validation - 2026-10-16

- Enabled existing ordinary policies at `go/journal/reader.go:230` and
  `rust/src/journal/src/lib.rs:437`; strict verification/capture keep explicit
  disabled policies. Promoted focused fixtures to public constructors.
- Focused tests pass 353 Go leaves and 11 Rust cases. All 35 executable wiki
  examples pass against the current source in a fresh isolated workspace;
  the 16-page structure check passes. Full suites remain unaccepted.
- Go recovered equal-count facet qualification is implemented. Both layout
  fixtures return the expected missing-value facet and counters. Ten paired
  allocation-only samples per layout compare identical current SDKs with/without
  the qualification: 525,394 to 1,104 bytes/op and 15 to 14 allocations/op.
  This eliminates the unmatched 512 KiB value copy. Concurrent validation makes
  those latency samples diagnostic only; the old rejecting SDK is not a valid
  damaged-file timing baseline.
- Shared public validation exposed an obsolete missing-global-next assertion:
  the declared million-entry array now returns its four intact entries, explicit
  recovery status and normal EOF. Updated that expectation and retained strict
  verification rejection. A later sparse lost-array case reveals the actual
  ordinary-filter fork described in Open decisions: both recover 350/1005
  entries; Go returns one row and Rust returns an unknown-posting-tail error.
  No dependent runtime change is authorized by this observation alone.
- Current Go Reader allocation shape is 760 bytes / 768-byte heap class, above
  the repaired baseline's 640-byte class. Recovery/chain fields require an
  internal storage-cost investigation; steady scan counts do not prove opening
  allocations unchanged. Preserve the approved copyable status/API and measure
  cold state storage and lazy exceptional chain state before accepting it.
- Read-only disassembly identifies two Rust code-generation candidates within
  the approved cold-path design: borrow the 32-byte Node at the cold next-array
  boundary to avoid duplicate staging, and inline the small healthy cursor
  dispatch while retaining cold recovery. Test each immutable private candidate
  independently, inspect emitted code, and measure before adopting. No bounds,
  type, size, cycle, prefix or filter-completeness check may be removed.
- The first full Rust workspace run fails one unchanged five-second retention
  timing precondition after 55 passing log-writer integration cases. The failure
  occurs before close: the expected pre-expiry archive was already pruned.
  Investigate elapsed-time sensitivity and an isolated unchanged-source rerun;
  do not attribute it to recovery, change retention behavior, or hide the run.

### Internal Storage Candidate Before Measurement - 2026-10-16

Test a private Go representation candidate before adopting it: keep immutable
recovery status/tail state behind a pointer allocated only after observed loss;
retain the previous pointer in refresh rollback snapshots; keep successful status
reads copyable and allocation-free. Make stateful unique-chain storage lazy only
on an exceptional non-monotonic link, preserving its full three-word guard and
reset semantics. Store the three fixed layout sizes (4/8/16/64/72) as byte-width
private cache fields and widen at their existing accessors. Pack the recovery
policy flag with existing boolean fields. These internal choices preserve APIs,
file bytes and validation, and aim to retain the baseline Reader heap class.

Risks are nil-state access, refresh rollback aliasing, directory dirty detection,
restart after a previously unordered chain, compiler layout and cache effects.
Keep recovery state immutable after construction, compare observed values rather
than pointer identity, and test regular/compact, amd64/386, healthy/damaged opens,
unique restarts, directory repair and failed refresh. The candidate must pass
allocation and per-path timing checks before adoption. This storage investigation
is independent of the pending ordinary-filter backend decision.

### Benchmark Evidence Instrumentation Plan - 2026-10-16

Extend both existing reader helpers with separate `sdk-digest` and
`facade-digest` preflight modes. Their ordered checksum consumes every payload
byte and length plus existing row/time markers; normal timing modes retain their
sampled checksum. Collect final file-access/mmap counters in the diagnostic SDK
mode before closing, retaining the existing Go pre-scan counter scope. These
preflights validate complete bytes separately from performance measurements;
they do not add a public SDK telemetry API, a dependency, or per-payload work to
normal timing modes. Directory/facade access statistics remain unavailable at
that public layer and must not be invented. Rebuild matching helper sources
against both frozen runtime trees before final measurements; retain earlier
binaries/results as historical candidates rather than reusing their timing.

The first private storage candidate passes selected tests and restores the
640-byte class, but read-only review establishes two costs: first supported
ascending stateful enumeration allocates a guard, and unchanged degraded refresh
allocates replacement status state. It is not accepted. Test a second private
representation retaining the guard inline: encode Brent's checkpoint schedule
with checkpoint plus a monotonically increasing step counter, updating at powers
of two. This is the same lazy Brent mechanism and needs the exhaustive graph
oracle plus counter-boundary checks. Explicit overflow ends traversal; a valid
physical journal cannot have that many distinct minimum-sized objects. Preserve
normal ordered links' inactive fast path and every early hash-hit allowance.
Reuse immutable recovery state only when newly derived status and tail values
are equal; do not mutate rollback-visible state. Measure both unusual valid
ascending chains and repeated damaged refresh, rather than hiding these costs.

V2's measured shape is 640 bytes / 704-byte heap class: the selected test reports
shape but does not itself enforce equality. It is not accepted. Same-pattern
source inspection finds `Reader.direction` is write-only: six assignments in
`reader.go` and two in `explorer.go`, with no reads, constructor literals or
external supported access. Remove this private dead bookkeeping in a third
private candidate; preserve the independent `DirectoryReader.direction` and
Explorer source-direction fields, which are read. This eliminates unnecessary
per-row writes and aims to restore the 640-byte class without allocating chain
state. Compile every caller and check amd64/386 shape, direction/recovery tests
and matched performance before adoption. No public direction API is removed.

### Decision Checkpoint - 2026-10-16

The ordinary-filter choice remains pending. SOW-0159 remains in-progress and
SOW-0121 remains paused; neither is complete. No native-filter implementation or
language-specific failure exception has been selected. Runtime storage/cursor
experiments remain private and are not adopted by passing selected tests alone.

- Current public runtime: full Go suite passes 1,175 leaf tests (1,314 test
  nodes), uncached; focused public recovery/header/opener coverage passes 353
  leaves. Rust public reader-hardening passes 11 cases; the full workspace has
  the retained five-second retention-precondition failure. Its unchanged binary
  passes the same exact test in isolation, which does not certify a clean full
  workspace run or establish the original failure's cause.
- All 35 executable wiki examples and the 16-page structure check pass.
  Whitespace checks and the changed-artifact in-progress SOW audit are clean.
  This is not a completed-SOW validation/review/commit gate.
- New full-byte SDK/facade preflights compile in both languages and pass 16
  checks against independent native ENTRY/DATA parsing, both layouts and both
  directions. Only the two benchmark helpers differ from the initial runtime
  source manifest; library runtime source is unchanged since those full/focused
  functional validations. Final matched baseline helpers still need rebuilding.
- Initial compact Rust timing: one regression, two inconclusive cases and one
  improvement over the four historical flags. Borrowed Node: four inconclusive.
  Plain inline hint: one regression and three inconclusive; disassembly retains
  the same hot boundaries. Forced inline: four inconclusive, with 856 additional
  text bytes and the Result staging/outlined dispatch still present. None
  satisfies the complete SDK performance gate. Keep all sources and raw samples.
- Go private V1 restores class 640 but adds an ascending-chain allocation and
  repeated recovery-state allocation; rejected. V2 removes those costs but
  measures class 704; rejected. V3 additionally removes write-only Reader
  direction bookkeeping: amd64 size 632/class 640 (baseline 624/class 640),
  386 size 524/class 576 (same baseline). Focused public recovery, 386 exhaustive
  chain/ascending and allocation-shape checks pass. Matching opening/header,
  ascending-chain, degraded-refresh and all remaining path timing/allocation
  gates remain required before adoption.
- Next narrow Rust experiments, if needed: extract missing-cursor native
  re-resolution into one cold helper while retaining both direction predicates,
  effective bounds and materialization; separately synthesize `Ok(true)` at the
  healthy return instead of forwarding the full Result. These are code-generation
  candidates within the approved cold-path design, not accepted optimizations.

Private execution evidence is rooted at the resumption workspace referenced by
`.local/sow-0159-20261015-i54ghwxo/sdk-current-resumption.json`. All current child
build/test/benchmark processes have reached a recorded terminal state. Retain the
required user's filter decision before dependent implementation, then resume the
complete local SDK gate and authorized whole-SOW review before the CLI SOW.

### User Filter Decision And Resumption - 2026-10-16

The user requires Go to match Rust completely for the pending ordinary-filter
choice. This selects native indexed filtering, including unknown-secondary-tail
errors, rather than recovered-only preflight or different language failures.
Equivalent operations must preserve Boolean grouping, same-field alternatives,
empty/binary/repeated values, sparse/dense/absent matches, bidirectional stepping,
realtime/cursor positioning, directory merging, live refresh and retry semantics.
Use Rust's native posting/cursor design as the behavioral reference, with Go's
existing global offset vector and hash/array access primitives. Analyze the
complete file/directory/facade movement and invalidation paths before edits;
do not flatten grouped expressions, scan/decode unrelated DATA, or materialize
all matches simply to avoid implementing a cursor. Existing raw Go movement
helpers retain their documented role; filtered Step/facade operations correspond
to Rust filtered iteration. Compiler minimums and SDK purity remain unchanged.

Earlier pending-decision checkpoint text above describes historical state, not
a current blocker. Independent Rust code-generation and Go storage investigations
can continue while the native-filter implementation is prepared and validated.
No SOW is completed by this decision; complete SDK validation/review/commit still
precedes resuming the CLI SOW.

### Native Go Filter Implementation Plan - 2026-10-16

Read-only file/directory/facade and Rust-reference analysis is complete. Keep
pending and compiled expressions behind the existing Reader filter pointer.
Resolve exact raw DATA payloads through the native hash table; each leaf retains
its DATA identity, inline posting, scalar ENTRY_ARRAY node metadata and current
slot/value. Cache node metadata for reverse traversal, never all matching rows.
Union/intersection advance posting cursors directly. Copy scalars before nested
accessor reads so mmap eviction and ReadAt scratch reuse cannot invalidate them.
Healthy first-match reads stay lazy; recovered posting qualification follows
Rust's cold scalar-prefix pass and rejects unknown required secondary tails.

Preserve Rust's pending-expression lifecycle, including replacement after a
successful build, invalid empty-builder errors and retry after lookup failures.
Go conjunction/disjunction methods will return errors, matching their Rust
counterparts; existing call statements can ignore the added result, while the
facade and internal consumers must propagate it. This is a consequence of the
approved equivalent failure semantics, not a separate filtering feature.
Missing DATA hash tables are errors for ordinary exact lookup. Retain existing
Explorer compatibility behavior by qualifying its helper at the ordinary-filter
boundary instead of globally changing Explorer fallbacks.

Filtered file stepping commits only a successful position and preserves the
last matching row at EOF/error. Head/tail/realtime/cursor seeks and raw/Explorer
movement must reconcile cached posting positions. Cursor seeks use metadata
without expanding unrelated DATA and explicitly move to tail on exhaustion.
Refresh rebuilds retained DATA leaves after a successful changed view, without
mutating rollback-visible state; a missing DATA leaf stays false until a new
filter build, as in Rust. Existing recovered-empty DATA identity is retained so
same-size repair can expose its surviving/new postings.

Forward filter mutations to every directory child and invalidate candidates.
Filtered candidates use child native stepping; raw/filtered mode changes must
discard prefetched candidates and reseek from the last emitted merge key.
Preserve ordering, duplicate suppression, realtime bounds, recovery restart and
the unfiltered sequential path. Keep strict snapshot/verify policy, compiler
floors, dependencies, runtime purity and paused SOW-0045 raw-refresh scope intact.

Validation adds direct/facade/directory Boolean and bidirectional results,
metadata-only native access, bounded healthy first-match work, malformed and
missing postings, general I/O retry, filtered EOF/realtime/cursor state, refresh
rollback and same-size repair. Extend shared recovered-tail expectations from
fixture structure (lost-entry sparse succeeds; unknown lost/partial-array sparse
tails error). Freeze ordinary-filter benchmark helpers into both baseline and
candidate; measure startup, first match, traversal, restart, realtime seek,
allocations and unfiltered opening/header paths before acceptance.

Reference inspection also confirms these ordinary-filter details: healthy
resolved posting populations remain fixed until a new builder is installed;
missing DATA remains false on recovery refresh; selected valid ENTRY objects do
not require membership in the global ENTRY array. The Go port keeps an optional
native selected offset in filter state for that last case, preserving existing
unfiltered Reader storage. Healthy duplicate/skipped posting values follow Rust's
needle traversal; recovered posting qualification still rejects non-increasing
values. Snapshot recovery can reread repaired bytes inside the original extent
and declared population. Add a separate cold prefix-rediscovery path, bypassing
the ordinary unchanged-header shortcut, without enabling snapshot growth.

The Go CLI and shared adapter need mechanical interface/error propagation for
the new operator results so the SDK chunk compiles with all consumers. Keep the
existing collectBoots indexed-error scan fallback in this SDK chunk; its broader
CLI policy belongs to the already paused SOW-0121 parity audit. No v262 CLI
implementation has started.

Private Go V3 opening/header comparison has 13 timing regressions, 12
inconclusive cases and two improvements over 27 cases; it is not accepted.
All median allocation counts match; median byte costs match except one lower
candidate result. The apparent individual byte increase is the same roughly
5,320-byte process event observed in both builds, not an established new steady
allocation cost. Read-only codegen evidence identifies the expanded ENTRY
validator (191 to 645 text bytes, frame 16 to 72), rather than changed native
slice acquisition. Preserve every bounds/layout check and test smaller hot
validation plus cold diagnostic construction in a separate private candidate.

The smaller validator reduces the measured overhead but still fails the gate:
12 regressions, 14 inconclusive cases and one improvement. Test one further
private candidate caching the validated arena end and effective ENTRY extent.
`Reader.path` is private write-only state (constructor assignment, no reads);
remove that unused string to fund the two scalar caches without increasing the
Reader allocation class. Recompute caches after size/header refresh and rollback.
Keep original arena-versus-physical failure classification on the cold path,
and retain location/alignment/type/size/layout checks on the healthy path.
Selected tests, codegen and matched per-case results remain prerequisites to
adoption; these optimizations do not waive the no-regression contract.

Initial native-filter validation passes 176 focused Go leaves (194 nodes) and
536 shared hardening observations, 268 per language. Both layouts, normal and
recovered forward/reverse reads, facades, directories and unknown sparse
secondary tails are included. New direct controls cover metadata-only selection,
posting/build I/O retry, snapshot repair, loss observed after open, EOF anchors,
orphan/duplicate posting behavior, mode transitions and malformed unrelated
compressed payloads. Subsequent fixes and final source still require the full
functional and performance gate.

### Newly Demonstrated Filter Anchor Decision - 2026-10-16

The user-facing question distinguishes complete behavior parity from copying a
newly demonstrated skipped-match result. The paired public-API probe positions
at the last of 1,005 rows, installs `COMMON=all AND SPARSE=some`, then steps
backward. Matching timestamps are 1000, 503000 and 1005000. Current Rust returns
1000; current Go returns the nearest preceding match 503000. After an intervening
forward EOF, both return 1000, skipping 503000. Private source, executable logs,
fixture/library hashes and results are retained under the post-decision
`filter-anchor-evidence/` directory. Rust's `JournalCursor::set_filter` preserves
the public location but leaves fresh posting cursors at their initial heads;
`previous_until` can then accept the first posting without moving to the bound.
An AND expression can also leave unevaluated children at earlier positions after
forward EOF, affecting the next direction change.

Question 1 offers A, repair both SDKs in this SOW with shared controls and the
strict performance gate (long-term-best recommendation), or B, preserve current
Rust behavior in Go and document/track the limitation (surgical for Rust but
introduces the skipped result in Go). Do not implement the dependent policy
until the user answers. The original native-index decision remains approved;
independent storage/codegen/performance and harness work continues.

The extent-cache candidate passes 185 selected test nodes and retains
size 632/class 640, but its 27-case timing still has nine regressions and 18
inconclusive results. It is not accepted. Test one further private candidate
extracting reader-access bounds diagnostics: keep overflow-safe containment and
the supported-int-size limit in a small hot check, move error construction into
a noinline helper. Once containment holds, the old combined end/size MaxInt test
is equivalent to checking size alone. Retain identical error precedence and
high-offset small-read support, including 386 coverage. This tests whether
inlining removes the remaining common accessor overhead without weaker checks.

The extracted accessor-bounds candidate improves 12 of 27 construction/header
cases; eleven remain inconclusive and four regress (small mmap opens in both
layouts, and pinned headers in both access backends). Median allocation counts
and bytes remain unchanged. This candidate remains unaccepted. Read-only
disassembly confirms the native opener, accessor constructors and file-header
reader match baseline instructions. The scratch header begins at unaligned
Reader offset 0x17b. Test a separate private candidate moving the existing
16-byte buffer before three one-byte layout caches, yielding offset 0x178 while
preserving subsequent offsets, total size and allocation class. Alignment may
reduce split stores on the pinned path; that effect is a hypothesis until
measured. Retain complete controls, including ordinary header paths affected by
moving the byte caches. Array-chunk bounds/error code and recovery dispatch
remain further inspected opening costs; no required validation is removed.

Matched baseline/current Go and Rust helpers, plus the private Rust cold-resolve
candidate, now build from retained immutable source manifests. Their filter
output preflight uses an independent native fixture-byte parser and full ordered
payload checksums, with binary matches, both directions, rebuild/restart/time
seeks and file/directory/open-files surfaces. A benchmark audit found the former
different-field AND case used identical posting populations. Strengthen it to
overlapping alternatives whose intersection differs from either operand and
their union, and validate benchmark semantic counters independently before
timing rather than accepting agreement between two potentially wrong outputs.

The alignment experiment has ten improvements, fourteen inconclusive cases and
three regressions over all 27 controls. The pinned mmap case still regresses;
alignment alone does not establish a repair and remains unaccepted. Next test
the inspected error-interface join in accessor bounds checks by expressing the
same containment predicate as a boolean and constructing errors only in its
failure branch. This preserves physical/int-size limits and error precedence.

For a separate opening candidate, reuse the arena bound already validated by
the array-header reader, move all-zero/error classification to cold helpers,
and replace redundant checked additions/division with equivalent bounded
arithmetic. Successful array-header validation proves offset+24 and the object
extent fit the arena; `toRead <= capacity` proves its byte product cannot
overflow. Compare that product directly with MaxInt. The 4/8-byte item widths
permit mask/shift arithmetic. Keep physical whole-object checks and recoverable
tail classification unchanged. Isolate global-array failure recovery in a cold
helper if it reduces the measured healthy frame cost; retain complete selected
recovery/bounds/386 validation and all construction/header timing controls.

Opening inspection also finds each array-header read allocates a temporary
24-byte copy even when the complete header fits a cached native window. Within
the private opening candidate, use an immediate borrowed header only for the
same fitting power-of-two window condition already used by ENTRY headers.
Parse all header scalars before nested reads. Preserve the original copying
readAt path for crossing, tiny and non-power-of-two windows; no new persistent
Reader storage, API or alias lifetime is introduced. Measure allocation and
timing on both layouts/backends, including pinned/crossing/recovery controls,
before considering adoption.

The first complete native-filter Go run finishes with 1,210 passing test leaves
and one failing leaf. `TestReaderHeaderOnlyOpenSkipsEntryArrayAndKeepsIndexedUnique`
finds five global offsets loaded by an unchanged-header unique query. The new
offset-population freshness check incorrectly treats a metadata-only nil slice
as a stale full reader. Preserve nil for metadata-only opens; use a nonnil empty
slice for loaded empty populations, without adding Reader storage/allocation.
Qualify the freshness predicate with that distinction. Also include actual
offset-population length changes in Refresh's result: a fresh filter group can
publish new header counters before the global vector is rebuilt, so comparing
only header/size/status can falsely report no change and skip a new raw row.
Exercise initially empty/nonempty readers, explicit refresh, raw continuation,
and successful/failed group builds. This repairs consequences of the new native
group visibility path; paused SOW-0045's unrelated raw unchanged-header repair
contract remains outside this chunk.

Focused source inspection additionally finds selected offsets already present
in the cached global vector receive only a 64-byte ENTRY header read. Unlike
Rust row loading, that does not establish the complete ENTRY body's bounds or
item alignment after live truncation/modification. Validate the selected
header's scalar alignment, arena extent and physical extent before committing
every filtered result, including ordinary global and orphan offsets. Reuse
the already-read scalars; do not reread item bodies or decode DATA. Preserve
arena errors versus recoverable physical tail loss and the cold zero-header
classification. Add public controls for retained-header/partial-body tail loss
and malformed item alignment after open in both layouts/backends.

Further native-path inspection identifies two related recovery consequences.
Snapshot group/recovery refresh currently clamps to its last observed physical
size, permanently narrowing the original bound after shrink. Preserve a separate
immutable size captured at open, alongside current physical size. Clamp repair
visibility to the original cap and original declared population, so restoring
bytes within that cap can work while later appended entries remain excluded.
This adds one scalar without increasing the current Reader allocation class;
carry it into the private compact-storage candidate and remeasure that class.
The original-size field is immutable and does not need refresh rollback state.

A realtime seek can load a stale global vector and discover recovery after the
filter expression was built. Refresh marks the expression stale, but the same
step currently evaluates the old leaves. Rebuild the retained DATA leaves on
that observed transition before using them, preserving lazy qualification and
the pending realtime bound. Add first-call EOF controls for a newly published
matching DATA whose ENTRY was not completed. These fixes implement the existing
repair/first-call parity contract; they do not resolve the separate pending
cursor-nearestness decision.

Ordinary movement errors must not reread DATA populations: marking every error
as expression-stale can widen an installed healthy filter after an append,
contrary to Rust's captured-population behavior. Invalidate only owned walk/node
cache state on those retries, preserving DATA identity, inline/head and captured
counts; reserve DATA requalification for actual recovery transitions. Commit
chain-guard changes only after successful node reads. Cover transient selected
ENTRY failure followed by append/retry in both backends.

Recovery can also change midway through a multi-leaf refresh. An earlier leaf
can be clipped to the old prefix while a later missing DATA header discovers a
complete repair. Repeat whole-expression requalification when its observed
recovery status/tail changes during the pass; install only a pass with one
consistent view. Keep intermediate children private and failures retryable.
This is a cold-path consistency loop, with no additional healthy per-row work.
Add a deterministic truncation/restore AND control and retain its failed
counterfactual evidence before accepting the fix.

Snapshot reference clarification: the preceding analysis incorrectly inferred a
frozen global declared population from Rust's public header snapshot. Modern
core header mappings and new exact DATA builders can see publications inside
the original byte extent. The user's Rust-parity instruction selects that
existing byte-bound contract, not a stronger immutable logical snapshot. Remove
the unsupported population claim from consumer docs and use fresh internal
header counters for native group/recovery refresh while retaining the original
byte cap. Already installed healthy DATA populations remain fixed as above.
Use the same bounded native vector refresh for filtered realtime/orphan
normalization under snapshot bounds; ordinary snapshot raw refresh policy is
otherwise unchanged.

Rust's cold snapshot prefix discovery also used only its original byte cap,
even after an observed post-open failure caused by physical shrink. Compute its
discovery extent as min(original cap, current physical length), preserving the
immutable cap itself. This adds no healthy per-row work and allows the same
observed prefix/repair policy in both languages. Test actual shrink/restore
after open in addition to at-open truncated fixtures; a zero status remains a
lazy observation and does not promise detection of every external modification.

The same-failure search finds Rust also refreshes Boolean children in place
without checking whether a later leaf changed the recovery view. Apply the
same whole-view stability check to both a compiled expression and the pending
builder's top-level groups. Keep recursive leaf refresh internal to one pass,
retain DATA identities across retry, and repeat only when status/tail metadata
changed. This repairs the existing observed-repair contract in both languages;
it does not alter the pending cursor-nearestness policy or healthy traversal.

The stable-pass loop also needs persistent retry state: a DATA refresh can fail
after earlier branches were replaced and after ENTRY discovery clears recovery.
The next build would otherwise skip requalification and retain clipped branches.
Add a deterministic pending-builder counterfactual with two rows: compile
COMMON and TARGET while healthy, truncate before TARGET, attempt a recovered
build, restore ENTRY bytes while leaving TARGET's header zero, retry, then repair
that header. The final AND must find the second row. Cover an installed
expression's analogous failed refresh. Preserve pending state across errors and
clear it only after a complete stable pass. Inspect private cursor padding for
flags before increasing public/private object sizes or adding allocations;
measure healthy filter overhead before acceptance. No public cursor fields or
FilterExpr variant shapes may change.

The private array-opening candidate finishes all 27 controls with five
regressions, eleven improvements and eleven inconclusive results. Reader
allocation grows from 640 to 704 bytes and latency regresses, so this
candidate is not accepted. Test a separate storage-only variant: put the
immutable initial byte cap in the accessor that observes it, funding that scalar
by narrowing its two normalized 0/1/2 access-mode caches to bytes. Replace the
Reader's duplicate staged-file cleanup path with a Boolean and derive the same
name from the owned file at close. Ordinary files remain unstaged. Preserve
staged cleanup/close behavior, snapshot bounds, mode statistics, allocation
classes and 386 high-offset small-read support; validate source-derived
equivalence and all construction/header controls before adoption. No runtime
installation, host journal operation or wider design change is involved.

The pending-builder control reproduces false EOF before the repair. The retry
implementation will keep refresh, recovered-population and unfinished-position
flags in existing InlinedCursor padding. A file-local sticky observation flag
avoids walking expression trees on healthy files that have never needed
requalification; damaged/repaired files inspect all leaves so public expression
composition cannot hide a pending subtree. Requalify all builder groups as one
batch, retaining pending flags after any error. Keep backward positioning pending
through fallible tail preparation without rereading DATA counts on a tail-only
retry. Preserve flags through cursor copies. Measure actual sizes and healthy
filter timings; source layout estimates alone are not acceptance evidence.

The storage-cap variant restores measured reader allocation from 704 to 640
bytes, with one allocation per operation. Its 27 construction/header controls
show eleven improvements, sixteen inconclusive results and no detected
regressions; inconclusive remains unaccepted as equivalence. Fifty-three
selected recovery/access/staging/high-offset test nodes pass on each of amd64
and 386. The sparse-file control reads a small range above 2 GiB and preserves
physical-bound versus integer-limit error precedence without allocating that
large range. Integrate the measured internal changes into the native-filter
source for whole-source functional/performance validation; retain every native
filter freshness, selected-body, orphan and snapshot correction. This is a
candidate integration, not final performance acceptance.

Rust's complete core suite now passes all 96 tests. New retry controls cover
pending and installed filters, forward/backward, both layouts, public Boolean
composition, temporary zero DATA population metadata, and a failed backward
preparation followed by append. The latter must keep the captured population
and return the old second row rather than the new third row. These functional
results do not establish the healthy timing or allocation gate.

The same-failure search confirms Go's transactional expression replacement does
not cover a previously compiled pending group: recovery can clear while another
group fails, with no installed expression to mark stale. The new pending-AND
control fails in both layouts/backends before repair. Mark the builder stale on
every successful recovery transition, including before installation, and honor
that flag in the pending build branch. Keep it until the complete build succeeds.
Also retain a known Match's DATA identity when a refreshed population is zero;
initially absent/empty matches keep their existing false semantics. Reuse Go's
transactional tree replacement and existing builder flag, adding no Reader
storage or healthy per-row scans. Add restoration controls for both consequences.

The first Go retry controls failed a fixture precondition: ordinary Refresh
correctly skipped an unchanged healthy header/extent, so they had not yet
established recovery. Those failed attempts are retained, not counted as bug
reproductions. Corrected controls use observable physical shrink. The matched
counterfactual now returns false EOF after repair in all eight layout/backend
leaves (pending groups and known-empty DATA); current source passes all 188
selected nodes. The complete integrated Go suite passes 1,459 nodes, with no
failures. Matched filtering helpers pass 480 small-fixture/full-payload
observations and 180 independent large-corpus golden observations.

Compiler-derived Rust layout controls confirm the retry flags add no size to
InlinedCursor (128 bytes), FilterExpr (136) or JournalCursor (280). Whole SDK
changes do increase JournalFile from 552 to 640 bytes and JournalReader from
608 to 656; these are recorded costs, not allocation/timing acceptance. Public
constructor/filter timing and actual allocation still require measurement.

Consumer documentation now explicitly covers pending-builder retries, facade
handle operator error results, and FlushMatches rewinding all reader surfaces.
The older Go API guide's immutable-state snapshot wording is corrected to the
approved original-byte-extent contract, with healthy filter populations kept
separate from fresh group visibility. No example fence or harness syntax changes.

The documentation blast check finds direct flush-to-head coverage weak or
masked outside Go's file reader. Extend the existing paired public adapter
probe with a clear-after-two-sparse-matches operation. It must then return the
complete unfiltered sequence from its first row, without an explicit seek.
Run it on file/directory SDK and facade surfaces in both layouts/languages.
This validates the approved Rust-parity consequence and does not add SDK APIs.

Integrated timing remains unaccepted. Current Rust has two compact facade-scan
regressions (median ratios 1.0214 and 1.0289). The separate cold-resolve candidate
has four inconclusive results, not equivalence. The integrated Go header matrix
has ten improvements, thirteen inconclusive cases and four regressions; the
private older-source result is not substituted for this integration result.

Accessor inspection finds hardware division repeated to compute native window
bases, including crossing/pinned reads. Test a private integrated candidate
caching the power-of-two condition in existing access-mode padding; use
offset & -windowSize for that case and retain division for arbitrary sizes.
Construction already rejects zero window sizes. Keep public sizes/statistics,
non-power-of-two behavior and overflow/bounds precedence unchanged. Measure
all 27 controls and actual object/allocation sizes, with high-offset/386 tests,
before integrating it. This is internal cost removal for the approved strict
performance gate, not a new access-mode policy.

The Rust footprint control identifies ordinary-reader storage costs beyond the
retry flags: inline recovered-prefix metadata and three-word guards retained by
two iterator options. Test a private combined candidate with the existing
cold-resolve codegen change, the equivalent two-word Brent schedule already
validated in Go, and a private recovery cell backed by a lazily initialized
boxed Cell. Its get/set API returns the same Copy values; never-recovered files
allocate no recovery box, clearing an uninitialized cell allocates nothing,
and later recovery/repair reuses the first box. Keep the active recovery check
on the cell's value, not whether storage was initialized. No unsafe code,
borrowed recovery references, public type fields or behavior changes are needed.
Measure actual sizes, healthy/damaged allocation costs and affected timings;
run the exhaustive guard model and complete core/recovery controls before any
candidate integration. Additional cold allocation is recorded explicitly.

The geometry candidate passes 198 focused nodes on each of amd64 and 386 and
all 264 new independent output observations (192 small/full-payload, 72 large
golden controls). Its header matrix has twelve improvements, fourteen
inconclusive results and one compact empty mmap-open regression, median 1.0111
with interval [1.00046, 1.03132]. All results remain diagnostic. A longer paired
empty-open control is running because the earlier integration flagged the
other layout, making repeatability unresolved.

Bounded disassembly identifies a concrete constructor consequence: baseline's
public OpenFileWithOptions wrapper is inlined into the caller, while the current
extra private wrapper prevents that and adds a 0x40-byte call frame.
configureLayout itself remains inlined, and both policy constructor frames are
0x3c0. Test a separate private geometry candidate having the public function call
openFileWithPolicy(path, opts, true, true) directly. Preserve the metadata-only
and strict private callers. This removes a redundant layer without changing the
approved recovery/load policy; verify actual inlining, controls and timing.

The Rust cold-storage candidate passes all 96 core tests after supplying the
missing private-checkout FSS fixture; the initial fixture failure is retained.
Compiler-derived sizes are JournalFile 568 and JournalReader 640, with posting,
expression and cursor unchanged. Separate allocator-instrumented binaries link
the matching benchmark dependency fingerprints, leaving normal timing binaries
unchanged. On both healthy 100,000-row layouts, open requests five allocations:
924 bytes baseline, 1,012 current inline-recovery source, and 940 candidate.
First scan requests three allocations/736 bytes; warmed scans request zero
allocations/bytes in all three builds. On the 1,004-row recovered prefix,
candidate open requests six allocations/1,162 bytes versus current five/1,154;
the extra lazy box adds one request and eight total requested bytes there.
Both have identical ordered full-payload counters and zero warm allocations.
These counts describe allocation requests, not allocator usable sizes or elapsed
performance. Cold/healthy timing gates remain open.

Current root adapters pass all 552 paired hardening observations against the
full-feature v262 oracle, including the sixteen new flush-to-head observations.
The first shared-run attempt only lost the copied executable bit and is
retained separately; rerunning the same binaries with executable permissions
passes. All sixteen wiki pages validate and all 35 examples compile/run after
the consumer contract corrections.

The longer twenty-pair Go empty-open controls are all inconclusive; allocation
counts/bytes match exactly. Rust cold-storage's four historical compact cases
are also all inconclusive. Neither set establishes equivalence or satisfies the
strict timing gate. Integrate Rust's independently validated cold-storage,
two-word guard and cold-resolve changes for whole-source validation while
retaining every pending-retry flag and current adapter probe. The complete core,
ordered output, footprint and allocator controls justify testing this combined
candidate, not accepting its final performance. Keep all prior measurements and
source/binary identities. No cursor-nearest policy change is included.

The flat Go public opener now actually inlines into its benchmark caller,
removing the extra wrapper frame. Its 198 focused nodes pass on each of amd64
and 386. Integrate that one-line call-path correction and the separately
validated cached window geometry for complete-source validation. Only the
power-of-two arithmetic path changes; arbitrary window sizes retain division.
Source comparison confirms all other Reader/accessor behavior is identical.
Final timings, allocation classes and wider suites remain acceptance gates.

A bounded Rust disassembly comparison finds that the current FileReader healthy
step makes a per-row call to step_inner, with six saved registers and a 0x158-byte
stack reservation. The baseline inlined this logic into step_valid. Test one
private candidate adding an inline hint to step_inner while preserving the cold
recovery and missing-cursor helpers. Verify actual code generation and ordered
outputs before measuring; no filter policy, retry semantics or public API changes.

The subsequent history check confirms plain/forced step_inner hints were already
measured earlier. The newly started combined-source build is retained for
code-generation inspection only; do not repeat its timing absent a distinct
actual change. Two separate, narrower candidates address demonstrated work:
OffsetArrayObject::len/is_empty can inline their capacity checks so callers do
not recompute the same kind/alignment/length checks after an outlined call;
CurrentRowView::clear_pins can keep its no-active-pins branch in a tiny inline
wrapper and put the existing fallible unpin work in a noinline helper. The
current false path saves seven registers before checking its Boolean, and row
advance/load can call it twice. Preserve every validation, error and row-pin
lifetime; active unpin is ordinary facade work and must not be marked cold.
Build independent candidates from the validated cold-storage source, inspect
actual code generation, then measure only concrete differences. These internal
cost removals serve the existing strict performance requirement.

The plain inline hint leaves the same call boundaries, function sizes and Result
staging in the combined candidate; do not time it again. The array-length variant
recompiled successfully, but the following clear-pins build incorrectly reused
that artifact (identical executable hash, no SDK compilation in its log). That
output is excluded as a clear-pins candidate. The shared Cargo target can reuse
relocated source fingerprints when independently prepared variants have older
mtimes than the preceding build. Prepare a new clear-pins source after that
build, apply its change then, retain a new manifest and require the log to show
the expected source compilation plus distinct code-generation evidence. Existing
cold-storage layout/allocator/core evidence is independently tied to its actual
compiled type sizes and behavior; do not infer a general validation failure.

### Completing Matched SDK Performance Coverage - 2026-10-16

The read-only coverage audit finds ReaderBoundsSnapshot scans do not measure
IndexedSnapshot capture/visitors, and the existing Rust example omits payloads
visited by Go's snapshot benchmark. Add matched file-only helper modes in both
languages: indexed-open, indexed-exact, indexed-field and indexed-all, with full
payload digest companions for the three visitors; verify-index and verify-file
count successful complete operations. Reuse one existing --match FIELD=value
for snapshot declarations and selection, and catalog-repetitions for complete
open/operate/close lifecycles. Snapshot callbacks add realtime and every payload.
Report capture counts and successful opens separately from rows visited. Reject
irrelevant live/backward labels; verifier output identifies its public SDK
defaults and rejects custom access/window labels it cannot honor.

Keep fixture creation, independent native-byte/full-payload preflight and process
startup outside the timer; include SDK open, traversal/verification and cleanup.
Use bounded multi-array fixtures for compatibility verification to avoid its
existing repeated-posting quadratic cost. Run plain/codec layouts and single
native Zstd frames with repeated DATA reuse; the existing zstd-known hardening
fixture includes skippable padding and cannot certify the native single-frame
path. Whole-file single-frame staging can use existing SDK/facade modes against
the original decompressed population. Old Rust's truncated concatenated output
is invalid as a performance baseline. Keyed verification's HMAC branch is
unchanged and shares the affected graph decoder, so retain sealed keyed
correctness and measure verify-file/index without adding a key-handling mode.

Copy the exact helper changes into fresh baseline and candidate sources before
final timing; qualify all new counters independently. Extend the existing
comparison runner with opt-in auxiliary cases, preserving its default matrix.
Update benchmark documentation with effective options and timer boundaries.
The same audit requires current writer raw/structured mixed-cardinality cases,
a repeated-compressed-DATA writer lookup control and reader refresh after
publication; immutable live bounds alone do not establish growth-branch cost.
These are validation consequences of existing shared lookup/recovery changes,
not new SDK APIs or unrelated optimization scope. Root remains the only writer.

The distinct array-length hint removes one call and array staging but retains
some repeated alignment/kind checks; four compact scan controls remain
inconclusive. Keep it private rather than adopting an unneeded extra change.
The fresh pin-clearing split puts false checks inline in advance, entry loading,
DATA restart and DATA clear; only the active path calls the unchanged fallible
unpin operation. Its four diagnostic scan comparisons have one improvement and
three inconclusive results (median ratios 0.98533, 0.98174, 0.98323, 0.99342).
Both distinct variants pass 264 independent ordered-output observations.
Integrate only the pin-clearing split for whole-source/floor validation and the
complete reader matrix. This is not a passed final timing gate.

Go's flat-opener 27-control matrix has eight improvements, sixteen inconclusive
results and three regressions: compact empty ReadAt open, ReadAt eviction and
ReadAt pinned headers. Keep every sample and allocation metric. The latter two
also flagged earlier integrated source and require targeted code-generation
investigation with the complete current validation preserved; removing the
wrapper did not establish the performance contract.

Complete source validation at this checkpoint passes Rust's 602 workspace tests
(five existing ignored), Go's 1,459 amd64 nodes, 1,460 386 nodes and 1,459 race
nodes. Go 1.26.2 passes 326 focused nodes on each architecture; Windows, macOS and
FreeBSD test builds pass. Runtime/compiler floors are unchanged. The new Rust
pin split and benchmark-helper coverage still require their own final checks.

The Go header comparison measures ReadAt eviction at 1,047 to 1,061 ns/op and
pinned headers at 789.25 to 809.20 ns/op; generic header parsing retains a nil
error result join in the valid path. Test a private scalar candidate: explicitly
reject buffers shorter than 16 with errInvalidJournal, copy type and size, and
keep the same type, minimum size, shape, arena and physical error ordering.
Generic parsing performs only that length check and those scalar extractions;
its unused flag has no effect here. After size >=64, size&(itemSize-1) is equal
to (size-64)&(itemSize-1), since both supported item sizes 4 and 16 divide 64.
Preserve offset-before-subtraction checks and cold zero-header classification.
Validate both architectures and bounded code generation, then measure affected
controls; no validation is removed and borrowed bytes never cross another read.

The complete Rust 1.91 compiler-floor workspace now passes, including the
pin-clearing integration and new auxiliary helper compilation. The exact test
counts and ignored cases are retained with the complete log.

The Go ENTRY scalar candidate's complete 27-control matrix has fifteen
improvements and twelve inconclusive results, with no flagged regressions.
Its validator shrinks from 107 to 94 instructions and 372 to 330 text bytes;
the same 32-byte frame and Reader/offset spills remain. Read-only adjudication
confirms all short/type/zero/shape/arena/physical outcomes and borrowed lifetimes.
Integrate only this equivalent validator for combined-source measurement;
twelve inconclusive cases do not establish the complete performance gate.

The auxiliary counter preflight passes 72 initial observations, then 128 after
adding the timed Go ReadAt and Rust whole-file snapshot strategies. All snapshot
visitors have sampled and full-byte counterparts, capture counts are independent,
and both verifiers use their documented defaults. Stock v262 verifies each
bounded 1,005-row mixed-cardinality source before preflight. All four matched
SDK/helper binaries pass; original hashes, complete stdout and process records
are retained. Compressed/staged fixtures still require their own full-byte
qualification against the declared native original before timing.

The complete matched Rust matrix finds additional metadata-stepping and
fresh-filter construction slowdowns; final acceptance remains blocked. Bounded
actual disassembly confirms a healthy per-row nested step_inner call with six
saved registers and 0x158 stack bytes, while baseline inlined that resolution.
The prior forced-inline experiment moved the call into a downstream resolver,
so an attribute alone is not a new remedy.

Prepare a distinct private source candidate: public JournalCursor::step resolves
its location directly and matches the Result. Only Ok(Some(location)) commits
the location and returns true; EOF and the unchanged error enter the existing
cold step_recovery. Keep the latter's single step_inner retry, including its
second-error/EOF behavior. Do not use an initial question-mark propagation that
would bypass recovery. Resolvers mutate only filter/array state, and the public
row loader retains its existing rollback. The short duplicated resolution
sequence keeps the healthy body at its public inline boundary; verify the entire
downstream call graph before any timing. This changes no nearest-match policy.
Build only after the current serialized timing sequence, then run focused/core
controls and independent output preflight before measurement.

The private per-file storage audit finds no dead field that can remove Rust's
sixteen extra healthy allocation-request bytes without changing live state or
introducing a wider storage design. All recovery latches, historical-header,
file ownership, publication and statistics fields have consumers. Retain that
measured footprint cost explicitly; do not delete a live field to improve sizes.

The complete reader matrix also flags fresh facade filter construction in the
regular layout (paired median ratios 1.07846 and 1.11349). Read-only bounded
disassembly finds new outlined closure/qualification calls and repeated
128-byte cursor staging; no added healthy syscall or per-build vector allocation
was found. Test a separate private healthy-constructor candidate alongside the
direct-step candidate. Preserve the public hidden constructor and consuming
qualification signatures. A private unit-returning qualifier mutates only owned
local cursors, returns immediately without recovery, and calls an outlined
recovery helper otherwise. Preserve provenance before empty-prefix clipping,
captured posting counts, pending flags and restoration of the inline position.
Drop borrowed DATA before qualification or any recovery read. The constructor's
error helper must preserve the original error when recovery is unchanged and
perform exactly one nonrecursive retry when recovery changes. Build after the
serialized measurements, inspect the complete actual call path, and validate
recovery/ordered outputs before paired timing. This is an implementation of the
existing accepted retry contract, with no change to the pending cursor policy.

The remaining compressed lookup control uses private matched writer helpers,
not a relabeled standard workload. Name it
`mixed-cardinality-long-reuse-32-fields`: repeat the 14-byte HOST_CLASS value
128 times, retain 32 fields and the 512-byte compression threshold, and report
2,606 logical bytes per row (817 + 127*14 + 11 profile-name bytes). Keep public
SDK options, append APIs, hash sizing and publication cadence identical within
each baseline/candidate pair. Build plain and Zstandard variants. Independently
require exactly one compressed DATA object, standard single-frame bytes without
padding, content size 1,803, complete decompressed byte identity and references
from every ENTRY. Full-byte reader checks precede timing for every mapping.
The standalone comparator must retain append-only timer boundaries and reject
incorrect metadata; existing standard workload checks remain intact.

The bounded directory/OpenFiles control uses two separately generated 1,005-row
original-profile files, with constructor-supplied distinct synthetic file IDs.
The second file starts at sequence 1,006, realtime 1700000000502500 and monotonic
50050250, retaining 500/50 microsecond increments and common machine/boot/seqnum
identity. Freeze native bytes and validate both files plus complete 2,010-row
forward/reverse results before timing. Do not patch file IDs in completed keyed
files, and do not describe two identical input files as multi-file coverage.

The complete matched Rust file matrix finishes all 268 comparisons with
unchanged semantic counters: 121 improvements, 134 inconclusive results and 13
flagged regressions. Facade forward stepping with whole-file mappings flags
both layouts and both bounds; fresh facade filter construction also flags both
layouts. Preserve the entire run. The private direct-step/constructor candidates
are diagnostic repairs to investigate those costs, not accepted replacements.
The combined SOW audit is clean at this in-progress checkpoint; that does not
satisfy the remaining acceptance or review gates.

Retained-reader publication controls will use a frozen synthetic Go publisher
and distinct baseline/candidate raw and filtered reader handles opened before
publication. The writer commits one changed batch outside reader timing and
waits for acknowledgement. Measure Go Refresh/SeekHead/drain and Rust
seek-head/drain under their actual public APIs; Rust has no FileReader Refresh
method. Fresh-filter cases flush/add/seek/drain without an earlier Refresh on
that handle. Explicit seeking normalizes pre-existing flush-position behavior.
Run unchanged physical allocation and a committed uncompressed 9 MiB DATA
growth case; assert physical/declared geometry and independent ordered digests.
Use a new publication per sample, not repeated unchanged polling. New recovery
status and old Go's incomplete reload outcomes cannot form a successful paired
baseline; keep repair costs as candidate diagnostics with their actual outputs.

### Failed-Candidate Positioning Evidence - 2026-10-16

Read-only adjudication finds a second position-policy difference. Rust
`rust/src/journal/src/lib.rs:635-656` advances the core reader before loading the
selected ENTRY. `reader.rs:135-136` leaves that candidate installed when recovery
does not apply. The new type check reports `InvalidObjectType`; that error is
ineligible for tail recovery. After a malformed second matching ENTRY fails,
restoring its bytes can therefore let getters expose the second row without a
successful iteration result, and a subsequent advance can skip it. This commit
ordering predates this SOW. Recovery-eligible anchor restoration is separate.

Go's native implementation commits at
`go/journal/reader_filter_step.go:63-68` only after the selected header/body
passes. It retains the earlier successful position on that error. The user's
complete parity requirement makes a common policy necessary. Question 2 offers
last-successful state in both SDKs or the existing Rust failed-candidate state
in both. The former is recommended long-term-best; the latter minimizes Rust
change but preserves the unreturned-row/skip behavior. These source-grounded
sequences still need paired executable controls after the serialized timing
batch. No error-position policy change has been implemented.

The retained baseline Go source confirms this is a consequence of the native
port: its `Step` called `Next` before `GetEntry`, and returned payload/header
errors without restoring `entryIndex`; `StepBack` did the same. Its existing
ENTRY header reader already rejected the wrong type. Thus both old Go and Rust
retain the failed candidate, whereas new Go retains the successful row. This
strengthens the need to disposition the port's changed error position, without
assuming permission to change Rust's existing behavior.

The same read-only scan found a pre-existing directory realtime-seek/getter
invalidation gap, independently of filtering. Track it in paused SOW-0045's
related healthy-position section; no runtime change or new prerequisite is
included here. Orphan getter routing, raw normalization, successful cursor/time
positioning and retained filtered EOF yielded no further concrete source-level
defect in this bounded scan. Executable validation remains distinct from that
source-only finding.

### Go Reader Timing Cost Investigation - 2026-10-16

The initial complete Go file comparisons flag several allocated-entry scans
around 1-2 percent and high-cardinality stateful unique enumeration around
4 percent. Actual matched binaries retain identical `readEntryAt` instructions,
calls and 712-byte frame, with the same 128-byte Entry allocation. Reader sizes
624/616 both occupy the same 640-byte amd64 heap class. No per-row allocation
increase was found in those paths.

The new unfiltered Step/StepBack call `errors.Is` even when raw movement returns
nil; baseline branches on nil first. Prepare a private candidate restoring the
healthy early return while retaining wrapped EOF/general error classification
and native filter dispatch. Measure this separately before combining changes.
Two further equivalent private controls may restore the valid-global-index
GetEntry path with a cold orphan/error fallback (28 to 50 instructions currently)
and outline only stateful unique cycle-error clearing/formatting (272 to 288
stack bytes currently). Retain the complete Brent guard and all valid unordered
chain behavior; do not outline the whole compatibility traversal as a shortcut.
Source/codegen observations identify actual work, not its causal timing share.
Validate public orphan/error/unique contracts and both architectures before
adoption; use matched helpers, output checks and affected timing controls.

For the pure unknown-size single-frame DATA control, prepare a separate private
fixture-generator fork of the frozen Go writer. Omit only its post-encoding
content-size-header insertion; retain its stream encoder, public writer path,
threshold, payloads, identities and references. This fork is a generator, never
a timed baseline/candidate SDK. Independently require one standard frame with
UNKNOWN content size, complete frame extent and exact 1,803-byte decompression,
then require stock verification and matching full-byte reader results before
using it as a valid-input compatibility timing control.

### Remaining-Control Harness Review - 2026-10-16

A read-only review found three defects in the private remaining-control
drivers before their results could be used as evidence. Native sequence-header
qualification reversed head/tail offsets; the correct offsets are 168/160.
The shared writer command builder rejects the intentionally private long-reuse
workload before execution, so its private comparator now constructs only that
workload's equivalent arguments directly. The retained-reader coordinator's
blocking response read bypassed its later timeout, and cleanup could stop at
the first unresponsive child. Response reception is now bounded, handles
partial/multiple lines per child, and cleanup visits every captured process
handle while retaining failures without signaling processes.

Static re-review confirms those repairs; syntax checks pass. The fixture
preparer was already loaded in a waiting interpreter, so its first attempt
retains the prior offset check. Preserve that diagnostic failure and run the
corrected preparer into a new output directory after it exits. Release the
queued dependent controls only through the corrected attempt's independently
qualified manifest. These harness repairs change no SDK source or accepted
positioning policy, and prepared controls are not validation results.

### Filtered Realtime-Seek Cost And Policy - 2026-10-16

Completed regular-layout Go comparisons show approximately 1.05 seconds versus
0.048 seconds per 1,000 filtered realtime seeks, about twenty-two times slower,
across SDK/facade, live/snapshot and both directions. The compact cases seen so
far reproduce that cost. The previous Go path binary-searched the global ENTRY
offsets and checked rows from that point. The native port binary-searches the
global offsets, then linearly walks the matching DATA posting list from its
head/tail to the target (`go/journal/reader_filter_posting.go:195-260`).

Rust currently has the same linear filtered alignment via
`InlinedCursor::next_until/previous_until` (`offset_array.rs:869-923`), despite
having a separate directed binary partition primitive at `:925`. This is not
evidence that Rust already uses binary posting search for ordinary filtering.
Switching explicit time seeks to binary posting search in both SDKs is a
behavior decision: a skipped malformed slot need not be read, and a probed slot
past an early answer can fail sooner. User question 3 explains this change in
error discovery, its performance benefit and the linear alternative. Keep
ordinary sequential stepping unchanged and do not implement the dependent
search policy before the user replies.

A separate read-only ownership audit confirms that both Rust path openers copy
an already-absolute path into a temporary PathBuf, then copy it again into the
retained repository path string. Test a private equivalent candidate that
borrows absolute inputs through `File::from_raw_path`; relative inputs retain
the existing current-directory join and error ordering. Both public signatures
and the retained path bytes stay unchanged. This removes a redundant temporary
allocation on every absolute-path variant, including historical/recovered
files, without moving cost to another path. For the recorded 103-byte fixture
path, five current requests total 940 bytes; eliminating that temporary predicts
four/837, which still needs executable measurement. Retained requested storage
remains sixteen bytes above baseline (837 versus 821); cumulative allocation
requests, retained/peak storage and allocator usable sizes are different
quantities. No broader storage redesign or public behavior change is included.

The complete matched Go file matrix finishes all 220 comparisons after
3,797.46 seconds, with unchanged semantic counters: 164 improvements,
35 inconclusive results and 21 regressions. Sixteen flagged cases are explicit
filtered realtime seeking across both layouts, APIs, bounds and directions.
Four allocated-entry scans and one high-cardinality stateful unique case form
the remaining five flags. This distinguishes the repeatable linear-seek cost
from the smaller call-path/code-generation investigations without accepting or
offsetting either group. The complete Rust/Go file matrix therefore contains
488 comparisons: 285 improvements, 169 inconclusive results and 34 flags.
None of those aggregates is a performance acceptance verdict.

After the current serialized measurements and private candidate validation,
capture actual generated call paths and bounded Go CPU profiles for entry
reads, high-cardinality stateful unique enumeration and explicit filtered time
seeks. Use frozen matched helpers and verify counters; keep allocator/profile
instrumentation out of ordinary timing binaries. The absolute-path candidate's
checks must also assert unchanged open reallocation count and first-scan
allocation cost on recovered inputs, not only healthy inputs.

The bounded plain auxiliary matrix completes forty comparisons in 121.10
seconds: thirteen improvements, twenty-six inconclusive results and one
regular-layout Go VerifyFile flag (paired median ratio 1.04389, interval
[1.00706, 1.05715], approximately 0.178 versus 0.182 seconds). Include that
verification path in the bounded Go profiles; it is not waived as noise.
Ten direction-specific standard before/after reports render successfully.
The first combined-direction report request was rejected by the existing
report key contract; its failed attempt is retained, and the supported
direction-specific summaries are used without changing reporting code.

Both private Rust reader candidates and all nine matched writer/generator
controls compile successfully. The loaded first fixture attempt fails exactly
at its stale reversed sequence-offset check; all eighteen preceding generator
and stock-verification subprocesses return zero. A corrected new-output attempt
then qualifies all twenty fixtures, including exact decompression and logical
row equality, native byte identity within each language/codec pair, distinct
multi-file IDs, disjoint ordered sequence/time ranges and stock verification.
Its qualified manifest releases the queued probe builds. These are fixture and
build results only, not SDK performance acceptance.

The executable failed-candidate probe passes all twelve observations (old Go,
new Go and Rust; regular/compact; forward/backward), with fixture bytes fully
restored after each sample. After repair, old Go/Rust getters expose the failed
middle match and advancing skips it; new Go getters retain the prior successful
match and advancing retries the middle match. This confirms user question 2's
source analysis. No common policy has been implemented while the reply is pending.

The first complete private Go journal-package validation fails eight top-level
tests, all with concrete missing-fixture/helper errors: historical Zstandard
journals, shared FSS vectors and the libsystemd binary-field helper source were
absent from the relocated source parent. Preserve that attempt; it is not an
SDK correctness verdict. Copy the exact repository fixture/FSS/binary/live
helper trees, verify their hashes, and rerun both architectures plus the queued
controls into a new validation directory. The dependent absolute-path build
stops before compilation on the failed prerequisite and is queued anew against
the repaired validation. Lesson: a private source copy needs the tests' relative
fixture/helper closure, not just language source files; Rust's FSS fixture alone
does not satisfy Go's full package closure.

Read-only inspection of frozen actual code justifies focused timing of all
five proposed cost removals. Go Step/StepBack retain their 48-byte frame but
skip errors.Is and its spills/sentinel loads on successful movement. GetEntry
shrinks from fifty to twenty-six instructions with its sixteen-byte frame
unchanged. Stateful unique enumeration drops from 201 to 158 instructions and
288 to 272 frame bytes; guard arithmetic and payload work remain unchanged.
Rust direct-step retains one outlined resolver call and unchanged posting
helpers, but removes the intermediate successful `Result<bool>` transport;
resolver frames shrink while the outer frame grows sixteen bytes. Rust's
constructor candidate removes two healthy call layers, while its two 128-byte
cursor copies remain. Do not describe those copies as eliminated or claim
timing wins from instruction/frame counts alone.

The repaired Go package passes 1,380 amd64 and 1,381 386 test nodes. Rust's
ninety-six core tests also pass. The first Rust core command mistakenly used
the dependency alias `journal-core`; the manifest's package selector is
`systemd-journal-sdk-core`. Correct that harness selector without rerunning
successful Go checks. Public recovery validation then passes twelve cases and
fails only on a missing relocated historical fixture directory; copying and
hash-verifying the complete fixture tree produces thirteen passing public
cases. Preserve all setup failures and reuse only genuinely passed source/binary
results; fixture additions do not modify SDK code.

The first unknown-size generator does not actually produce an unknown-size
frame: independent libzstd qualification reports content size 1,803. Merely
omitting the writer's header-insertion helper is insufficient because the Go
encoder optimizes a short buffered stream on close. Reject that mislabeled
fixture before any reader timing. Prepare a new generator-only source and
binary that writes the payload in two portions and explicitly flushes between
them, forcing the encoder's streaming header before final length is known.
Keep normal writer hashing, offsets and object bookkeeping; do not patch a
completed journal's object layout. Independently require UNKNOWN content size,
one complete standard frame, full decompression, all ENTRY references and stock
verification in both layouts. This remains a private fixture generator, never
a timed SDK baseline/candidate.

An existing local Valgrind 3.27.1 installation provides software call/instruction
profiling despite unavailable hardware perf events. After retained-reader timing
finishes, profile the frozen Rust baseline/current/direct-step/direct-builder
helpers serially on core offsets, facade stepping, payload visits and fresh
filter construction. Disable remote symbol lookup and keep all outputs/caches
inside the private task directory. Validate semantic counters. Whole-process
software instruction/branch simulation and per-function call costs are diagnostic
evidence, not native elapsed-performance or hardware-event measurements.

The explicit-streaming generator and qualification succeed in both layouts:
each compressed DATA object holds one 59-byte standard UNKNOWN-size frame,
decompresses to the exact 1,803-byte payload and is referenced by all 1,005
ENTRY objects. Stock v262 verification passes. The failed known-size attempt
remains intact; only the newly qualified paths enter reader controls.
Both Rust candidates also pass all 192 small full-byte filtering observations
and 72 large independent-golden observations before diagnostic timing.

The post-matrix checkpoint audit again reports complete-and-clean SOW
initialization, with SDK-0159 solely in progress and all three user behavior
choices retained. That is a framework checkpoint, not completed acceptance.
Extract the private-validation setup lesson into the journal compatibility
skill: preserve the tested source's relative fixture/helper closure, verify
copied bytes, and select Cargo packages from their manifest names rather than
folder names or dependency aliases. This changes the validation method only.

### Retained Readers And Equivalent Cost Controls - 2026-10-16

The retained-reader publication control passes all 416 observations across
regular/compact layout, same-file/growing-file publication, Go/Rust and
unfiltered/filtered readers. Its sixteen paired timing comparisons record
twelve improvements and four inconclusive results, with no flagged regression.
Publication occurs outside the measured reader interval. Inconclusive timings
remain unproven under the performance contract; correctness agreement alone
does not close the gate.

The remaining independent reader preflight passes 3,264 observations across
known/unknown-size compressed DATA, whole-file Zstandard, directory/OpenFiles,
both layouts and the four frozen baseline/candidate helpers. Full-byte native
goldens and strict verification qualify those inputs before native timing.
These passed preflights do not establish the unexecuted timing results.

The private Rust absolute-path borrowing control passes all 42 allocation and
path-equivalence checks. On the recorded healthy inputs, open requests change
from the original SDK's five allocations/924 bytes, through the current
five/940, to four/837; first-scan and warm-scan costs remain unchanged. Recovered
open changes from current six/1,162 to five/990 on the longer recorded path.
Retained recovery storage still adds sixteen bytes per file; fewer cumulative
allocation requests do not prove reduced retained or peak memory. Relative and
absolute paths, both reader APIs, bounds and mmap strategies preserve full-byte
results. Native timing is still required before adopting this private control.

All twelve Go CPU profiles and sixteen Rust software call/instruction profiles
complete with independently checked semantic counters. Rust direct stepping
removes about twenty-three simulated instructions per row while its underlying
offset and ENTRY operations remain unchanged. Direct filter construction
removes two call layers and about thirty-one instructions per build; the two
128-byte cursor copies remain. Those profiles justify focused native timings
and are not elapsed-performance evidence.

The first focused Go successful-step comparison finishes eight cases: two
improvements and six inconclusive results, with identical counters and no
flagged regression. The serialized getter, unique, Rust step and Rust builder
comparisons continue. None of these equivalent private candidates has yet
been adopted into the root SDK. The three pending user behavior choices remain
unchanged and no dependent policy has been implemented.

The in-progress getter comparison flags four facade-stepping cases in both
layouts/directions at approximately ten to thirteen percent, although that
measured facade path calls the realtime getter rather than GetEntry. Keep the
candidate unadopted and inspect normalized generated code before attributing
the cause. A smaller GetEntry body is not sufficient acceptance evidence.

Queue the already prepared matched writer controls only after all five hot-path
timing stages finish. Then compile a separate non-instrumented Rust open/header/
drop probe against fingerprint-matched original and absolute-borrow libraries;
check the borrowed library hash against its validated build. Compare both
layouts, small/large healthy inputs, core/SDK, live/snapshot, both mmap strategies
and absolute/relative paths with ten alternating measured pairs and two warmups.
Keep header counters identical, record process resources, and isolate timings
from build/profiling/test work. Relative paths are an explicit unchanged-path
control. These queued measurements do not depend on pending filter policies.

The five focused hot-path controls finish all ninety-four comparisons with
matching counters: ten improvements, seventy inconclusive results and fourteen
flags. Getter splitting flags four facade cases; Rust direct stepping flags
four compact cases; direct construction flags six restart cases. Keep those
candidates out of the root SDK. Go successful-step and unique cycle-error
outlining have no flags in their bounded deltas, but remain unaccepted as full
SDK performance equivalence. Fewer instructions do not override native timing.

The unique-profile/codegen investigation identifies avoidable work that both
baseline and current perform: stateful unique traversal needs object metadata
and next-FIELD offset, but the generic DATA parser loads five unused scalars
and transports/copies the complete 64-byte result through multiple frames.
Prepare a private unique-specific 24-byte result control without the rejected
getter split, and compare it against the generic accessor and original SDK.
Preserve the exact 64-byte access request in both layouts, accessor/short-buffer
errors, DATA type and declared-size thresholds (64 regular, 72 compact), flags,
next link, borrowed-byte lifetime and unique state transitions. Add differential
accessor checks including short buffers and injected errors, then run existing
cycle/restart/compression tests and both Go architectures before timing. Do not
add alignment, arena, posting or zero-tail checks on this existing compatibility
path. The unused-work finding is not proof of the earlier four-percent flag's
cause, and the candidate must pass actual timing before root integration.

The read-only getter follow-up compares forty-one generated functions. All
forty outside GetEntry have the same normalized operations, calls, frames and
spills; recursive source comparison confirms only reader_entry.go changed.
Some unchanged functions and interface tables move. In particular, the two
iterator-reset method slots move from sharing one 64-byte block to straddling
a boundary. This establishes a layout difference, not causation; preserve the
four adverse native measurements and the candidate's rejected status.

The matched writer matrix finishes all thirty-two comparisons and 768 retained
observations, with native byte identity within each language and stock-verified
validation samples. It records eight improvements, twenty-two inconclusive
results and two flags: Go regular/plain/per-entry-publication structured writes
at ratio 1.01960, interval [1.00269, 1.05581], and Rust compact/long-Zstandard/
no-intermediate-publication structured writes at 1.02943, [1.00901, 1.06592].
Both remain open. Investigate changed shared validation/chain dependencies;
do not weaken checks or infer causation from a short timing difference.

Read-only pre-execution review catches a private differential-test setup error:
synthetic Go Readers must configure their cached layout after setting compact
flags. Fix both before tests run, so the checks exercise actual 64/72-byte
declared-size thresholds. The new accessor itself preserves the intended
contract. Rust open-probe package names, fingerprint linkage, APIs and fixture
paths also check out. Qualify its header observations independently from native
header fields: both small inputs declare five entries/tail sequence five;
both large inputs declare 100,000/100,000. The probe's digest is therefore zero
on these particular inputs and is not a full-row correctness check; the earlier
independent full-byte preflights provide row validation separately. Retain the
active binaries/driver and postcheck all observations against native metadata.

Read-only writer codegen review finds two bounded equivalent controls to test.
Go findData/findField already branch on a nonzero next-hash link, then repeat a
terminal-zero guard/boolean merge outside that branch. Move only guard calls
under those existing nonzero branches, after depth updates; updateHashChainDepth
keeps its guard before depth increment. A zero next link returns true without
state mutation today, so skipping that redundant call preserves checks, errors
and mutation order. Larger guard frames/initialization remain and the earlier
timing flag is not thereby explained. Preserve acyclic unordered and cyclic
behavior; validate both architectures and byte-identical writer controls.

Rust's new complete-stream fallback is inlined into decompress_zstd_payload,
growing that function from 0xcd6 to 0x67a3 bytes and displacing native success
far into its body. Both baseline and current enter with roughly eleven-KiB
frames and stack probes; the current frame is smaller, so do not describe this
as an added-frame regression. Test inline(never) on the existing private
streaming-with-cap helper only, leaving native-first dispatch, output cleanup,
complete-stream validation and cap unchanged. Keep ordinary unknown-size streams
as a valid performance case; do not mark that helper cold. Build the candidate
from the root SDK, excluding rejected step/builder candidates. Check actual
generated native frames/calls, core/public recovery/compression suites, known
and unknown DATA/full-file compressed reader preflights and matched compressed
writer/read timings before any root integration. Neither control changes a
public contract or depends on pending filter policies.

The repaired unique-header control passes eleven targeted tests on each Go
architecture and all sixty-four compressed unique observations, including
unknown-size DATA, whole-file compression and both access strategies. Its final
combined comparison against the original repaired SDK records twenty
improvements and four inconclusive results across twenty-four cases, with no
flagged slowdown. Intermediate outline/header deltas retain one small flag each
on other paths; they remain diagnostic measurements, not separate shipped
baselines. Apply the validated successful-step early return, unique cycle-error
outline and narrow header together to the root SDK as an equivalent cost
removal. Preserve generic DATA parsing for other consumers, the rejected getter
split, and all pending filter policies. Overall performance acceptance remains
open; the complete original-baseline gate still applies after integration.

All 1,536 Rust open observations pass the independent native-header postcheck,
including unchanged qualified fixture hashes and frozen binary hashes. The
sixty-four comparisons record two improvements and sixty-two inconclusive
results, with no flagged slowdown. This confirms the scoped observation contract
and allocation findings, not elapsed equivalence or a full-row digest claim.

The integrated Go narrow helper's actual amd64 code uses a 24-byte frame versus
the generic wrapper's 168 bytes plus its separate 64-byte parser frame. The
enumerator falls from 272 to 144 bytes. Five unused scalar loads and the full
64-byte header copies disappear. The payload callback and visitation have
unchanged normalized code/allocation calls; static code does not substitute
for measured allocation counts. Exact runtime files match the tested frozen
candidate, and the root accessor contract test is copied unchanged.

Writer-cost candidates pass 105/106 selected Go test nodes on amd64/386,
ninety-six Rust core tests, thirteen public reader-hardening tests and all
ninety-six independent full-payload compressed reader observations. This is
selected Go writer/append/chain coverage, not every writer-related test.
Their native timings are ongoing.

Review catches a disassembly harness defect: objdump demangling with raw nm
symbols exits successfully while emitting empty bodies. Preserve the exact
loaded initial coordinator source and reject those captures as body evidence.
The corrected separate collection uses raw symbols without demangling and
requires an actual symbol label/instructions. All four captures pass. The
outlined Rust native decoder now has ninety-seven static instructions and
reserves forty stack bytes after six pushes, replacing the large inlined
fallback frame/probes. The fallback remains a tail call on native failure.
This is actual codegen evidence; the unchanged caps/cleanup and decoded bytes
are separately tested, and elapsed tradeoffs remain unaccepted until measured.

All thirty-two writer-cost deltas finish with two improvements, twenty-eight
inconclusive results and two flags. Neither private writer change is accepted.
The compressed-reader deltas also flag several cases, including whole-file
inputs whose DATA is uncompressed. Read-only checks confirm all 183 shared Rust
source/Cargo/config files match except the intended inlining annotation; nested
FFI build outputs do not affect these package builds. Both release logs show
fresh SDK/helper compilation, pinned equivalent flags/features and frozen
matching binary hashes. No source/helper/stale-link mismatch explains the flags.

Before further micro-tuning, queue a bounded same-binary timing diagnostic after
the active compressed sequence. Compare each frozen executable against itself
on representative entry/unique/facade/core/verification paths, plus fresh/restart
filters with 250,000 repetitions to lengthen very short samples. Use the same
alternating ten-pair/two-warmup method and independent counters. This measures
the harness/host variation floor; it does not waive adverse candidate results,
introduce a slowdown tolerance or turn inconclusive results into accepted
equivalence. Keep the required zero-slowdown contract and all pending product
choices unchanged.

The first compressed sequence stops on a harness scope error: low-level
JournalFile core modes receive an entire .zst archive and correctly reject its
non-journal magic. The SDK layer stages archives; low-level core modes require
native journal bytes. Preserve the 218 completed reader comparisons (93 known,
93 unknown, 32 archive SDK cases), exclude sixteen unsupported core/archive
combinations across both layouts, and resume the remaining supported cases
without rerunning completed observations. The first same-binary coordinator
correctly refuses the failed prerequisite before launching any diagnostic.

The resumed archive indexed-open control reveals an unbounded workload choice:
1,000 full archive decompressions take 25-26 seconds per sample, roughly ten
minutes per paired case. Prepare twenty opens per sample with the same ten
measured pairs/two warmups and equal before/after counters; native file contents
and the zero-slowdown rule stay unchanged. User question 4 requests permission
to terminate only this task's verified active benchmark child under captured
driver PID 2731853, retain completed results, and rerun unfinished controls.
Server instructions require approval before killing processes; no signal has
been sent and the existing run remains active while permission is pending.

Prepare one isolated Rust reader-data-cursor identity control from root source,
excluding the rejected stepping/builder changes and the unaccepted decoder
outline. Retain the factory Result through the DATA guard scope. Borrow only
its discriminants; Err/None return before reading recovery state. Healthy
Ok(Some(_)) returns the intact Result when recovered_last_entry is None; an
active prefix delegates to the unchanged consuming for_reader. Some(None)
continues to identify an active empty prefix. The outer error/recover/rebuild
wrapper and public layouts/signatures remain unchanged. The recovered branch
performs one extra cached status read but no discovery or I/O. Inspect actual
copy/frame behavior, then validate core/public recovery/filter controls and
longer paired fresh/restart timing before root integration. This is equivalent
constructor cost removal, not a pending position/time-seek policy change.

The regular archive stage finishes naturally with all fifty-three resumed
comparisons, and compact known-size DATA also finishes. The previously named
regular archive driver has exited, so question 4 about terminating its child is
withdrawn; no process was signalled. Before the compact archive stage starts,
preserve the original loaded wrapper and bound future staged indexed-open
controls to twenty opens per sample. This affects only a future benchmark
invocation: completed observations and the currently running native unknown-size
DATA stage keep their loaded scripts. Preserve ten alternating measured pairs,
two warmups, matching counters and separate original/candidate results.

All four resumed compressed-reader stages finish, with every supported case
retained. The identical-binary diagnostic then fails before timing: selecting
only verify-file removes the snapshot declaration required by the independent
auxiliary oracle. This is a private harness setup failure, not an SDK result.
Preserve the failed output. Repair only oracle setup in a separately named
wrapper by deriving goldens from the original complete auxiliary declaration;
retain the selected timed cases and executable arguments unchanged. Run the
repaired diagnostic in fresh output directories before further timed controls.

The complete private Rust decoder-outline comparison covers 542 supported
reader cases: 19 improvements, 410 inconclusive results and 113 adverse flags.
All retained operation counters match; sixteen low-level core/archive cases
are excluded because the low-level API accepts native journal bytes only.
Reject the outline as an elapsed-performance candidate; it has not entered
root source. Reduced static frames/instructions do not overrule these results.
No original-baseline SDK performance blocker is closed by this experiment.

The repaired identical-binary diagnostic retains selected timed cases/arguments
and uses the full original auxiliary declaration only for independent oracle
setup. Read-only review confirms this separation and identical executable
hashes in both sides of the manifests. Queue isolated constructor build/tests
only after both native diagnostic stages finish; no build/profile/test overlaps
the measurements. The timing classifier itself has no equality verdict, so
a confidence interval crossing one is neither a detected regression nor proof
of unchanged performance. No actual user choice specified rejecting every
such interval; that requirement appears in this SOW-authored interpretation.
Do not silently convert it to a pass: present the measurement-method limitation
with concrete identical-binary evidence when available.

Read-only review catches a second private diagnostic setup mistake: paths
passes the 100,000-entry corpus to Go VerifyFile despite the existing bounded
verification rule. verify_graph.go rebuilds repeated posting lists per entry;
this fixture invokes its known quadratic path. Four ordinary regular-layout
same-binary cases finish with overlapping intervals before the first verify
sample. Preserve them. Prepare a corrected four-stage plan: compact ordinary
paths, independent regular/compact verification on qualified 1,005-entry inputs,
and the longer fresh/restart filter controls. The ordinary path stages carry
no auxiliary verification cases. Confirm the bounded headers before execution.
User question 4 is a new approval request for the current diagnostic child
under captured driver PID 2871669, not the naturally completed archive driver.
Stop only the verified SDK verification child using a stable process handle,
then let its timing/coordinator parents collect failure normally. No signal or
replacement workload is authorized by an unanswered question; server process
approval comes from the user-provided conversation instructions.

Read-only spot-check finds no concrete defect in the prepared targeted stop
or bounded continuation: driver/time/SDK ancestry, start times, argv and
executable are rechecked around pidfd acquisition; the continuation requires
the old failed result/stop record, preserves four completed cases by hash and
serializes all stages. Both replacement verification headers declare 1,005
entries. Scripts remain unexecuted while user question 4 awaits approval.
The three behavior choices remain pending; both requested SOWs remain open.

### Execution Blocker Audit - 2026-10-16

The same unanswered user decisions remain across three consecutive goal turns:
reverse-filter nearestness, failed-entry position, explicit filtered time-seek
search/error discovery, and permission to stop the owned oversized diagnostic.
The latest direct process/session inspection at 21:42 UTC confirms the same
verification child is still live with its saved parent/start identity. Four
completed comparisons are retained; replacement controls and constructor
validation have not started. The process is not assumed terminal from missing
output. No permission has arrived, so no signal or dependent policy edit is
allowed. Remaining implementation and serialized acceptance cannot proceed
without those decisions. Mark the thread goal blocked; keep this SOW
in-progress and SOW-0121 paused, preserving all work and the full objective.

### Operational Question Withdrawn - 2026-10-17

At 04:34 UTC, direct session collection shows the diagnostic coordinator
ended after its existing 1,800-second subprocess timeout. The waiting candidate
build also ended before starting validation. Direct process inspection confirms
the captured driver, time wrapper and SDK child are all absent. Operational
question 4 therefore no longer requires a user decision; no new termination
action is needed. The three SDK behavior choices remain pending. Preserve the
failed diagnostic and four completed cases; do not treat its terminal state as
performance acceptance or claim the constructor tests ran.

### Approved Three-Fix Resumption - 2026-10-17

The user explicitly approves all three recommended SDK fixes and asks whether
they were reproduced and whether they are inherited or newly introduced.
Prior evidence includes an executable reverse-anchor probe, twelve paired
error-position observations, and sixteen adverse Go explicit-time comparisons.
Those are discovery controls, not a claim that permanent regression tests pass.

Root-cause and blast-radius update: Rust retains stale posting heads after
filter installation/AND early exhaustion; FileReader commits its core location
before row load; the native Go port uses a linear posting seek after the
global time anchor. Existing Rust anchor and load ordering predate this SOW;
new validation adds observable failure triggers and the Go port changes error
position. Confirm inherited behavior separately against the frozen baseline.

Implementation/validation plan: preserve current source bytes, add permanent
paired layout/direction controls and run them before runtime edits. Correct
nearestness using existing bounded cursor reset/alignment paths; restore the
prior location/cache after nonrecoverable row-load errors without cloning
whole cursors per row. Add explicit-time binary posting search using captured
counts and existing checked node/slot access, preserving zero/singleton lazy
paths, recovery identity, chain guards and retry invalidation. Tests must cover
EOF/direction changes, filter installation at a resolved row, repair/retry,
boundary times and visited-slot failures. Add a probe-count bound rather than
a timing assertion to verify reduced search work. Run both SDK/facade surfaces,
layouts and Go backends, relevant recovery/filter suites, then isolated release
comparisons. Include direct baseline executable controls for provenance.

No changes to the format, runtime purity, writer contract or ordinary sequential
filter error discovery are authorized by these fixes. Maintain docs/specs for
changed reader behavior after tests establish it. All synthetic evidence stays
inside this repository, with existing local caches and no live host journal.
The three decision blockers are resolved; SOW-0159 remains the sole in-progress
implementation and SOW-0121 remains paused. The earlier diagnostic processes
are absent, so no termination permission or signal is needed.

Frozen-baseline execution confirms Rust reverse skips and invalid-size error
bookmark leakage in both layouts. Old Go reaches the middle match on direct
reverse but repeats the final match after forward EOF; the native port changed
that sequence to skipping the middle. Original outputs are retained under
approved-fixes/baseline-provenance. Permanent pre-fix controls reproduce Rust
anchor/position failures and all sixteen Go explicit-time read-count failures
(517/522 reads for 1,005 postings). Raw unfiltered Go Step is lazy and does not
load ENTRYs; its error-position controls use dense/sparse filters, while Rust
also covers unfiltered selected-row loading.

The bounded position repair uses a private Rust leaf-direction tag beside
existing flags, invalidated on installation or explicit resolved-position
changes. Prepare head/tail only when a visited leaf changes direction; preserve
recovery flags and captured counts. Go stores direction beside walkValid,
invalidates before fallible reset, and commits anchors after ENTRY validation.
Restore Rust's old location after unsuccessful selected-row recovery without
cloning a full cursor. A compact ReadAt repair control also exposes copied
window staleness after malformed ENTRY bytes are repaired. Use existing cold
window invalidation before retry, including after intervening getters, while
retaining captured populations and avoiding Stat/header refresh on this path.

The focused post-fix controls pass: 190 Go test events across six new test
groups and eighteen Rust public hardening cases. Each of the sixteen Go
explicit-time probe controls now reads 55 times, versus 517/522 before the fix.
This establishes reduced search work, not elapsed performance acceptance.
Duplicate-time tests identify the exact selected sequence number, cover live
and snapshot bounds, and cross posting-node boundaries. Visited zero slots
error and permit repair/retry; unvisited malformed slots do not prevent an
explicit time query, but ordinary sequential traversal still reports them.

Retry also invalidates copied windows after a failed Go filter build, not only
after a selected ENTRY error. Rust public cursor requalification rebuilds at
the head and must retain that new direction rather than copy the old backward
tag. A new core control reproduced the latter failure before its correction;
the full workspace and optional allocative-feature checks remain to run.
The nested AND/OR direction test was corrected to force the intended early
branch exhaustion; its corrected form passes after implementation, but was
not executed against the pre-fix source. Keep that distinction in the evidence.

Private evidence for this resumption lives under approved-fixes-k_n92jni:
before-tests/before-2 retain expected failures, baseline-provenance retains
fifteen successful build/probe commands against the frozen original SDK,
before-requalification retains the new core failure, and after-2 retains the
successful focused controls. Full Go amd64/386/race and Rust 1.91 workspace
plus allocative-feature validation have now started against current source.
No native timing runs overlap those checks.

The first whole-source checkpoint passes Go amd64 (1,650 test events), Go
386 (1,651), and the complete Rust 1.91 workspace, including 185 SDK and 97
core tests. The optional allocative-feature command initially fails offline
because build inputs are absent from the local cache, rather than because of
a compiler/test failure. Reuse checksum-matched existing cache archives and
fetch only already-locked missing build inputs into the repository cache;
the retry passes all 97 core tests and three existing ignored documentation
examples. No dependency version, compiler floor or system installation changes.
Go race validation remains running at this checkpoint.

Same-failure inspection extends the approved position repair to directory
movement. Sequential traversal commits the next file index before that child's
ENTRY load; merging can prefetch a different row in the selected child before
another child fails. Restoring file-local state alone does not preserve the
directory's last successful payload/metadata selection. This is an inherited
directory state issue, not the separate seek/getter gap tracked in SOW-0045.

Apply the already-approved last-successful-position contract across those
wrappers. Add regular/compact, SDK/facade, forward/backward, filtered/unfiltered
and Go backend controls for sequential file transitions and merged direction
changes, including repair/retry and complete payload/cursor identity. Preserve
the previous selection with scalar state and existing cold row-reset paths;
avoid full cursor cloning, row scans and new steady-state allocations. Retain
unaffected prefetched candidates and captured filter populations. Go's lazy
unfiltered movement also needs rollback when directory key loading fails.
No new error category, recovery policy or sequential posting policy is added.

Source inspection also finds fresh Rust filter installation invalidates every
leaf immediately before Head/Tail/realtime positioning resets those leaves
again. Limit installation invalidation to resolved locations, matching the
existing set_location pattern. Preserve all resolved-anchor controls and
measure fresh/restart behavior with the final release source.

Go race validation also passes all 1,650 test events. The corrected directory
fixtures use a shared seqnum identity with disjoint sequence ranges for the
sequential fast path, and overlapping independent identities for merge cases.
The first fixture attempt did not establish the intended sequential path and
is retained as a harness failure. The corrected pre-fix controls reproduce
wrong payload selection/unset cursors; after scalar rollback, all 56 included
Go layout/backend/API/filter/direction cases and 32 Rust cases pass.

Directory rollback keeps the previous child/index, clears that child's cached
candidate and row state, and retains other candidates plus operational bounds.
Go snapshots raw movement's realtime anchor as well as index/override. A private
directory retry flag fits beside the existing flags and invalidates copied
windows on the next movement, after any intervening getter. Its final layout
and elapsed cost still require measurement. No steady-state heap storage is
added. The pre-existing Go unfiltered backward time-probe error suppression is
now explicitly tracked in paused SOW-0045, with that one unsupported error
trigger excluded from these position tests rather than counted as a pass.

Two final directory controls are required before freezing source. A first
backward sequential error has no committed row; restoring Go's unset index
while retaining its prepared direction would make retry return EOF. Reset
initial sequential direction on that cold error path. Rust also needs to
retain merge-active routing after consuming an explicit realtime seek;
otherwise retry can enter the sequential fast path and skip an already
prefetched candidate. Add the corresponding private flag alongside existing
directory flags, clear it on explicit boundary resets, and preserve it through
merged success/error/recovery. Test initial boundary and pending-time errors,
repair, complete remaining order and EOF in both languages and facades.

Both final controls fail before those corrections and pass afterward: sixteen
Go initial-tail cases return premature EOF, while Rust's pending-time case
skips a retained candidate. Cold Go initial-error direction reset and a Rust
merge-active flag resolve them. The flags fit existing directory padding;
no persistent candidate-offset fields or full cursor copies are introduced.
The final-source workspace passes Rust 1.91 (187 public SDK tests, 97 core),
the optional allocative feature (97 core tests), Go amd64 (1,756 test events)
and Go 386 (1,757). Final race checks remain running at this checkpoint.

Final release adapters pass all 552 shared chain/stream/recovery observations
against the pinned full-feature v262 oracle. All 35 consumer examples pass;
the wiki validator checks 16 pages and git diff whitespace checks are clean.
The local SOW audit reports initialization complete and clean; this is a
framework checkpoint, not SDK performance acceptance or SOW completion.

Final layout/allocation measurements confirm the latest flags add no size:
Go Reader is 616 bytes/class 640 on amd64 (baseline 624/class 640) and 524/class
576 on 386 (unchanged); DirectoryReader is 168/class 176 and 84/class 96
respectively, unchanged. Native Go filterBuilder is 120/class 128 on amd64 and
68/class 80 on 386, versus the old scanning builder's 72/class 80 and 36/class
48. This fixed representation cost belongs to the approved native-filter port;
do not describe it as unchanged builder memory. Rust posting/filter/cursor
remain 128/136/280 bytes and DirectoryReader remains 352. Core JournalFile is
568 versus baseline 552, and JournalReader 640 versus 608. Healthy SDK open
still makes five allocation requests, totaling 940 versus 924 requested bytes;
the sixteen extra bytes retain recovery state. Both 100,000-row layouts keep
identical content counters, first-scan allocation requests and zero warm-scan
allocations. These are requested-byte measurements, not peak/usable allocator
memory or elapsed-performance proof.

The user has been asked one evidence-method decision: keep the no-tolerated
reproducible-slowdown requirement while using a fixed finite comparison plan,
or retain this SOW's stronger self-authored rule that blocks every inconclusive
case. Four identical-binary controls have overlapping intervals and no adverse
flags; they demonstrate uncertainty, not candidate equivalence. No gate change
or slowdown waiver is inferred while the answer is pending. Continue approved
local validation and matching baseline/final-source measurements independently.
The frozen plan retains ten alternating measured pairs/two warmups per case,
all 488 original file cases without repeating explicit-time cases, bounded
auxiliary verification, directory/OpenFiles, compressed/staged and writer
controls, plus one independent confirmation block for adverse cases. Preserve
every block and investigate reproducible costs; never sample until favourable.

The final race run also passes all 1,756 Go test events. Release builds use
matching baseline/candidate compilers (Go 1.27.1 and Rust 1.97.1); the supported
minimums remain Go 1.26.2 and Rust 1.91, tested separately. The thirty-two
explicit filtered realtime comparisons all show improvements against the
original SDK baseline. The complete remaining-file matrix is still running;
the time-seek result does not establish acceptance of unrelated paths.

Read-only same-failure inspection finds one new cold retry-loop issue before
close-out: DirectoryReader.Close retains child readers whose accessors have
been cleared. A first filtered movement returns the existing invalid-journal
error and arms retryCopies; a second movement would call the cleared accessor.
This originates in the new directory rollback code, rather than the baseline.
The minimal-complete repair preserves the existing closed-reader error and
guards copied-window invalidation when the accessor is absent, inside the
error-only retry loop. Add repeated closed-directory movement controls for
both directions and SDK/facade APIs. The nil accessor is independent of file
layout and access backend, so do not duplicate this control across those
dimensions. Reproduce
before the guard, then validate the guard. No Rust equivalent is needed:
Rust reader destruction consumes ownership and exposes no post-Close method.
Do not overlap those test/build processes with the active native timing
sequence. Record the pre-guard binary provenance of that sequence and rebuild
the final source before subsequent directory measurements.

Decision 11 supersedes the earlier exhaustive performance execution plan.
The user authorized stopping only the captured broad benchmark driver; after
identity verification, its interrupt completed and the coordinator exited.
All 32 time-seek comparisons and 28 completed ordinary file comparisons remain
retained, with no regression flag in those completed cases. The interrupted
stage is intentionally partial, not a failed SDK operation or a completed
matrix. No further combinatorial stages are required mechanically.

Read-only performance inspection confirms that ordinary movement retains
native arrays/reusable row buffers, compressed values remain outside metadata
work, new directory rollback uses scalar checkpoints, and the closed-accessor
guard adds no executed healthy-path operation. Fixed feature memory costs
remain as recorded. Target the actual remaining concerns: fresh/restarted
filter construction, representative sequential/merged directory movement and
the two earlier adverse writer observations. Retain existing stream/recovery,
allocation and work-count evidence; do not build another framework to certify
each layout/backend/API combination. Unadopted constructor-identity, decoder,
getter and cursor experiments are rejected from this implementation unless a
credible reproduced cost requires reconsidering them. The abandoned additional
identical-binary continuation is superseded by this evidence-method decision.

The closed-directory control reproduces the second-movement nil-accessor panic
before the guard (closed-retry-before). The guard preserves invalid-journal
errors without changing successful traversal. Its post-fix focused checks are
the next validation step; prior full-source suites are not relabelled as
having executed this final guard.

The focused final-source comparison isolates a real Rust setup cost: regular
100,000-row facade/snapshot/windowed/forward fresh filters take 1.04870 times
the baseline cost, with a paired 95 percent interval [1.04488, 1.06466]. The
same installed-filter restart improves (0.98348, [0.96674, 0.98590]). This is
evidence to revisit the narrow constructor-copy candidate, rather than expand
the full matrix or accept an observed slowdown. Go's corresponding fresh and
restart controls improve to ratios 0.12883 and 0.01039 respectively.

Apply only the existing reader_data_cursor closure optimization to current
Rust source: retain the factory Result intact, release the DATA guard at the
inner scope boundary, and return Err/None or healthy Ok(Some(cursor)) without
the consuming for_reader round-trip. Read-only verification confirms healthy
for_reader returns its input unchanged, the cached recovery-status read has no
side effects, and active empty/nonempty prefixes still take the unchanged
qualification path. Preserve the outer error/recover/rebuild wrapper and all
later direction/population corrections. No public signature, file format,
error policy, allocation policy or query algorithm changes. The recovered
branch adds one cached status read; it does not discover recovery or perform
I/O. Validate existing core/public recovery and filter retry controls, then
repeat this actual setup workload with matching helpers and complete counters.

The focused ordinary comparison also reproduces smaller observations:
Go materialized SDK-entry ratio 1.01989 [1.00369, 1.02332], Rust whole-file
facade-next 1.05624 [1.01301, 1.16967], while Go borrowed payload visitation
improves to 0.90686 [0.89302, 0.93333]. Inspect actual executed work before
adding tests or changing algorithms. Disassembly finds Go's readEntryAt frame,
instruction count and allocation/map/string/copy calls unchanged, with only
a byte-sized cached payload-offset load changed in the DATA path. The ordinary
GetEntry path now checks the special filtered orphan index before valid range
checks. Make the ordinary unsigned range check first and retain the identical
special -2/override behavior only on the out-of-range path. The unsigned check
rejects negative indices on amd64 and 386 because slice lengths fit MaxInt.
This removes ordinary-position branches without changing errors or access.

Rust's scalar position checkpoint is already inlined; no Result allocation or
getter call remains to remove. The selected-ENTRY error recovery body is still
expanded into step_valid, adding cold recovery locals and restore/invalidation
blocks to its frame. Mark that existing recover_after_error body cold and
non-inlined, preserving the body, checkpoint and every rollback/retry rule.
Neither change adds a public API or weakens validation. Run affected Go/Rust
controls and repeat only the representative observations against the original
baseline. Do not infer that these few instructions explain every timing delta.

Release-build-5 confirms the intended generated paths. Go GetEntry's normal
prefix falls from 17 instructions/four conditional branches to 14/two;
header cache-fill position work falls from 15/five to 12/three. The original
GetEntry prefix was nine/two, so do not claim complete instruction identity.
Rust's recovery helper is now called only after failed ENTRY loading, and
step_valid shrinks by 1,054 code bytes. Its stack reservation increases from
0x4f8 to 0x508 rather than shrinking; no frame improvement is claimed. Healthy
DATA cursor construction bypasses for_reader, although the outer 128-byte
result transfer remains. Elapsed acceptance still needs the focused repeat.

The final-source existing integration runners pass all 63 strict verifier,
34 directory and 42 mixed-directory checks against systemd v262. Retained
directory fixtures are reused without regeneration/deletion; command mapping
selects the pinned oracle explicitly. Previous 32 general, 72 compression,
20 compact, 18 binary, 18 live-feature and 552 shared hardening observations
remain valid with their original provenance. They are not relabelled as new
runs. No writer, codec, FSS or publication algorithm changed in the final
position/constructor code-generation edits.

Final Rust 1.91 workspace validation passes, including 187 SDK and 97 core
tests; the optional allocative feature also passes 97 core tests. The final
Go amd64 full run passes 1,760 test events but its live stock-reader stress
test fails when the writer exceeds the existing 45-second deadline. The same
control previously passed in 12.94 seconds. This is not a reader assertion
failure and is retained as a failed full run, not reported green. Concurrent
release linking/workspace testing is a working theory for contention, not a
proven cause. Run that unchanged 1,200-entry/fsync-every-entry control alone,
without other task builds/tests/measurements. Do not relax its timeout or
change writer behavior to obtain a pass. The failed Go chain did not execute
its final 386/race commands; validate their affected read paths explicitly.

The unchanged isolated stress control passes in 38.03 seconds with the original
45-second deadline: all 1,200 entries are written and observed in order. This
resolves the functional test failure but does not establish the cause of the
earlier timeout or explain the duration difference from the prior 12.94-second
run. Retain both results and avoid attributing environment variability to an
SDK code change without evidence. Final 386/race commands select affected
reader/filter/entry/snapshot/recovery/Explorer/facade/chain tests and omit the
unrelated Go live-writer group; the preceding complete suites and this isolated
durability control retain their separate meanings.

The final representative reader controls no longer reproduce the earlier
setup/ordinary costs after the equivalent code-generation corrections: Rust
fresh setup is 0.99693 [0.94300, 1.09289], installed restart improves to
0.97172 [0.87909, 0.98150], Rust facade-next is 1.00501 [0.94628, 1.02716],
and Go materialized entry access is 1.00389 [0.99594, 1.04393]. Uncertain
intervals are not claims of equality; the unchanged native algorithms,
allocation/work counters and removal of redundant healthy-path work provide
the additional decision-11 evidence. Final Go 386 and race selections each
pass 1,153 affected reader/filter/entry/recovery/directory test events.

The Go regular/plain/publish-1 structured writer confirmation still reports
1.04883 [1.02723, 1.09979]. The Rust compact/long-Zstd writer observation
does not reproduce (1.01045 [0.97594, 1.06459]). A 100,005-row append-only Go
profile produces byte-identical journals and takes 5.87984 seconds before and
5.79988 seconds after this SOW, but that larger control does not establish
the smaller case's cause. DATA header reads and publication dominate; regular
DATA array traversal also carries added guard control flow.

The private hash-lookup terminal-zero relocation is not adopted. Its latest
two original-baseline controls are inconclusive, and an older private delta
flagged long-Zstd/raw/publish-1 at 1.01864 [1.01040, 1.04408]. Retain both
results rather than choosing the favourable block. A distinct equivalent
cost removal is justified in appendToDataEntryArray: the existing branch has
already proved the next offset nonzero. Test `steps != 0 || next <= offset`
at that call site and invoke the unchanged advanceUnordered only when true.
This is exactly advance(current, next, false) under that nonzero precondition,
including continued tracking after an unordered link. It removes the ordinary
boolean success-result merge without changing traversal, error, allocation,
I/O, layout or writer publication contracts. Validate existing writer/append/
cycle controls on amd64 and 386, inspect generated code, then repeat only the
two affected representative Go writer observations. No new writer cache,
framework or algorithm is proposed.

The ascending-link specialization passes 62 existing writer/append/guard test
events each on supported Go 1.26.2 amd64 and 386. Saved Go 1.27.1 disassembly
confirms the ordinary link skips boolean result merging: ten instructions
become six useful instructions plus one alignment NOP, with three conditional
branches becoming two. The 72-byte frame and guard initialization remain;
active/unordered links retain the original checks. This is instruction/work
evidence, not an attribution of the earlier timing magnitude.

The final two original-baseline writer comparisons retain ten alternating
measured pairs and two warmups: regular/plain/publish-1 structured is 0.98828
[0.88800, 1.08311], regular/long-Zstd/publish-1 raw is 0.96598
[0.92051, 1.01694]. Neither flags a regression; both remain statistically
inconclusive. Every 10,005-row/32-field output matches the baseline bytes and
stock v262 verification passes. The earlier 1.04883 block is retained against
its previous source, not erased or averaged with these samples. Acceptance
under decision 11 combines the targeted repeats, larger append-only control,
equivalent reduced control flow, unchanged I/O/allocation/publication paths,
and writer correctness tests. It is an engineering judgment that no remaining
reproducible writer regression is established, not a slowdown waiver or proof
of exact equality. No further combinatorial timing stage is required.

Final writer evidence is writer-ascending-guard (source/build/test/codegen
provenance) and writer-ascending-control (48 warmup/measured observations).
Reader/interoperability evidence remains release-build-5: only writer_arrays.go
changed afterward. Other private lookup-relocation candidates remain rejected.
Original-baseline executable provenance covers the Rust reverse/bookmark
failures and Go direct/EOF anchors. Inherited original-Go fatal-load and broad
directory-origin conclusions rely on source inspection; those exact original
baseline executable controls were not run. Corrected nested Boolean coverage
was not executed before its fix. Do not strengthen those provenance claims.

### Internal Final-Review Corrections - 2026-10-17

The pre-review framework audit is complete and clean, but whole-surface
read-only review finds three contract gaps before external review. The earlier
local-readiness statement is superseded until these repairs are validated.

- Both directory FlushMatches implementations rewind their children but retain
  a directory-level pending realtime seek. AddMatch; SeekRealtime(later);
  FlushMatches; Next therefore skips the beginning despite the documented and
  approved head-reset contract. Clear only the pending directory seek in flush;
  do not clear it in general filter mutation, which must preserve explicit
  positioning. Test SDK/facade, sequential/merged and both file layouts.
- Sequential directory EOF discards the last selected anchor. Reversing then
  boundary-resets and repeats the endpoint instead of returning its nearest
  predecessor/successor. The native Go port newly enables filtered sequential
  directories; Rust has the same inherited inconsistency. Extend the existing
  scalar rollback to filtered EOF only: retain the directory key/index and
  selected child's position, discard only its candidate, and preserve other
  candidates. Rust needs a hidden, nonallocating core filter-presence query on
  this EOF-only path; do not duplicate filter state or change raw/unfiltered
  semantics. Test repeated EOF in both directions, then reversal, SDK/facade,
  sequential/merged and regular/compact.
- Rust recovery discovery bounds snapshot shrink by the physical file length,
  but recovered_posting_count uses the original snapshot cap. With COMMON's
  first secondary array truncated after its first valid slot, two global rows
  survive; a later slot read reports UnexpectedEof instead of normal prefix
  EOF. Bound cold recovered posting qualification by reader_extent, retaining
  the original growth cap. Inspect the partial recovered-array view for the
  same physical-bound mismatch. Add paired language snapshot controls and
  repair/retry; preserve strict constructors and healthy paths.

These are consequences of already-approved rewind, nearestness and intact-
prefix/normal-EOF contracts, with no new user design choice. Reproduce them
before implementation, fix minimally, then run affected suites and review the
complete final surface. The added work is in cold flush/EOF/recovery paths;
no new framework or combinatorial timing batch is required.

The flush same-failure search also finds Rust FileReader retains its cached
CurrentRowView after the core cursor is rewound. Getters can therefore return
the old row until another movement. This is inherited, but is within the
approved paired flush/head-reset contract. Clear that existing row cache with
the same best-effort path used by seek_head, then clear matches. Add paired
file/directory SDK/facade getter controls before the fix; no extra native path,
allocation or new reset contract is needed.

The Go review additionally identifies resolved-anchor backward preparation
that eagerly reads an AND leaf Rust never visits, and repeated resolved EOF
that restarts an entire posting walk. Reproduce both with an absent final AND
leaf/damaged earlier array and an accessor work counter. On resolved resets,
arm backward leaves lazily using the unused node=-2 sentinel, materializing
only in the existing zero-value seek branch. Preserve eager explicit Tail
preparation and binary realtime behavior. Record a successful resolved reset's
offset/direction as its walk anchor before seeking, including EOF; the existing
error invalidation still clears it. This adds no per-leaf field or healthy
cached-value branch.

Go partial-array recovery also needs the existing main array reader's MaxInt
byte-count check before allocation. An accessor-backed oversized extent can
prove the current panic on both architectures without creating a huge file or
allocating its body. This is a cold bounds check, not a streaming redesign.

Finally, count-bounded array walkers can revisit an earlier array and finish
their declared population before lazy Brent detects the cycle. This violates
the controlled-error claim for actually traversed cycles. Reproduce a short
A-to-B-to-A population first; unused terminal next links remain untraversed.
Investigate cold checks against already visited native-array prefixes, avoiding
healthy preflight, per-row sets or reads of unneeded future headers. Resolve
the shared reader/posting/recovery/writer blast radius before acceptance;
do not treat bounded duplicate output as successful cycle handling.

Refined minimal EOF repair: preserve only the committed directory key while
unsetting its public index, matching the existing merged-EOF state. The
existing direction/key check then selects merge-based reversal at the saved
anchor. Children already retain their filtered EOF positions; no child reset,
index checkpoint extension or per-row key copy is required. Rust's hidden
filter query runs only at sequential EOF against the first child; public
directory filter mutators apply to every child and retain pending builders.

Refined snapshot repair avoids per-slot metadata calls or new cold state.
WindowManager already retains the original open size in retained_size; only
writer-owned sync/growth can subsequently change it. In the Snapshot arm of
reader_extent, cache min(retained_size, physical length) in file_size. Existing
recovery discovery performs that one cold metadata read; posting qualification,
partial array length and get_slice header bounds then share the observed
physical extent. Repair can restore up to the original cap. Keep LiveFile and
WriterOwned behavior unchanged. No healthy layout, allocation or read cost
changes.

The short-cycle controls reproduce required revisits in Go global/posting
reads and both writer helpers, plus Rust SDK reads, public bare Node.next and
both writer helpers. The first Rust SDK attempt had a test-only dependency
alias typo; the corrected control reproduces both failures. Rust's unused
terminal self-link control already passes; Go incorrectly checks that link.

Go repair uses a shared cold native-header prefix check only before following
a required nonzero link at or below the highest visited array offset. It checks the target
against head-through-current, never future nodes. The helper retains a local
termination guard for malformed/unreachable prefixes. Native filter leaves
already retain node metadata, so compare against that vector on exceptional
links without I/O or separate Brent state. A high-water scalar is necessary:
after an accepted descending edge, an ascending edge can revisit an older node.
The descending-prefix/ascending-revisit control reproduces this omission in
the first cold-prefix candidate; do not accept only first-backedge coverage.
The scalar replaces the previous two-word guard and changes only array
transitions, not ordinary within-node movement. Remove the now-unused bounded-walk
guards; unbounded hash/FIELD guards remain. Readers ignore terminal next links
once their captured population is complete; writers still validate links they
must follow to choose a new writable slot. Use existing error categories.

Rust's public bare Node lacks a starting point, so the user has been asked the
irreducible representation choice: replace cached next with head at the same
32-byte size, or add a head field and grow to 40 bytes. No dependent Node
representation change is implemented while that answer is pending. Continue
the independent Go repair and validation.

The user selects **1A on 2026-10-17**: keep Node at 32 bytes, replace cached
next with the chain head, and read the next link at array transitions. Pack
the exceptional-order flag in an unused alignment bit of that head word.
After a descending/equal edge, check every required transition against the
visited prefix, including an ascending revisit of an earlier higher node.
Arbitrary non-head cursor reconstruction conservatively arms that flag.
Within-array row movement and cursor size stay unchanged. Validate the Node
size, short cycles, descending-prefix revisits, acyclic backward links and
unused terminal links, then representative reader performance.

Use a shared cold Rust prefix helper with a native-header callback. Writer
walkers retain head/highest scalars in place of the two-word Brent guard;
drop array guards before exceptional prefix checks. Recovery uses its fresh,
physical/captured-extent array-prefix reader and validates only required links
after subtracting the consumed count. Preserve each writer's existing tail
stop predicate and cached-tail fast path. This is the already-approved
counted-cycle repair, independent of the Node representation decision, with
no new healthy reads or allocations in these writer/recovery loops.

A final same-failure audit identifies nested rollback ownership in both directory
implementations. After sequential traversal advances to another child, a newly
observed recovery restarts merging through the public rollback wrapper while
the public key still belongs to the previously selected child. A later probe
error therefore restores the scanning child and discards its prefetched row;
the outer wrapper then restores the selected child. Repair/retry can skip the
nearest surviving row. Reproduce three disjoint filtered files with rows
1..9, select row 4 backward, lose row 3's header and damage row 8's type. The
merged probe must fail while preserving row 4; after repairing row 8, retry
must return row 2, then row 1. Go's new control reproduces row 1 instead of row
2 before the fix. Run the paired Rust control before changing runtime code.

The minimal repair is to call the existing inner merge routine at the internal
recovery restart only: Go stepSequentialReader and Rust sequential forward and
backward. The outer public movement wrapper already owns rollback. Public
entry points retain their wrapper, other prefetched candidates remain intact,
and no new per-row checkpoint or state is introduced. This is a consequence of
the approved selected-row/error-retry contract, not a new positioning policy.
The same-failure caller search finds only those three nested wrapper sites.

The paired nested-rollback control reproduces the same row-2 skip in Rust.
Both implementations pass after changing only their internal recovery restart
calls to the inner routine. Go's final supported-floor directory/recovery/cycle
selection passes 170 test events on amd64, 386 and race; Rust's complete public
SDK library passes 197 tests on Rust 1.91. Independent source recheck confirms
one public rollback owner, retained other candidates, and no new healthy work.
The short-cycle repairs also pass the Rust 1.91 whole workspace (196 SDK/98
core before this final directory control), allocative core (98), and Go's
all-package amd64/386/race suites (1,819/1,820/1,819, with only the unchanged
live-writer stress control excluded as explicitly documented above).

Final release-build-7 retains matched helper source and binary hashes. Its
shared hardening run passes all 552 observations; verifier, directory and mixed
matrices pass 63/34/42 stock-v262 observations. Existing verified examples are
unaffected by these final internal representation/rollback corrections: there
is no changed public signature or example flow. The Node shape assertion now
uses two u64 and two usize words, preserving 32-bit test portability as well as
the approved 64-bit 32-byte size. Specs and compatibility guidance record
required counted revisits, ignored unused terminal links and single-owner
rollback validation.

The final performance confirmation reuses existing runners only: Rust core
forward/backward, facade-next and dense filtering on regular/compact fixtures,
plus the two previously sensitive Go writer cases. The normal Rust path keeps
four-word Nodes, unchanged within-array movement and no new allocation; an
uncached transition reads its current native header. Cached reverse Node chains
retain their existing reuse. Go's normal transition replaces lazy-guard state
with a high-water scalar and introduces no reads or allocations. Exceptional
acyclic backward links remain accepted and may rescan only their visited prefix;
no future suffix is preflighted. Directory rollback corrections run solely at
recovery transitions and add no healthy check, allocation or I/O. Earlier
writer-ascending results refer to their earlier source; the counted-array
replacement requires its own two targeted controls and does not inherit those
binary hashes or code-generation counts.

Final counted-array performance confirmation passes semantic counters, immutable
fixture/binary checks and stock writer verification. Rust's eight cases retain
seven measured pairs and two warmups: regular core-next forward/backward and
compact dense filtering improve; the other five comparisons are inconclusive.
No case has a regression flag. Go's two writer cases retain ten pairs and two
warmups: regular/plain/publish-1 structured is 1.01477 [0.94232, 1.05638], and
regular/long-Zstd/publish-1 raw is 0.97970 [0.96239, 1.00383]. Both remain
inconclusive. All 10,005-row/32-field writer outputs match original-baseline
bytes and stock v262 verification passes. Evidence is counted-performance and
writer-counted-control, with writer-counted-guard source/build provenance.

Decision-11 acceptance combines these final targeted repeats with unchanged
within-node work, the approved Node32 shape, no new healthy-path allocation,
reviewed native reads and the earlier retained representative results. There
is no established remaining reproducible regression; no speedup offsets an
adverse case and no uncertain interval is called proof of equality. The final
array repair replaces the earlier writer guard, so its earlier ten-to-six
instruction observation is historical, not claimed for the final binary.
The sole extra normal Rust work is the approved native current-array header
lookup at an uncached transition; the row path and cached reverse nodes retain
their shape. Independent internal source audits find no remaining blocker in
the repaired counted-array and directory-retry surfaces. Whole-SOW external
review remains the final independent technical gate.

### Required Engine Cache Migration - 2026-10-17

The final artifact audit traces a reachable legacy cache consequence. At
baseline 4c49205, Node::next treats a missing required array link as successful
EOF (offset_array.rs:59-76), and List::collect_offsets returns a partial vector
(:327-349). Core open validates mapping bounds without a graph pass. With no
source timestamp field or facets, FileIndexer builds a partial histogram and
FileIndex from those offsets (file_indexer.rs:150,185-224,473-501). Count-bounded
cycles can similarly supply duplicates. The engine stores successful indexes
(indexing.rs:549-552); offline archived indexes remain fresh indefinitely
(file_index.rs:94-102), and compatible cache hits return without reopening the
journal (indexing.rs:392-398). This is source-proven reachability, not a claim
that any real cache contains such an index.

The existing product-scope rule requires CACHE_VERSION to change whenever
indexer inclusion, computation or serialization changes. Apply that established
policy here: increment journal-engine's generation from 3 to 4, leaving the
schema, key construction and cache algorithms unchanged. Old keys miss; valid
files rebuild on demand once after upgrade, while malformed files now fail
indexing and can be retried later. Old cache storage follows normal cache
eviction, with no new deletion operation. Cache-hit and steady-state
reader/writer work remain unchanged. Go has no equivalent persisted engine
cache in this source tree; its explicit IndexedSnapshot contract is unchanged.
This is a required consequence of the approved native-array repair, not an
optional cache redesign or a new user policy exception.

Stream completion alone does not require this bump: FileIndexer skips compressed
facet DATA and parses source timestamps from raw payloads; query-time payloads
decode dynamically. Ordinary public recovery also remains separate from the
core FileIndexer open path. Record generation 4 and the one-time rebuild in the
spec and unreleased consumer migration note. Add a small archived-file indexer
regression with a healthy control, required missing link and short cycle;
validate it and the existing cache-key/engine tests on Rust 1.91. No reader or
writer timing repeat is required for an unchanged cache algorithm and a key
generation constant.

The original-source archived FileIndexer control reproduces successful indexing
of only four of 13 rows for the regular missing-link case. It stops at that
first assertion; the baseline did not execute the other variants. Current
Rust 1.91 index/engine package suites pass, including the four malformed
variants in the new test, healthy archived freshness and existing v3/v4 key
non-reuse. All 35 verified documentation examples pass again and all 16 wiki
pages validate. Malformed archives now fail indexing rather than being promised
a successful rebuild; valid archives incur the documented once-on-demand cost.

The generation constant belongs only to journal-engine, which is not a
reader-core benchmark, conformance adapter or journalctl dependency. The final
reader/writer source used by release-build-7 is unchanged; cache-migration
retains the separate final optional-engine test/source provenance. No cache
algorithm, persistent schema shape, compiler floor or public reader path is
changed by this migration. Internal artifact recheck finds this consequence
covered. The complete candidate is locally ready for its authorized whole-SOW
static external review, subject to the final audit result below.

The final pre-external audit reports `SOW initialization complete and clean`
with changed-file sensitive-data scanning enabled. `git diff --check` passes;
source, tests and durable artifacts use synthetic data and contain no user's
personal name. Runtime implementation is frozen for the whole-SOW review.

### Whole-SOW External Review - 2026-10-17

After complete local validation and the complete-and-clean audit, the full
neutral prompt was displayed and the six authorized static reviewers started
in parallel. Session: `ef5096c38bfe40ea80265cb9344505bc`. The prompt covers the
complete SOW, tracked and untracked changes against prerequisite 4c49205,
indirect consumers, performance evidence and strict/pure SDK contracts. Review
is read-only with no tests, builds or delegation. Runtime source is frozen.
This SDK round is distinct from prerequisite SOW-0108's historical round.
Reviewer results and independent dispositions will be recorded before close.

Qwen's initial invocation failed before returning a review with HTTP 429
Too Many Requests. Its sole permitted retry uses the identical displayed
prompt and the same reviewer session. Successful reviewers are not rerun.
The other five reviewers continue independently; no technical verdict is
inferred from a transport failure.

Qwen's one retry also failed before producing a review with HTTP 429. Both
attempts are retained in the session artifacts; no further retry is permitted
or planned. This reviewer supplies no technical verdict and leaves a stated
coverage gap. The five other requested reviewers remain independent of that
transport outcome, and their results will be preserved individually.

GLM and Minimax return `PRODUCTION GRADE` for the complete SDK candidate.
Independent disposition of their observations:

- GLM's unfiltered realtime error suppression is verified in the unchanged
  raw search and frozen baseline. The existing SOW-0045 mapping covers it;
  filtered explicit realtime search propagates visited errors here.
- GLM's raw automatic-refresh error position observation is also present in
  baseline Next/nextAfterTailRefresh: the index increments before a refresh
  error returns. SOW-0045's queued failed-refresh rollback and healthy-position
  analysis covers that contract. This is not substituted for the selected
  filtered-row retry guarantee implemented and validated in this SOW.
- Minimax labels two Rust realtime observations P2 while explicitly describing
  correct behavior and no production defect. Reject those defect labels.
  InlineCursor::head retains the native inline first posting; backward
  partition index zero correctly returns it when the next posting exceeds the
  anchor. The caller retains the resulting posting cursor intentionally.
  Existing requested-direction, reverse-anchor, duplicate-bound and visited-slot
  tests cover that positioning contract. Additional test-only suggestions do
  not establish an uncovered behavior or justify redundant controls.
- Minimax's Go P2 sentinel suggestion is rejected. node=-1 with value=inline
  denotes the real inline position. seek returns that value without another
  read when it satisfies the requested bound; it reads the first array slot
  only when moving beyond it. The -2 sentinel denotes an unmaterialized
  backward tail and is inspected only when value is zero. Changing node alone
  in this nonzero-inline branch would provide no claimed benefit.
- Minimax's two P3 observations explicitly identify no defect. Trailing Zstd
  garbage is already an expected-error shared case; finite native-file bounds
  make the guard's u64 overflow limit unreachable for distinct valid objects.
  No new runtime code, test framework or optional cleanup is warranted.
- The remaining Minimax notes summarize already retained history, rejected
  private candidates, superseded evidence policy and mapped SOWs. They add no
  uncovered defect. Its descriptions of a known-broken stress test or proven
  contention root cause are not accepted: the full-run deadline failure and
  isolated 38.03-second pass remain recorded, with the cause unproven. The
  final all-package counts explicitly exclude only that stress control.

The other three available reviewers remain in progress. No successful reviewer
is rerun because of another reviewer's transport failure.

MiMo, Kimi and DeepSeek each reach the configured 1,800-second execution
limit with no stdout review or verdict. The session metadata records timeout
and owned-harness termination; their source-inspection logs are retained.
Each receives its sole permitted retry in the same session with the identical
full-scope prompt, displayed again before execution. These are independent
timeout retries, not a new debugging round. Runtime source is unchanged and
the two successful reviews are preserved without rerunning them. Qwen has
already exhausted its retry and is excluded.

### External Finding Verification And Repair Gate - 2026-10-17

Kimi returns NEEDS CHANGES with directory-filter anchor loss and Rust indexer
traversal-error suppression findings. Root source inspection confirms that
both directory mutation paths reset the emitted key, contradicting the
already-approved reseek-from-emitted-key and nearest-installation contract.
Prior installation controls exercise files; directory controls install before
their first row or exercise raw/filtered movement without changing the filter
at a committed directory row. This omission means the earlier primary audit
was incomplete. Do not close on the two positive reports.

Add paired controls using the existing six-row sequential/merged directory
fixture. Select row 3, install/replace a match or commit a conjunction or
disjunction, then require rows 4..6 forward or 2..1 backward. Cover SDK/facade,
regular/compact and the Go access backends. Run before changing runtime code.
Repair by separating mutation invalidation from FlushMatches' full reset:
retain the committed key/index, discard prefetched candidates and revalidate
the changed expression through existing merge-key positioning. Preserve
pending explicit seeks, error/retry state and boundary behavior when no key
exists. Keep healthy per-row state/work unchanged; mutation-only reseeking is
required by the existing contract. Same-failure inspection covers all three
operators in both languages, raw/filtered transitions and recovery restarts.

Independently verify the indexer finding and reproduce its actual requested
facet/source-timestamp cases before any indexer repair. Preserve intentionally
documented per-object tolerance unless a new native traversal failure must
propagate. Any required consequences belong to this existing SDK SOW; no new
product policy or compiler/native dependency change is proposed. Let the two
remaining current review attempts finish under their existing limits; do not
launch another review until repairs, complete internal validation and audit
are finished. Then review the complete candidate again under standing
authorization, rather than using reviewers as an iterative debugger.

The paired directory controls reproduce endpoint replay: Go's 128 subcases
fail; Rust stops at regular/sequential/SDK/forward, returning 1000 instead of
4000. Backward Go controls return 6000 instead of 2000. The original reset
implementations predate this SOW; this is an inherited omission inside the
approved installation contract. An earlier Go test itself expects replay
after mutation and needs its expected sequence corrected.

The mutation audit also covers a consumed initial realtime seek after load
failure: before clearing its bound, restore the target to pending when no
committed key exists, without overriding a newer pending seek. Invalidate
before fallible child group loops. Existing key-based merge preparation and
full-key comparisons provide the repair, with no new per-row field or branch.

The Rust indexer finding is source-verified. Unchanged baseline FIELD DATA and
posting-list handlers suppress new cycle/required-link errors, yielding
successful incomplete generation-4 indexes. Reproduce healthy global arrays
with requested MESSAGE facets or repeated source timestamps and damaged FIELD
DATA links/postings. Propagate CyclicObjectChain through FIELD DATA setup and
iteration handlers; propagate failed required posting collection by operation.
Reuse reader_inlined_cursor with the borrowed DATA header to reject
positive-count missing inline/head pointers while retaining zero-reference
orphans. Preserve unrelated per-object tolerance, invalid timestamp fallback,
compression/size/cardinality limits and per-file batch failure isolation.
Test regular/compact facet/source cases, missing links, required cycles and
missing positive-count pointers, plus healthy/tolerated controls and actual
cache omission. Reuse native headers and cache helpers; no dependency, cache
schema or steady-state algorithm change is required.

MiMo and DeepSeek also exhaust their one retries at the execution limit with
no verdicts. Preserve every attempt; these reviewers and Qwen leave coverage
gaps. GLM and Minimax are positive; Kimi identifies the verified blockers,
so the candidate is not accepted. Complete repairs, the full primary audit
and validation before the next whole-SOW review. No further retry belongs
to this round.

The first repair passes all 128 Go mutation cases, the paired Rust mutation
matrix and both consumed-realtime-seek controls. All 20 requested facet/source
index damage cases and the actual engine cache-omission test also pass.
Source review identifies a necessary refinement before acceptance: forcing a
resolved non-overlapping directory into permanent merging adds all-file work
and can expose a distant ENTRY error before nearer healthy matches. Retain
sequential state only when non-overlap remains proven, merging has not begun,
a direction and committed key exist, and the selected file index is valid.
Clear candidates in that cold mutation branch; preserve the existing general
key/seek invalidation for every other state. Existing reversal, mode-change,
recovery and rollback checks remain authoritative. Add a paired three-file
control: select row 4 under COMMON, damage unvisited row 7, replace COMMON,
return rows 5 and 6, then report the error while retaining row 6 and permit
repair/retry to row 7. Run the control against the first repair before changing
the helper. This bounds the consequence directly without a timing framework,
new state or a new per-row branch, consistent with performance decision 11.

Both first-repair controls reproduce the unnecessary distant-file error before
row 5; the guarded repair passes both layouts, repair/retry and the existing
directory suites. Go has the same invalidation inside its existing raw/filtered
mode-change branch. Apply the identical proven-sequential predicate there,
retaining direction only in that state while always updating mode and clearing
candidates. This covers first filter installation as well as replacement and
avoids a Go/Rust native-path discrepancy. Existing file offset normalization,
reversal and directory rollback provide the position guarantees. Extend the
same representative control with installation after raw movement, reproducing
the mode-change issue before its repair; do not add a timing matrix.

Same-failure inspection finds an inherited indexer panic when a requested
facet posting points to an offset absent from the global ENTRY map. A DATA
object's own aligned offset in its inline posting reproduces the existing
missing-map branch without requiring any extra validation work. AC1's
controlled invalid-link outcome includes this affected index consumer. Replace
the panic on that cold branch with the existing MissingOffset error and add a
regular/compact reproducer before the repair. Preserve normal membership
lookups, allocation and traversal costs. Qualify migration prose to encountered
required native-chain/posting failures; documented value tolerance is retained,
so it must not promise universal malformed-file verification.

The absent-map facet control reproduces the inherited panic before repair and
passes after returning MissingOffset on the existing branch. A separate source
map can admit an unknown target without a native traversal error. That inherited
cross-index membership/captured-tail lead is tracked in pending SOW-0161, with
its unexecuted prediction explicitly distinguished from measured failures.
Strict bidirectional membership remains a verification responsibility; no new
source-map certification pass or live-index behavior is introduced here.

### Post-Repair Primary Audit And Validation - 2026-10-17

The complete SDK candidate is locally validated again. Go 1.26.2 all-package
amd64/386/race runs pass 1,995/1,996/1,995 test events, excluding only the
unchanged live-stock stress control whose earlier isolated pass and full-run
deadline failure remain qualified above. Rust 1.91 workspace passes, including
200 SDK and 98 core tests, plus all 98 allocative core tests. The final index
and engine tests cover 22 requested-field damage cases and actual damaged-file
cache omission with a healthy sibling. The 306-event focused Go directory suite
and 14 Rust directory tests pass. Before controls retain endpoint replay,
unnecessary distant-file errors and the absent-map panic with exact provenance.

Release-build-8 retains 359 source hashes and 11 rebuilt tool hashes. All 552
shared observations and 63/34/42 pinned-v262 verify/directory/mixed checks pass.
All 35 documentation examples and 16 wiki pages pass. Final Rust test and
release manifests match current source; the Go-only runner captured the Rust
indexer immediately before its cold panic repair, so its Go source is exact
and the separate final Rust run certifies that later Rust file. Do not treat
that irrelevant cross-language manifest difference as a Go execution claim.

The primary audit covers the complete original implementation and new consumer
consequences, all three directory operators, failed initial time seeks, native
sequential versus merged positions, raw/filtered transitions, recovery restarts
and rollback, field/posting error catches, checked positive-count pointers,
cache generation/insertion and strict-versus-tolerated boundaries. The inherited
source-map membership question is separately recorded in SOW-0161 without a
new certification pass. No further verified in-scope blocker remains locally.

Performance decision 11 applies: directory invalidation reuses proven sequential
state inside cold mutations/mode switches, adding no steady-row condition or
checkpoint. Indexer changes propagate existing errors and check borrowed DATA
header scalars once per requested value; the absent-map repair changes only
an existing error branch. No extra container, row scan, decompression or healthy
posting-array read is added. Earlier measured native-reader/writer controls
remain applicable with this bounded-work and source review evidence. No speed
equality is inferred from an uncertain timing interval.

The changed-file scan covers 120 durable artifacts without findings; all 97
changed/new task files pass the personal-name check. The audit reports complete
and clean, and whitespace checks pass. Specs, user docs, compatibility skill,
status indexes and follow-up mapping reflect the repair. Runtime source is
frozen for the new whole-SOW external round; only review/closure evidence may
be appended while it runs. This is a material-fix review after complete internal
audit and executable validation, not a partial debugging review.

The new full-scope static round starts at 09:35 UTC in session
076f2256bda9495486f72a229e32851a, with the complete neutral prompt displayed
before execution. All six authorized reviewers start in parallel. Qwen's first
attempt returns HTTP 429 before any verdict; its sole retry uses the identical
displayed prompt in the same session. Other reviewers continue independently.
No source edits, executable review workloads or CLI implementation run during
this round.

Qwen's second attempt also returns HTTP 429 with no stdout or verdict. Both
6-second attempts remain retained; its retry allowance is exhausted. This is
a reviewer coverage gap, not an implementation verdict. The other five
reviewers remain independent and in progress.

Release-build-8/retained now preserves immutable copies of all 11 tool binaries
and all 359 manifested Go/Rust source files, with verified SHA-256 identities.
The task's reusable bin directory can therefore be rebuilt during the later
CLI SOW without losing this SDK's final executable/source evidence or the
current CLI baseline. This is evidence retention only; reviewed source is
unchanged.

Minimax returns PRODUCTION GRADE after 1,680.1 seconds. Independent
adjudication rejects unnecessary changes in its nonblocking notes:

- The recovered Go posting helper has only two callers, both of which enforce
  the symmetric count/inline check before calling it. Repeating the check in
  the callee changes no reachable behavior and adds no missing protection.
- Removed path fields are private implementation storage, not a public API.
  File.Name retains the original staging name, exactly as the previous stored
  string did, including after external renaming. No correction is needed.
- Rust refresh_recovery_batch explicitly clears every recovery_pending flag
  after a stable pass. Ordinary movement checks the flags but does not rebuild
  every recovered leaf on each row. Failed pending refresh remains retryable by
  contract. Reject the claimed repeated-rebuild scenario; no optimization is
  required for acceptance.
- The unrelated-note claim that an unfiltered-reader issue is excluded from
  final Go counts is incorrect. The only excluded test is the unchanged live
  stress control. Actual inherited raw refresh/seek/probe issues remain mapped
  to SOW-0045, without changing the tested selection contract here.
- SOW-0161 remains a separately recorded inherited membership investigation.
  Inconclusive measurements and future cache-version requirements are already
  qualified in the canonical evidence; neither is a new defect or a reason to
  expand the framework. Use recorded case identities rather than the report's
  imprecise performance-case labels.

No verified blocker or additional required change follows from this report.
The four other reviewers subsequently reach their execution limits without
verdicts, as recorded in the terminal disposition below.

### Final Review Disposition And Closure - 2026-10-17

Fresh session 076f2256bda9495486f72a229e32851a is terminal. Minimax returns
PRODUCTION GRADE after 1,680.1 seconds. GLM, MiMo, Kimi and DeepSeek each reach
the 1,800-second execution limit (observed 1,800.7-1,800.8 seconds), returning
no stdout and no verdict. Qwen returns HTTP 429 after 6.3 seconds and again
after its sole retry of 6.2 seconds. Every attempt and result is retained.
These are five coverage gaps, not five approvals; earlier positive reports
apply only to their earlier candidate.

The current positive report is independently adjudicated above. No verified
in-scope blocker remains after the complete internal audit and executable
validation. The reviewer policy permits at most one retry for a transient
failure; it does not require another long retry batch. No additional retry
is launched for the four timed-out reviewers. This decision preserves their
coverage gaps and uses the completed current report with the full primary
validation, rather than treating failed invocations as verdicts. No review
waiver or unanimous production-grade conclusion is claimed.

The final source remains identical to release-build-8's 359-file manifest.
The retained directory contains verified immutable copies of those sources and
all 11 tools, preserving both SDK provenance and the later CLI baseline.
Only review/closure records change after source freeze. Follow-up search maps
all live remaining work to SOW-0121, SOW-0122, SOW-0045, SOW-0160 or SOW-0161;
rejected optional experiments have no remaining delivery promise. SOW-0071's
independent static-guard regression remains separately paused.

The completed SOW moves to done with both status indexes updated. The closing
audit reports SOW initialization complete and clean; whitespace and changed-file
personal-name checks pass. The independent read-only closure check finds no
verified unresolved blocker and identifies one stale SOW-0045 scheduling
sentence, which is corrected. Stage explicit paths and create the combined
signed implementation/lifecycle commit after the final artifact check.
SOW-0121 may activate only after that commit succeeds. No push is authorized.

## Regression - 2026-10-25

### Evidence And Pre-Implementation Gate

The user approved repairing confirmed PR12 findings and individually adjudicating
all60 review threads. SDK repairs execute before CLI SOW-0121; no parallel
implementation SOW is active. At2c12221 the public Go directory facade reproduces
SeekCursor(t beyond newest) success followed by Next returning the first row.
facade.go:563 resets an already exhausted reader with SeekTail, clearing the
directory direction. Preserve observed EOF instead, validating forward EOF and
backward tail behavior for file/directory and filtered/unfiltered callers.
Do not broaden this into the separately paused healthy-refresh SOW-0045.

Rust reader_helpers.rs:265 maps every streaming copy error into decompression
failure, including propagated source/destination I/O. Restore I/O classification
for actual transport/storage errors, retaining malformed/truncated stream errors
as decompression failures. Review core stream decoding because decoder wrappers
can themselves erase source errors. Use injected failing Read/Write tests, not
actual disk exhaustion or production files. Normal decoding must keep existing
streaming behavior and allocations. No dependency or compiler changes.

Inspect other SDK comments with concrete reproducers before accepting them.
TestCursor EOF direct API inconsistency is a separate pending user decision;
facade semantics remain native-compatible. NUL native path construction/serde
consistency and small consumer/probe documentation issues are within reviewed
blast radius; avoid optional refactoring or blanket benchmark/test expansion.
No implementation of an unresolved policy is authorized. Existing completed
SOW specifications and prior failure controls are reused. Risks are position
reversal semantics, decoder error taxonomy and consumer error matching. Fixtures
are synthetic, scratch/logs under.local only. Update relevant specs/docs/skills,
status indexes and thread evidence; perform focused plus representative SDK
validation and internal final review before signed coherent closure.

### User Decision - Direct Cursor EOF

The user selected 1A: both Go direct Reader.TestCursor and
DirectoryReader.TestCursor return false with ErrEndOfEntries when no current
entry exists at EOF. The facade normalizes that error to false, nil, matching
native cursor-test behavior. Preserve errors unrelated to EOF. Cover both
reader types and filtered/unfiltered exhaustion, then successful reverse steps.

### Repair Validation And Remaining Decision

Implemented the approved direct cursor EOF contract and facade normalization.
The directory reader uses a private EOF sentinel, so it explicitly translates
that sentinel to the public ErrEndOfEntries. The four facade regression cases
exercise file/directory and filtered/unfiltered EOF, repeated forward steps,
direct/facade cursor tests, and reverse selection of the final row.

Rust stream errors now inspect decoder error sources on failure only. Injected
source PermissionDenied and destination StorageFull retain I/O categories;
truncated streams remain decoding failures. Healthy streaming loops and buffers
are unchanged. Native raw path construction rejects NUL consistently; the probe
preserves array-shaped field output on failure. Consumer documentation clarifies
EOF, strict verification and caller-excluded indexed snapshot capture, and the
adapter help lists inspect-reader.

Validation on the current repair source:

- Pure Go journal and adapter suites pass (137.480 s and 0.005 s).
- Rust core, registry and SDK suites, including doctests, pass.
- Shared reader-hardening suite passes 552 observations against pinned v262.
- All wiki pages validate and executable documentation examples pass.
- Internal final-review of this bounded repair diff finds no blocking defect;
  its verdict excludes the separately investigated shared-cursor issue below.
- Healthy per-entry work is unchanged. New error-source traversal occurs only
  on decoding failure, cursor error normalization occurs on metadata calls,
  and path validation occurs during construction. No broad benchmark expansion
  is needed for these changes.

Review item 13 is now reproduced using advanced public core JournalCursors
sharing one JournalFile: both read two surviving entries, repair restores the
third, cursor A sees it, and cursor B remains at EOF after shared recovery clears.
The reproducer uses doc-hidden recovery initialization, not independent high-level
readers. Per-filter observed recovery revision versus an explicit-rebuild-only
advanced contract is awaiting a user design decision. Do not close this SOW or
advance CLI implementation until resolved. Item 32 is disproven as stated:
filtered EOF retains its last selected index; the questioned past-end branch
represents explicit tail/raw state, so moving it back would alter positioning.

Raw synthetic logs and reproduction sources remain under .local/pr12-sdk-triage.
The published PR remains at 2c12221; its native jobs pass, while Codacy upload
and remote-configuration failures remain separately recorded, not counted as
successful checks. No new push has occurred.

### Scope Correction - Corruption Handling Versus Synthetic Restoration

The user clarified that SDK behavior must mimic systemd: readers extract readable
data without modifying source journals; corruption is not an invitation to repair
source bytes in place. The prior per-filter revision recommendation and associated
1A/1B question are withdrawn pending realistic supported-operation evidence.
The shared-cursor reproduction restores damaged bytes through a separate test
writer. It proves behavior under synthetic restoration, not normal journald
corruption handling. It is not sufficient evidence for added runtime bookkeeping.

Upstream systemd/systemd @ 8cc40e0c5e9234bf45084751ac53b1fbfe70b492:
src/shared/journal-file-util.c:487 rejects or renames/replaces corrupt files;
src/journal/journald-manager.c:907 selects rotation on corruption;
src/libsystemd/sd-journal/sd-journal.c:1699 opens readers O_RDONLY.
The format concurrency contract separately allows temporarily inconsistent
structures during ordinary publication. That must not be conflated with rewriting
corrupt entries. Audit shipped source-write behavior and repair-specific promises,
tests and runtime costs before any further recovery-design implementation.
No per-filter recovery revision implementation has been made.

### Approved Scope Audit - 2026-10-25

The user approved the correction audit. Preserve default read-only intact-prefix
recovery and ordinary publication semantics; do not treat externally rewriting
corrupt entries as a supported lifecycle or a reason for runtime overhead.
First remove consumer/spec promises that instruct repair/retry and correct the
runtime skill's test interpretation. Retain synthetic fault-injection evidence
only as such, not as proof of a journald publication sequence. Review existing
requalification/state rollback before removing it: these mechanisms can also
protect initial recovery, error atomicity and Boolean-query completeness.
No new source-repair API, synchronization, revision counter or per-entry check
is authorized. Independent read-only audits cover runtime causality and test
classification. Validate docs, test evidence and any actual code changes, with
an internal final review of the corrected scope. Historical SOW evidence remains
intact and is superseded by this explicit scope correction.

### Recovery Scope Audit Results

Independent runtime and test audits confirm no source-byte reconstruction in
reader recovery or writer reopen. Readers use read-only descriptors/mappings;
whole-file decompression writes a separate staging file. Writer open rejects
observed invalid committed bounds/tail rather than reconstructing them. This
is not a claim of full journald writer-lifecycle parity: core ONLINE reopen and
optional directory policy remain existing explicitly scoped SDK behavior.

Corrected product scope, Reader-APIs, Go API and the compatibility skill to remove
promised automatic refresh after arbitrary external repair. Fault-restoration
controls remain labelled synthetic robustness evidence; they do not establish
an upstream requirement. Renamed the misleading same-size Publication Go test
to SyntheticRestoration. The shared 552-observation suite constructs static
mutations before opening readers and remains valid static recovery evidence.
Actual Writer.Append publication and injected transport-error retry controls
remain distinct. Do not mechanically rename synthetic restoration as publication.

Retain initial prefix discovery, effective counts/tail metadata, posting clipping,
stable Boolean qualification, pending-builder/error rollback and directory-change
notification: these serve initial damaged-file reads without later reconstruction.
Retain explicit header refresh and copied-window invalidation: real appends into
preallocated space can change index bytes without changing file size.

A concrete cost needs an EOF-refresh policy decision: Rust cursor.rs:90 calls
reader_recovery.rs:419 prefix rediscovery on every EOF while recovery is active;
Go reader_filter_step.go:87-98 calls reader_filter_recovery.go:32-69, reloading
ENTRY arrays. Unchanged damaged archived files can therefore incur repeated work
proportional to their readable population. No locks or atomics are involved.
No runtime change to that policy has been made in this correction.

Upstream journal-file.c:2421-2435 initializes ENTRY before linkage; :2289-2298
publishes global ENTRY linkage after a memory fence; :2159-2163 and :2193-2210
publish slots/links before counts; :2303-2324 update tail and DATA links. Zeroing
and restoring committed ENTRY bytes is not this writer sequence. Weak storage
visibility still warrants care before suppressing live rediscovery. The earlier
shared-cursor generation proposal is rejected for its unsupported reproducer;
it is not an unresolved request for user approval.

### Native EOF Evidence And Validation

Both baseline v260.1 sd-journal.c:1011-1016 and CLI target v262
sd-journal.c:1016-1021 skip an exhausted file when direction and declared entry
count are unchanged. This is concrete evidence against treating unchanged-EOF
full-prefix rescans as a systemd requirement. A new user decision proposes the
native eligibility rule for both SDKs, preserving initial recovery and changed
count/direction handling. Implementation awaits that decision; this replaces
neither healthy-filter captured-population policy nor the separate SOW-0045 scope.

Scope-correction checks: 16 wiki pages validate, all executable documentation
examples pass, whitespace checks pass, and focused Go synthetic-restoration,
actual preallocated Writer.Append and accessor-I/O retry controls pass (0.444 s).
Runtime behavior remains unchanged by the scope-correction edits.

Internal scope-artifact review found three remaining repair/retry promises in
Go-API and Rust-API wiki pages. Those are corrected; the broader consumer/spec/
skill search now finds only explicit exclusions or unrelated software/cache
repair references. Re-review passes with no blocker in this bounded correction.
Wiki and whitespace checks pass after the final prose edits; executable example
blocks are unchanged from the passing example run. The SDK SOW remains open for
its outstanding implementation/review work and the native EOF policy decision.

### User Decision - Native EOF Eligibility

The user selected 1A: apply systemd's unchanged direction/count EOF rule to both
SDKs. Include Go raw Next as well as filtered Step; preserve explicit Refresh,
initial damaged-prefix recovery, movement reversal, seeks and changed counts.
Keep EOF memo state outside healthy Go readers in the existing recovery
allocation. Rust's public JournalCursor layout must not change: put terminal
memo state in the private JournalReader wrapper and move automatic EOF refresh
there, retaining core cursor initial-error recovery. The low-level cursor is a
positioning primitive, not a separate automatic repair-following service.
Clear terminal memos on successful movement, seeks and filter changes; never
cache failed refreshes. Compare declared header population, not recovered count.
No cross-thread synchronization or per-filter recovery revisions are introduced.
Validate actual layout, bounded read work at repeated EOF, unchanged source bytes,
actual Writer.Append, reversed movement and both filtered/raw access paths.
Synthetic restoration controls must no longer demand automatic visibility after
an unchanged terminal poll; retain their initial recovery/rollback assertions.

### Native EOF Implementation And Evidence

Implemented the approved rule. Go stores direction/count/validity in its existing
exceptional recovery allocation; raw Next and filtered Step consult it before
repeating terminal work. Explicit Refresh, seeks, filter changes and successful
movement invalidate it. Recovery equality ignores these traversal memo fields,
so directory notifications and rollback compare only actual prefix state.
The healthy filtered path checks the existing recovery pointer before invoking
the counter-read helper. Unchanged recovered EOF reads the declared count once
without rebuilding copied windows or scanning the prefix.

Rust JournalReader owns a private 16-byte optional direction/count memo; it checks
terminal state before cursor traversal. Automatic recovery EOF refresh moved from
the public JournalCursor into that wrapper. The public cursor layout is unchanged,
and its initial-error recovery remains. No mutex, atomic, per-filter revision,
new heap allocation or source-file writes were introduced.

Validation:

- Full Go journal/adapter suite passes (118.935 s); after early-gate refinement,
  focused recovery/filter suite passes (7.114 s).
- Rust SDK 202 tests and core 99 tests pass, with doctest results preserved.
- Rebuilt shared adapters pass all 552 reader-hardening observations against the
  pinned v262 oracle. All executable wiki examples pass; wiki/whitespace checks pass.
- New Go test covers raw/filtered readers in all three access selections,
  bounded reads over ten unchanged EOF polls, identical source bytes, reversal,
  unchanged-count synthetic restoration, and a real appended entry changing count.
  Repeated EOF allocates zero times per poll in all six cases.
- Equivalent Rust raw/filtered control preserves source bytes and validates
  unchanged EOF, reversal and real append count changes.
- Go Reader remains 616 bytes; its exceptional recovery state is 120 bytes
  (the added memo resides only in already allocated recovered state). Rust
  JournalReader is 656 bytes including its 16-byte memo; JournalCursor is 280
  bytes and unchanged. These are local 64-bit layout observations, not API sizes.
- Same-failure search includes Go raw EOF, filtered EOF, explicit Refresh,
  seeks/filter mutation and Rust wrapper/core ownership. Synthetic restore tests
  preserve their initial damaged-prefix assertions and now test unchanged EOF
  before explicit repositioning; they no longer demand repair-following polling.
- Independent internal final-review passes the current implementation, including
  targeted Go/Rust reruns. No external reviewers invoked.

Performance evidence is bounded-work and allocation evidence plus code review,
not a claim of measured throughput equality. Removing population-sized repeated
scans dominates the constant counter check on recovered EOF. Healthy paths do
not scan or allocate for this memo. No broad measurement framework was added.
Remaining unrelated PR findings and CLI work keep this SOW/PR work open; this
entry does not assert entire PR readiness or clear remote Codacy service failures.

### Publication Checkpoint - 2026-10-26

The user explicitly requested committing and pushing the validated current work,
then three independent internal final reviews of different PR aspects. This is
an interim implementation checkpoint, not SOW completion or merge approval.
Preserve the current/in-progress lifecycle because remaining PR findings and
CLI work are not completed. External-model reviews remain waived in favor of
internal reviewers as previously requested. The three review scopes are SDK
correctness/recovery/performance, CLI compatibility/portability, and validation/
consumer contracts/CI; each reviews the entire PR surface relevant to its scope.

Before push, the published head remains 2c12221. Native Linux/macOS/Windows/
FreeBSD checks and Go/Rust coverage generation passed. Codacy SARIF generation
and coverage upload retain the previously diagnosed remote-service failures;
no unrelated CI configuration change is included. Local validation and internal
review evidence for this checkpoint are recorded above.

### Regression Closure - 2026-10-26

The user authorized fixing the remaining review findings. Independent SDK review
of a1144dcbf537 passes after Go focused suites, Rust core99 and SDK202 tests.
Prior shared552/docs35/wiki16 checks and bounded-work/allocation evidence remain
recorded above. No SDK-specific blocker remains. The latest publication did expose
a CLI consumer of the changed facade EOF result; do not claim whole-PR readiness.
That fix, Windows glob/URL defects, explicit oracle qualification and CLI example
paths are tracked by reopened SOW-0121 before proceeding sequentially.

SDK review items: cursor EOF, compressed-stream I/O classification, NUL-path
construction, array-shaped probe diagnostics and public help/docs are implemented.
Shared advanced-cursor synthetic restoration is rejected as a production repair
requirement; native EOF semantics supersede it. Unknown posting completeness,
strict cache traversal and required facet errors remain intentional. Optional
cleanup/extra test cross-products, redundant assertions and test-only dependency
relocation are rejected as unnecessary to the approved outcome; generic DATA
decoder diagnostics remain an existing limitation, not silently accepted data.

Artifact gate: specs, runtime compatibility skill, consumer docs and SOW indexes
are updated. AGENTS.md needs no change: existing systemd/purity/performance rules
already cover this outcome. No exported operator skill exists. The lesson is to
validate a realistic upstream trigger before treating synthetic restoration as a
required lifecycle. Scratch/raw reports remain ignored; durable evidence is
sanitized. No source repair or new host probing was introduced. Existing broader
follow-ups remain SOW-0045/0071/0122/0160/0161; new CLI findings map to real reopened
SOW-0121. The user-requested interim checkpoint explicitly split implementation
publication from this lifecycle handoff. Run the local audit before committing
this transition; SDK and CLI implementation are not executed in parallel.

Final lifecycle audit reports complete and clean. Current Codacy Markdown
findings in this record were corrected before the handoff commit. The unchanged
Bandit execution error predates this repair and remains an explicit coverage
limitation in SOW-0121; no speculative CI configuration change is made.

## Regression - 2026-10-26 - Facade Cursor EOF Classification

### Pre-Implementation Gate

The approved public TestCursor EOF correction exposed a missed consumer contract:
Go SdJournalGetCursor forwards private errEndOfEntries (facade.go:287), while
TestCursor now correctly reports false,nil. Invocation listing and ordinary
cursor output cannot identify EOF using public errors. The user approved fixing
confirmed review findings; normalize facade GetCursor EOF to existing ErrNoEntry
and use that metadata-only result in both CLI consumers. Preserve all other
errors. Direct getters and Rust behavior remain unchanged. This is a prerequisite
to the paused CLI regression; preserve its uncommitted Windows edits untouched.

Reuse existing errors.Is and cursor regression fixtures. Test initial/beyond-tail
facade getters for file/directory inputs, positioned cursors and real errors;
run CLI invocation and cursor controls. No payload expansion, synchronization,
new allocation or file mutation is added. Raw evidence stays ignored under
.local; tests use synthetic journals. Update facade contract spec and status
indexes; no operator action or dependency changes are needed. Review the narrow
SDK correction before closing this prerequisite and resuming CLI work. No open
design choice is needed to expose the existing public no-entry error.

The shared beyond-tail control also reproduces Rust CLI `--cursor` reporting
`unset cursor` from its entry getter. Its cursor getter already exposes NoEntry.
Apply the same metadata-only error fallback to the Rust CLI, preserving the
original error unless cursor metadata confirms absence. This is the same
consumer failure class, with no Rust SDK change. Portable smoke now includes
both cursor options and updates its observation count.

Runtime verification disproved the initial assumption that every Rust facade
cursor getter already normalized absence: directory seek-tail can leave its
selected file at UnsetCursor, mapped to generic Other. Normalize UnsetCursor
only in facade get_cursor to NoEntry, matching Go, and add file/directory
regression controls. Do not broaden the global error mapper or suppress I/O.
The earlier focused reviewer PASS is superseded pending this runtime correction.

### Facade Cursor Regression Closure - 2026-10-26

Both facade cursor getters now expose typed absence; unrelated errors remain
unchanged. Both CLI cursor consumers and Go invocation listing use metadata
checks on mismatch/error paths. Go focused pure-SDK cursor tests and full CLI
unit tests pass; Rust all 19 facade tests and CLI 27 unit/11 subprocess tests
pass. Shared portable smoke passes 134/134 across both commands, including new
future cursor and after-cursor controls. Pinned v262 independently returns empty
success for both options. Internal final review passes the corrected scope after
the earlier Rust assumption was disproved by execution. No payload expansion,
locking, file writes or successful SDK path reads were added; no benchmark
framework or throughput claim is warranted for this error-only correction.

Same-failure search covered both production Go TestCursor consumers and Rust
cursor getter delegation through directories. Specs document the public getter
classification. Existing consumer docs promise native-style cursor behavior;
this repairs that promise without a new workflow or example, so no docs or
operator-skill update is required. Runtime project skills already require EOF,
facade, directory and cross-language controls; no new workflow rule is needed.
AGENTS.md remains applicable. Both lifecycle indexes are reconciled. Synthetic
fixtures and ignored logs satisfy the sensitive-data gate. The lesson is to
execute error-mapping tests through delegated file readers, not infer directory
behavior from its outer bounds check. Remaining Windows/oracle/docs findings
stay in the real reopened SOW-0121. No untracked deferred SDK repair remains.
The local audit reports complete and clean; implementation and completed
lifecycle are committed together before resuming CLI work.

## Regression - 2026-10-26 - Filtered Sequence Cursor Exhaustion

### Pre-Implementation Gate

The explicit-v262 query matrix passes 3033/3034 observations; Go's filtered
single-file partial sequence cursor beyond the tail returns the last row.
SdJournalOpenFiles selects a direct Reader for one path. Filtered Step preserves
its committed row at EOF, and the duplicated facade cursor-seek loop returns
without invalidating that row. Direct Reader.SeekCursor already clears position
with SeekTail at exhaustion. Reuse that existing seeker for the single-reader
facade; retain the directory loop and its exhausted-direction guarantee. This
preserves normal traversal EOF semantics and adds no synchronization or payload
work. The user authorized confirmed fixes; no new product choice is needed.

Pause CLI work and preserve its edits. Add file/directory filtered future-sequence
controls, reproduce before the fix, verify forward EOF and backward last row,
then rerun existing cursor tests and the failed oracle case. Review the narrow
SDK change before closing; rerun the full CLI matrix after resuming0121. Existing
specs already require native seek behavior; no contract change is intended.
Keep synthetic fixtures and ignored raw evidence, update both lifecycle indexes,
and record unchanged docs/skills rationale at close.

The pre-fix regression also proves directory reversal skips the last match:
its retained currentKey anchors reverse movement even though seek found no row.
On directory cursor-seek exhaustion only, reuse clearDirectoryPosition to clear
that anchor while retaining the exhausted forward direction and child state.
Repeated Next must stay exhausted; Previous now resets from the tail. Ordinary
filtered iteration EOF retention is intentionally unchanged.

### Filtered Sequence Cursor Closure - 2026-10-26

The regression test failed before the fix for both stale file position and
skipped directory reversal. It now passes for a file, one-file directory and
overlapping two-file directory. Focused pure-Go facade/cursor and related
filter/directory tests pass. The original matrix command now returns empty
success. Independent internal final review passes 12 combinations: single-file,
deduplicated directory and overlapping merge, filtered/unfiltered, future time
and sequence cursors; each checks absent cursor, three repeated forward EOF
calls and reversal to the last row. No extra child seeks are necessary: existing
direction-change handling supplies the tail boundary after clearing the anchor.

Same-pattern analysis covers direct versus facade seekers and both directory
traversal strategies. Ordinary filtered EOF behavior remains unchanged. Reusing
the direct seeker removes duplicated work; the directory correction changes two
fields only after an unsuccessful seek. No benchmark or performance waiver is
needed. Existing specs, consumer docs, AGENTS.md and runtime skills already
require these contracts and controls, so this correction needs no further
artifact change beyond tests, evidence and lifecycle indexes. No operator skills
exist. Synthetic evidence is retained ignored; no personal or operational data
is added. The lesson is to distinguish failed seek boundaries from ordinary
iteration EOF. Remaining CLI validation is owned by reopened0121; no SDK item
is deferred. Audit must remain complete and clean before this coherent commit.
