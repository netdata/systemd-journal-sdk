#!/usr/bin/env bash

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
GRAY='\033[0;90m'
NC='\033[0m'

repo_root() {
  git rev-parse --show-toplevel
}

run() {
  printf >&2 '%b%s >%b ' "${GRAY}" "$(pwd)" "${NC}"
  printf >&2 '%b' "${YELLOW}"
  printf >&2 '%q ' "$@"
  printf >&2 '%b\n' "${NC}"

  if "$@"; then
    return 0
  else
    local exit_code=$?
    printf >&2 '%b%s%b\n' "${RED}" '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━' "${NC}"
    printf >&2 '%b[ERROR]%b Command failed with exit code %s: %b%s%b\n' "${RED}" "${NC}" "${exit_code}" "${YELLOW}" "$1" "${NC}"
    printf >&2 '%b        Full command:%b %s\n' "${RED}" "${NC}" "$*"
    printf >&2 '%b        Working dir:%b %s\n' "${RED}" "${NC}" "$(pwd)"
    printf >&2 '%b%s%b\n' "${RED}" '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━' "${NC}"
    return "${exit_code}"
  fi
}

finish_report() {
  local path="$1"
  if [ ! -s "${path}" ]; then
    printf >&2 '%b[ERROR]%b Expected coverage report is missing or empty: %s\n' "${RED}" "${NC}" "${path}"
    exit 1
  fi
  printf >&2 '%b[OK]%b Coverage report: %s\n' "${GREEN}" "${NC}" "${path}"
}

normalize_lcov_sf_prefix() {
  local input="$1"
  local output="$2"
  local from="$3"
  local to="$4"

  awk -v from="SF:${from}" -v to="SF:${to}" '
    index($0, from) == 1 {
      print to substr($0, length(from) + 1)
      next
    }
    { print }
  ' "${input}" > "${output}"
}

normalize_coverprofile_prefix() {
  local input="$1"
  local output="$2"
  local from="$3"
  local to="$4"

  awk -v from="${from}" -v to="${to}" '
    NR == 1 && $0 ~ /^mode: / {
      print
      next
    }
    index($0, from) == 1 {
      print to substr($0, length(from) + 1)
      next
    }
    { print }
  ' "${input}" > "${output}"
}
