#!/usr/bin/env bash

set -euo pipefail
# Keep token-bearing upload runs quiet even if a caller invokes bash -x.
set +x

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tests/coverage/lib.sh
source "${SCRIPT_DIR}/lib.sh"

ROOT=$(repo_root)
COVERAGE_ROOT=${1:-"${ROOT}/.local/coverage-artifacts"}

GO_REPORT="${COVERAGE_ROOT}/coverage-go/coverage.out"
RUST_REPORT="${COVERAGE_ROOT}/coverage-rust/lcov.info"

if [ -z "${CODACY_PROJECT_TOKEN:-}" ]; then
  printf >&2 '%b[SKIP]%b CODACY_PROJECT_TOKEN is not set; coverage upload skipped.\n' "${YELLOW}" "${NC}"
  exit 0
fi

# Upload with the repository token only. The reporter switches to account-token
# mode when these are set, and an account token from the environment would be
# sent in place of the repository token.
unset CODACY_API_TOKEN CODACY_ORGANIZATION_PROVIDER CODACY_USERNAME CODACY_PROJECT_NAME
export CODACY_PROJECT_TOKEN
export CODACY_REPORTER_VERSION="${CODACY_REPORTER_VERSION:-14.1.3}"
export CODACY_REPORTER_TMP_FOLDER="${CODACY_REPORTER_TMP_FOLDER:-${ROOT}/.local/codacy/coverage-reporter}"

REPORTER_SCRIPT="${ROOT}/.local/codacy/coverage-reporter/get-${CODACY_REPORTER_VERSION}.sh"
REPORTER_SCRIPT_URL="https://raw.githubusercontent.com/codacy/codacy-coverage-reporter/${CODACY_REPORTER_VERSION}/get.sh"

for report in "${GO_REPORT}" "${RUST_REPORT}"; do
  finish_report "${report}"
done

cd "${ROOT}"
run mkdir -p "$(dirname -- "${REPORTER_SCRIPT}")"
run curl -LsSf "${REPORTER_SCRIPT_URL}" -o "${REPORTER_SCRIPT}"
run bash "${REPORTER_SCRIPT}" report --partial --force-coverage-parser go -r "${GO_REPORT}"
run bash "${REPORTER_SCRIPT}" report --partial -r "${RUST_REPORT}"
run bash "${REPORTER_SCRIPT}" final
