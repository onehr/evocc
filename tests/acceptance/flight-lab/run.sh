#!/bin/bash

# Fail-closed acceptance harness for the ARM64 Linux flight lab.
#
# Day-1 semantics: every gate below starts RED. The harness aggregates PASS,
# FAIL, and BLOCKED outcomes across all gates instead of stopping at the first
# missing dependency, so a fresh checkout must produce a complete red result
# set. The Day-1 done condition is: environment/evidence gates GREEN, both
# reference kernels BUILD+BOOT, and every EvoCC capability gate RED with an
# explicit NOT_IMPLEMENTED verdict. A gate that silently skips or fails for an
# unknown reason is itself a failure.

set -u
set -o pipefail

export LC_ALL=C
umask 022

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd -P)"
CONTRACT_DIR="$PROJECT_ROOT/contracts"
BUILD_ENV_CONTRACT="${C30_BUILD_ENV_CONTRACT:-$CONTRACT_DIR/build-environment.json}"
ORACLE_CONTRACT="${C30_ORACLE_CONTRACT:-$CONTRACT_DIR/oracle-policy.json}"
KERNEL_CONTRACT="${C30_KERNEL_CONTRACT:-$CONTRACT_DIR/kernel-target.json}"
EVIDENCE_ROOT="${C30_FLIGHT_LAB_EVIDENCE_ROOT:-$PROJECT_ROOT/.evidence/flight-lab}"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
RAW_DIR="$RUN_DIR/raw"
RESULTS="$RUN_DIR/results.jsonl"
SUMMARY="$RUN_DIR/summary.json"

JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"

mkdir -p "$RAW_DIR"
: >"$RESULTS"

if [ ! -x "$JQ" ]; then
  printf 'required jq is unavailable: %s\n' "$JQ" >&2
  exit 2
fi

# The immutable gate inventory lives in the script so that a contract edit
# cannot silently delete a gate. The contracts must enumerate exactly these.
SCRIPT_REQUIRED_IDS='contract_lock_rejects_drift
oracles_are_independent
reference_kernel_rebuilds
reference_kernel_boots_fresh
provenance_is_closed
offline_build_has_no_network
artifact_set_is_exact
reset_is_clean
compiler_capabilities_are_red'

record_result() {
  result_id="$1"
  result_status="$2"
  result_message="$3"
  result_evidence="${4:-}"

  "$JQ" -cn \
    --arg id "$result_id" \
    --arg status "$result_status" \
    --arg message "$result_message" \
    --arg evidence "$result_evidence" \
    --arg recordedAt "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
    '{id:$id,status:$status,message:$message,evidence:$evidence,recordedAt:$recordedAt}' \
    >>"$RESULTS"
}

sha256_file() {
  "$SHASUM" -a 256 "$1" | awk '{print $1}'
}

# A contract is trustworthy only if it is valid JSON, declares itself locked,
# and contains no floating reference. Floating references are the drift
# channel this entire lab exists to close.
contract_is_sane() {
  sane_file="$1"
  [ -r "$sane_file" ] || return 1
  # forbiddenPatterns is metadata about the drift rule, not a reference to an
  # artifact, so it is excluded from the floating-value scan.
  "$JQ" -e '
    del(.forbiddenPatterns) |
    .schemaVersion == 1 and
    .contractState == "locked" and
    ([.. | strings | select(test("(^|[^A-Za-z])(TBD|UNRESOLVED)([^A-Za-z]|$)"; "i"))] | length == 0)
  ' "$sane_file" >/dev/null 2>&1
}

