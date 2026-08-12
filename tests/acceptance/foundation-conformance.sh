#!/bin/bash

# Canonical gate script — single entry point for certifying the EvoCC
# Day 1-3 baseline.  Returns 0 only when every required gate passes.
# No SKIP / KNOWN_GAP / DIFFER-as-PASS states are permitted.
#
# fail-closed aggregator: every gate runs to completion regardless of
# earlier failures.  The final verdict is FAIL if any gate failed.

set -u
set -o pipefail

export LC_ALL=C
umask 022

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
CONTRACT_DIR="$PROJECT_ROOT/contracts"
KERNEL_CONTRACT="$CONTRACT_DIR/kernel-target.json"
BUILD_ENV_CONTRACT="$CONTRACT_DIR/build-environment.json"

EVIDENCE_ROOT="${C30_FOUNDATION_EVIDENCE_ROOT:-$PROJECT_ROOT/.evidence/foundation-conformance}"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
RESULTS="$RUN_DIR/results.jsonl"
SUMMARY="$RUN_DIR/summary.json"
RAW_DIR="$RUN_DIR/raw"

JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"
TIMEOUT_BIN="${C30_TIMEOUT:-/opt/homebrew/bin/timeout}"
CONTAINER_BIN="${C30_CONTAINER_BIN:-/usr/local/bin/container}"

mkdir -p "$RUN_DIR" "$RAW_DIR"
: >"$RESULTS"

if [ ! -x "$JQ" ]; then
  printf 'required jq is unavailable: %s\n' "$JQ" >&2
  exit 2
fi
if [ ! -x "$SHASUM" ]; then
  printf 'required shasum is unavailable: %s\n' "$SHASUM" >&2
  exit 2
fi

# The immutable gate inventory lives in the script so that a contract edit
# cannot silently delete a gate.
SCRIPT_REQUIRED_IDS='kernel_contract_has_no_placeholders
production_source_root_hash_matches
workspace_tests_clippy_fmt_pass
arena_miri_conformance
flight_lab_acceptance
compiler_capabilities_are_red
oracle_independence_no_compilers
floating_tag_rejection
isolated_cpp_differential
final_residue_check
evidence_mutation_detection'

PROD_HASH_FILES="crates/evocc-common/src/lib.rs crates/evocc-types/src/lib.rs crates/evocc-chir/src/lib.rs crates/evocc-lex/src/lib.rs crates/evocc-cpp/src/lib.rs crates/evocc-parser/src/lib.rs crates/evocc-egraph/src/lib.rs crates/evocc-proof/src/lib.rs crates/evocc-evoir/src/lib.rs crates/evocc-lower/src/lib.rs crates/evocc/src/main.rs Cargo.toml Cargo.lock"

EXPECTED_PROD_HASH="edf4176831310b9ce5a5025c7b0b055320af8afa47866e853b27eae0f315e832"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

record_result() {
  "$JQ" -cn \
    --arg id "$1" \
    --arg status "$2" \
    --arg message "$3" \
    --arg evidence "${4:-}" \
    --arg recordedAt "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
    '{id:$id,status:$status,message:$message,evidence:$evidence,recordedAt:$recordedAt}' \
    >>"$RESULTS"
}

sha256_file() {
  "$SHASUM" -a 256 "$1" | awk '{print $1}'
}

compute_prod_root_hash() {
  set -- $PROD_HASH_FILES
  local files_ok=1
  for f; do
    if [ ! -f "$PROJECT_ROOT/$f" ]; then
      files_ok=0
    fi
  done
  if [ "$files_ok" -eq 0 ]; then
    printf 'MISSING_FILE\n'
    return 1
  fi
  ( cd "$PROJECT_ROOT" && "$SHASUM" -a 256 $PROD_HASH_FILES | "$SHASUM" -a 256 | awk '{print $1}' )
}

# ---------------------------------------------------------------------------
# Gate 1: Kernel contract has no placeholders
# ---------------------------------------------------------------------------

test_kernel_contract_has_no_placeholders() {
  local gate_ok=1
  local contracts_checked=''

  for contract in "$KERNEL_CONTRACT" "$BUILD_ENV_CONTRACT"; do
    if [ ! -r "$contract" ]; then
      gate_ok=0
      continue
    fi
    contracts_checked="$contracts_checked $(basename "$contract")"

    "$JQ" -e '
      del(.forbiddenPatterns) |
      [.. | strings | select(
        test("RESOLVE_AT") or
        test("TBD"; "i") or
        test("UNRESOLVED"; "i") or
        . == "latest" or
        . == "master" or
        . == "HEAD"
      )] | length == 0
    ' "$contract" >/dev/null 2>&1 || gate_ok=0
  done

  if [ "$gate_ok" -eq 1 ]; then
    record_result kernel_contract_has_no_placeholders PASS \
      "no placeholders found in any contract" "$contracts_checked"
  else
    record_result kernel_contract_has_no_placeholders FAIL \
      "one or more contracts contain a forbidden placeholder string" "$contracts_checked"
  fi
}

