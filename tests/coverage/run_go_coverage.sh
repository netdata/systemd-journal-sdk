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
SUMMARY="${OUT_DIR}/coverage.txt"

cd "${ROOT}/go"
run go test -covermode=atomic -coverpkg=./... -coverprofile="${RAW_REPORT}" ./...
run go tool cover -func="${RAW_REPORT}"
run go tool cover -func="${RAW_REPORT}" -o "${SUMMARY}"
normalize_coverprofile_prefix \
  "${RAW_REPORT}" \
  "${REPORT}" \
  "github.com/netdata/systemd-journal-sdk/go/" \
  "go/"
finish_report "${REPORT}"
