# SOW-0102 - Netdata Function Source Selector Labels

## Status

Status: completed

Sub-state: implemented, locally validated, reviewed by the full approved reviewer pool, and ready for release packaging.

## Requirements

### Purpose

Make the Netdata-compatible journal function API fit multiple SDK consumers by allowing each consumer to name and describe the `__logs_sources` selector in its own domain language while preserving the existing systemd-journal defaults.

### User Request

The Netdata SNMP traps function receives one journal directory per trap job. The SDK currently names the `__logs_sources` selector `Journal Sources`, but the SNMP traps UI should show `Trap Jobs`. Open an SDK SOW to make this customizable per SDK consumer.

### Assistant Understanding

Facts:

- The Rust and Go SDKs both expose `NetdataFunctionConfig`.
- Rust and Go `NetdataFunctionConfig` already let consumers customize default
  facets, default view keys, and default histogram field.
- Rust and Go still hardcode the required source selector metadata for
  `__logs_sources`.
- The Netdata SNMP traps consumer wants the selector label `Trap Jobs`.
- The existing systemd-journal consumer should keep the default label `Journal Sources`.
- The user decided on 2026-06-11 that this must be a common SDK API, not a
  Go-only extension.

Inferences:

- The clean end state is an SDK API extension, not a Netdata-side response rewrite.
- The API should expose consumer-configurable source selector label/help fields
  with backward-compatible defaults.
- Rust and Go must expose the same API concept and semantics, using
  language-idiomatic names only where language style requires it.

Unknowns:

- Whether Node.js and Python expose equivalent Netdata function wrappers today.
  Implementation must verify and keep language parity if they do.

### Acceptance Criteria

- Rust and Go SDK consumers can set the `__logs_sources` selector name to
  `Trap Jobs` through the same API concept.
- Existing Rust and Go consumers that do not set the new option still receive
  `Journal Sources`.
- Selector help text is configurable in Rust and Go using the same API concept,
  or the SOW records evidence why name-only customization is sufficient.
- Tests cover default behavior and custom consumer behavior in Rust and Go.
- Public API documentation documents the new configuration field(s) for Rust
  and Go.
- No Netdata-specific post-processing is required to rename `__logs_sources`.

## Analysis

Sources checked:

- `rust/src/journal/src/netdata.rs`
- `rust/src/journal/src/netdata.rs` tests
- `go/journal/netdata.go`
- `go/journal/netdata_test.go`
- `AGENTS.md`
- `.agents/sow/SOW.template.md`
- `.agents/sow/SOW-status.md`

Current state:

- `rust/src/journal/src/netdata.rs` defines `NetdataFunctionConfig` with
  `default_facets`, `default_view_keys`, and `default_histogram`.
- `rust/src/journal/src/netdata.rs` builds the `__logs_sources` required
  parameter with hardcoded `name: "Journal Sources"` and `help: "Select the
  logs source to query"`.
- `go/journal/netdata.go` defines `NetdataFunctionConfig` with `DefaultFacets`, `DefaultViewKeys`, and `DefaultHistogram`.
- `go/journal/netdata.go` builds the `__logs_sources` required parameter with hardcoded `name: "Journal Sources"` and `help: "Select the logs source to query"`.
- No Node.js or Python Netdata function wrapper surface was found in the first
  repo search; implementation must verify before close.
- Netdata SNMP traps needs the same selector to represent trap jobs, not generic journal sources.

Risks:

- A Netdata-side rewrite would hide an SDK limitation and create per-consumer response patching.
- Renaming `__logs_sources` itself would break existing request selections; only the display metadata should be customizable.
- Changing the default label would affect existing systemd-journal function users; defaults must stay unchanged.

## Pre-Implementation Gate

Status: ready

Problem / root-cause model:

- The SDK makes a consumer-specific display decision inside generic Netdata function metadata. The field id `__logs_sources` is generic enough for API compatibility, but the displayed name/help are domain-specific. This causes SNMP traps to show `Journal Sources` for what are actually trap jobs.

Evidence reviewed:

- `rust/src/journal/src/netdata.rs` config surface has existing default
  customization fields for facets, view keys, and histogram.
