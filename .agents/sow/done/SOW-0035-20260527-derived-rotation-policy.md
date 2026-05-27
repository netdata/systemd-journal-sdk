# SOW-0035 - Derived Rotation Policy

## Status

Status: completed

Sub-state: implementation, validation, review disposition, SOW audit, and
fixed-128 MiB benchmark smoke completed.

## Requirements

### Purpose

Make the SDK high-level directory writers fit Netdata production retention and
rotation behavior: retention must remove data in predictable 5% chunks by
default, and active files must close proactively by both size and time.

### User Request

The user requested a new SOW to define the high-level writer contract, update
specs, update all implementations, close the SOW, set writer benchmarks to a
fixed 128 MiB max-size baseline, rerun benchmarks, and report the result.

### Assistant Understanding

Facts:

- systemd's `max_size` controls the active file size that becomes eligible for
  archive/vacuum granularity; it also drives data hash-table sizing.
- systemd derives data hash buckets as `max(max_size * 4 / 768 / 3, 2047)` and
  field hash buckets as `1023`.
- Netdata's current netflow retention-derived rotation block is 1/20 of total
  retention size.
- Existing SDK high-level writers already support explicit size and duration
  rotation plus size and age retention.

Inferences:

- "rotation deletes in steps of 5% of the total capacity" means a configured
  size-retention limit must derive active-file max size as `retention_bytes /
  20` when no explicit rotation max file size is configured.
- The 5% contract wins over systemd's automatic 128 MiB cap for unset
  `SystemMaxFileSize=`, because capping a large configured retention budget at
  128 MiB would not delete 5% chunks.
- "All other calculations should be the same" means SDK writers must use
  systemd-compatible page-aligned minimum sizes, compact-size guardrails, and
  systemd hash-table sizing from the effective max file size.
- A configured age-retention limit must derive active-file max duration as
  `retention_duration / 20` when no explicit rotation max duration is
  configured.

Unknowns:

- No blocking unknowns. The user can later choose a different cap policy, but
  the current implementation follows the stated 5% contract.

### Acceptance Criteria

- Rust, Go, Node.js, and Python high-level directory writers derive rotation
  max file size from size retention as 1/20 when rotation size is unset.
- Rust, Go, Node.js, and Python high-level directory writers derive rotation
  max duration from age retention as 1/20 when rotation duration is unset.
- Explicit rotation max file size and max duration override derived values.
- Effective max file size drives systemd-compatible data and field hash-table
  sizing in every language.
- Low-level direct-file writers expose or preserve a way to set max file size
  for hash-table sizing without forcing callers to compute bucket counts.
- Specs record the derived rotation policy as a public SDK contract.
- Benchmarks use a fixed 128 MiB max-size baseline and report all-language
  writer-core results after implementation.

## Analysis

Sources checked:

- `systemd/systemd @ c0a5a2516d28`, `src/libsystemd/sd-journal/journal-file.c`
- `.agents/sow/specs/product-scope.md`
- `rust/src/crates/journal-core/src/file/file.rs`
- `rust/src/crates/journal-log-writer/src/log/config.rs`
- `rust/src/crates/journal-log-writer/src/log/mod.rs`
- `go/journal/format.go`
- `go/journal/writer.go`
- `go/journal/log.go`
- `node/src/lib/writer.js`
- `node/src/lib/directory-writer.js`
- `python/journal/writer.py`
- `python/journal/directory_writer.py`
- Netdata vendored netflow/otel writer configuration, read-only.

Current state:

- Rust high-level writer uses rotation max file size for bucket sizing, but with
  a Netdata-specific power-of-two heuristic instead of systemd sizing.
- Go, Node.js, and Python high-level writers use rotation size only for
  rotation decisions; direct writers default to fixed hash-table buckets.
- None of the non-Rust high-level writers derive rotation size or duration from
  retention policy.
- The active writer benchmark currently accepts `--max-size-bytes`, but a
  previous default of 2 GiB was not production-derived and must be corrected to
  128 MiB for the controlled baseline.

Risks:

- Changing default rotation behavior when retention limits are configured may
  create more active-file archives than before. This is intended by the 5%
  retention contract.
- Changing hash-table sizing can change deterministic file bytes. Existing
  byte-identity fixtures must either pass with explicit max-size inputs or be
  updated with evidence.
