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
NORMALIZED_REPORT="${OUT_DIR}/lcov.normalized.info"

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  printf >&2 '%b[ERROR]%b cargo-llvm-cov is not installed. Install it with: cargo install cargo-llvm-cov --locked\n' "${RED}" "${NC}"
  exit 1
fi

cd "${ROOT}/rust"
# Keep show-env and the workspace test run on the same profile/object directory.
PROFILE_DIR="${CARGO_LLVM_COV_TARGET_DIR:-${CARGO_TARGET_DIR:-${ROOT}/rust/target}/llvm-cov-target}"
export CARGO_LLVM_COV_TARGET_DIR="${PROFILE_DIR}"
run cargo llvm-cov clean --workspace
run cargo llvm-cov --workspace --no-report
run cargo llvm-cov show-env --sh > "${OUT_DIR}/cli-env.sh"
(
  # This environment is emitted by the installed coverage tool, not user input.
  # shellcheck disable=SC1091
  source "${OUT_DIR}/cli-env.sh"
  # Cargo integration tests already build this instrumented command.
  run test -x "${PROFILE_DIR}/debug/journalctl"
  run "${PYTHON:-python3}" "${ROOT}/tests/interoperability/run_journalctl_portable_smoke.py" \
    --rust "${PROFILE_DIR}/debug/journalctl"
)
run cargo llvm-cov report --lcov --output-path "${RAW_REPORT}"
run normalize_lcov_sf_prefix "${RAW_REPORT}" "${NORMALIZED_REPORT}" "${ROOT}/rust/" "rust/"
validate_lcov_records "${NORMALIZED_REPORT}"
run filter_lcov_sf_paths "${NORMALIZED_REPORT}" "${REPORT}"
validate_lcov_records "${REPORT}"
assert_no_coverage_test_paths "${REPORT}" lcov
finish_report "${REPORT}"
