# SOW-0039 - RAW Byte Field Name Reader Representation

## Status

Status: open

Sub-state: created on 2026-05-28 as a tracked follow-up from SOW-0038 review.

## Requirements

### Purpose

Ensure RAW-mode journal files with field names that are valid at the on-disk
DATA/FIELD structure layer can be represented consistently by SDK reader APIs,
including languages whose idiomatic field maps are string-keyed.

### User Request

The user required RAW mode to allow everything the journal file structures
allow, subject to mandatory structural rules such as non-empty names and no
`=` in field names. SOW-0038 implemented the writer policy. Review found a
separate reader representation gap for non-UTF8 field names in Node.js and
Python map APIs.

### Assistant Understanding

Facts:

- RAW writer policy accepts byte-oriented field names in Node.js and Python
  when callers pass `Buffer` / bytes-like names.
- Node.js and Python readers preserve raw DATA payload bytes in their payload
  arrays.
- Node.js and Python field maps decode field names as UTF-8 strings today.
- A non-UTF8 RAW field name cannot be represented losslessly as a JavaScript or
  Python string-keyed map without an explicit encoding or an alternate API.

Inferences:

- Directly changing `fields` map keys to bytes would be a breaking reader API
  change for existing callers.
- The likely clean design is to add an explicit byte-preserving reader surface
  while keeping existing string-keyed convenience maps.

Unknowns:

- The accepted cross-language API shape for byte-preserving field names needs
  a product decision before implementation.

### Acceptance Criteria

- Specs define how RAW byte field names are represented by each reader API.
- Rust, Go, Node.js, and Python readers expose equivalent byte-preserving
  access to DATA payload field names and values.
- Existing string-keyed convenience maps remain documented and backward
  compatible, or any breaking change is explicitly approved.
- Tests cover RAW field names containing non-UTF8 bytes, lowercase names,
  symbols, long names, and values containing binary bytes.
- Cross-language readers agree on the same RAW byte-name fixture.

## Analysis

Sources checked:

- `node/src/lib/reader.js`
- `python/journal/reader.py`
- `node/src/lib/writer.js`
- `python/journal/writer.py`
- `.agents/sow/current/SOW-0038-20260528-field-name-policy-layers.md`

Current state:

- Node.js reader converts field names with `name.toString('utf8')` for map
  keys while retaining raw `payloads`.
- Python reader converts field names with `do['name'].decode('utf-8')` for map
  keys while retaining raw `payloads`.
- RAW writers in both languages can accept bytes-like names, subject to
  non-empty and no `=`.

Risks:

- A caller using RAW mode with non-UTF8 field names may lose exact field-name
  identity when using convenience field maps in Node.js or Python.
- A rushed fix could break existing reader users by changing map key types.
- Query, unique, JSON/export, and journalctl surfaces may need consistent
  policy for byte field names.

## Pre-Implementation Gate

Status: needs-user-decision

Problem / root-cause model:

- The journal file format stores FIELD and DATA names as bytes before the first
  `=`.
- Some SDK reader APIs currently expose field maps keyed by language strings.
- RAW mode expands writer capability beyond stock systemd field-name syntax,
  exposing the mismatch between byte-oriented storage and string-keyed maps.

Evidence reviewed:

- `node/src/lib/reader.js`: field map uses UTF-8 string keys.
- `python/journal/reader.py`: field map uses UTF-8 string keys.
- `node/src/lib/writer.js`: RAW writer names may be `Buffer` / `Uint8Array`.
- `python/journal/writer.py`: RAW writer names may be bytes-like.

Affected contracts and surfaces:

- Rust, Go, Node.js, and Python reader entries.
- Directory readers and file readers.
- Query, unique, export/JSON helpers, and journalctl rewrites where they expose
  field names.
- Specs and end-user README/API docs.

Existing patterns to reuse:

- Existing `payloads` arrays already preserve raw `FIELD=value` bytes in
  Node.js and Python.
- Rust and Go can already represent arbitrary bytes with byte slices or strings
  backed by bytes, but their public contracts still need explicit wording.

Risk and blast radius:

- Reader API compatibility risk is high if map key types change.
- Test fixture and interoperability scope is moderate because only RAW
  systemd-incompatible names need this behavior.

Sensitive data handling plan:

- Use only synthetic bytes and placeholder field names.
- Do not include real hostnames, SNMP communities, trap payloads, customer
  identifiers, credentials, bearer tokens, private endpoints, or incident data.

Implementation plan:

1. Decide the byte-preserving reader API shape.
2. Update specs and docs with the selected contract.
3. Add shared RAW byte-name fixtures and conformance tests.
4. Implement byte-preserving reader accessors across all languages.
5. Validate direct file readers, directory readers, query paths, and journalctl
   behavior where applicable.

Validation plan:

- Run language test suites.
- Run RAW byte-name cross-language reader fixture tests.
- Confirm existing string-keyed maps keep their documented behavior.
- Run read-only reviewers with the full SOW scope.

Artifact impact plan:

- AGENTS.md: likely no update unless project-wide reader policy changes.
- Runtime project skills: update journal compatibility skill if a durable RAW
  reader workflow rule is added.
- Specs: update product scope with byte-name reader representation.
- End-user/operator docs: update Rust/Go/Node.js/Python reader docs.
- End-user/operator skills: no current output/reference skills expected.
- SOW lifecycle: this pending SOW tracks the SOW-0038 non-blocking review
  finding.
- SOW-status.md: list this SOW as pending.

Open-source reference evidence:

- External OSS references were not checked for this pending SOW. The finding
  came from local SDK reader and writer code review.

Open decisions:

1. Byte-preserving reader API shape:
   - A. Add parallel raw-field accessors while keeping existing `fields` maps
     string-keyed.
   - B. Change existing field maps to byte-keyed / encoded-keyed structures.
   - C. Document that RAW non-UTF8 field names are only available through raw
     payload arrays.

Recommendation:

- A. It is backward-compatible, explicit, and preserves exact RAW behavior
  without surprising existing map users.

## Implications And Decisions

Pending user decision.

## Plan

1. Decide API shape.
2. Implement tests and byte-preserving readers.
3. Update docs/specs and run review.

## Delegation Plan

Implementer:

- Local implementation by the project manager unless the user changes routing.

Reviewers:

- Read-only reviewers from the configured reviewer pool.

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

- Record implementation, validation, reviewer, and audit failures in this SOW.

## Execution Log

### 2026-05-28

- Created as a pending follow-up from SOW-0038 review.

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

- This SOW contains no raw secrets, credentials, bearer tokens, SNMP community
  strings, customer names, personal data, customer-identifying IP addresses,
  private endpoints, or proprietary incident details.

Artifact maintenance gate:

- AGENTS.md: pending implementation decision.
- Runtime project skills: pending implementation decision.
- Specs: pending implementation decision.
- End-user/operator docs: pending implementation decision.
- End-user/operator skills: pending implementation decision.
- SOW lifecycle: open in `.agents/sow/pending/`.
- SOW-status.md: updated when this SOW was created.

Specs update:

- Pending implementation.

Project skills update:

- Pending implementation.

End-user/operator docs update:

- Pending implementation.

End-user/operator skills update:

- Pending implementation.

Lessons:

- RAW writer capability must be paired with an explicit reader representation
  contract for field names that are not naturally string-compatible.

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
later testing or use found broken behavior. Use a dated
`## Regression - YYYY-MM-DD` heading at the end of the file. Never prepend
regression content above the original SOW narrative.