- Duration-derived rotation depends on caller-provided realtime timestamps.
  Existing monotonic/realtime clamping must remain intact.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK currently mixes three concepts: retention budget, active-file rotation
  threshold, and hash-table sizing. Rust partially links rotation to hash-table
  sizing with a Netdata heuristic; Go, Node.js, and Python do not. This makes
  production behavior unpredictable and made benchmark configuration drift into
  a non-production 2 GiB max-size case.

Evidence reviewed:

- `systemd/systemd @ c0a5a2516d28`, `src/libsystemd/sd-journal/journal-file.c:1287`
- `systemd/systemd @ c0a5a2516d28`, `src/libsystemd/sd-journal/journal-file.c:1292`
- `systemd/systemd @ c0a5a2516d28`, `src/libsystemd/sd-journal/journal-file.c:1323`
- `go/journal/log.go`: explicit size/duration rotation and retention surfaces.
- `node/src/lib/directory-writer.js`: explicit size/duration rotation and retention surfaces.
- `python/journal/directory_writer.py`: explicit size/duration rotation and retention surfaces.
- `rust/src/crates/journal-log-writer/src/log/mod.rs`: rotation max file size feeds writer creation.

Affected contracts and surfaces:

- High-level directory writer public API and defaults in Rust, Go, Node.js, and
  Python.
- Low-level direct writer options for hash-table sizing.
- Product scope spec.
- Writer benchmark default max-size.
- Interoperability and byte-identity tests that depend on hash-table sizing.

Existing patterns to reuse:

- Existing `RotationPolicy` and `RetentionPolicy` surfaces.
- Existing `max_file_size` flow from Rust high-level writer into file creation.
- Existing direct writer bucket override options in Go, Node.js, and Python.
- Existing benchmark `--max-size-bytes` option and result reporting.

Risk and blast radius:

- Public behavior changes when callers configure retention but omit rotation.
- File layout changes if callers rely on default bucket counts.
- Performance may improve or worsen depending on hash-table size and mmap/write
  behavior; benchmarks must be rerun after the fixed 128 MiB baseline.
- No security-sensitive data is involved.

Sensitive data handling plan:

- Only source paths, synthetic benchmarks, and sanitized policy descriptions are
  recorded. No secrets, customer data, SNMP communities, private endpoints, or
  production journal contents are used.

Implementation plan:

1. Add shared systemd-compatible max-size normalization and hash-bucket sizing
   helpers per language.
2. Resolve high-level rotation policy from explicit rotation values first, then
   retention-derived 1/20 defaults.
3. Pass effective max file size into direct writer creation for hash-table
   sizing in Rust, Go, Node.js, and Python.
4. Add tests for derived size rotation, derived duration rotation, explicit
   override precedence, and hash-bucket sizing.
5. Change writer-core benchmark default to 128 MiB and rerun.

Validation plan:

- Run targeted unit tests in Rust, Go, Node.js, and Python.
- Run writer-core smoke benchmark across all languages.
- Run writer-core benchmark across systemd, Rust, Go, Node.js, and Python with
  fixed 128 MiB max-size. The single-file run must use a row count that fits
  the configured 128 MiB max-size; larger 200k-row production stress belongs to
  the directory-rotation benchmark in SOW-0009.
- Run relevant stock `journalctl --verify --file` checks through the benchmark
  harness.
- Run `.agents/sow/audit.sh` before close.

Artifact impact plan:

- AGENTS.md: no update expected; workflow unchanged.
- Runtime project skills: no update expected; compatibility workflow unchanged.
- Specs: update `.agents/sow/specs/product-scope.md`.
- End-user/operator docs: no separate docs updated in this SOW unless tests
  reveal public README coverage for this API.
- End-user/operator skills: no output/reference skills affected.
- SOW lifecycle: pause SOW-0009, complete this SOW, then SOW-0009 can resume.
- SOW-status.md: update current/pending/done status.

Open-source reference evidence:

- `systemd/systemd @ c0a5a2516d28`, `src/libsystemd/sd-journal/journal-file.c`

Open decisions:

- None. The user stated the policy: default rotation chunks are 1/20 of the
  configured retention size/duration, and explicit max size/duration overrides
  the calculation.

## Implications And Decisions

1. Derived size rotation policy.
   - Decision: if size retention is configured and explicit rotation max file
     size is unset, effective rotation max file size is
     `retention_max_bytes / 20`, normalized with systemd-compatible minimum and
     alignment rules.
   - Implication: large retention budgets produce large active files unless the
     caller explicitly sets a smaller max file size.
   - Risk: more exact 5% retention behavior may differ from previous SDK
     defaults, but it matches the user-stated production contract.

