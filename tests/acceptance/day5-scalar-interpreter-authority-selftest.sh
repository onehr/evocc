#!/bin/bash

# Negative controls for the authoritative Day-5 interpreter join.  A caller
# must not be able to replace Cargo, Python, or the freshly built subject.

set -eu

export LC_ALL=C
export TZ=UTC
export PATH=/usr/bin:/bin:/usr/sbin:/sbin

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
GATE="$SCRIPT_DIR/day5-scalar-interpreter.sh"
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/c30-day5-authority.XXXXXX")"
MARKER="$TEST_DIR/fake-tool-ran"
FAKE_TOOL="$TEST_DIR/fake-tool"

cleanup() {
  rm -rf "$TEST_DIR"
}
trap cleanup EXIT HUP INT TERM

printf '#!/bin/sh\n: > "%s"\nexit 0\n' "$MARKER" >"$FAKE_TOOL"
chmod +x "$FAKE_TOOL"

run_mutant() {
  mutant="$1"
  stdout="$TEST_DIR/$mutant.stdout"
  stderr="$TEST_DIR/$mutant.stderr"
  shift

  set +e
  /usr/bin/env "$@" /bin/bash "$GATE" >"$stdout" 2>"$stderr"
  rc=$?
  set -e
  [ "$rc" -eq 2 ] || {
    printf '%s mutant returned rc=%s, expected 2\n' "$mutant" "$rc" >&2
    exit 1
  }
  [ ! -e "$MARKER" ] || {
    printf '%s mutant executed the fake tool\n' "$mutant" >&2
    exit 1
  }
  [ ! -s "$stdout" ] || {
    printf '%s mutant unexpectedly wrote stdout\n' "$mutant" >&2
    exit 1
  }
  expected='external Day-5 build or interpreter override is not permitted'
  [ "$(sed -n '1p' "$stderr")" = "$expected" ] || {
    printf '%s mutant produced the wrong rejection\n' "$mutant" >&2
    exit 1
  }
}

run_mutant fake-cargo "C30_CARGO=$FAKE_TOOL"
run_mutant fake-python "C30_PYTHON=$FAKE_TOOL"
run_mutant fake-subject "C30_DAY5_INTERPRETER=$FAKE_TOOL"

if grep -F 'command -v cargo' "$GATE" >/dev/null || \
   grep -F 'C30_CARGO:-' "$GATE" >/dev/null || \
   grep -F 'C30_PYTHON:-' "$GATE" >/dev/null || \
   ! grep -F 'export PATH=/usr/bin:/bin:/usr/sbin:/sbin' "$GATE" >/dev/null; then
  printf 'gate still contains an ambient Cargo or Python selection path\n' >&2
  exit 1
fi

printf '{"status":"PASS","mutantsRejected":["fake-cargo","fake-python","fake-subject"]}\n'
