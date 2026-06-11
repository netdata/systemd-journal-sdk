---
name: project-docs-authoring
description: "Mandatory rules when creating or editing consumer wiki documentation under docs/, including the verified-examples contract, marker grammar, placeholder paths, and validation commands."
---
# Project Docs Authoring

## Purpose

Keep the consumer wiki (`docs/`) decision-oriented, honest, and machine
verified: every Rust and Go code example is compiled and executed in CI, so
examples are contracts, not illustrations.

## Scope

Use this skill when:

- creating or editing any page under `docs/`;
- adding, changing, or removing code examples;
- changing `tests/docs/check_wiki_docs.py` or
  `tests/docs/verify_examples.py`;
- changing `.github/workflows/wiki.yml` or
  `.github/workflows/docs-examples.yml`.

Do not use this skill for `documentation/` (internal notes, not published).

## Mandatory Knowledge

- `docs/` is the published GitHub wiki source (SOW-0100). Internal links use
  `[[Page-Name|Label]]`; internal `*.md` Markdown links are rejected by the
  validator.
- Every fenced `rust` or `go` block in `docs/` must be immediately preceded
  by an HTML-comment marker: either
  `<!-- verify-example: lang=<rust|go> id=<unique-slug> -->` or
  `<!-- illustrative-only: <reason> -->`. Markers are invisible in the
  rendered wiki.
- `verify-example` attributes: `lang` (required, must match the fence),
  `id` (required, `[a-z0-9-]+`, unique across all pages), `mode=run`
  (default, compile and execute) or `mode=build` (compile only),
  `fixture=basic` (default), `prelude=<name>` for fragments that continue
  from registered setup code in `tests/docs/verify_examples.py` (`PRELUDES`).
- Examples must use only the documented placeholder paths, which the harness
  substitutes at run time (longest match first):
  `/var/log/journal/example/system.journal` (single-file fixture),
  `/var/log/journal` (directory fixture), `/var/log/journal-sdk` and
  `/var/log/journal-sdk/example.journal` (per-example scratch),
  `/tmp/example.journal` (scratch).
- Rust examples are rustdoc-shaped: top-level `?` is allowed and hidden
  lines start with `# ` (kept in compiled code, hidden in rendered docs); a
  final `# Ok::<(), Box<dyn std::error::Error>>(())` line marks Result-main
  wrapping. Go examples are function-body fragments that may `return err`;
  the harness wraps them in `func run() error`.
- Examples must be standalone-compilable after wrapping: in Go, do not bind
  variables that are never used, and do not re-declare prelude variables
  with `:=` (use `if err := ...; err != nil` forms).
- Fixtures are synthetic and deterministic, built by the harness with the
  in-repo Python SDK; never reference the host journal in examples, and
  never use real identities.
- Local validation before push:
  `python3 tests/docs/check_wiki_docs.py` and
  `python3 tests/docs/verify_examples.py`. CI runs both
  (`.github/workflows/wiki.yml` validates and publishes;
  `.github/workflows/docs-examples.yml` compiles and runs examples).
- When validation tooling runs `cargo`/`go` locally, export cache
  redirection (`CARGO_HOME`, `CARGO_TARGET_DIR`, `GOMODCACHE`, `GOCACHE`
  under `.local/caches/`) per the orchestration skill cache rules.
- Wiki pages document marker syntax inside ```markdown fences
  (`docs/Wiki-Publishing.md`); the validator and harness are fence-aware and
  ignore marker text inside fenced blocks. Preserve that property when
  changing either tool.
- Performance guidance honesty rule: `docs/Production-Profiles.md` names
  Rust and Go as production throughput targets and Python/Node.js as
  compatibility surfaces. Do not weaken that statement without benchmark
  evidence recorded in an active SOW.
- The docs perception model: pages are organized around choosing an API
  surface (idiomatic SDK, facade, Explorer, Netdata function boundary,
  journalctl rewrite CLI, verifier). New content should answer "which
  surface and why" before "how".

## Workflow Checklist

1. Edit or add the page; keep wiki-style links.
2. Mark every rust/go fence (verify-example or illustrative-only with a
   reason).
3. Run both validators locally; iterate until green.
4. For new example shapes, extend the harness (preludes, fixtures,
   substitutions) through the active SOW's implementer routing, with unit
   tests.
5. Record docs changes in the active SOW.

## Evidence

- `.agents/sow/done/SOW-0100-20260608-consumer-docs-github-wiki.md`: wiki
  structure, link rules, publishing.
- `.agents/sow/current/SOW-0103-20260611-docs-api-perception-and-verified-examples.md`:
  verified-examples harness, marker grammar, fixtures, CI wiring.
- `docs/Wiki-Publishing.md`: author-facing summary of the same rules.

## Update Rules

Update this skill when:

- the marker grammar, placeholder vocabulary, preludes, or fixtures change;
- a new language gains verified-example support;
- validation commands or CI workflows change.