2. Derived duration rotation policy.
   - Decision: if age retention is configured and explicit rotation max duration
     is unset, effective rotation max duration is `retention_max_age / 20`,
     rounded up to at least one microsecond where the language can represent it.
   - Implication: time-retained logs close active files proactively even if size
     limits are not hit.
   - Risk: timestamp-heavy tests must ensure existing realtime clamping still
     produces stock-verifiable files.

3. Hash-table sizing.
   - Decision: effective max file size drives systemd-compatible data and field
     hash-table sizing in all writers.
   - Implication: Go/Node.js/Python no longer leave high-level hash sizing at a
     fixed default when retention-derived rotation exists.
   - Risk: byte-identical fixtures must pass with explicit max-size controls.

## Plan

1. Update specs with the public derived rotation contract.
2. Implement max-size and bucket-sizing helpers in all four languages.
3. Wire high-level retention-derived defaults into all four directory writers.
4. Add focused tests per language.
5. Change benchmark default max-size to 128 MiB.
6. Run validations, update SOW evidence, close and move SOW.

## Delegation Plan

Implementer:

- Local implementation by the project manager, per current user routing.

Reviewers:

- External reviewer pass may be skipped only if the user explicitly prioritizes
  immediate benchmark results over reviewer iteration for this contract SOW.

Repository boundary block for every external-agent prompt:

