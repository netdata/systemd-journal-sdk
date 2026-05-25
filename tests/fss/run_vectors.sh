#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
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
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_SCRIPT="$SCRIPT_DIR/build.sh"
FIXTURE="$SCRIPT_DIR/fixtures/fsprg-vectors-v01.json"
TEMP_FIXTURE="$ROOT/.local/fsprg-vectors-generated.json"
CANDIDATE_FIXTURE="$ROOT/.local/fsprg-vectors-generated.json.tmp"
UPDATE_MODE=false

for arg in "$@"; do
  if [[ "$arg" == "--update" ]]; then
    UPDATE_MODE=true
  fi
done

run mkdir -p "$ROOT/.local"
GENERATOR_OUTPUT="$(run "$BUILD_SCRIPT")"
GENERATOR="$(printf '%s\n' "$GENERATOR_OUTPUT" | tail -n 1)"
if [[ ! -x "$GENERATOR" ]]; then
  printf >&2 "${RED}[ERROR]${NC} Built generator is not executable: %s\n" "$GENERATOR"
  exit 1
fi

trap 'rm -f "$CANDIDATE_FIXTURE"' EXIT

run "$GENERATOR" > "$CANDIDATE_FIXTURE"

if ! python3 -m json.tool "$CANDIDATE_FIXTURE" > /dev/null 2>&1; then
  printf >&2 "${RED}[ERROR]${NC} Generated fixture is not valid JSON: %s\n" "$CANDIDATE_FIXTURE"
  exit 1
fi

run mv "$CANDIDATE_FIXTURE" "$TEMP_FIXTURE"

if [[ "$UPDATE_MODE" == true ]]; then
  run cp "$TEMP_FIXTURE" "$FIXTURE"
  printf >&2 "${GREEN}[OK]${NC} Fixture updated: %s\n" "$FIXTURE"
else
  if ! diff -q "$FIXTURE" "$TEMP_FIXTURE" > /dev/null 2>&1; then
    printf >&2 "${RED}[FAIL]${NC} Generated fixture differs from committed fixture.\n"
    printf >&2 "${RED}       ${NC} Run with --update to refresh the fixture.\n"
    printf >&2 "${GRAY}       ${NC} diff %s %s\n" "$FIXTURE" "$TEMP_FIXTURE"
    diff -u "$FIXTURE" "$TEMP_FIXTURE" >&2 || true
    exit 1
  fi
  printf >&2 "${GREEN}[PASS]${NC} Generated fixture matches committed fixture.\n"
fi
