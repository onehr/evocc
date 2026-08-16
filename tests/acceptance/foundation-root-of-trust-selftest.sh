#!/bin/bash

# Adversarial tests for the Day 1-3 foundation aggregator. This script tests
# the checked-in harness itself; it does not certify compiler functionality.

set -u
set -o pipefail

export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
HARNESS_SCRIPT_DIR="$SCRIPT_DIR"
HARNESS_PROJECT_ROOT="$PROJECT_ROOT"
FOUNDATION="$SCRIPT_DIR/foundation-conformance.sh"
FLIGHT_LAB="$SCRIPT_DIR/flight-lab/run.sh"
VERIFIER="$SCRIPT_DIR/verify-foundation-evidence.sh"
REQUIRED_IDS="$PROJECT_ROOT/tests/contracts/foundation-required-ids.txt"
CPP_REQUIRED="$PROJECT_ROOT/tests/contracts/cpp-required-fixtures.txt"
JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"

if [ ! -r "$FOUNDATION" ] || [ ! -r "$FLIGHT_LAB" ] || \
   [ ! -r "$VERIFIER" ] || [ ! -r "$REQUIRED_IDS" ] || \
   [ ! -r "$CPP_REQUIRED" ] || \
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
TAG_FUNCTION="$(extract_function test_floating_tag_rejection)"
if [ -z "$FLIGHT_FUNCTION" ] || [ -z "$CPP_FUNCTION" ] || [ -z "$TAG_FUNCTION" ]; then
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
    'else' \
    '  output="${C30_FAKE_ROOT:?}/.evidence/cpp-differential/current"' \
    'fi' \
    'if [ "${C30_FAKE_WRITE:-0}" -eq 1 ]; then' \
    '  mkdir -p "$output"' \
    '  cp "${C30_FAKE_TEMPLATE:?}" "$output/summary.json"' \
    'fi' \
    'exit "${C30_FAKE_RC:-0}"' \
    >"$path"
  chmod +x "$path"
}

write_cpp_summary() {
  local target="$1"
  local variant="$2"
  local rows="$target.rows"
  local index=0
  local first_fixture=''
  : >"$rows"

  while IFS= read -r fixture; do
    [ -n "$fixture" ] || continue
    index=$((index + 1))
    [ -n "$first_fixture" ] || first_fixture="$fixture"

    if [ "$variant" = missing ] && [ "$index" -eq 34 ]; then
      continue
    fi
    if [ "$variant" = extra_substitution ] && [ "$index" -eq 34 ]; then
      fixture='999-unexpected.c'
    fi
    if [ "$variant" = duplicate ] && [ "$index" -eq 34 ]; then
      fixture="$first_fixture"
    fi

    local status='PASS'
    local evocc_rc=0
    local gcc_rc=0
    local evocc_signal=0
    local timed_out=false
    local diagnostic=''

    if [ "$fixture" = '025-include-missing.c' ]; then
      evocc_rc=1
      gcc_rc=1
      diagnostic='E0105'
    fi
    if [ "$index" -eq 1 ]; then
      case "$variant" in
        status_*) status="${variant#status_}" ;;
        rc_mismatch) gcc_rc=1 ;;
        panic) evocc_rc=101; gcc_rc=1; evocc_signal=6 ;;
        timeout) evocc_rc=124; gcc_rc=1; timed_out=true ;;
      esac
    fi

    "$JQ" -cn \
      --arg file "$fixture" \
      --arg status "$status" \
      --arg diagnosticCode "$diagnostic" \
      --arg stdoutSha256 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
      --argjson evoccRc "$evocc_rc" \
      --argjson gccRc "$gcc_rc" \
      --argjson evoccSignal "$evocc_signal" \
      --argjson timedOut "$timed_out" \
      '{file:$file,status:$status,evoccRc:$evoccRc,gccRc:$gccRc,
        evoccSignal:$evoccSignal,timedOut:$timedOut,
        evoccStdoutSha256:$stdoutSha256,gccStdoutSha256:$stdoutSha256,
        diagnosticCode:$diagnosticCode}' >>"$rows"
  done <"$CPP_REQUIRED"

  if [ "$variant" = empty_declared ]; then
    : >"$rows"
    "$JQ" -n \
      '{runId:"current",total:34,counts:{PASS:34},results:[],overall:"PASS"}' \
      >"$target"
  else
    "$JQ" -s '
      . as $results |
      {
        runId:"current",
        total:($results|length),
        counts:($results|group_by(.status)|map({key:.[0].status,value:length})|from_entries),
        results:$results,
        overall:(if (($results|length)==34 and all($results[];.status=="PASS")) then "PASS" else "FAIL" end)
      }
    ' "$rows" >"$target"
  fi
  rm -f "$rows"
}