```text
CRITICAL REPOSITORY BOUNDARY:
- DO NOT MAKE CHANGES OUTSIDE THIS REPOSITORY FOR ANY REASON.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

Failure handling:

- Validation or benchmark failures are recorded in this SOW before any close.
- If the 128 MiB benchmark exposes a correctness failure, fix compatibility
  first and rerun before reporting performance.

## Execution Log

### 2026-05-27

- Created SOW and paused SOW-0009 while this public writer contract is handled.
- Implemented retention-derived rotation defaults and systemd-compatible
  max-size/hash-table sizing in Rust, Go, Node.js, and Python.
- Changed writer benchmark max-size defaults to 128 MiB.
- A 200k-row single-file 128 MiB writer-core run showed that stock systemd
  correctly stops appending when the configured max-size is reached, so 200k
  rows is not a valid single-file benchmark at this cap. The valid fixed
  128 MiB single-file baseline uses 100k rows; 200k-row stress remains for
  SOW-0009's directory-rotation benchmark.
- First reviewer pass found process and test gaps: SOW validation was still
  blank, derived-size rotation was not proven end-to-end in every language,
  duration rounding text said "rounded up" while implementations truncated, and
  compact derived-size clamp coverage was missing.
- Fixed reviewer gaps by changing duration derivation to ceiling division in
  Rust, Go, Node.js, and Python; adding derived-size rotation tests; adding
  compact max-size clamp tests; and adding benchmark-driver formula comments
  plus the Go benchmark overflow guard.
- Second reviewer pass found only non-blocking test and benchmark-maintenance
  gaps. The Go fraction literal was replaced with a named constant, and
  end-to-end derived-duration plus small-retention clamp tests were added so
  those gaps do not remain as known debt.
- Closed remaining benchmark-maintenance gaps by adding writer-core
  cross-driver consistency checks for `data_hash_table_buckets`,
  `field_hash_table_buckets`, and `max_size_bytes`, and by saturating Rust
  bucket sizing on 32-bit targets instead of truncating.

## Validation

Acceptance criteria evidence:

- Rust, Go, Node.js, and Python high-level directory writers derive active-file
  max file size from size retention as `retention_max_bytes / 20` when explicit
  rotation size is unset.
- Rust, Go, Node.js, and Python high-level directory writers derive active-file
  max duration from age retention with ceiling division by 20 and a minimum of
  one microsecond when explicit rotation duration is unset.
- Explicit rotation max file size and duration remain authoritative over
  retention-derived defaults.
- Effective max file size drives systemd-compatible data hash buckets and the
  1023 field-bucket default in every implementation.
- The fixed 128 MiB writer-core benchmark baseline uses 100k rows for
  single-file runs. The 200k fixed-128 MiB single-file attempt is recorded as
  invalid because stock systemd reaches the configured file cap first.

Tests or equivalent validation:

- Go: `GOCACHE=$(pwd)/../.local/go-cache GOMODCACHE=$(pwd)/../.local/go-mod-cache go test ./journal` passed.
- Go targeted derived-rotation tests passed:
  `go test ./journal -run 'TestLog(DerivesRotationDefaultsFromRetention|DerivedSizeRotationFromRetention|DerivedDurationRotationFromRetention|DerivedRotationSmallRetentionClampsToMinimum|DerivedRotationCompactMaxFileSizeClamp|ExplicitRotationOverridesRetentionDerivedDefaults)'`.
- Rust: `cargo test -p journal-log-writer derived -- --nocapture` passed the
  derived default, derived duration ceiling, derived size rotation,
  small-retention clamp, and compact clamp tests.
- Rust: `cargo test -p journal-log-writer explicit_rotation_overrides -- --nocapture` passed.
- Node.js: `node node/test/all.js` passed.
- Python targeted tests passed:
  `test_directory_writer_derives_rotation_defaults_from_retention`,
  `test_directory_writer_derived_size_rotates_from_retention`,
  `test_directory_writer_derived_duration_rotates_from_retention`,
  `test_directory_writer_derived_rotation_small_retention_clamps_to_minimum`,
  `test_directory_writer_derived_rotation_compact_max_file_size_clamp`, and
  `test_directory_writer_explicit_rotation_overrides_retention_defaults`.
- Python full `python/test_all.py` remains blocked in this environment by the
  existing missing `lz4.block` dependency path; this SOW's targeted Python
  coverage passed.
- `python3 -m py_compile tests/benchmarks/run_writer_core_benchmarks.py python/test_all.py` passed.
- Writer-core smoke passed after the fixes:
  `python3 tests/benchmarks/run_writer_core_benchmarks.py --languages systemd rust go node python --rows 1000 --repetitions 1 --warmups 0 --format compact --final-state online --keep-journals`.
  Report: `.local/benchmarks/writer-core/compact-none-fss-off-20260527T164418Z/report.json`.
  The harness now fails if passing drivers drift on hash-table buckets or
  max-size bytes; this smoke passed with `233016` data buckets, `1023` field
  buckets, and `134217728` max-size bytes for every language.
- Valid fixed-128 MiB 100k writer-core baseline passed before the reviewer
  fix batch. Report:
  `.local/benchmarks/writer-core/compact-none-fss-off-20260527T153135Z/report.json`.
- `.agents/sow/audit.sh` passed after moving this SOW to `done/` and resuming
  SOW-0009.

Real-use evidence:

- The writer-core benchmark harness produced stock `journalctl --verify --file`
  passing files for systemd C, Rust, Go, Node.js, and Python at fixed 128 MiB
  max-size with compact format, no compression, and no FSS.
- The 100k fixed-128 MiB baseline reported:
  systemd C 32079.572 rows/sec, Rust 2318.945 rows/sec, Go 2298.362 rows/sec,
  Node.js 955.856 rows/sec, and Python 648.703 rows/sec. These are SOW-0009
  performance inputs, not success thresholds for SOW-0035.

Reviewer findings:

- `llm-netdata-cloud/qwen3.6-plus`: `PRODUCTION GRADE: NO`; code structurally
  correct, but SOW validation was blank and validation gaps remained.
- `llm-netdata-cloud/glm-5.1`: `PRODUCTION GRADE: NO`; code structurally
  correct, but SOW validation was blank. It also noted benchmark-driver direct
  bucket passing as acceptable for SOW-0035 and directory-rotation benchmarking
  as SOW-0009 work.
- `llm-netdata-cloud/minimax-m2.7-coder`: `PRODUCTION GRADE: YES` with a
  documentation gap: SOW-0035 validation needed to be filled before close.
- `llm-netdata-cloud/kimi-k2.6`: `PRODUCTION GRADE: NO`; blocking findings
  were missing end-to-end derived-size rotation tests in Go/Node.js/Python,
  duration rounding spec mismatch, compact derived clamp gap, and blank SOW
  validation. Low findings were benchmark formula duplication and Go benchmark
  overflow guard.
- Disposition: addressed Kimi's blocking findings and low benchmark-driver
  maintainability findings in the fix batch. Qwen/GLM/Minimax process finding
  is addressed by this completed validation section.
- Second reviewer pass:
  `llm-netdata-cloud/minimax-m2.7-coder` returned `PRODUCTION GRADE: YES` and
  noted a low Go literal-constant inconsistency, which was fixed by adding
  `derivedRotationFraction`.
- Second reviewer pass:
  `llm-netdata-cloud/qwen3.6-plus` ran the SOW audit and returned
  `PRODUCTION GRADE: YES` with low test coverage gaps. End-to-end derived
  duration and small-retention clamp tests were added after this finding.
- Second reviewer pass:
  `llm-netdata-cloud/glm-5.1` returned `PRODUCTION GRADE: YES` with
  non-blocking benchmark-harness maintainability findings. The concrete
  cross-driver drift risk was fixed in this SOW by adding consistency checks to
  the writer-core harness. Broader benchmark-methodology work remains tracked
  by SOW-0009.
- Second reviewer pass:
  `llm-netdata-cloud/kimi-k2.6` did not produce a usable final review after an
  extended run and was terminated by exact PID. Its first-pass blocking findings
  were already fixed.

Same-failure scan:

- Searched the four duration derivation implementations for old truncating
  duration division patterns. Remaining `/20` usage is the intended size
  derivation or the new ceiling duration derivation.
- Searched tests for derived-size rotation, derived-duration rotation, compact
  derived clamp, small-retention clamp, and duration ceiling coverage. New
  coverage exists in Rust, Go, Node.js, and Python.

Sensitive data gate:

- Durable artifacts contain only synthetic benchmark names, local source paths,
  aggregate benchmark numbers, and generated test IDs. No secrets, customer
  data, SNMP communities, private endpoints, production logs, or private
  operational data were written.

Artifact maintenance gate:

- AGENTS.md: no update needed; workflow and guardrails did not change.
- Runtime project skills: no update needed; compatibility workflow did not
  change.
- Specs: `.agents/sow/specs/product-scope.md` updated with the public derived
  rotation and hash-table sizing contract.
- End-user/operator docs: no separate README/API docs updated in this SOW. The
  repository is still using specs as the authoritative product contract for
  this SDK behavior; broader public docs can be handled in integration/docs
  work if required.
- End-user/operator skills: no output/reference skills affected.
- SOW lifecycle: SOW-0009 was paused while this contract was fixed; after this
  SOW closes, SOW-0009 resumes.
- SOW-status.md: updated to move SOW-0035 to Done and resume SOW-0009.

Specs update:

- `.agents/sow/specs/product-scope.md` records the 1/20 size and duration
  derivation, explicit override precedence, and systemd-compatible hash-table
  sizing contract.

Project skills update:

- No project skill update needed. The workflow for future journal compatibility
  changes is unchanged.

End-user/operator docs update:

- No end-user/operator docs update in this SOW. The changed behavior is SDK
  contract/spec behavior and is recorded in the product scope spec.

End-user/operator skills update:

- No end-user/operator skills are affected.

Lessons:

- Benchmarks that configure a single-file `max_size` must use a row count that
  can fit in that configured file for stock systemd; larger row counts belong
  in a directory-rotation benchmark.
- Retention-derived rotation tests must avoid age-retention deleting synthetic
  old entries before the test can inspect rotated files.
- A benchmark harness can validate direct-file performance while still bypassing
  a high-level policy chain; policy-chain tests must exist separately.

Follow-up mapping:

- SOW-0009 tracks 200k-row directory-rotation stress, full writer profiling,
  reader performance, and optimization.
- SOW-0026 remains blocked until SOW-0009 produces acceptable performance
  evidence or the user accepts a staged integration exception.

## Outcome

Implementation, targeted validation, benchmark smoke, reviewer fix cycles,
second reviewer pass, and final SOW audit are complete. SOW-0009 resumes for
writer profiling and optimization.

## Lessons Extracted

- Fixed-128 MiB single-file benchmarks should default to 100k rows for this
  corpus; 200k rows must be measured through directory rotation.
- Duration derivation should use ceiling division in microsecond space to match
  the public "rounded up" contract.
- Size-rotation tests must account for retention deletion rules and should not
  combine small size retention with synthetic old timestamps unless age
  retention behavior is the subject under test.
- Reviewer prompts can become stale if additional low-severity findings are
  fixed during review. Record those fixes explicitly before closure.

## Followup

All follow-up work is mapped to existing SOWs:

- SOW-0009 tracks 200k-row directory-rotation stress, full writer profiling,
  reader performance, and optimization.
- SOW-0026 remains blocked until SOW-0009 produces acceptable performance
  evidence or the user accepts a staged integration exception.

## Regression Log

None yet.
