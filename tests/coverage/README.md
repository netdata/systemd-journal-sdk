# Coverage

This directory contains local coverage helpers used by the GitHub Actions
coverage workflow.

Reports are written under `.local/coverage/` by default:

```bash
tests/coverage/run_go_coverage.sh
tests/coverage/run_python_coverage.sh
tests/coverage/run_node_coverage.sh
tests/coverage/run_rust_coverage.sh
```

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
