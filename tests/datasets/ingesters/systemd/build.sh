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
ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
SYSTEMD_COMMIT="c0a5a2516d28601fb3afc1a77d7b42fcfe38fced"
SYSTEMD_REMOTE="${SYSTEMD_REMOTE:-https://github.com/systemd/systemd.git}"
SYSTEMD_SRC="$ROOT/.local/systemd-v260.1-src"
SYSTEMD_BUILD="$ROOT/.local/systemd-v260.1-build"
HELPER_SRC="$SYSTEMD_SRC/src/libsystemd/sd-journal/test-dataset-ingester.c"
MESON_FILE="$SYSTEMD_SRC/src/libsystemd/meson.build"

run mkdir -p "$ROOT/.local"

if [[ ! -d "$SYSTEMD_SRC/.git" ]]; then
  run git clone --depth 1 --branch v260.1 "$SYSTEMD_REMOTE" "$SYSTEMD_SRC"
fi

current_commit="$(git -C "$SYSTEMD_SRC" rev-parse HEAD)"
if [[ "$current_commit" != "$SYSTEMD_COMMIT" ]]; then
  run git -C "$SYSTEMD_SRC" fetch --depth 1 origin "$SYSTEMD_COMMIT"
  run git -C "$SYSTEMD_SRC" checkout --detach "$SYSTEMD_COMMIT"
fi

run cp "$SCRIPT_DIR/dataset_ingester.c" "$HELPER_SRC"

run python3 - "$MESON_FILE" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text()
entry = """        {
                'sources' : files('sd-journal/test-dataset-ingester.c'),
                'type' : 'manual',
        },
"""
if "sd-journal/test-dataset-ingester.c" not in text:
    marker = """        {
                'sources' : files('sd-journal/test-journal-append.c'),
                'type' : 'manual',
        },
"""
    if marker not in text:
        raise SystemExit("could not find test-journal-append marker in systemd meson.build")
    text = text.replace(marker, marker + entry)
    path.write_text(text)
PY

if [[ ! -f "$SYSTEMD_BUILD/build.ninja" ]]; then
  run meson setup "$SYSTEMD_BUILD" "$SYSTEMD_SRC"
fi

run ninja -C "$SYSTEMD_BUILD" test-dataset-ingester
printf '%s\n' "$SYSTEMD_BUILD/test-dataset-ingester"