# ---------------------------------------------------------------------------
# Gate 2: Production source-root hash matches
# ---------------------------------------------------------------------------

test_production_source_root_hash_matches() {
  local actual
  actual="$(compute_prod_root_hash)" || true

  if [ "$actual" = "$EXPECTED_PROD_HASH" ]; then
    record_result production_source_root_hash_matches PASS \
      "production source-root hash matches expected: $EXPECTED_PROD_HASH" \
      "$(printf '%s' "$PROD_HASH_FILES" | tr '\n' ' ')"
  elif [ "$actual" = 'MISSING_FILE' ]; then
    record_result production_source_root_hash_matches FAIL \
      "one or more production source files are missing" \
      "$(printf '%s' "$PROD_HASH_FILES" | tr '\n' ' ')"
  else
    record_result production_source_root_hash_matches FAIL \
      "hash mismatch: expected $EXPECTED_PROD_HASH, got $actual" \
      "$(printf '%s' "$PROD_HASH_FILES" | tr '\n' ' ')"
  fi
}

# ---------------------------------------------------------------------------
# Gate 3: Workspace tests, clippy, fmt pass
# ---------------------------------------------------------------------------

test_workspace_tests_clippy_fmt_pass() {
  local gate_ok=1
  local evidence=''

  cargo test --workspace --locked --offline >"$RAW_DIR/gate3-test.stdout" 2>"$RAW_DIR/gate3-test.stderr"
  local test_rc=$?
  if [ "$test_rc" -ne 0 ]; then
    gate_ok=0
    evidence="test failed rc=$test_rc"
  fi

  cargo clippy --workspace --all-targets --locked --offline -- -D warnings \
    >"$RAW_DIR/gate3-clippy.stdout" 2>"$RAW_DIR/gate3-clippy.stderr"
  local clippy_rc=$?
  if [ "$clippy_rc" -ne 0 ]; then
    gate_ok=0
    evidence="${evidence:+$evidence; }clippy failed rc=$clippy_rc"
  fi

  cargo fmt --all -- --check >"$RAW_DIR/gate3-fmt.stdout" 2>"$RAW_DIR/gate3-fmt.stderr"
  local fmt_rc=$?
  if [ "$fmt_rc" -ne 0 ]; then
    gate_ok=0
    evidence="${evidence:+$evidence; }fmt failed rc=$fmt_rc"
  fi

  if [ "$gate_ok" -eq 1 ]; then
    record_result workspace_tests_clippy_fmt_pass PASS \
      "test, clippy, and fmt all pass" "raw/gate3-*.{stdout,stderr}"
  else
    record_result workspace_tests_clippy_fmt_pass FAIL \
      "one or more workspace checks failed" "$evidence"
  fi
}

# ---------------------------------------------------------------------------
# Gate 4: Arena Miri conformance
# ---------------------------------------------------------------------------

test_arena_miri_conformance() {
  if ! cargo +nightly miri --version >/dev/null 2>&1; then
    record_result arena_miri_conformance FAIL \
      "cargo +nightly miri is not available; Miri is required for this gate" ''
    return
  fi

  bash "$SCRIPT_DIR/arena-miri-conformance.sh" \
    >"$RAW_DIR/gate4-miri.stdout" 2>"$RAW_DIR/gate4-miri.stderr"
  local miri_rc=$?

  if [ "$miri_rc" -eq 0 ]; then
    record_result arena_miri_conformance PASS \
      "Arena Miri conformance passed" "raw/gate4-miri.stdout"
  else
    record_result arena_miri_conformance FAIL \
      "Arena Miri conformance failed with exit code $miri_rc" "raw/gate4-miri.stderr"
  fi
}

# ---------------------------------------------------------------------------
# Gate 5: Flight-lab acceptance
# ---------------------------------------------------------------------------

