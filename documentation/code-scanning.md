# Code Scanning And Codacy Triage

This repository treats static-analysis findings as a release and Netdata
integration gate. GitHub CodeQL and Codacy SARIF are configured as
reporting-only until the existing actionable baseline is fixed or explicitly
dispositioned.

## Workflows

- `.github/workflows/codeql.yml` runs advanced GitHub CodeQL for Go,
  JavaScript/TypeScript, Python, and Rust.
- `.github/workflows/codacy-sarif.yml` runs Codacy Analysis CLI, uploads SARIF
  to GitHub code scanning, and summarizes findings without committing raw
  SARIF or source snippets.
- If the repository secret `CODACY_API_TOKEN` is configured, the Codacy workflow
  also exports Codacy cloud issues into `.local/codacy/` on the runner and adds
  a sanitized summary to the job summary.

## Local Codacy Export

Raw Codacy exports must stay under `.local/`.

```bash
export CODACY_API_TOKEN=...
python3 tests/code_scanning/export_codacy_issues.py \
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