- `rust/src/journal/src/netdata.rs` hardcodes `__logs_sources` name/help in
  `required_source_params`.
- `go/journal/netdata.go` config surface has existing default customization fields for facets, view keys, and histogram.
- `go/journal/netdata.go` hardcodes `__logs_sources` name/help in `requiredSourceParams`.
- Netdata SNMP traps uses SDK source metadata where source names are trap job names.

Affected contracts and surfaces:

- Rust SDK public API: `NetdataFunctionConfig`.
- Go SDK public API: `NetdataFunctionConfig`.
- Netdata-compatible function info responses: `required_params`.
- Netdata consumers using `__logs_sources` selections.
- SDK documentation for the Netdata function API.
- Potential Node.js and Python Netdata function wrapper parity if equivalent
  APIs exist.

Existing patterns to reuse:

- Keep backward-compatible default behavior through
  `NetdataFunctionConfig::systemd_journal()` in Rust.
- Keep backward-compatible default behavior through
  `SystemdJournalNetdataFunctionConfig()` in Go.
- Follow existing `DefaultFacets`, `DefaultViewKeys`, and `DefaultHistogram` config style.
- Preserve the wire id `__logs_sources`; only metadata label/help should vary.
- Keep the Rust and Go API concepts common. Names may be idiomatic for each
  language, but semantics and defaults must match.

Risk and blast radius:

- Low if implemented as additive optional config with default values.
- Medium if Rust and Go diverge in field names, semantics, defaults, or emitted
  function metadata.
- Medium if Node.js or Python wrappers exist and are left inconsistent.
- UI behavior changes only for consumers that explicitly set the new option.

Sensitive data handling plan:

- No sensitive data is needed. SOW, docs, and tests must use generic examples such as `Trap Jobs`; do not record real trap job names, device names, addresses, credentials, SNMP communities, or customer-identifying data.

Implementation plan:

1. Add source selector metadata fields to the Rust and Go Netdata function
   config surfaces, using a common concept such as source selector name and
   source selector help.
2. Initialize Rust defaults in `NetdataFunctionConfig::systemd_journal()` and
   Go defaults in `SystemdJournalNetdataFunctionConfig()` to preserve `Journal
   Sources` and the existing help text.
3. Use the config values in Rust `required_source_params` and Go
   `requiredSourceParams`.
4. Add Rust and Go tests for default metadata and custom metadata, including
   the `Trap Jobs` case.
5. Check Node.js and Python for equivalent Netdata function wrappers; either
   update them or record evidence that they have no implemented wrapper
   surface.
6. Update Rust and Go API documentation with the new config fields and the SNMP
   traps use case.

Validation plan:

- Run focused Rust tests for `systemd-journal-sdk` Netdata function metadata.
- Run focused Go tests for `go/journal`.
- Run any shared Netdata function parity tests affected by info responses.
- Run repository SOW audit.
- If other language wrappers exist, run their focused tests too.

Artifact impact plan:

- AGENTS.md: no update expected; this is an SDK API extension, not a workflow rule.
- Runtime project skills: no update expected unless implementation reveals a reusable Netdata function wrapper workflow.
- Specs: update if a Netdata function API spec exists or if implementation creates one.
- End-user/operator docs: update API docs for SDK consumers.
- End-user/operator skills: no output/reference skill expected.
- SOW lifecycle: pending SOW opened; move to current only when implementation starts.
- SOW-status.md: update pending list with SOW-0102.

Open-source reference evidence:

- None checked. This is a local SDK API surface issue driven by an existing Netdata consumer; external OSS reference is not needed before implementation.

Open decisions:

- Resolved: Netdata SNMP traps should display the selector as `Trap Jobs`.
- Resolved: the API concept must be common across active SDK language wrappers.
- Implementation should still decide exact field names after checking the
  current public API style, but Rust and Go semantics/defaults must match.

## Implications And Decisions

1. Selector wording location
   - Decision: implement in the SDK config, not in Netdata response post-processing.
   - Implication: all SDK consumers get a clean public API for domain-specific source labels.
   - Risk: requires SDK release and Netdata dependency update before Netdata can consume it.

