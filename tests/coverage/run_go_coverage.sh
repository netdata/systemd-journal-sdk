#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tests/coverage/lib.sh
source "${SCRIPT_DIR}/lib.sh"

ROOT=$(repo_root)
OUT_DIR=${1:-"${ROOT}/.local/coverage/go"}
OUT_DIR=$(mkdir -p "${OUT_DIR}" && cd "${OUT_DIR}" && pwd)
REPORT="${OUT_DIR}/coverage.out"
RAW_REPORT="${OUT_DIR}/coverage.raw.out"
NORMALIZED_REPORT="${OUT_DIR}/coverage.normalized.out"
SUMMARY_REPORT="${OUT_DIR}/coverage.summary.out"
SUMMARY="${OUT_DIR}/coverage.txt"

cd "${ROOT}/go"
run go test -covermode=atomic -coverpkg=./... -coverprofile="${RAW_REPORT}" ./...
normalize_coverprofile_prefix \
  "${RAW_REPORT}" \
  "${NORMALIZED_REPORT}" \
  "github.com/netdata/systemd-journal-sdk/go/" \
  "go/"
filter_go_coverage_paths "${NORMALIZED_REPORT}" "${REPORT}"
assert_no_coverage_test_paths "${REPORT}" go
awk '
  NR == 1 {
    print
    next
  }
  { print "./" $0 }
' "${REPORT}" > "${SUMMARY_REPORT}"
cd "${ROOT}"
run go tool cover -func="${SUMMARY_REPORT}"
run go tool cover -func="${SUMMARY_REPORT}" -o "${SUMMARY}"
finish_report "${REPORT}"
