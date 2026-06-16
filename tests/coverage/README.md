# Coverage

This directory contains local coverage helpers used by the GitHub Actions
coverage workflow.

Reports are written under `.local/coverage/` by default:

```bash
tests/coverage/run_go_coverage.sh
tests/coverage/run_rust_coverage.sh
```

Coverage reports uploaded to Codacy intentionally exclude tests, fixtures, and
test harnesses. The filtering happens in the generated reports because Codacy
coverage-only exclusions are controlled by the coverage producer, not by the
repository analysis configuration.

Current report filters remove:

- Go: `internal/testcmd/`, `tests/`, `test/`, `testdata/`, and `*_test.go`.
- Rust LCOV: `internal/testcmd/`, `tests/`, `test/`, `tests.rs`,
  `testdata/`, `*_test.rs`, `*_tests.rs`, and `examples/`.

Codacy upload uses the account-token environment supported by Codacy Coverage
Reporter:

```text
Required environment:
- CODACY_API_TOKEN: account API token stored in the shell or CI secret store
- CODACY_ORGANIZATION_PROVIDER: gh
- CODACY_USERNAME: netdata
- CODACY_PROJECT_NAME: systemd-journal-sdk
```

```bash
tests/coverage/upload_codacy_coverage.sh .local/coverage-artifacts
```

The upload helper intentionally skips when `CODACY_API_TOKEN` is absent. This
keeps pull requests from forks and local validation tokenless while still making
the missing secret visible in the workflow summary.

The Codacy reporter installer and reporter binary are pinned to version
`14.1.3`. Reporter downloads and caches are kept under
`.local/codacy/coverage-reporter/` by default.

The Codacy file-metrics audit helper under `tests/code_scanning/` was validated
with Codacy Cloud CLI `1.0.0`. It uses Codacy's generated API client because
the public CLI does not currently expose file-level metric export as a stable
command.
