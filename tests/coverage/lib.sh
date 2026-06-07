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

filter_go_coverage_paths() {
  local input="$1"
  local output="$2"

  awk '
    NR == 1 && $0 ~ /^mode: / {
      print
      next
    }
    {
      path = $0
      sub(/:.*/, "", path)
      if (path ~ /(^|\/)internal\/testcmd\//) next
      if (path ~ /(^|\/)tests?\//) next
      if (path ~ /(^|\/)testdata\//) next
      if (path ~ /_test\.go$/) next
      print
    }
  ' "${input}" > "${output}"
}

filter_lcov_sf_paths() {
  local input="$1"
  local output="$2"

  awk '
    /^SF:/ {
      skip = 0
      path = substr($0, 4)
      if (path ~ /(^|\/)internal\/testcmd\//) skip = 1
      if (path ~ /(^|\/)tests?\//) skip = 1
      if (path ~ /(^|\/)tests\.rs$/) skip = 1
      if (path ~ /(^|\/)testdata\//) skip = 1
      if (path ~ /_tests?\.rs$/) skip = 1
      if (path ~ /(^|\/)examples?\//) skip = 1
    }
    !skip { print }
    /^end_of_record$/ { skip = 0 }
  ' "${input}" > "${output}"
}

validate_lcov_records() {
  local report="$1"
  local sf_count
  local end_count

  sf_count=$(awk 'BEGIN { count = 0 } /^SF:/ { count += 1 } END { print count }' "${report}")
  end_count=$(awk 'BEGIN { count = 0 } /^end_of_record$/ { count += 1 } END { print count }' "${report}")
  if [ "${sf_count}" = "${end_count}" ]; then
    return 0
  fi

  printf >&2 '%b[ERROR]%b LCOV report has %s SF records but %s end_of_record markers: %s\n' \
    "${RED}" "${NC}" "${sf_count}" "${end_count}" "${report}"
  exit 1
}

assert_no_coverage_test_paths() {
  local report="$1"
  local format="$2"
  local leaked=""

  case "${format}" in
    go)
      leaked=$(awk 'NR > 1 { path = $0; sub(/:.*/, "", path); if (path ~ /(^|\/)internal\/testcmd\// || path ~ /(^|\/)tests?\// || path ~ /(^|\/)testdata\// || path ~ /_test\.go$/) print path }' "${report}")
      ;;
    lcov)
      leaked=$(awk '/^SF:/ { path = substr($0, 4); if (path ~ /(^|\/)internal\/testcmd\// || path ~ /(^|\/)tests?\// || path ~ /(^|\/)tests\.rs$/ || path ~ /(^|\/)testdata\// || path ~ /_tests?\.rs$/ || path ~ /(^|\/)examples?\//) print path }' "${report}")
      ;;
    *)
      printf >&2 '%b[ERROR]%b Unknown coverage report format: %s\n' "${RED}" "${NC}" "${format}"
      exit 1
      ;;
  esac

  if [ -z "${leaked}" ]; then
    return 0
  fi

  printf >&2 '%b[ERROR]%b Coverage report contains test or test-harness paths: %s\n' "${RED}" "${NC}" "${report}"
  printf >&2 '%s\n' "${leaked}"
  exit 1
}
