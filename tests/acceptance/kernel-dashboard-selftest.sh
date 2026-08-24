#!/bin/sh
# The roadmap gate: the kernel parse/preprocess dashboard is produced by a
# tracked script from observer-sealed evidence. This self-test is the red
# checkpoint for that script; it fails as a missing capability until the script
# exists and passes its own self-test.
set -eu
ROOT="$(cd "$(dirname "$0")/../.." && pwd -P)"
SCRIPT="$ROOT/tests/acceptance/publish-kernel-dashboard.py"
if [ ! -f "$SCRIPT" ]; then
  echo "kernel-dashboard: FAIL: missing capability: publish-kernel-dashboard.py does not exist" >&2
  exit 1
fi
exec python3 "$SCRIPT" --self-test
