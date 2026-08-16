#!/bin/bash

# Prove that the checked-in e-graph tests kill structural no-op/corruption
# mutants. A compile error is not accepted as a killed behavioral mutant.

set -u
set -o pipefail

export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
SOURCE="$PROJECT_ROOT/crates/evocc-egraph/src/lib.rs"
BUILDER="$SCRIPT_DIR/egraph-mutant-builder.py"
CARGO_BIN="${C30_CARGO:-$(command -v cargo)}"
PYTHON_BIN="${C30_PYTHON:-/usr/bin/python3}"
TIMEOUT_BIN="${C30_TIMEOUT:-/opt/homebrew/bin/timeout}"

if [ ! -r "$SOURCE" ] || [ ! -r "$BUILDER" ] || [ ! -x "$CARGO_BIN" ] || \
   [ ! -x "$PYTHON_BIN" ] || [ ! -x "$TIMEOUT_BIN" ]; then
  printf 'egraph mutation self-test setup is incomplete\n' >&2
  exit 2
fi

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/c30-egraph-mutants.XXXXXX")" || exit 2
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM

PASSED=0
FAILED=0

make_crate() {
  local root="$1"
  mkdir -p "$root/src"
  printf '%s\n' \
    '[package]' \
    'name = "evocc-egraph-mutant"' \
    'version = "0.0.0"' \
    'edition = "2021"' \
    '' \
    '[dependencies]' \
    >"$root/Cargo.toml"
}

run_baseline() {
  local root="$SCRATCH/baseline"
  make_crate "$root"
  cp "$SOURCE" "$root/src/lib.rs"
  if CARGO_TARGET_DIR="$root/target" "$TIMEOUT_BIN" 60 \
      "$CARGO_BIN" test --manifest-path "$root/Cargo.toml" --offline \
      >"$root/test.stdout" 2>"$root/test.stderr"; then
    PASSED=$((PASSED + 1))
    printf 'PASS egraph_baseline_control\n'
  else
    FAILED=$((FAILED + 1))
    printf 'FAIL egraph_baseline_control: unchanged source did not pass\n' >&2
  fi
}

run_mutant() {
  local mutant="$1"
  local root="$SCRATCH/$mutant"
  local compile_rc
  local behavior_rc
  make_crate "$root"
  if ! "$PYTHON_BIN" "$BUILDER" "$SOURCE" "$root/src/lib.rs" "$mutant" \
      >"$root/mutate.stdout" 2>"$root/mutate.stderr"; then
    printf 'egraph mutation self-test could not construct %s\n' "$mutant" >&2
    exit 2
  fi

  if CARGO_TARGET_DIR="$root/target" "$TIMEOUT_BIN" 60 \
      "$CARGO_BIN" test --manifest-path "$root/Cargo.toml" --no-run --offline \
      >"$root/compile.stdout" 2>"$root/compile.stderr"; then
    compile_rc=0
  else
    compile_rc=$?
  fi
  if [ "$compile_rc" -ne 0 ]; then
    printf 'egraph mutant %s failed to compile, rc=%s\n' "$mutant" "$compile_rc" >&2
    exit 2
  fi

  if CARGO_TARGET_DIR="$root/target" "$TIMEOUT_BIN" 60 \
      "$CARGO_BIN" test --manifest-path "$root/Cargo.toml" --offline \
      >"$root/test.stdout" 2>"$root/test.stderr"; then
    behavior_rc=0
  else
    behavior_rc=$?
  fi

  if [ "$behavior_rc" -ne 0 ] && [ "$behavior_rc" -ne 124 ] && \
     grep -Fq 'test result: FAILED' "$root/test.stdout"; then
    PASSED=$((PASSED + 1))
    printf 'PASS %s\n' "$mutant"
  else
    FAILED=$((FAILED + 1))
    printf 'FAIL %s: behavioral suite rc=%s\n' "$mutant" "$behavior_rc" >&2
  fi
}

run_baseline
run_mutant rebuild_noop
run_mutant repair_noop
run_mutant missing_parent_update
run_mutant collapsed_constant

if [ $((PASSED + FAILED)) -ne 5 ]; then
  printf 'egraph mutation denominator changed: expected 5, observed %s\n' \
    "$((PASSED + FAILED))" >&2
  exit 2
fi

printf 'egraph mutation self-test: %d passed, %d failed\n' "$PASSED" "$FAILED"
[ "$FAILED" -eq 0 ]
