#!/bin/bash

# Canonical production join for the deterministic Day-4 generated corpus.

set -eu

export LC_ALL=C
export PYTHONDONTWRITEBYTECODE=1

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
CARGO_BIN="${C30_CARGO:-$(command -v cargo)}"
PYTHON_BIN="${C30_PYTHON:-/usr/bin/python3}"
TARGET_DIR="${C30_DAY4_TARGET_DIR:-$PROJECT_ROOT/target}"
CORPUS="$SCRIPT_DIR/day4-generated-corpus.py"
PROBE="$TARGET_DIR/debug/examples/day4_chir_probe"
EVOCC="$TARGET_DIR/debug/evocc"

if [ ! -x "$CARGO_BIN" ] || [ ! -x "$PYTHON_BIN" ] || [ ! -x "$CORPUS" ]; then
  printf 'Day-4 generated conformance setup is incomplete\n' >&2
  exit 2
fi

cd "$PROJECT_ROOT"
"$PYTHON_BIN" "$CORPUS"
CARGO_TARGET_DIR="$TARGET_DIR" "$CARGO_BIN" build \
  -p evocc-parser --example day4_chir_probe --locked --offline
CARGO_TARGET_DIR="$TARGET_DIR" "$CARGO_BIN" build \
  -p evocc --locked --offline
"$PYTHON_BIN" "$CORPUS" --behavior-chir "$PROBE"
"$PYTHON_BIN" "$CORPUS" --behavior-diagnostics "$PROBE"
"$PYTHON_BIN" "$CORPUS" --behavior-cli "$EVOCC" --jobs "${C30_DAY4_JOBS:-8}"
