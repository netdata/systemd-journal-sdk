#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tests/coverage/lib.sh
source "${SCRIPT_DIR}/lib.sh"

ROOT=$(repo_root)
OUT_DIR=${1:-"${ROOT}/.local/coverage/python"}
OUT_DIR=$(mkdir -p "${OUT_DIR}" && cd "${OUT_DIR}" && pwd)
COVERAGE_FILE="${OUT_DIR}/.coverage"
REPORT="${OUT_DIR}/cobertura.xml"
SUMMARY="${OUT_DIR}/coverage.txt"

if ! python3 -m coverage --version >/dev/null 2>&1; then
  printf >&2 '%b[ERROR]%b Python coverage.py is not installed. Install it with: python3 -m pip install --upgrade coverage\n' "${RED}" "${NC}"
  exit 1
fi

cd "${ROOT}/python"
run env PYTHONPATH=. COVERAGE_FILE="${COVERAGE_FILE}" \
  python3 -m coverage run --branch --source=journal test_all.py
run env COVERAGE_FILE="${COVERAGE_FILE}" \
  python3 -m coverage xml -o "${REPORT}"
run env COVERAGE_FILE="${COVERAGE_FILE}" \
  python3 -m coverage report --show-missing
run env COVERAGE_FILE="${COVERAGE_FILE}" \
  python3 -m coverage report --show-missing > "${SUMMARY}"
finish_report "${REPORT}"
