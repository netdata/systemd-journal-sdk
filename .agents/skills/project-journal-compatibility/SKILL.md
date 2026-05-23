---
name: project-journal-compatibility
description: "Mandatory compatibility rules when changing journal file readers, writers, fixtures, conformance tests, interoperability tests, or journalctl rewrites."
---
# Project Journal Compatibility

## Purpose

Keep all implementations aligned with the systemd journal file format, Netdata Rust reader/writer sources, and the project scope decisions.

## Scope

Use this skill when:

- importing or adapting Rust journal code;
- implementing journal readers or writers in Rust, Go, Node.js, or Python;
- porting systemd tests or fixtures;
- building shared interoperability tests;
- implementing journalctl rewrites.

Do not use this skill for:

- pure repository bootstrap work;
- generic SOW maintenance unrelated to journal behavior.

## Mandatory Knowledge

- Baseline compatibility target is `systemd/systemd` tag `v260.1`.
- The test scope is SDK conformance plus file-backed journalctl behavior.
- Do not implement daemon-only journalctl commands such as daemon sync, flush, rotate, or relinquish-var operations.
- journalctl already treats repeated matches for the same field as OR alternatives and different fields as AND.
- The `+` separator is a systemd journalctl disjunction feature to replicate for file-backed journalctl behavior; it is not a new extension.
- Each language must provide two API layers: idiomatic SDK API plus a libsystemd-compatible reader facade. The facade is required unless a SOW records concrete evidence that it would require native bindings, violate the pure-language policy, or create an unsafe/unrepresentable API in that language.
- The final writer target includes compression and Forward Secure Sealing, but implementation may be phased.
- Pure-language dependencies are allowed; CGO, native Node.js addons, and linking to system journal libraries are not allowed.
- Every external-agent prompt must include the canonical repository-boundary block verbatim from `AGENTS.md` or `.agents/skills/project-agent-orchestration/SKILL.md`.

## Best Practices

- Treat systemd source and tests as the compatibility authority when documentation and implementation disagree.
- Keep shared tests language-neutral and run them against all SDKs.
- Prove cross-language interoperability with files written by each implementation and read by every implementation.
- Separate reader support for existing historical files from writer feature milestones.
- Record excluded upstream tests with a reason and extract file-level behavior where practical.

## Bad Practices

- Do not implement daemon/service behavior just to satisfy journalctl daemon-control options.
- Do not rely on one language's tests as sufficient for all languages.
- Do not use native bindings or system journal libraries to pass tests.
- Do not silently skip corrupted fixture behavior; record expected errors and recovery behavior.

## Workflow Checklist

1. Confirm the active SOW names the exact compatibility surface being changed.
2. Identify relevant systemd tests, fixtures, and Netdata Rust source paths.
3. Add or update shared tests before accepting implementation as complete.
4. Run the same conformance suite across every affected language.
5. Run interoperability tests across every writer/reader pair affected by the SOW.
6. Record benchmark or profiling evidence when the SOW includes performance claims.

## Validation Checklist

Before claiming production-grade compatibility:

- Shared conformance tests pass for every targeted language.
- Cross-language writer/reader matrix passes for every targeted file variant.
- journalctl behavior is tested against file-backed fixtures.
- Daemon-only journalctl commands are not implemented and have documented behavior.
- Dependency audit confirms no CGO, native Node.js addon, or system journal library linkage.

## Evidence

- `AGENTS.md`: project goals and scope decisions.
- `.agents/sow/specs/product-scope.md`: product scope and compatibility contracts.
- `.agents/sow/done/SOW-0001-20260523-project-bootstrap-and-orchestration.md`: initial decisions and evidence ledger.

## Update Rules

Update this skill when:

- compatibility baseline changes;
- a new journal file feature becomes in scope;
- reviewer findings expose a missed compatibility or validation requirement;
- a phase adds a durable implementation workflow that future agents must repeat.
