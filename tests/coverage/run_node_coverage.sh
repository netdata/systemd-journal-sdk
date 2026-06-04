#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tests/coverage/lib.sh
source "${SCRIPT_DIR}/lib.sh"

ROOT=$(repo_root)
OUT_DIR=${1:-"${ROOT}/.local/coverage/node"}
OUT_DIR=$(mkdir -p "${OUT_DIR}" && cd "${OUT_DIR}" && pwd)
REPORT="${OUT_DIR}/lcov.info"

export npm_config_cache="${npm_config_cache:-${ROOT}/.local/npm-cache}"
if [[ " ${NODE_OPTIONS:-} " != *" --max-old-space-size="* ]]; then
  export NODE_OPTIONS="--max-old-space-size=8192 ${NODE_OPTIONS:-}"
fi
if [[ " ${NODE_OPTIONS:-} " != *" --no-deprecation "* ]]; then
  export NODE_OPTIONS="--no-deprecation ${NODE_OPTIONS:-}"
fi

cd "${ROOT}/node"
run npm exec --yes --package monocart-coverage-reports@2.12.12 -- mcr \
  --reports lcovonly,console-summary \
  --lcov \
  --outputDir "${OUT_DIR}" \
  --baseDir "${ROOT}/node" \
  --all "${ROOT}/node/src" \
  --entryFilter "{'**/.local/**':false,'**/node_modules/**':false,'**/test/**':false,'**/vendor/**':false,'**/src/**':true,'**/**':false}" \
  --sourceFilter "{'**/.local/**':false,'**/node_modules/**':false,'**/test/**':false,'**/vendor/**':false,'**/src/**':true,'**/**':false}" \
  npm test
finish_report "${REPORT}"
