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
NODE_REPORT="${COVERAGE_ROOT}/coverage-node/lcov.info"
PYTHON_REPORT="${COVERAGE_ROOT}/coverage-python/cobertura.xml"

if [ -z "${CODACY_API_TOKEN:-}" ]; then
  printf >&2 '%b[SKIP]%b CODACY_API_TOKEN is not set; coverage upload skipped.\n' "${YELLOW}" "${NC}"
  exit 0
fi

export CODACY_ORGANIZATION_PROVIDER="${CODACY_ORGANIZATION_PROVIDER:-gh}"
export CODACY_USERNAME="${CODACY_USERNAME:-netdata}"
export CODACY_PROJECT_NAME="${CODACY_PROJECT_NAME:-systemd-journal-sdk}"
export CODACY_REPORTER_VERSION="${CODACY_REPORTER_VERSION:-14.1.3}"
export CODACY_REPORTER_TMP_FOLDER="${CODACY_REPORTER_TMP_FOLDER:-${ROOT}/.local/codacy/coverage-reporter}"

REPORTER_SCRIPT="${ROOT}/.local/codacy/coverage-reporter/get-${CODACY_REPORTER_VERSION}.sh"
REPORTER_SCRIPT_URL="https://raw.githubusercontent.com/codacy/codacy-coverage-reporter/${CODACY_REPORTER_VERSION}/get.sh"

for report in "${GO_REPORT}" "${RUST_REPORT}" "${NODE_REPORT}" "${PYTHON_REPORT}"; do
  finish_report "${report}"
done

cd "${ROOT}"
run mkdir -p "$(dirname -- "${REPORTER_SCRIPT}")"
run curl -LsSf "${REPORTER_SCRIPT_URL}" -o "${REPORTER_SCRIPT}"
run bash "${REPORTER_SCRIPT}" report --partial --force-coverage-parser go -r "${GO_REPORT}"
run bash "${REPORTER_SCRIPT}" report --partial -r "${RUST_REPORT}"
run bash "${REPORTER_SCRIPT}" report --partial --prefix node/ -r "${NODE_REPORT}"
run bash "${REPORTER_SCRIPT}" report --partial --prefix python/ -r "${PYTHON_REPORT}"
run bash "${REPORTER_SCRIPT}" final
