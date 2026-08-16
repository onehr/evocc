#!/bin/bash

# Adversarial tests for the Day 1-3 foundation aggregator. This script tests
# the checked-in harness itself; it does not certify compiler functionality.

set -u
set -o pipefail

export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
FOUNDATION="$SCRIPT_DIR/foundation-conformance.sh"
FLIGHT_LAB="$SCRIPT_DIR/flight-lab/run.sh"
VERIFIER="$SCRIPT_DIR/verify-foundation-evidence.sh"
REQUIRED_IDS="$PROJECT_ROOT/tests/contracts/foundation-required-ids.txt"
JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"

if [ ! -r "$FOUNDATION" ] || [ ! -r "$FLIGHT_LAB" ] || \
   [ ! -r "$VERIFIER" ] || [ ! -r "$REQUIRED_IDS" ] || \
   [ ! -x "$JQ" ] || [ ! -x "$SHASUM" ]; then
  printf 'foundation self-test setup is incomplete\n' >&2
  exit 2
fi

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/c30-foundation-selftest.XXXXXX")" || exit 2
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM

PASSED=0
FAILED=0

pass() {
  PASSED=$((PASSED + 1))
  printf 'PASS %s\n' "$1"
}

fail() {
  FAILED=$((FAILED + 1))
  printf 'FAIL %s: %s\n' "$1" "$2" >&2
}

extract_function() {
  local name="$1"
  awk -v declaration="$name() {" '
    $0 == declaration { inside = 1 }
    inside { print }
    inside && $0 == "}" { exit }
  ' "$FOUNDATION"
}

FLIGHT_FUNCTION="$(extract_function test_flight_lab_acceptance)"
CPP_FUNCTION="$(extract_function test_isolated_cpp_differential)"
if [ -z "$FLIGHT_FUNCTION" ] || [ -z "$CPP_FUNCTION" ]; then
  printf 'foundation self-test could not extract subject functions\n' >&2
  exit 2
fi

make_child() {
  local path="$1"
  mkdir -p "$(dirname "$path")"
  printf '%s\n' \
    '#!/bin/bash' \
    'set -u' \
    'if [ "${C30_FAKE_KIND:-}" = flight ]; then' \
    '  output="${C30_FAKE_ROOT:?}/.evidence/flight-lab/current"' \
    '  payload='"'"'{"runId":"current","overall":"PASS","results":[]}'"'" \
    'else' \
    '  output="${C30_FAKE_ROOT:?}/.evidence/cpp-differential/current"' \
    '  payload='"'"'{"runId":"current","total":34,"counts":{"PASS":34},"results":[]}'"'" \
    'fi' \
    'if [ "${C30_FAKE_WRITE:-0}" -eq 1 ]; then' \
    '  mkdir -p "$output"' \
    '  printf "%s\\n" "$payload" >"$output/summary.json"' \
    'fi' \
    'exit "${C30_FAKE_RC:-0}"' \
    >"$path"
  chmod +x "$path"
}

run_child_case() {
  local case_id="$1"
  local kind="$2"
  local write_current="$3"
  local child_rc="$4"
  local stale_pass="$5"
  local expected_status="$6"
  local root="$SCRATCH/$case_id"

  mkdir -p "$root/tests/acceptance/flight-lab" "$root/raw"
  make_child "$root/tests/acceptance/flight-lab/run.sh"
  make_child "$root/tests/acceptance/cpp-differential.sh"

  if [ "$stale_pass" -eq 1 ]; then
    if [ "$kind" = flight ]; then
      mkdir -p "$root/.evidence/flight-lab/old"
      printf '%s\n' '{"runId":"old","overall":"PASS","results":[]}' \
        >"$root/.evidence/flight-lab/old/summary.json"
    else
      mkdir -p "$root/.evidence/cpp-differential/old"
      printf '%s\n' \
        '{"runId":"old","total":34,"counts":{"PASS":34},"results":[{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"},{"status":"PASS"}]}' \
        >"$root/.evidence/cpp-differential/old/summary.json"
    fi
  fi

  PROJECT_ROOT="$root"
  SCRIPT_DIR="$root/tests/acceptance"
  RAW_DIR="$root/raw"
  RESULTS="$root/results.tsv"
  : >"$RESULTS"
  export C30_FAKE_ROOT="$root"
  export C30_FAKE_KIND="$kind"
  export C30_FAKE_WRITE="$write_current"
  export C30_FAKE_RC="$child_rc"

  record_result() {
    printf '%s\t%s\n' "$1" "$2" >>"$RESULTS"
  }

  if [ "$kind" = flight ]; then
    eval "$FLIGHT_FUNCTION"
    test_flight_lab_acceptance
  else
    eval "$CPP_FUNCTION"
    test_isolated_cpp_differential
  fi

  local actual_status
  actual_status="$(tail -n 1 "$RESULTS" | awk -F '\t' '{print $2}')"
  if [ "$actual_status" = "$expected_status" ]; then
    pass "$case_id"
  else
    fail "$case_id" "expected $expected_status, observed ${actual_status:-EMPTY}"
  fi
}

