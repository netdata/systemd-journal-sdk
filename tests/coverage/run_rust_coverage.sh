#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tests/coverage/lib.sh
source "${SCRIPT_DIR}/lib.sh"

ROOT=$(repo_root)
OUT_DIR=${1:-"${ROOT}/.local/coverage/rust"}
OUT_DIR=$(mkdir -p "${OUT_DIR}" && cd "${OUT_DIR}" && pwd)
REPORT="${OUT_DIR}/lcov.info"
RAW_REPORT="${OUT_DIR}/lcov.raw.info"

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  printf >&2 '%b[ERROR]%b cargo-llvm-cov is not installed. Install it with: cargo install cargo-llvm-cov --locked\n' "${RED}" "${NC}"
  exit 1
fi

cd "${ROOT}/rust"
run cargo llvm-cov clean --workspace
run cargo llvm-cov --workspace --lcov --output-path "${RAW_REPORT}"
run normalize_lcov_sf_prefix "${RAW_REPORT}" "${REPORT}" "${ROOT}/rust/" "rust/"
finish_report "${REPORT}"