run_child_case() {
  local case_id="$1"
  local kind="$2"
  local write_current="$3"
  local child_rc="$4"
  local stale_pass="$5"
  local expected_status="$6"
  local summary_variant="${7:-exact}"
  local root="$SCRATCH/$case_id"

  mkdir -p "$root/tests/acceptance/flight-lab" "$root/raw"
  make_child "$root/tests/acceptance/flight-lab/run.sh"
  make_child "$root/tests/acceptance/cpp-differential.sh"
  if [ "$kind" = flight ]; then
    printf '%s\n' '{"runId":"current","overall":"PASS","results":[]}' >"$root/template.json"
  else
    write_cpp_summary "$root/template.json" "$summary_variant"
  fi

  if [ "$stale_pass" -eq 1 ]; then
    if [ "$kind" = flight ]; then
      mkdir -p "$root/.evidence/flight-lab/old"
      printf '%s\n' '{"runId":"old","overall":"PASS","results":[]}' \
        >"$root/.evidence/flight-lab/old/summary.json"
    else
      mkdir -p "$root/.evidence/cpp-differential/old"
      write_cpp_summary "$root/.evidence/cpp-differential/old/summary.json" exact
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
  export C30_FAKE_TEMPLATE="$root/template.json"

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
  PROJECT_ROOT="$HARNESS_PROJECT_ROOT"
  SCRIPT_DIR="$HARNESS_SCRIPT_DIR"
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

# The CPP denominator is an exact filename set, not a producer-declared count.
run_child_case cpp_empty_declared_denominator cpp 1 0 0 FAIL empty_declared
run_child_case cpp_missing_fixture cpp 1 0 0 FAIL missing
run_child_case cpp_extra_substitutes_missing cpp 1 0 0 FAIL extra_substitution
run_child_case cpp_duplicate_substitutes_missing cpp 1 0 0 FAIL duplicate

# Every non-PASS status must make the official parent gate fail.
run_child_case cpp_skip_status cpp 1 0 0 FAIL status_SKIP
run_child_case cpp_differ_status cpp 1 0 0 FAIL status_DIFFER
run_child_case cpp_known_gap_status cpp 1 0 0 FAIL status_KNOWN_GAP
run_child_case cpp_oracle_conflict_status cpp 1 0 0 FAIL status_ORACLE_CONFLICT

# Equal stdout is not success when return/signal/timeout semantics differ.
run_child_case cpp_equal_stdout_rc_mismatch cpp 1 0 0 FAIL rc_mismatch
run_child_case cpp_panic_is_not_controlled_rejection cpp 1 0 0 FAIL panic
run_child_case cpp_timeout_is_not_controlled_rejection cpp 1 0 0 FAIL timeout

# The real child must fail when neither subject nor oracle is available, and
# its result vocabulary must remain closed even on that failure path.
ORACLE_ROOT="$SCRATCH/oracle-unavailable"
if C30_CONTAINER_EVIDENCE_ROOT="$ORACLE_ROOT" \
   C30_EVOCC="$SCRATCH/no-evocc" \
   C30_CONTAINER_BIN="$SCRATCH/no-container" \
   C30_JQ="$JQ" \
     bash "$SCRIPT_DIR/cpp-differential.sh" \
     >"$SCRATCH/oracle-unavailable.stdout" 2>"$SCRATCH/oracle-unavailable.stderr"; then
  oracle_rc=0
else
  oracle_rc=$?
fi
oracle_summary="$(find "$ORACLE_ROOT" -name summary.json -type f 2>/dev/null | sort | tail -n 1)"
if [ "$oracle_rc" -ne 0 ] && [ -n "$oracle_summary" ] && \
   "$JQ" -e '.overall == "FAIL" and .total == 34' "$oracle_summary" >/dev/null 2>&1; then
  pass oracle_unavailable_is_nonzero
else
  fail oracle_unavailable_is_nonzero "rc=$oracle_rc summary=${oracle_summary:-MISSING}"
fi
if [ -n "$oracle_summary" ] && \
   "$JQ" -e 'all(.results[]; .status == "PASS" or .status == "FAIL")' \
     "$oracle_summary" >/dev/null 2>&1; then
  pass oracle_unavailable_uses_closed_status
else
  fail oracle_unavailable_uses_closed_status 'oracle-unavailable path emitted a status outside PASS/FAIL'
fi

# The official result vocabulary is closed: only PASS and FAIL are legal.
if grep -E 'record_result.*(BLOCKED|SKIPPED|SKIP|KNOWN_GAP|DIFFER)' \
    "$FOUNDATION" "$FLIGHT_LAB" >/dev/null; then
  fail result_status_vocabulary 'official harness can emit a non-PASS/FAIL status'
else
  pass result_status_vocabulary
fi

run_tag_case() {
  local case_id="$1"
  local execution_reference="$2"
  local observed_id="$3"
  local expected_status="$4"
  local root="$SCRATCH/$case_id"
  local expected_id='sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
  local manifest_id='sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'
  mkdir -p "$root"
  "$JQ" -n \
    --arg expected "$expected_id" \
    --arg manifest "$manifest_id" \
    --arg observed "$observed_id" \
    --arg executionReference "$execution_reference" \
    '{buildImage:{sealedImageId:$expected,manifestDigest:$manifest,
      observedImageId:$observed,executionReference:$executionReference}}' \
    >"$root/build-environment.json"

  BUILD_ENV_CONTRACT="$root/build-environment.json"
  RESULTS="$root/results.tsv"
  : >"$RESULTS"
  record_result() {
    printf '%s\t%s\n' "$1" "$2" >>"$RESULTS"
  }
  eval "$TAG_FUNCTION"
  test_floating_tag_rejection
  local actual_status
  actual_status="$(tail -n 1 "$RESULTS" | awk -F '\t' '{print $2}')"
  if [ "$actual_status" = "$expected_status" ]; then
    pass "$case_id"
  else
    fail "$case_id" "expected $expected_status, observed ${actual_status:-EMPTY}"
  fi
}

