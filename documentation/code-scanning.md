# Code Scanning And Codacy Triage

This repository treats static-analysis findings as a release and Netdata
integration gate. GitHub CodeQL and Codacy SARIF are configured as
enforcing gates when the tuned Codacy configuration is available. The only
non-enforcing path is the no-token/no-config Codacy fallback, which uploads an
empty SARIF closeout for historical stale alerts instead of running Codacy's
noisy local default configuration.

## Workflows

- `.github/workflows/codeql.yml` runs advanced GitHub CodeQL for Go,
  JavaScript/TypeScript, Python, and Rust.
- `.github/workflows/codacy-sarif.yml` runs Codacy Analysis CLI, uploads SARIF
  to GitHub code scanning, and summarizes findings without committing raw
  SARIF or source snippets.
- If `.codacy/codacy.config.json` is committed or the repository secret
  `CODACY_API_TOKEN` is configured, the Codacy workflow runs the tuned
  configuration and fails on analyzer findings or analyzer infrastructure
  failure.
- If neither committed config nor `CODACY_API_TOKEN` is available, the workflow
  does not run Codacy's local default configuration. It uploads an empty SARIF
  closeout for the historical default-config tool names that previously
  produced stale GitHub code-scanning alerts.
- The stale-alert closeout tool names are maintained in
  `tests/code_scanning/write_empty_codacy_sarif.py`. They are intentionally
  limited to GitHub tool names that already had stale alerts under the
  `codacy-analysis-cli` SARIF category. Do not add current-only analyzer tool
  names without checking alert history first.
- If `CODACY_API_TOKEN` is configured, the workflow also exports Codacy cloud
  issues into `.local/codacy/` on the runner and adds a sanitized summary to
  the job summary.

## Local Codacy Export

Raw Codacy exports must stay under `.local/`.

```bash
python3 tests/code_scanning/export_codacy_issues.py \
  --source cli \
  --provider gh \
  --organization netdata \
  --repository systemd-journal-sdk \
  --branch master \
  --output-dir .local/codacy

python3 tests/code_scanning/summarize_findings.py \
  --codacy-issues .local/codacy/codacy-issues.json \
  --json-output .local/codacy/codacy-cloud-summary.json \
  --markdown-output .local/codacy/codacy-cloud-summary.md
```

The local export path uses the authenticated `codacy` CLI. GitHub Actions uses
the API-token path because hosted runners do not have access to local Codacy
credentials.

## Local SARIF Summary

```bash
codacy-analysis analyze . \
  --output-format sarif \
  --output .local/codacy/codacy-analysis.sarif

python3 tests/code_scanning/summarize_findings.py \
  --sarif .local/codacy/codacy-analysis.sarif \
  --json-output .local/codacy/codacy-analysis-summary.json \
  --markdown-output .local/codacy/codacy-analysis-summary.md
```

## Disposition Policy

Every finding must be one of:

- fixed;
- false positive with rule/path evidence;
- generated or vendored artifact with the narrowest practical suppression;
- test fixture with evidence that the finding is intentional and harmless;
- accepted limitation with explicit user approval.

Broad repository-wide exclusions are not acceptable unless the SOW records why
the narrower alternative is unsafe or impractical.
