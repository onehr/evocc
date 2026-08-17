#!/bin/bash

# Authoritative Day-5 scalar interpreter join.  The production binary is
# always rebuilt from this checkout into a new target directory; callers
# cannot inject a subject executable or a precomputed observation stream.

set -eu

export LC_ALL=C
export TZ=UTC
export PYTHONDONTWRITEBYTECODE=1
export PATH=/usr/bin:/bin:/usr/sbin:/sbin

if [ "$#" -ne 0 ]; then
  printf 'usage: %s\n' "$0" >&2
  exit 2
fi
if [ "${C30_DAY5_INTERPRETER+x}" = x ] || \
   [ "${C30_CARGO+x}" = x ] || \
   [ "${C30_PYTHON+x}" = x ] || \
   [ "${CARGO_TARGET_DIR+x}" = x ]; then
  printf 'external Day-5 build or interpreter override is not permitted\n' >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
CORPUS="$SCRIPT_DIR/day5-scalar-interpreter.py"
CONTRACT="$PROJECT_ROOT/tests/contracts/day5-scalar-interpreter.json"
BUILD_ENV="$PROJECT_ROOT/contracts/build-environment.json"
PYTHON_BIN="/usr/bin/python3"

for path in "$CORPUS" "$CONTRACT" "$BUILD_ENV"; do
  [ -f "$path" ] && [ ! -L "$path" ] || {
    printf 'required Day-5 scalar interpreter input is missing or symbolic: %s\n' "$path" >&2
    exit 2
  }
done
[ -x "$PYTHON_BIN" ] && [ -f "$PYTHON_BIN" ] && [ ! -L "$PYTHON_BIN" ] || {
  printf 'fixed Day-5 Python is unavailable, non-regular, or symbolic\n' >&2
  exit 2
}

contract_value() {
  "$PYTHON_BIN" -I -c '
import json
import sys

value = json.load(open(sys.argv[1], encoding="utf-8"))["toolchain"]["hostReplay"]
for component in sys.argv[2].split("."):
    value = value[component]
if not isinstance(value, str) or "\n" in value:
    raise SystemExit("invalid hostReplay authority field")
print(value)
' "$BUILD_ENV" "$1"
}

sha256_file() {
  "$PYTHON_BIN" -I -c '
import hashlib
import sys

digest = hashlib.sha256()
with open(sys.argv[1], "rb") as source:
    for block in iter(lambda: source.read(1024 * 1024), b""):
        digest.update(block)
print(digest.hexdigest())
' "$1"
}

CARGO_BIN="$(contract_value cargo.path)"
CARGO_SHA256="$(contract_value cargo.sha256)"
CARGO_VERSION="$(contract_value cargo.version)"
RUSTC_BIN="$(contract_value rustc.path)"
RUSTC_SHA256="$(contract_value rustc.sha256)"
RUSTC_VERSION="$(contract_value rustc.version)"

for tool in "$CARGO_BIN" "$RUSTC_BIN"; do
  [ -x "$tool" ] && [ -f "$tool" ] && [ ! -L "$tool" ] || {
    printf 'frozen Day-5 Rust tool is unavailable, non-regular, or symbolic: %s\n' "$tool" >&2
    exit 2
  }
done
[ "$(sha256_file "$CARGO_BIN")" = "$CARGO_SHA256" ] && \
[ "$(sha256_file "$RUSTC_BIN")" = "$RUSTC_SHA256" ] && \
[ "$("$CARGO_BIN" --version)" = "$CARGO_VERSION" ] && \
[ "$("$RUSTC_BIN" --version)" = "$RUSTC_VERSION" ] || {
  printf 'frozen Day-5 Rust tool identity differs from build-environment.json\n' >&2
  exit 2
}

TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/c30-day5-interpreter.XXXXXX")"
cleanup() {
  rm -rf "$TARGET_DIR"
}
trap cleanup EXIT HUP INT TERM

cd "$PROJECT_ROOT"
"$PYTHON_BIN" -I "$CORPUS" audit
"$PYTHON_BIN" -I "$CORPUS" self-test

# Build in a deliberately minimal environment.  In particular this removes
# RUSTC_WRAPPER, RUSTC_WORKSPACE_WRAPPER, RUSTFLAGS,
# CARGO_ENCODED_RUSTFLAGS, and every CARGO_BUILD_* variable.  The one RUSTC
# value supplied below is the exact binary authenticated above.
TOOLCHAIN_BIN="${RUSTC_BIN%/rustc}"
/usr/bin/env -i \
  LC_ALL=C \
  TZ=UTC \
  PATH="$TOOLCHAIN_BIN:/usr/bin:/bin:/usr/sbin:/sbin" \
  RUSTC="$RUSTC_BIN" \
  CARGO_NET_OFFLINE=true \
  CARGO_TARGET_DIR="$TARGET_DIR" \
  "$CARGO_BIN" build \
  -p evocc --bin evocc --locked --offline

SUBJECT="$TARGET_DIR/debug/evocc"
[ -x "$SUBJECT" ] && [ -f "$SUBJECT" ] && [ ! -L "$SUBJECT" ] || {
  printf 'fresh Day-5 production binary is missing, non-regular, or symbolic\n' >&2
  exit 1
}
"$PYTHON_BIN" -I "$CORPUS" behavior "$SUBJECT"