run_tag_case immutable_image_identity_control \
  'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
  'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' PASS
run_tag_case observed_image_id_mismatch \
  'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' \
  'sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc' FAIL
run_tag_case floating_execution_reference \
  'evo-flight-lab:sealed' \
  'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' FAIL

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
EVIDENCE_BASE="$SCRATCH/evidence-base"
cp -R "$EVIDENCE" "$EVIDENCE_BASE"

if C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$FROZEN_MANIFEST_SHA" \
    bash "$VERIFIER" "$EVIDENCE" >/dev/null 2>&1; then
  pass intact_external_manifest_control
else
  fail intact_external_manifest_control 'intact evidence was rejected'
fi

# The current manifest must detect ordinary namespace/content mutations.
printf '%s\n' '{"id":"control","status":"PASS"}' >"$EVIDENCE/results.jsonl"
if C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$FROZEN_MANIFEST_SHA" \
    bash "$VERIFIER" "$EVIDENCE" >/dev/null 2>&1; then
  fail one_byte_artifact_mutation 'verifier accepted changed content under the original manifest'
else
  pass one_byte_artifact_mutation
fi

rm -rf "$EVIDENCE"
cp -R "$EVIDENCE_BASE" "$EVIDENCE"
rm -f "$EVIDENCE/results.jsonl"
if C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$FROZEN_MANIFEST_SHA" \
    bash "$VERIFIER" "$EVIDENCE" >/dev/null 2>&1; then
  fail missing_artifact_mutation 'verifier accepted a missing artifact'
else
  pass missing_artifact_mutation
fi

rm -rf "$EVIDENCE"
cp -R "$EVIDENCE_BASE" "$EVIDENCE"
printf '%s\n' surprise >"$EVIDENCE/extra.txt"
if C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$FROZEN_MANIFEST_SHA" \
    bash "$VERIFIER" "$EVIDENCE" >/dev/null 2>&1; then
  fail extra_artifact_mutation 'verifier accepted an undeclared artifact'
else
  pass extra_artifact_mutation
fi

rm -rf "$EVIDENCE"
cp -R "$EVIDENCE_BASE" "$EVIDENCE"
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

rm -rf "$EVIDENCE"
cp -R "$EVIDENCE_BASE" "$EVIDENCE"
printf '%s\n' surprise >"$EVIDENCE/extra.txt"
(
  cd "$EVIDENCE" || exit 2
  find . -type f ! -name evidence-manifest.sha256 | sort \
    | while IFS= read -r file; do "$SHASUM" -a 256 "$file"; done \
    >evidence-manifest.sha256
)
if C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$FROZEN_MANIFEST_SHA" \
    bash "$VERIFIER" "$EVIDENCE" >/dev/null 2>&1; then
  fail resigned_extra_artifact 'verifier accepted an extra artifact after manifest regeneration'
else
  pass resigned_extra_artifact
fi

if [ $((PASSED + FAILED)) -ne 30 ]; then
  printf 'foundation self-test denominator changed: expected 30, observed %s\n' \
    "$((PASSED + FAILED))" >&2
  exit 2
fi

printf 'foundation root-of-trust self-test: %d passed, %d failed\n' "$PASSED" "$FAILED"
[ "$FAILED" -eq 0 ]