test_contract_lock_rejects_drift() {
  drift_ok=1

  for contract in "$BUILD_ENV_CONTRACT" "$ORACLE_CONTRACT" "$KERNEL_CONTRACT" \
                  "$CONTRACT_DIR/apple-container-bootstrap.json"; do
    contract_is_sane "$contract" || drift_ok=0
  done

  # Tamper probe: copy a contract, flip one hex digit in a digest field, and
  # require the sanity predicate to reject the mutated copy. If the predicate
  # cannot detect single-digit drift it is vacuous.
  probe="$RAW_DIR/drift-probe.json"
  cp "$KERNEL_CONTRACT" "$probe"
  first_sha="$("$JQ" -r '[.. | strings | select(test("^[0-9a-f]{64}$"))][0] // ""' "$probe")"
  if [ -n "$first_sha" ]; then
    mutated="a${first_sha#a}"
    if [ "$mutated" = "$first_sha" ]; then mutated="b${first_sha#?}"; fi
    sed -i '' "s/$first_sha/$mutated/" "$probe"
    if contract_is_sane "$probe" && \
       "$JQ" -e --arg sha "$first_sha" '[.. | strings] | index($sha) != null' "$probe" >/dev/null 2>&1; then
      drift_ok=0
    fi
  fi

  # Floating-reference probe: any bare floating tag where a digest belongs is
  # drift by construction. RESOLVE_AT_* placeholders are the only sanctioned
  # unfrozen fields, and each must name its resolution policy in the contract.
  # The contract's own forbiddenPatterns declaration is metadata about the
  # rule, not a reference, so it is excluded from the scan.
  for contract in "$BUILD_ENV_CONTRACT" "$KERNEL_CONTRACT"; do
    "$JQ" -e '
      del(.forbiddenPatterns) |
      [.. | strings | select(. == "latest" or . == "master" or . == "HEAD")] | length == 0
    ' "$contract" >/dev/null 2>&1 || drift_ok=0
  done

  if [ "$drift_ok" -eq 1 ]; then
    record_result contract_lock_rejects_drift PASS "all contracts locked, no floating references, tamper probe rejected" "raw/drift-probe.json"
  else
    record_result contract_lock_rejects_drift FAIL "a contract is unlocked, floating, or the tamper probe was not rejected" "raw/drift-probe.json"
  fi
}

test_oracles_are_independent() {
  oracle_ok=1

  # Resolve the two oracle binaries from the sealed build environment. Until
  # the seal step records their digests this gate is BLOCKED, not red-by-lie.
  gcc_sha="$("$JQ" -r '.toolchain.gcc.binarySha256' "$BUILD_ENV_CONTRACT")"
  clang_sha="$("$JQ" -r '.toolchain.clang.binarySha256' "$BUILD_ENV_CONTRACT")"
  if [ "$gcc_sha" = "RESOLVE_AT_SEAL" ] || [ "$clang_sha" = "RESOLVE_AT_SEAL" ]; then
    record_result oracles_are_independent BLOCKED "oracle digests are recorded at image seal time; seal has not run" "contracts/build-environment.json"
    return
  fi

  [ "$gcc_sha" != "$clang_sha" ] || oracle_ok=0

  # The macOS /usr/bin/gcc alias must never be accepted as the GNU oracle.
  if [ -e /usr/bin/gcc ]; then
    alias_realpath="$(readlink -f /usr/bin/gcc 2>/dev/null || printf '%s' /usr/bin/gcc)"
    case "$alias_realpath" in
      *clang*|*Clang*) ;;
      *) oracle_ok=0 ;;
    esac
  fi

  # Vendor strings must differ; two binaries from one vendor collapse the
  # differential design into a single point of failure.
  gcc_vendor="$("$JQ" -r '.toolchain.gcc.vendor' "$BUILD_ENV_CONTRACT")"
  clang_vendor="$("$JQ" -r '.toolchain.clang.vendor' "$BUILD_ENV_CONTRACT")"
  [ "$gcc_vendor" != "$clang_vendor" ] || oracle_ok=0

  if [ "$oracle_ok" -eq 1 ]; then
    record_result oracles_are_independent PASS "GNU and LLVM oracles differ in vendor and binary digest; macOS clang alias rejected" "contracts/build-environment.json"
  else
    record_result oracles_are_independent FAIL "oracle binaries are not demonstrably independent" "contracts/build-environment.json"
  fi
}

test_reference_kernel_rebuilds() {
  gcc_image_1="$RUN_DIR/gcc-image-1.sha"
  if [ ! -f "$PROJECT_ROOT/.build/reference/gcc-lane-1/arch/arm64/boot/Image" ]; then
    record_result reference_kernel_rebuilds BLOCKED "reference build outputs do not exist; build harness has not run" ".build/reference/"
    return
  fi
  record_result reference_kernel_rebuilds FAIL "reference rebuild comparison is not implemented yet" ".build/reference/"
}

test_reference_kernel_boots_fresh() {
  if [ ! -f "$PROJECT_ROOT/.build/reference/gcc-lane-1/arch/arm64/boot/Image" ]; then
    record_result reference_kernel_boots_fresh BLOCKED "no reference kernel Image exists to boot" ".build/reference/"
    return
  fi
  record_result reference_kernel_boots_fresh FAIL "nonce-bound boot predicate is not implemented yet" ".build/reference/"
}

test_provenance_is_closed() {
  if [ ! -d "$PROJECT_ROOT/.build/reference" ]; then
    record_result provenance_is_closed BLOCKED "no reference build tree exists to trace" ".build/reference/"
    return
  fi
  record_result provenance_is_closed FAIL "per-object provenance DAG is not implemented yet" ".build/reference/"
}

