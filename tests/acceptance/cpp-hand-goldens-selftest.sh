#!/bin/bash

set -u
set -o pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
VALIDATOR="$SCRIPT_DIR/verify-cpp-hand-goldens.sh"
CONTRACT="$PROJECT_ROOT/tests/contracts/cpp-hand-goldens.json"
REQUIRED="$PROJECT_ROOT/tests/contracts/cpp-required-fixtures.txt"
JQ="${C30_JQ:-/opt/homebrew/Cellar/jq/1.8.2/bin/jq}"

[ -x "$VALIDATOR" ] && [ -x "$JQ" ] || exit 2
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/c30-cpp-hand-selftest.XXXXXX")" || exit 2
trap '/bin/rm -rf "$SCRATCH"' EXIT HUP INT TERM

PASSED=0
FAILED=0

run_case() {
  local id="$1"
  local contract="$2"
  local required="$3"
  local expected="$4"
  local rc=0
  bash "$VALIDATOR" "$contract" "$required" >/dev/null 2>&1 || rc=$?
  if [ "$rc" -eq "$expected" ]; then
    PASSED=$((PASSED + 1))
    printf 'PASS %s\n' "$id"
  else
    FAILED=$((FAILED + 1))
    printf 'FAIL %s expected_rc=%s actual_rc=%s\n' "$id" "$expected" "$rc" >&2
  fi
}

run_case intact "$CONTRACT" "$REQUIRED" 0

"$JQ" '.fixtures |= .[0:33]' "$CONTRACT" >"$SCRATCH/missing.json"
run_case missing_fixture "$SCRATCH/missing.json" "$REQUIRED" 1

"$JQ" '.fixtures[33].file = "999-unexpected.c"' "$CONTRACT" >"$SCRATCH/extra.json"
run_case extra_substitutes_missing "$SCRATCH/extra.json" "$REQUIRED" 1

"$JQ" '.fixtures[33].file = .fixtures[0].file' "$CONTRACT" >"$SCRATCH/duplicate.json"
run_case duplicate_substitutes_missing "$SCRATCH/duplicate.json" "$REQUIRED" 1

"$JQ" '.fixtures[0].expectedRc = 1' "$CONTRACT" >"$SCRATCH/wrong-rc.json"
run_case wrong_controlled_rc "$SCRATCH/wrong-rc.json" "$REQUIRED" 1

"$JQ" '.fixtures[0].diagnostic = {code:"E0000"}' "$CONTRACT" >"$SCRATCH/valid-diag.json"
run_case diagnostic_on_valid "$SCRATCH/valid-diag.json" "$REQUIRED" 1

"$JQ" '(.fixtures[] | select(.file=="025-include-missing.c") | .diagnostic.code) = "E9999"'   "$CONTRACT" >"$SCRATCH/wrong-diag.json"
run_case wrong_invalid_diagnostic "$SCRATCH/wrong-diag.json" "$REQUIRED" 1

"$JQ" '.fixtures[0].tokens[0].kind = "magic"' "$CONTRACT" >"$SCRATCH/unknown-kind.json"
run_case unknown_token_kind "$SCRATCH/unknown-kind.json" "$REQUIRED" 1

"$JQ" '.fixtures[0].tokens = []' "$CONTRACT" >"$SCRATCH/empty-required.json"
run_case empty_required_output "$SCRATCH/empty-required.json" "$REQUIRED" 1

"$JQ" '(.fixtures[] | select(.file=="032-empty-file.c") | .tokens) = [{kind:"identifier",spelling:"surprise"}]'   "$CONTRACT" >"$SCRATCH/nonempty-empty.json"
run_case nonempty_empty_fixture "$SCRATCH/nonempty-empty.json" "$REQUIRED" 1

sed '$d' "$REQUIRED" >"$SCRATCH/truncated-required.txt"
run_case truncated_required_ids "$CONTRACT" "$SCRATCH/truncated-required.txt" 1

if [ $((PASSED + FAILED)) -ne 11 ]; then
  printf 'cpp hand-golden selftest denominator changed\n' >&2
  exit 2
fi
printf 'cpp hand-golden selftest: %d passed, %d failed\n' "$PASSED" "$FAILED"
[ "$FAILED" -eq 0 ]