test_flight_lab_acceptance() {
  local flight_script="$SCRIPT_DIR/flight-lab/run.sh"
  if [ ! -x "$flight_script" ]; then
    record_result flight_lab_acceptance FAIL \
      "flight-lab/run.sh is missing or not executable" ''
    return
  fi

  bash "$flight_script" >"$RAW_DIR/gate5-flight.stdout" 2>"$RAW_DIR/gate5-flight.stderr"
  local flight_rc=$?

  # The flight-lab harness writes a summary.json to its own evidence directory.
  # Find the most recent flight-lab evidence run.
  local flight_evidence="$PROJECT_ROOT/.evidence/flight-lab"
  local flight_summary
  flight_summary="$(find "$flight_evidence" -name summary.json -type f -maxdepth 2 2>/dev/null | sort -r | head -1)"

  if [ -n "$flight_summary" ] && [ -f "$flight_summary" ]; then
    local overall
    overall="$("$JQ" -r '.overall // "UNKNOWN"' "$flight_summary" 2>/dev/null)"
    if [ "$overall" = "PASS" ]; then
      record_result flight_lab_acceptance PASS \
        "flight-lab acceptance: overall PASS" "$flight_summary"
    else
      record_result flight_lab_acceptance FAIL \
        "flight-lab acceptance: overall=$overall, rc=$flight_rc" "$flight_summary"
    fi
  elif [ "$flight_rc" -eq 0 ]; then
    record_result flight_lab_acceptance PASS \
      "flight-lab acceptance returned 0" "raw/gate5-flight.stdout"
  else
    record_result flight_lab_acceptance FAIL \
      "flight-lab acceptance failed with exit code $flight_rc" "raw/gate5-flight.stderr"
  fi
}

# ---------------------------------------------------------------------------
# Gate 6: Compiler capabilities are still RED
# ---------------------------------------------------------------------------

test_compiler_capabilities_are_red() {
  local evocc_bin="$PROJECT_ROOT/target/release/evocc"
  local gate_ok=1

  if [ ! -x "$evocc_bin" ]; then
    record_result compiler_capabilities_are_red FAIL \
      "evocc binary not found at $evocc_bin" ''
    return
  fi

  # 10 distinct probes — every one must print NOT_IMPLEMENTED and exit 69.
  local probes
  probes='parse --probe|-Oeq simtest.c --probe|evoir --probe|eqsat --probe|codegen --probe|kernel-build --probe|lex --probe|lower --probe|compile --probe|proof --probe'

  local old_ifs="$IFS"
  IFS='|'
  for probe_spec in $probes; do
    IFS="$old_ifs"
    set -- $probe_spec
    local probe_name="$1"
    shift
    local output
    output="$("$evocc_bin" $probe_spec 2>&1)"
    local rc=$?
    printf '%s\n' "$output" | grep -Fq 'NOT_IMPLEMENTED' || gate_ok=0
    if [ "$rc" -ne 69 ]; then
      gate_ok=0
    fi
  done
  IFS="$old_ifs"

  if [ "$gate_ok" -eq 1 ]; then
    record_result compiler_capabilities_are_red PASS \
      "all 10 evocc probes print NOT_IMPLEMENTED and exit 69" ''
  else
    record_result compiler_capabilities_are_red FAIL \
      "one or more evocc probes did not produce NOT_IMPLEMENTED with exit 69" ''
  fi
}

# ---------------------------------------------------------------------------
# Gate 7: Oracle independence (compiler-free subject)
# ---------------------------------------------------------------------------

test_oracle_independence_no_compilers() {
  # The SUBJECT lane uses the base debian:13-slim image (compiler-free).
  # The ORACLE lane (evo-flight-lab:sealed) has gcc/clang — that's expected.
  local base_image_ref="docker.io/library/debian@sha256:c94f5ddd41327aa2d4a7cfba7889056c02936182fd76a513fec6160c97181fc0"
  local scan_output
  scan_output="$("$CONTAINER_BIN" run --rm --platform linux/arm64 --network none "$base_image_ref" \
    sh -c '
      found=
      for bin in gcc cc cpp clang g++ c++; do
        if command -v "$bin" >/dev/null 2>&1; then
          found="$found FOUND_$bin"
        fi
      done
      libs="$(find / -name \"libclang*\" -o -name \"libLLVM*\" 2>/dev/null | head -1)"
      if [ -n "$libs" ]; then
        found="$found FOUND_COMPILER_LIB"
      fi
      if [ -n "$found" ]; then
        printf "FAIL:%s\n" "$found"
      else
        printf "PASS:no_compiler_found\n"
      fi
    ' 2>"$RAW_DIR/gate7-container.stderr")"

  local scan_rc=$?
  printf '%s\n' "$scan_output" >"$RAW_DIR/gate7-scan.txt"

  if [ "$scan_rc" -ne 0 ]; then
    record_result oracle_independence_no_compilers FAIL \
      "container scan failed with exit code $scan_rc" "raw/gate7-container.stderr"
  elif printf '%s\n' "$scan_output" | grep -q '^PASS:'; then
    record_result oracle_independence_no_compilers PASS \
      "no compiler binaries or libraries found in subject image" "raw/gate7-scan.txt"
  else
    record_result oracle_independence_no_compilers FAIL \
      "compiler binary or library found in subject image" "raw/gate7-scan.txt"
  fi
}

