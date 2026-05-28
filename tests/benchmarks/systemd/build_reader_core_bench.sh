#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
YELLOW='\033[1;33m'
GRAY='\033[0;90m'
NC='\033[0m'

run() {
  printf >&2 "${GRAY}%s >${NC} " "$(pwd)"
  printf >&2 "${YELLOW}"
  printf >&2 "%q " "$@"
  printf >&2 "${NC}\n"

  if "$@"; then
    return 0
  else
    local exit_code=$?
    printf >&2 "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
    printf >&2 "${RED}[ERROR]${NC} Command failed with exit code %s: ${YELLOW}%s${NC}\n" "$exit_code" "$1"
    printf >&2 "${RED}        Full command:${NC} %s\n" "$*"
    printf >&2 "${RED}        Working dir:${NC} %s\n" "$(pwd)"
    printf >&2 "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
    return "$exit_code"
  fi
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
OUT="$ROOT/.local/benchmarks/bin/systemd-reader-core-bench"

run mkdir -p "$(dirname "$OUT")"

if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists libsystemd; then
  # shellcheck disable=SC2207
  CFLAGS=($(pkg-config --cflags libsystemd))
  # shellcheck disable=SC2207
  LIBS=($(pkg-config --libs libsystemd))
else
  CFLAGS=()
  LIBS=(-lsystemd)
fi

run cc -O3 -DNDEBUG "${CFLAGS[@]}" "$SCRIPT_DIR/reader_core_bench.c" "${LIBS[@]}" -o "$OUT"
printf '%s\n' "$OUT"
