# systemd Journal SDK Project Instructions

## Glossary

- CGO: Go's C interoperability mechanism.
- DATA, ENTRY, FIELD: systemd journal file object types for a field/value payload, a journal row, and a field name.
- FTS: full-text search.
- IP: Internet Protocol.
- SNMP: Simple Network Management Protocol.
- SOW: Statement of Work.

## Goals

Pure Rust and Go SDKs for reading and writing systemd journal files, plus
file-backed journalctl rewrites in both languages. Success means:

- The SDKs read and write valid journal files without CGO or system journal
  libraries (CLI-only native code: see Project-specific overrides).
- Files written by either language are readable by the other and by compatible
  systemd tooling where applicable.
- One shared conformance suite, fixture set, interoperability tests and
  benchmarks apply to both implementations.
- APIs use the format's native performance features (see Performance Contract).
- Daemon-only journalctl operations are out of scope.

Project SOW status: initialized

## Go/Rust Feature Parity

Accepted SDK functionality ships in both Go and Rust in the same feature scope,
idiomatic per language but with equivalent behavior, failure semantics and
guarantees. Plans and acceptance criteria name both surfaces and validate each,
including cross-language file behavior. A consumer using only one language
does not justify a gap. A language-specific exception or staged delivery needs
an explicit user decision recorded in the SOW, and any remaining gap is
resolved with the user before publication.

## Performance Contract

This is a high-performance journal SDK, not only a compatibility clone. Every
reader, writer, query helper and facade uses the most efficient journal-native
path that preserves the documented contract; any deviation needs SOW evidence
and a user decision.

- Answer requests from FIELD/DATA hash tables, FIELD DATA chains, DATA and ENTRY
  arrays, offsets, sequence/time metadata and mmap slices instead of expanding
  every entry, unless SOW evidence shows the indexed path is unsafe or slower.
  For example, unfiltered unique values walk the FIELD's DATA chain as systemd
  does, and field-name enumeration walks FIELD hash tables, falling back to
  entry scans only for documented cases where the table is unsafe to traverse.
- Do not decode, decompress, copy, hash, sort or allocate outside the requested
  scope. Compressed DATA stays compressed unless its value is needed for
  filtering, faceting, FTS or returned rows. Repeated parsing of reusable DATA
  objects is a performance bug.
- Explorer column catalogs come from FIELD indexes.
  `ExplorerQuery::debug_collect_column_fields_by_row_traversal` is a debug tool;
  any production use, benchmark or compatibility claim with it enabled is
  invalid and is an explorer bug.
- Choose evidence by tracing the changed work: careful review and
  representative tests can show no regression; use existing benchmarks and
  targeted profiles for credible added costs. Do not build large measurement
  frameworks for small changes. An inconclusive timing proves neither equality
  nor regression; keep and investigate adverse observations. Compare against
  systemd and the faster sibling implementation where practical.
- Record any intentionally slower path in the SOW, docs and tests with its
  reason, measured cost and acceptable conditions.

## Single Ownership Of Journal Behavior

Each journal behavior has exactly one owner per language: file naming and
directory layout, header interpretation, ID, cursor and match grammars, field
validation, output formats, boot listing, verification key syntax. The SDK
owns it; the journalctl rewrites, the Netdata layer and the facades call the
owner instead of copying it. A copy means every later fix must be made twice
and is usually missed, so duplication is a correctness risk, not a style
issue.

- When a consumer needs behavior the SDK keeps private, expose it as a public
  SDK function in both languages rather than copying it (Go `internal/` has no
  Rust equivalent across crates).
- When a consumer and the SDK disagree, decide which one is right against the
  owning oracle (systemd for journalctl and the format, the Netdata plugin for
  Netdata behavior) before merging; never merge a difference silently.
