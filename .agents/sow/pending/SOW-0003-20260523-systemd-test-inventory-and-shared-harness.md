# SOW-0003 - systemd Test Inventory And Shared Harness

## Status

Status: open

Sub-state: pending after SOW-0002 creates initial repo structure and Rust import.

## Requirements

### Purpose

Create a shared conformance harness from applicable systemd journal tests and fixtures.

### User Request

Find all related systemd journal read/write tests and port the applicable file-backed/API behavior into this repo.

### Assistant Understanding

Facts:

- The shared conformance suite must be based on applicable systemd journal tests and fixtures.
- The suite must be language-neutral and reusable across every SDK.

Inferences:

- The harness runner format must be selected before implementation agents start work.

Unknowns:

- The exact language-neutral runner format is not selected yet.

### Acceptance Criteria

- Applicable systemd tests are inventoried with include/exclude reason.
- Fixtures from systemd baseline `v260.1` are copied or generated inside this repo with provenance.
- Shared tests are language-neutral and can target every SDK implementation.
- Shared fixture/test schema decisions refine any preliminary directories created by SOW-0002.
- Excluded daemon/service tests have explicit reasons and any extractable file-level behavior is tracked.
- No changes are made outside this repository.

## Analysis

Sources checked:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/test-journal*.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/test/test-journal-importer.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/journal-data/`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/test-journals/`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/units/TEST-04-JOURNAL*.sh`

Current state:

- Pending after SOW-0002 creates the initial repo structure and Rust source import.
- Harness runner decision remains open.

Risks:

- A weak harness could let language implementations drift while passing local tests.
- Daemon-only behavior can accidentally expand the project scope.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- Compatibility needs a single test source of truth before multiple language implementations diverge.

Evidence reviewed:

- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/libsystemd/sd-journal/test-journal*.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `src/test/test-journal-importer.c`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/journal-data/`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/test-journals/`
- `systemd/systemd @ c0a5a2516d28601fb3afc1a77d7b42fcfe38fced`, `test/units/TEST-04-JOURNAL*.sh`

Affected contracts and surfaces:

- Shared fixtures.
- Shared test schemas.
- Language test adapters.
- journalctl file-backed behavior.

Existing patterns to reuse:

- Decision B: SDK conformance plus file-backed journalctl behavior.
- Product scope spec matching semantics.

Risk and blast radius:

- Porting daemon-only behavior would expand scope incorrectly.
- Weak shared harness would let languages pass incompatible tests.

Sensitive data handling plan:

- systemd fixtures are public upstream artifacts.
- Store provenance as upstream repository plus commit and relative path.

Implementation plan:

1. Inventory tests and fixtures.
2. Classify include/exclude with evidence.
3. Copy allowed fixtures into repo.
4. Design shared test data schema.
5. Create runner contract for each language implementation.

Validation plan:

- Harness can run against a stub or imported Rust implementation and report structured pass/fail.
- Inventory covers every journal-related systemd test file discovered.

Artifact impact plan:

- Specs: update test-scope details.
- Runtime project skills: update if harness workflow becomes durable.
- End-user/operator docs: not expected in this phase.
- SOW lifecycle: move to current before implementation.
- SOW-status.md: update when this SOW moves to current or closes.

Open decisions:

1. Shared harness runner format must be selected before implementation.
   - Option A: Language-neutral fixture and test manifests, likely JSON or YAML, with one adapter executable per language returning structured results.
     - Pros: keeps the conformance suite independent from any one SDK language.
     - Cons: requires a small runner contract before implementation starts.
     - Implication: every language can be tested the same way, including journalctl CLI behavior.
     - Risk: the manifest schema must be versioned carefully as journal features expand.
   - Option B: Python-driven harness that invokes each language adapter and owns most assertions.
     - Pros: fast to build and convenient for fixture orchestration.
     - Cons: Python becomes the privileged test language before its SDK exists.
     - Implication: non-Python implementations may be coupled to Python test assumptions.
     - Risk: cross-language failures can be harder to attribute to harness vs SDK behavior.
   - Option C: Rust-driven harness based on the imported Rust implementation.
     - Pros: can reuse imported Rust code early.
     - Cons: risks making Rust behavior the test oracle instead of systemd fixtures and documented rules.
     - Implication: ports may clone Rust bugs instead of systemd-compatible behavior.
     - Risk: undermines the goal of a language-neutral conformance suite.
   - Recommendation: Option A, with systemd fixtures and explicit expected outcomes as the oracle.
   - Selection: pending activation decision.

## Implications And Decisions

1. Shared harness runner format
   - Current state: unresolved.
   - Required before activation: choose the language-neutral test manifest, fixture layout, runner protocol, and result format.
   - Implication: this decision becomes the contract every SDK adapter must satisfy.
   - Risk: a weak or language-biased harness can let SDKs drift while still passing their local tests.

## Plan

1. Resolve and record the shared harness runner format decision before implementation.
2. Activate this SOW by moving it to `current/` and setting `Status: in-progress`.
3. Delegate implementation to the selected implementer using the repository-boundary block.
4. Review the inventory, harness schema, fixture provenance, and audit results before closing.

## Delegation Plan

- Implementer: `llm-netdata-cloud/minimax-m2.7-coder`, or fallback to `llm-netdata-cloud/qwen3.6-plus` then `llm-netdata-cloud/glm-5.1` if minimax fails or is unavailable.
- Reviewers: at least two from `llm-netdata-cloud/kimi-k2.6`, `llm-netdata-cloud/mimo-v2.5-pro`, `llm-netdata-cloud/qwen3.6-plus`, `llm-netdata-cloud/glm-5.1`.
- Every prompt must include the canonical repository-boundary block from `AGENTS.md`.
- Failure handling: record implementer or reviewer model failure in this SOW, substitute only from the approved model list, rerun full-scope review after fixes, and do not close if `.agents/sow/audit.sh` fails.

## Execution Log

Pending activation.

## Validation

Pending activation and implementation.

## Outcome

Pending.

## Lessons Extracted

Pending activation.

## Followup

Pending activation.

## Regression Log

None yet.