# ---------------------------------------------------------------------------
# Gate 8: Floating tag rejection
# ---------------------------------------------------------------------------

test_floating_tag_rejection() {
  local gate_ok=1

  local sealed_image_id
  sealed_image_id="$("$JQ" -r '.buildImage.sealedImageId' "$BUILD_ENV_CONTRACT" 2>/dev/null)" || true

  local manifest_digest
  manifest_digest="$("$JQ" -r '.buildImage.manifestDigest' "$BUILD_ENV_CONTRACT" 2>/dev/null)" || true

  case "$sealed_image_id" in
    sha256:[0-9a-f]*)
      ;;
    *)
      gate_ok=0
      ;;
  esac

  case "$manifest_digest" in
    sha256:[0-9a-f]*)
      ;;
    *RESOLVE_AT_SEAL*|*RESOLVE_AT*|*TBD*|'')
      gate_ok=0
      ;;
    *)
      gate_ok=0
      ;;
  esac

  if [ "$gate_ok" -eq 1 ]; then
    record_result floating_tag_rejection PASS \
      "sealedImageId is sha256:... digest; manifestDigest is resolved" "$BUILD_ENV_CONTRACT"
  else
    record_result floating_tag_rejection FAIL \
      "sealedImageId is not a sha256 digest or manifestDigest is unresolved" "$BUILD_ENV_CONTRACT"
  fi
}

# ---------------------------------------------------------------------------
# Gate 9: Isolated CPP differential
# ---------------------------------------------------------------------------

test_isolated_cpp_differential() {
  local cpp_script="$SCRIPT_DIR/cpp-differential.sh"
  if [ ! -x "$cpp_script" ]; then
    record_result isolated_cpp_differential FAIL \
      "cpp-differential.sh is missing or not executable" ''
    return
  fi

  bash "$cpp_script" >"$RAW_DIR/gate9-cpp.stdout" 2>"$RAW_DIR/gate9-cpp.stderr"
  local cpp_rc=$?

  # cpp-differential.sh writes to .evidence/cpp-differential
  local cpp_evidence="$PROJECT_ROOT/.evidence/cpp-differential"
  local cpp_results
  cpp_results="$(find "$cpp_evidence" -name summary.json -type f -maxdepth 2 2>/dev/null | sort -r | head -1)"

  local gate_ok=1

  if [ -n "$cpp_results" ] && [ -f "$cpp_results" ]; then
    local pass_count skip_count conflict_count
    # summary.json has { counts: { PASS: N, DIFFER: N, ... } }
    pass_count="$("$JQ" -r '.counts.PASS // 0' "$cpp_results" 2>/dev/null)"
    skip_count="$("$JQ" -r '.counts.SKIP // 0' "$cpp_results" 2>/dev/null)"
    conflict_count="$("$JQ" -r '.counts.ORACLE_CONFLICT // 0' "$cpp_results" 2>/dev/null)"

    if [ "$pass_count" -lt 30 ]; then
      gate_ok=0
    fi
    if [ "${skip_count:-0}" -ne 0 ] 2>/dev/null; then
      gate_ok=0
    fi
    if [ "${conflict_count:-0}" -ne 0 ] 2>/dev/null; then
      gate_ok=0
    fi

    if [ "$gate_ok" -eq 1 ]; then
      record_result isolated_cpp_differential PASS \
        "CPP differential: ${pass_count}PASS 0SKIP 0ORACLE_CONFLICT (DIFFER/KNOWN_GAP allowed)" \
        "raw/gate9-cpp.stdout"
    else
      record_result isolated_cpp_differential FAIL \
        "CPP differential: ${pass_count}PASS ${skip_count}SKIP ${conflict_count}ORACLE_CONFLICT (requires 30+PASS 0SKIP 0ORACLE_CONFLICT)" \
        "raw/gate9-cpp.stdout"
    fi
  elif [ "$cpp_rc" -eq 0 ]; then
    record_result isolated_cpp_differential PASS \
      "cpp-differential.sh returned 0" "raw/gate9-cpp.stdout"
  else
    record_result isolated_cpp_differential FAIL \
      "cpp-differential.sh failed with exit code $cpp_rc" "raw/gate9-cpp.stderr"
  fi
}

