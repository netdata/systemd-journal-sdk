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
UNIT_REPORT="${OUT_DIR}/coverage.unit.out"
CLI_DIR=$(mktemp -d "${OUT_DIR}/cli.XXXXXX")
mkdir "${CLI_DIR}/profiles"

cd "${ROOT}/go"
run env CGO_ENABLED=0 go test -count=1 ./...
run env CGO_ENABLED=1 go test -count=1 -covermode=atomic -coverpkg=./... -coverprofile="${UNIT_REPORT}" ./...
run env CGO_ENABLED=1 go build -cover -covermode=atomic -coverpkg=./... -o "${CLI_DIR}/journalctl" ./cmd/journalctl
run env GOCOVERDIR="${CLI_DIR}/profiles" "${PYTHON:-python3}" \
  "${ROOT}/tests/interoperability/run_journalctl_portable_smoke.py" --go "${CLI_DIR}/journalctl"
run go tool covdata textfmt -i="${CLI_DIR}/profiles" -o="${CLI_DIR}/coverage.out"
# Merge by source block so subprocess hits supplement rather than replace unit tests.
run awk '
  FNR == 1 {
    if ($0 != "mode: atomic") { print "ERROR: expected atomic profile: " FILENAME > "/dev/stderr"; exit 1 }
    next
  }
  $1 in statements && statements[$1] != $2 {
    print "ERROR: inconsistent coverage block: " $1 > "/dev/stderr"; exit 1
  }
  { statements[$1] = $2; hits[$1] += $3 }
  END {
    print "mode: atomic"
    for (block in statements) printf "%s %d %.0f\n", block, statements[block], hits[block]
  }
' "${UNIT_REPORT}" "${CLI_DIR}/coverage.out" > "${RAW_REPORT}"
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