# Positive controls prove that the extracted checked-in functions are live.
run_child_case flight_fresh_pass flight 1 0 0 PASS
run_child_case cpp_fresh_pass cpp 1 0 0 PASS

# A successful process without a current-run result is not evidence.
run_child_case flight_zero_without_summary flight 0 0 0 FAIL
run_child_case cpp_zero_without_summary cpp 0 0 0 FAIL

# A failed process must not borrow an older PASS summary from a shared tree.
run_child_case flight_failure_with_stale_pass flight 0 1 1 FAIL
run_child_case cpp_failure_with_stale_pass cpp 0 1 1 FAIL

# The official result vocabulary is closed: only PASS and FAIL are legal.
if grep -E 'record_result[[:space:]]+[^[:space:]]+[[:space:]]+(BLOCKED|SKIP|KNOWN_GAP|DIFFER)' \
    "$FOUNDATION" "$FLIGHT_LAB" >/dev/null; then
  fail result_status_vocabulary 'official harness can emit a non-PASS/FAIL status'
else
  pass result_status_vocabulary
fi

# High-level convenience gates may not erase the normative G1-G8 denominator.
ACTUAL_IDS="$SCRATCH/actual-ids.txt"
awk '
  /^SCRIPT_REQUIRED_IDS=/ {
    inside = 1
    sub(/^[^=]*=/, "")
    sub(/^\047/, "")
    print
    next
  }
  inside && $0 == "\047" { exit }
  inside { print }
' "$FOUNDATION" >"$ACTUAL_IDS"

missing_id=''
while IFS= read -r required_id; do
  [ -n "$required_id" ] || continue
  if ! grep -Fxq "$required_id" "$ACTUAL_IDS"; then
    missing_id="$required_id"
    break
  fi
done <"$REQUIRED_IDS"

if [ -z "$missing_id" ]; then
  pass normative_gate_denominator
else
  fail normative_gate_denominator "missing required gate id $missing_id"
fi

# A manifest is not a root of trust if the evidence producer can mutate the
# result and simply generate a new manifest. The caller freezes the original
# manifest digest out of band and supplies it to the verifier.
EVIDENCE="$SCRATCH/evidence"
mkdir -p "$EVIDENCE"
printf '%s\n' '{"overall":"FAIL"}' >"$EVIDENCE/summary.json"
printf '%s\n' '{"id":"control","status":"FAIL"}' >"$EVIDENCE/results.jsonl"
(
  cd "$EVIDENCE" || exit 2
  find . -type f ! -name evidence-manifest.sha256 | sort \
    | while IFS= read -r file; do "$SHASUM" -a 256 "$file"; done \
    >evidence-manifest.sha256
)
FROZEN_MANIFEST_SHA="$($SHASUM -a 256 "$EVIDENCE/evidence-manifest.sha256" | awk '{print $1}')"

if C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$FROZEN_MANIFEST_SHA" \
    bash "$VERIFIER" "$EVIDENCE" >/dev/null 2>&1; then
  pass intact_external_manifest_control
else
  fail intact_external_manifest_control 'intact evidence was rejected'
fi

printf '%s\n' '{"id":"control","status":"PASS"}' >"$EVIDENCE/results.jsonl"
(
  cd "$EVIDENCE" || exit 2
  find . -type f ! -name evidence-manifest.sha256 | sort \
    | while IFS= read -r file; do "$SHASUM" -a 256 "$file"; done \
    >evidence-manifest.sha256
)

if C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$FROZEN_MANIFEST_SHA" \
    bash "$VERIFIER" "$EVIDENCE" >/dev/null 2>&1; then
  fail resigned_result_mutation 'verifier accepted mutated evidence with a regenerated manifest'
else
  pass resigned_result_mutation
fi

printf 'foundation root-of-trust self-test: %d passed, %d failed\n' "$PASSED" "$FAILED"
[ "$FAILED" -eq 0 ]
