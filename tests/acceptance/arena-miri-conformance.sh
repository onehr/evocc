#!/bin/bash
# Miri conformance: run selected unsafe-free crates under Miri to detect
# undefined behavior. The arena allocator is the primary target because it
# does raw pointer arithmetic.
set -eu
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"

cd "$PROJECT_ROOT"

printf '=== arena (unsafe) miri conformance ===\n'
MIRI_LOG="$(mktemp "${TMPDIR:-/tmp}/c30-arena-miri.XXXXXX")"
trap 'rm -f "$MIRI_LOG"' EXIT

if ! cargo +nightly miri test -p evocc-common -- arena >"$MIRI_LOG" 2>&1; then
  cat "$MIRI_LOG" >&2
  exit 1
fi

grep -E '(running|test result|arena_slice_honors|arena_rolls_back|arena_continues)' "$MIRI_LOG"
grep -F 'test arena_slice_honors_alignment_and_runs_destructors ... ok' "$MIRI_LOG" >/dev/null
grep -F 'test arena_rolls_back_a_partially_initialized_slice ... ok' "$MIRI_LOG" >/dev/null
grep -F 'test arena_continues_after_an_element_destructor_panics ... ok' "$MIRI_LOG" >/dev/null
if grep -E 'test result: FAILED| [1-9][0-9]* failed' "$MIRI_LOG" >/dev/null; then
  cat "$MIRI_LOG" >&2
  exit 1
fi

printf 'arena-miri-conformance: PASS\n'