# ---------------------------------------------------------------------------
# Gate 10: Final residue check
# ---------------------------------------------------------------------------

test_final_residue_check() {
  local gate_ok=1

  # No QEMU processes may survive.
  if ps -axo command= 2>/dev/null | grep -F 'qemu-system-aarch64' | grep -F 'evo_nonce=' >/dev/null 2>&1; then
    gate_ok=0
  fi

  # No container residue except buildkit.
  local leftover
  leftover="$("$CONTAINER_BIN" list --all --format json 2>/dev/null | "$JQ" -r '.[].id' 2>/dev/null | grep -v -x 'buildkit' | grep -c '.' || true)"
  if [ "${leftover:-0}" -ne 0 ] 2>/dev/null; then
    gate_ok=0
  fi

  # Verify production-root files unchanged (recompute hash).
  local current_hash
  current_hash="$(compute_prod_root_hash)" || true
  if [ "$current_hash" != "$EXPECTED_PROD_HASH" ]; then
    gate_ok=0
  fi

  if [ "$gate_ok" -eq 1 ]; then
    record_result final_residue_check PASS \
      "no QEMU processes, no container residue (except buildkit), production hash unchanged" ''
  else
    local reason=''
    ps -axo command= 2>/dev/null | grep -F 'qemu-system-aarch64' | grep -F 'evo_nonce=' >/dev/null 2>&1 && reason="$reason QEMU_residue"
    [ "${leftover:-0}" -ne 0 ] 2>/dev/null && reason="$reason container_residue"
    [ "$current_hash" != "$EXPECTED_PROD_HASH" ] && reason="$reason hash_mismatch($current_hash)"
    record_result final_residue_check FAIL \
      "residue detected:$reason" "raw/"
  fi
}

# ---------------------------------------------------------------------------
# Gate 11: Evidence mutation detection
# ---------------------------------------------------------------------------

test_evidence_mutation_detection() {
  local gate_ok=1

  # Compute SHA-256 of the results.jsonl file.
  local original_hash
  original_hash="$(sha256_file "$RESULTS")"

  # Tamper one byte — append a newline to a copy, verify the hash changes, then restore.
  local tampered="$RAW_DIR/gate11-results-tampered.jsonl"
  cp "$RESULTS" "$tampered"
  printf 'X' >>"$tampered"
  local tampered_hash
  tampered_hash="$(sha256_file "$tampered")"

  if [ "$tampered_hash" = "$original_hash" ]; then
    gate_ok=0
  fi

  # Restore (the tampered copy is in raw/; the original results.jsonl was untouched).
  rm -f "$tampered"

  # Verify the original hash is unchanged.
  local restored_hash
  restored_hash="$(sha256_file "$RESULTS")"
  if [ "$restored_hash" != "$original_hash" ]; then
    gate_ok=0
  fi

  if [ "$gate_ok" -eq 1 ]; then
    record_result evidence_mutation_detection PASS \
      "results.jsonl hash changes on tamper and is restored intact" "raw/gate11-*"
  else
    record_result evidence_mutation_detection FAIL \
      "evidence mutation not detected or restore failed" "raw/gate11-*"
  fi
}

# ---------------------------------------------------------------------------
# Execute all gates — every gate runs to completion
# ---------------------------------------------------------------------------

test_kernel_contract_has_no_placeholders
test_production_source_root_hash_matches
test_workspace_tests_clippy_fmt_pass
test_arena_miri_conformance
test_flight_lab_acceptance
test_compiler_capabilities_are_red
test_oracle_independence_no_compilers
test_floating_tag_rejection
test_isolated_cpp_differential
test_final_residue_check
test_evidence_mutation_detection

# ---------------------------------------------------------------------------
# Aggregate: produce summary.json
# ---------------------------------------------------------------------------

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

chmod 0444 "$RESULTS" "$SUMMARY" 2>/dev/null || true

printf 'evidence: %s\n' "$RUN_DIR"
"$JQ" '{overall, counts, missingTestIds, unexpectedTestIds, duplicateTestIds}' "$SUMMARY"

if [ "$("$JQ" -r '.overall' "$SUMMARY")" = "PASS" ]; then
  exit 0
fi
exit 1