2. SNMP traps display wording
   - Decision: use `Trap Jobs`.
   - Implication: the selector describes the operational object users configure in Netdata.
   - Risk: none beyond normal UI string review.

3. Common SDK API
   - Decision: implement the same source-selector metadata API concept in Rust
     and Go, and apply it to any future language wrapper that exposes the
     Netdata function API.
   - Implication: consumers can rely on matching behavior across SDK
     languages; Rust and Go documentation and tests must be updated together.
   - Risk: public API naming has to balance language idiom with cross-language
     clarity. Divergent field names are acceptable only when they preserve the
     same concept, defaults, and emitted function metadata.

## Plan

1. Move this SOW to current when ready to implement.
2. Add backward-compatible source selector metadata config in Rust and Go.
3. Add focused Rust and Go tests for default and custom labels.
4. Update Rust and Go API documentation.
5. Validate and run reviewers against the completed SOW if implementation is non-trivial.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Use the approved reviewer pool after the whole SOW is implemented and locally validated, not after each small edit.

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

- If language parity or API compatibility risks are found, pause and record evidence before changing public API shape.

## Execution Log

### 2026-06-11

- Opened pending SOW from Netdata SNMP traps integration feedback.
- Recorded user decision that the source selector metadata API must be common
  across active SDK language wrappers, not a Go-only extension.
- Moved the SOW to current and implemented common source selector metadata
  config in Rust and Go:
  - Rust `NetdataFunctionConfig` now has `source_selector_name` and
    `source_selector_help`, initialized by `NetdataFunctionConfig::systemd_journal()`.
  - Go `NetdataFunctionConfig` now has `SourceSelectorName` and
    `SourceSelectorHelp`, initialized by `SystemdJournalNetdataFunctionConfig()`
    and filled by `NewNetdataJournalFunction()` for zero-value configs.
  - Rust `required_source_params` and Go `requiredSourceParams` now emit
    configured metadata while preserving the fixed `__logs_sources` wire id.
- Added focused Rust and Go tests for default metadata and custom `Trap Jobs`
  metadata.
- Added reviewer-requested coverage for Rust empty selector metadata fallback
  and explicit default-path `__logs_sources` id assertions.
- Verified Node.js and Python do not expose an equivalent Netdata function
  wrapper surface in this repository, so no language parity edit was needed.
- Updated Rust and Go API documentation plus the Netdata function/facets spec.
- Recorded API compatibility note: Rust's public `NetdataFunctionConfig` is a
  public struct, so adding fields is a source-level API change for consumers
  using direct struct literals. The SDK is still pre-1.0, and the user decision
  for a common config API takes priority here. Runtime defaults remain
  unchanged for constructors and default configs.
- First reviewer pass returned production-grade votes and non-blocking cleanup
  notes. Added explicit default-path id assertions, Rust empty-selector
  fallback coverage, and removed stale validation placeholders.
- Final same-scope reviewer pass returned production-grade votes from all
  approved reviewers.

## Validation

Acceptance criteria evidence:

- Rust consumers can set `source_selector_name = "Trap Jobs"` and
  `source_selector_help = "Select the trap job to query"` through
  `NetdataFunctionConfig`.
- Go consumers can set `SourceSelectorName = "Trap Jobs"` and
  `SourceSelectorHelp = "Select the trap job to query"` through
  `NetdataFunctionConfig`.
- Rust and Go default constructors still emit `Journal Sources` and `Select the
  logs source to query`.
- Rust and Go still emit the stable `__logs_sources` id; no request/selection
  protocol changes were made.
- Node.js/Python search for `NetdataFunction`, `__logs_sources`, `Journal
  Sources`, `required_params`, and `RunDirectoryRequest` found no equivalent
  wrapper surface to update.
- Public docs now describe the source selector metadata fields:
  - `docs/Rust-API.md`
  - `docs/Go-API.md`
  - `go/API.md`
  - `rust/README.md`
- Product/API spec now records that `__logs_sources` is the fixed wire id while
  selector name/help are SDK metadata:
  - `.agents/sow/specs/systemd-journal-plugin-facets.md`

Tests or equivalent validation:

- `cargo fmt --manifest-path rust/Cargo.toml --all`: passed.
- `gofmt -w go/journal/netdata.go go/journal/netdata_test.go`: passed.
- `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk source_selector_metadata`: passed, 3 tests after reviewer cleanup.
- `cargo test --manifest-path rust/Cargo.toml -p systemd-journal-sdk netdata`: passed, 60 tests after reviewer cleanup.
- `go test ./journal` from `go/`: passed.
- `go test ./...` from `go/`: passed.
- `python3 tests/docs/check_wiki_docs.py`: passed, 15 wiki markdown files.
- `git diff --check`: passed.
- `.agents/sow/audit.sh`: passed; verdict `SOW initialization complete and clean`.

Reviewer findings and dispositions:

- `llm-netdata-cloud/glm-5.1`: PRODUCTION GRADE. No blocking findings.
- `llm-netdata-cloud/mimo-v2.5-pro`: PRODUCTION GRADE. No blocking findings.
- `llm-netdata-cloud/qwen3.6-plus`: PRODUCTION GRADE. Suggested explicit
  default-path id assertions and removing stale validation placeholders.
  Disposition: implemented.
- `llm-netdata-cloud/kimi-k2.6`: PRODUCTION GRADE. Suggested optional Rust
  empty-selector fallback coverage. Disposition: implemented.
- `llm-netdata-cloud/minimax-m3-coder`: PRODUCTION GRADE. Noted stale SOW
  validation placeholders. Disposition: removed.
- `llm-netdata-cloud/deepseek-v4-pro`: PRODUCTION GRADE on final same-scope
  rerun after cleanup. No blocking findings.

Same-failure search:

- `rg -n "NetdataFunction|netdata function|__logs_sources|Journal Sources|required_params|RunDirectoryRequest" node python -S`: no equivalent Node.js/Python wrapper surface.
- `rg -n "Journal Sources|Select the logs source" rust/src go docs README.md rust/README.md go/API.md`: remaining occurrences are defaults, tests, and docs/spec text.

Sensitive data gate:

- Passed. Durable artifacts use generic labels only; no real trap job names,
  hostnames, IP addresses, SNMP communities, credentials, or customer data were
  recorded.

Artifact maintenance gate:

- `AGENTS.md`: no update needed; no workflow or project-wide guardrail changed.
- Runtime project skills: no update needed; no reusable work procedure changed.
- Specs: updated `.agents/sow/specs/systemd-journal-plugin-facets.md`.
- End-user/operator docs: updated Rust and Go API docs plus Rust README and Go
  package API reference.
- End-user/operator skills: no output/reference skill affected.
- SOW lifecycle: moved from pending to current for implementation; completion
  moves this SOW to done; status ledgers updated.
- `.agents/sow/SOW-status.md`: updated.

Follow-up mapping:

- No valid deferred implementation item remains. The user requested a `0.6.4`
  release for Netdata consumption after this SOW; release work is separate from
  this SOW and must use the project release-tagging workflow.

## Outcome

Completed. Rust and Go now expose common Netdata source selector metadata
configuration for the fixed `__logs_sources` selector:

- Rust: `NetdataFunctionConfig::source_selector_name` and
  `NetdataFunctionConfig::source_selector_help`.
- Go: `NetdataFunctionConfig.SourceSelectorName` and
  `NetdataFunctionConfig.SourceSelectorHelp`.

The default systemd-journal metadata remains `Journal Sources` and `Select the
logs source to query`. Consumers such as SNMP traps can set domain wording such
as `Trap Jobs` without changing the stable request/selection wire id.

## Lessons Extracted

- The `__logs_sources` field id is protocol state; selector name/help are
  display metadata. Keeping that split explicit avoids UI-specific rewrites and
  request compatibility risk.
- A pre-1.0 public struct field addition can still affect direct struct-literal
  users, so public docs and tests must make the default constructor path clear.
- Reviewer cleanup should always include a stale-placeholder scan before the
  final SOW closeout.

## Followup

No implementation follow-up remains. The requested `0.6.4` release is handled
after this SOW through the project release-tagging workflow.

## Regression Log

None yet.

Append regression entries here only after this SOW was completed or closed and later testing or use found broken behavior. Use a dated `## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend regression content above the original SOW narrative.