- Look-alikes that follow different contracts (for example journalctl's
  systemd vacuum rules versus the SDK writer's own retention) are not
  duplicates; say so in a comment at the less obvious one.
- `tests/duplication/check_duplication.py` (CI: `duplication.yml`) fails when
  production code gains duplicated blocks against
  `tests/duplication/baseline.json`. It catches copy-paste only; reviews must
  still ask whether the SDK already does what new code reimplements. Rerun it
  with `--update` only to lock in a reduction or an approved exception.

## Runtime Purity Architecture

Keep four layers separate in both languages:

1. **Core file-format SDK** - reads and writes journal files using only
   caller-provided paths, bytes, options, timestamps and machine/boot/seqnum
   IDs. It never runs programs, probes host identity or locks writers by default.
2. **Systemd/journald compatibility layer** - journald field naming, metadata
   and API conventions. It may require caller-provided identity but never
   discovers it silently.
3. **Optional identity helper** - discovers machine/boot identity only when a
   caller opts in and passes the result on.
4. **Optional writer-lock helper** - cooperating-writer exclusion on opt-in.
   One writer per file is an operational contract, but systemd defines no
   portable lock protocol, so this is not systemd compatibility. Core writer
   constructors expose no lock options; callers wrap writer use themselves.

Core reader/writer paths never read `/proc`, `/host/proc`, `/etc/machine-id`,
`/var/lib/dbus/machine-id`, platform registries, `sysctl`, `system_profiler` or
`ps`, and never use shell commands or subprocess APIs; only the optional helpers
may. Tests, docs and examples use synthetic identities unless testing a helper.

Review enforces this; there is no automated purity scan, because a text scan
gave false confidence while covering a fraction of the core. When a change
touches core SDK code, search the diff for process launches (`os/exec`, `exec.Command`,
`exec.CommandContext`, `os.StartProcess`, `syscall.ForkExec`/`Exec`,
`std::process`, `Command::new`) and host-path or `sysctl` access. The writer-lock helper lives inside the core packages
(`go/journal/lock*.go`, `go/journal/file_lock*.go`,
`rust/src/crates/journal-core/src/file/lock.rs`) and may use them; no other core
file may call into it.

## SOW System

This project uses a self-contained local Statement of Work system: this file,
the SOW files, specs and project skills. Use global SOW templates or scripts
only when the user asks to repair or upgrade the framework.

### Roles

- **User:** purpose, scope, design forks, risk acceptance, destructive approvals and final product judgment.
- **Project manager (the agent):** SOW creation and planning, code, tests, docs, benchmarks, evidence, self-review, status reporting and gate enforcement.

### Review Gate

The review gate is the test suite plus the project manager's own review. Do
not run or recommend external reviewers.

- Finish the whole active SOW locally, including tests and evidence, then
  review the entire SOW and changed surface as one batch, not partial churn.
- Review for behavior, regressions, Go/Rust parity, security, unwanted side
  effects, production readiness and single ownership (does the SDK already
  do what the change reimplements?); record each finding and its disposition.
- After material fixes, rerun tests and re-review the whole scope.
- A phase advances only when tests pass and blocking findings are resolved,
  never on "mostly ok".

Done SOWs may record external reviewers or delegated implementation from
earlier practice; that does not describe current practice.

### Required First Checks

Before non-trivial work, read pending/current SOWs and relevant specs for
overlap and prior decisions, load every project skill whose trigger matches,
treat code and data as ground truth, and ask the user only for irreducible
product, design or risk decisions.

### Git Worktrees

Create a git worktree only when the user asks for or approves it.

### Repository Boundary

CRITICAL: Do not make changes outside this repository. This applies to all assistants and helper agents.

Canonical external-agent prompt block:

```text
CRITICAL REPOSITORY BOUNDARY:
- Do not make changes outside this repository for any reason.
- Repository path: current repository root.
- You may inspect external references read-only when the task requires it.
- Write, edit, delete, move, reset, checkout, install, generate, cache, or format nothing outside this repository.
- The only write exception outside the repository is /tmp.
- Prefer .local/ inside this repository for scratch work, generated temporary files, cloned references, logs, and working notes.
```

- The block binds this session as well as helper agents. Include it verbatim
  in every helper-agent prompt; exceptions need a user decision in the SOW.
- Investigation and review helpers are read-only; their prompts forbid
  creating, modifying, deleting, moving, formatting, staging or committing files.
- Package metadata commands (`go get`, `go list`, `cargo metadata`,
  `npm view`, `pip download`, …) write caches even when they look read-only:
  point `GOMODCACHE`, `GOCACHE`, `GOPATH`, `CARGO_HOME`, `npm_config_cache`,
  `PIP_CACHE_DIR` and equivalents under `.local/` or `/tmp`, or forbid fetching.

### Sensitive Data In Durable Artifacts

SOWs, specs, docs, skills, agent instructions and code comments are
commit-ready and public by default.

CRITICAL: Never write raw sensitive data to durable artifacts.
This covers credentials of any kind (passwords, API keys, tokens, SNMP
communities, private keys, credentialed connection strings, session cookies)
and identifying data (community member or customer names and identifiers,
personal data, customer-identifying public IPs, private endpoints, account IDs,
proprietary incident details).

Use placeholders such as `[REDACTED_SECRET]` or `[CUSTOMER]`, aliases whose
mapping is not stored in the repository, and citations of paths, commands or
error classes instead of values; summarize logs. If raw sensitive data is needed
to continue, stop and ask the user for a secure handling path. Sanitize any
found before commit; if it was already committed, tell the user and do not
rewrite history without approval.

### Open-Source Reference Evidence

Cite external open-source evidence as `owner/repo @ commit` followed by
repository-relative `path:line`, with `owner/repo` taken from the remote. Never
cite workstation mirror paths: they are meaningless to other readers and leak
local layout.

### Pre-Implementation Gate

Before implementation starts, the active SOW's `## Pre-Implementation Gate`
must show that the problem is understood: the root cause with evidence, the
affected contracts and both language surfaces, the risks, the plan, how it will
be validated, and any open decisions. Scale the depth to the change; mark
headings that do not apply with a one-line reason instead of padding them. If
investigation cannot resolve an unknown, stop and ask the user.

### When A SOW Is Required

Non-trivial work needs a SOW: features, behavioral bug fixes, refactors,
migrations, product-impacting docs, process or skill changes, spec hygiene,
regressions, and anything of unclear risk. Typos, formatting, behavior-free
renames and low-risk search/replace do not. When unsure, treat it as non-trivial.

### SOW Locations

SOWs live in `.agents/sow/{pending,current,done}/` (empty directories keep a
`.gitkeep`), specs in `.agents/sow/specs/`, and are created from
`.agents/sow/SOW.template.md` as `SOW-NNNN-YYYYMMDD-{slug}.md`. The audit is
`.agents/sow/audit.sh`. Status and directory must agree: `open` in `pending/`,
`in-progress` or `paused` in `current/`, `completed` or `closed` in `done/`.

### SOW Completion And Commit

The successful terminal status is `completed`; `done` is only a directory
name, so never write `Status: done` or `Status: complete`.

- Mark a SOW `completed` only after its implementation, docs, specs, skills,
  validation and follow-ups are finished and `.agents/sow/audit.sh` reports
  `SOW initialization complete and clean`. The audit can exit zero while
  reporting partial state, so read the verdict. If it fails, record and repair
  it before closing or advancing.
- Mark an obsolete or superseded SOW `closed` with its reason and its
  successor or disposition recorded; the audit must still be clean.
- Commit the work, artifact updates, status change and move to `done/`
  together in one commit unless the user asks otherwise, so a completed SOW
  never lives apart from its implementation. Prefer committing each verified
  chunk before the next, to keep rollback points.
- Never use `git add -A` or `git add .`; stage explicit paths only.

### One SOW At A Time

Never run multiple SOWs as one batch; merge overlapping work before
implementation or split it and finish one first. Read-only helper agents may
run in parallel within the active SOW. Progress reports are not stop points:
an in-progress SOW continues until delivered, failed with evidence, blocked on
a real user decision, or superseded by the user.

### User Decisions

Present concrete evidence (files/lines or sources), numbered options with pros,
cons and risks, and a recommendation with its reasoning. Record the decision in
the SOW before implementing it.

### Followup Discipline

"Deferred" is not a terminal outcome. Before a SOW closes, every valid leftover
item is implemented, rejected with evidence, or tracked by a real pending SOW,
so work does not silently disappear.

### Regressions

When behavior a completed SOW claimed working breaks later, reopen that SOW
instead of creating a new one: move it back to `current/`, mark it
`in-progress`, and append a dated `## Regression - YYYY-MM-DD` section at the
end recording what broke, why validation missed it, the fix, and its
validation. Never prepend; the original narrative stays readable first.

### Validation Gate

A SOW is complete when its Validation section lets a reader trust the outcome:
acceptance criteria met with evidence, tests run and their results, real-use
evidence where a runnable path exists, self-review findings and dispositions,
a search for the same failure elsewhere, the sensitive-data check, what
happened to follow-ups, and every other record the template's `## Validation`
section lists.

### Artifact Maintenance Gate

At close, state which durable artifacts changed and why the others did not
need to: `AGENTS.md`, project skills, specs, end-user docs (README, `docs/`,
help text), and `.agents/sow/SOW-status.md`, which is updated whenever SOW state
changes. Stale specs and docs are how this project accumulates false guidance,
so check them against the change rather than by habit.

### Specs

Specs in `.agents/sow/specs/` record WHAT the project does today, not
aspirations. Update them when shipped work changes behavior, public contracts,
data formats, operational guarantees or known edge cases. If specs and code
disagree, record the discrepancy in the SOW and resolve or track it.

### Project Skills

Project skills record HOW to work here. Skills under
`.agents/skills/project-*/SKILL.md` must be considered before non-trivial work;
load each one whose trigger matches.

Runtime input skills:

- `.agents/skills/project-docs-authoring/SKILL.md`
- `.agents/skills/project-journal-compatibility/SKILL.md`
- `.agents/skills/project-release/SKILL.md`

Legacy runtime skills:

- None.

Output/reference skills:

- None.

### Project-specific overrides

- Targets: SDK journal-format compatibility follows `systemd/systemd` tag
  `v260.1`. The file-backed journalctl commands follow tag `v262`
  (8cc40e0c5e9234bf45084751ac53b1fbfe70b492); that does not move the SDK format
  target or compiler minimums.
- CLI-only native code: the two journalctl commands may use native PCRE2-8
  >=10.42 (the Go command therefore needs CGO and a C compiler) and the platform
  C library for Unix globbing, thread-local locale and terminal queries; Windows
  uses the Unicode path contract. These exceptions never extend to the SDKs:
  keep CLI builds explicitly CGO-enabled and pure SDK checks separate. SDKs stay
  CGO-free, and nothing links system journal libraries or depends on
  systemd/libjournal. Common compression libraries are allowed after review.
- Compiler minimums: SDK Go and Rust minimums never exceed Netdata's declared
  ones (the consuming Go module's `go` directive and Rust workspace's
  `rust-version`, read from current Netdata source), because a higher minimum
  makes the SDK unusable by Netdata. This holds for dependency refreshes and
  vulnerability or scanner cleanup too: pick compatible dependency versions and
  disposition findings by reachability. Newer toolchains are extra validation
  targets only.
- API layers: each language provides an idiomatic SDK API plus a
  libsystemd-compatible reader facade, unless a SOW shows the facade would need
  native bindings, break the pure-language policy, or be unsafe or
  unrepresentable.
- Writer target includes compression and Forward Secure Sealing where systemd
  defines them, possibly phased. Readers handle the historical variants covered
  by the shared conformance fixtures.
- journalctl semantics: repeated matches on one field are already OR, so no
  `KEY in [values]` syntax is needed; the `+` disjunction separator is systemd
  behavior to replicate. Daemon-only commands (sync, flush, rotate,
  relinquish-var) are out of scope.
- Before Netdata integration or stable release work advances, GitHub code
  scanning and Codacy findings are fixed or dispositioned in the SOW. Raw SARIF,
  Codacy exports and scanner logs stay under `.local/`; durable artifacts get
  sanitized summaries only.