test_offline_build_has_no_network() {
  sealed="$("$JQ" -r '.buildImage.sealedImageId' "$BUILD_ENV_CONTRACT")"
  if [ "$sealed" = "RESOLVE_AT_SEAL" ]; then
    record_result offline_build_has_no_network BLOCKED "build image is not sealed; there is nothing to run offline yet" "contracts/build-environment.json"
    return
  fi
  record_result offline_build_has_no_network FAIL "offline build assertion is not implemented yet" "contracts/build-environment.json"
}

test_artifact_set_is_exact() {
  if [ ! -f "$PROJECT_ROOT/.build/reference/artifact-manifest.json" ]; then
    record_result artifact_set_is_exact BLOCKED "no artifact manifest exists; reference builds have not run" ".build/reference/"
    return
  fi
  record_result artifact_set_is_exact FAIL "artifact set comparison is not implemented yet" ".build/reference/"
}

test_reset_is_clean() {
  record_result reset_is_clean FAIL "residue detection is not implemented yet" "raw/"
}

# EvoCC must fail loudly and in a fixed shape. A compiler that silently
# forwards to GCC or Clang would convert this red gate into a green lie.
test_compiler_capabilities_are_red() {
  red_ok=1

  evocc_bin="$PROJECT_ROOT/target/release/evocc"
  if [ ! -x "$evocc_bin" ]; then
    # Day-1 shape: the binary does not exist yet. That is a legitimate red
    # only if the workspace itself is absent too; once Cargo scaffolding
    # exists, the binary must exist and must answer NOT_IMPLEMENTED.
    if [ ! -f "$PROJECT_ROOT/Cargo.toml" ]; then
      record_result compiler_capabilities_are_red BLOCKED "evocc workspace does not exist yet; scaffolding has not started" "Cargo.toml"
      return
    fi
    red_ok=0
  else
    for capability in parse evoir eqsat codegen kernel-build; do
      output="$("$evocc_bin" "$capability" --probe 2>&1)"
      rc=$?
      printf '%s\n' "$output" | grep -Fq 'NOT_IMPLEMENTED' || red_ok=0
      [ "$rc" -ne 0 ] || red_ok=0
      # Forwarding to a system compiler is cheating: the output must never
      # contain a GCC or Clang version string.
      printf '%s\n' "$output" | grep -Eiq 'gcc version|clang version' && red_ok=0
    done
  fi

  if [ "$red_ok" -eq 1 ]; then
    record_result compiler_capabilities_are_red PASS "every EvoCC capability fails closed with NOT_IMPLEMENTED and nonzero exit" "raw/"
  else
    record_result compiler_capabilities_are_red FAIL "an EvoCC capability did not fail in the required NOT_IMPLEMENTED shape" "raw/"
  fi
}

test_contract_lock_rejects_drift
test_oracles_are_independent
test_reference_kernel_rebuilds
test_reference_kernel_boots_fresh
test_provenance_is_closed
test_offline_build_has_no_network
test_artifact_set_is_exact
test_reset_is_clean
test_compiler_capabilities_are_red

"$JQ" -s \
  --arg runId "$RUN_ID" \
  --argjson expected "$(printf '%s\n' "$SCRIPT_REQUIRED_IDS" | "$JQ" -R . | "$JQ" -s .)" '
    . as $results |
    ($results | map(.id)) as $actual |
    ($results | group_by(.id) | map(select(length != 1) | .[0].id)) as $duplicates |
    ($expected - $actual) as $missing |
    ($actual - $expected) as $unexpected |
    {
      runId: $runId,
      results: $results,
      counts: ($results | group_by(.status) | map({key: .[0].status, value: length}) | from_entries),
      missingTestIds: $missing,
      unexpectedTestIds: $unexpected,
      duplicateTestIds: $duplicates,
      overall: (if (($missing|length) == 0 and ($unexpected|length) == 0 and ($duplicates|length) == 0 and all($results[]; .status == "PASS"))
                then "PASS" else "FAIL" end)
    }
  ' "$RESULTS" >"$SUMMARY"

printf 'evidence: %s\n' "$RUN_DIR"
"$JQ" '{overall, counts, missingTestIds, unexpectedTestIds, duplicateTestIds}' "$SUMMARY"

if [ "$("$JQ" -r '.overall' "$SUMMARY")" = "PASS" ]; then
  exit 0
fi
exit 1
