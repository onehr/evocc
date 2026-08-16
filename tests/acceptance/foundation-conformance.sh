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

JQ="${C30_JQ:-/opt/homebrew/Cellar/jq/1.8.2/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"
TIMEOUT_BIN="${C30_TIMEOUT:-/opt/homebrew/Cellar/coreutils/9.11/bin/gtimeout}"
CONTAINER_BIN="${C30_CONTAINER_BIN:-/usr/local/bin/container}"
EVIDENCE_VERIFIER="$SCRIPT_DIR/verify-foundation-evidence.sh"

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
evidence_mutation_detection
root_of_trust_envelope
clean_clone_offline_replay
lexer_full_goldens
cpp_three_way_conformance
parser_matrix
egraph_independent_checker
workspace_fresh_clone_replay
'
SCRIPT_REQUIRED_IDS="$(printf '%s\n' "$SCRIPT_REQUIRED_IDS" | awk 'NF')"

PROD_HASH_FILES="crates/evocc-common/src/lib.rs crates/evocc-types/src/lib.rs crates/evocc-chir/src/lib.rs crates/evocc-lex/src/lib.rs crates/evocc-cpp/src/lib.rs crates/evocc-parser/src/lib.rs crates/evocc-egraph/src/lib.rs crates/evocc-proof/src/lib.rs crates/evocc-evoir/src/lib.rs crates/evocc-lower/src/lib.rs crates/evocc/src/main.rs Cargo.toml Cargo.lock"

EXPECTED_PROD_HASH="ef616a1c6a79c8875d63823c5b614b6e74a1243acf2381b7d99b91fb6b36bb2b"

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

  cargo test --workspace --all-targets --locked --offline >"$RAW_DIR/gate3-test.stdout" 2>"$RAW_DIR/gate3-test.stderr"
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

  local flight_evidence="$PROJECT_ROOT/.evidence/flight-lab"
  local before="$RAW_DIR/gate5-flight-before.txt"
  local after="$RAW_DIR/gate5-flight-after.txt"
  local created="$RAW_DIR/gate5-flight-created.txt"
  find "$flight_evidence" -mindepth 2 -maxdepth 2 -name summary.json -type f \
    2>/dev/null | sort >"$before"

  bash "$flight_script" >"$RAW_DIR/gate5-flight.stdout" 2>"$RAW_DIR/gate5-flight.stderr"
  local flight_rc=$?
  find "$flight_evidence" -mindepth 2 -maxdepth 2 -name summary.json -type f \
    2>/dev/null | sort >"$after"
  comm -13 "$before" "$after" >"$created"

  local created_count
  created_count="$(awk 'NF { count += 1 } END { print count + 0 }' "$created")"
  local flight_summary=''
  if [ "$created_count" -eq 1 ]; then
    flight_summary="$(sed -n '1p' "$created")"
  fi

  if [ "$flight_rc" -ne 0 ] || [ "$created_count" -ne 1 ] || \
     [ ! -f "$flight_summary" ] || \
     ! "$JQ" -e '
       type == "object" and
       (.runId | type == "string" and length > 0) and
       .overall == "PASS" and
       (.results | type == "array") and
       all(.results[]; .status == "PASS")
     ' "$flight_summary" >/dev/null 2>&1; then
    record_result flight_lab_acceptance FAIL \
      "flight-lab must return zero and create exactly one current PASS summary; rc=$flight_rc created=$created_count" \
      "raw/gate5-flight-created.txt"
  else
    record_result flight_lab_acceptance PASS \
      "flight-lab created one causally joined PASS summary" "$flight_summary"
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
  if "$JQ" -e '
      .buildImage as $image |
      ($image.sealedImageId | type == "string" and test("^sha256:[0-9a-f]{64}$")) and
      ($image.manifestDigest | type == "string" and test("^sha256:[0-9a-f]{64}$")) and
      ($image.observedImageId | type == "string" and test("^sha256:[0-9a-f]{64}$")) and
      ($image.executionReference | type == "string" and test("^sha256:[0-9a-f]{64}$")) and
      $image.observedImageId == $image.sealedImageId and
      $image.executionReference == $image.sealedImageId
    ' "$BUILD_ENV_CONTRACT" >/dev/null 2>&1; then
    record_result floating_tag_rejection PASS \
      "sealed, observed, and execution image identities are the same immutable digest" "$BUILD_ENV_CONTRACT"
  else
    record_result floating_tag_rejection FAIL \
      "image identity is floating, malformed, or not joined to the observed sealed image" "$BUILD_ENV_CONTRACT"
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

  local cpp_evidence="$PROJECT_ROOT/.evidence/cpp-differential"
  local before="$RAW_DIR/gate9-cpp-before.txt"
  local after="$RAW_DIR/gate9-cpp-after.txt"
  local created="$RAW_DIR/gate9-cpp-created.txt"
  find "$cpp_evidence" -mindepth 2 -maxdepth 2 -name summary.json -type f \
    2>/dev/null | sort >"$before"

  bash "$cpp_script" >"$RAW_DIR/gate9-cpp.stdout" 2>"$RAW_DIR/gate9-cpp.stderr"
  local cpp_rc=$?
  find "$cpp_evidence" -mindepth 2 -maxdepth 2 -name summary.json -type f \
    2>/dev/null | sort >"$after"
  comm -13 "$before" "$after" >"$created"

  local created_count
  created_count="$(awk 'NF { count += 1 } END { print count + 0 }' "$created")"
  local cpp_results=''
  if [ "$created_count" -eq 1 ]; then
    cpp_results="$(sed -n '1p' "$created")"
  fi

  local required_fixtures
  required_fixtures='001-object-macro-simple.c
002-object-macro-multi-token.c
003-object-macro-empty.c
004-function-macro-simple.c
005-function-macro-nested.c
006-function-macro-no-args.c
007-variadic-macro-normal.c
008-variadic-macro-empty.c
009-variadic-macro-named.c
010-paste-ident-ident.c
011-paste-ident-number.c
012-paste-number-number.c
013-paste-after-expand.c
014-stringify-simple.c
015-stringify-with-spaces.c
016-stringify-before-expand.c
017-ifdef-defined.c
018-ifdef-not-defined.c
019-ifndef.c
020-if-defined.c
021-if-arithmetic.c
022-include-quote.c
024-include-angle.c
025-include-missing.c
026-recursive-macro-blocked.c
027-indirect-recursive-blocked.c
028-predefined-file.c
029-predefined-line.c
030-predefined-date.c
031-predefined-stdc.c
032-empty-file.c
033-comments-only.c
034-define-no-space.c
035-hash-at-eof.c'
  local expected_json
  expected_json="$(printf '%s\n' "$required_fixtures" | "$JQ" -R . | "$JQ" -s .)"

  if [ "$cpp_rc" -ne 0 ] || [ "$created_count" -ne 1 ] || \
     [ ! -f "$cpp_results" ] || \
     ! "$JQ" -e --argjson expected "$expected_json" '
       type == "object" and
       (.runId | type == "string" and length > 0) and
       .overall == "PASS" and
       .total == 34 and
       .counts == {PASS:34} and
       (.results | type == "array" and length == 34) and
       ([.results[].file] | sort) == ($expected | sort) and
       ([.results[].file] | unique | length) == 34 and
       all(.results[];
         .status == "PASS" and
         (.evoccRc | type == "number") and
         (.gccRc | type == "number") and
         (.evoccSignal == 0) and
         (.timedOut == false) and
         (.evoccStdoutSha256 | test("^[0-9a-f]{64}$")) and
         (.gccStdoutSha256 | test("^[0-9a-f]{64}$")) and
         .evoccStdoutSha256 == .gccStdoutSha256 and
         (if .file == "025-include-missing.c"
          then .evoccRc == 1 and .gccRc == 1 and .diagnosticCode == "E0105"
          else .evoccRc == 0 and .gccRc == 0
          end)
       )
     ' "$cpp_results" >/dev/null 2>&1; then
    record_result isolated_cpp_differential FAIL \
      "CPP child must return zero and create one exact, causally joined 34-case summary; rc=$cpp_rc created=$created_count" \
      "raw/gate9-cpp-created.txt"
  else
    record_result isolated_cpp_differential PASS \
      "CPP differential has exact IDs and joined return/signal/timeout/hash evidence" "$cpp_results"
  fi
}

# ---------------------------------------------------------------------------
# Gate 10: Final residue check
# ---------------------------------------------------------------------------

test_final_residue_check() {
  local gate_ok=1

  # No QEMU processes may survive.
  if ! ps -axo command= >"$RAW_DIR/gate10-processes.txt" 2>"$RAW_DIR/gate10-processes.stderr"; then
    gate_ok=0
  elif grep -F 'qemu-system-aarch64' "$RAW_DIR/gate10-processes.txt" \
      | grep -F 'evo_nonce=' >/dev/null 2>&1; then
    gate_ok=0
  fi

  # No container residue except buildkit.
  local leftover=0
  if ! "$CONTAINER_BIN" list --all --format json \
      >"$RAW_DIR/gate10-containers.json" 2>"$RAW_DIR/gate10-containers.stderr" \
    || ! "$JQ" -e 'type == "array"' "$RAW_DIR/gate10-containers.json" >/dev/null 2>&1; then
    gate_ok=0
  else
    leftover="$("$JQ" -r '.[].id' "$RAW_DIR/gate10-containers.json" \
      | grep -v -x 'buildkit' | grep -c '.' || true)"
    if [ "${leftover:-0}" -ne 0 ] 2>/dev/null; then
      gate_ok=0
    fi
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
    if [ ! -s "$RAW_DIR/gate10-processes.txt" ]; then
      reason="$reason process_inventory_unavailable"
    elif grep -F 'qemu-system-aarch64' "$RAW_DIR/gate10-processes.txt" \
        | grep -F 'evo_nonce=' >/dev/null 2>&1; then
      reason="$reason QEMU_residue"
    fi
    [ ! -s "$RAW_DIR/gate10-containers.json" ] && reason="$reason container_inventory_unavailable"
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
  local fixture="$RAW_DIR/gate11-fixture"
  rm -rf "$fixture"
  mkdir -p "$fixture/raw"
  printf '{"overall":"PASS"}\n' >"$fixture/summary.json"
  printf '{"id":"control","status":"PASS"}\n' >"$fixture/results.jsonl"
  printf 'raw witness\n' >"$fixture/raw/witness.txt"
  write_evidence_manifest "$fixture"
  local expected_manifest_sha
  expected_manifest_sha="$(sha256_file "$fixture/evidence-manifest.sha256")"

  C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$expected_manifest_sha" \
    bash "$EVIDENCE_VERIFIER" "$fixture" >/dev/null 2>&1 || gate_ok=0

  local original="$RAW_DIR/gate11-results.original"
  cp "$fixture/results.jsonl" "$original"
  printf 'X' >>"$fixture/results.jsonl"
  C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$expected_manifest_sha" \
    bash "$EVIDENCE_VERIFIER" "$fixture" >/dev/null 2>&1 && gate_ok=0
  mv "$original" "$fixture/results.jsonl"

  printf 'surprise\n' >"$fixture/raw/unexpected.txt"
  C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$expected_manifest_sha" \
    bash "$EVIDENCE_VERIFIER" "$fixture" >/dev/null 2>&1 && gate_ok=0
  rm -f "$fixture/raw/unexpected.txt"

  rm -f "$fixture/raw/witness.txt"
  C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$expected_manifest_sha" \
    bash "$EVIDENCE_VERIFIER" "$fixture" >/dev/null 2>&1 && gate_ok=0
  rm -rf "$fixture"

  if [ "$gate_ok" -eq 1 ]; then
    record_result evidence_mutation_detection PASS \
      "independent verifier rejects modified, added, and missing evidence" "evidence-manifest.sha256"
  else
    record_result evidence_mutation_detection FAIL \
      "evidence mutation not detected or restore failed" "raw/gate11-*"
  fi
}

# ---------------------------------------------------------------------------
# Repository-external freeze envelope for normative tests and contracts
# ---------------------------------------------------------------------------

test_root_of_trust_envelope() {
  local verifier="$SCRIPT_DIR/verify-root-of-trust-envelope.sh"
  local selftest="$SCRIPT_DIR/root-of-trust-envelope-selftest.sh"
  local envelope="${C30_ROOT_OF_TRUST_ENVELOPE:-}"
  local expected_sha="${C30_EXPECTED_ROOT_OF_TRUST_ENVELOPE_SHA256:-}"

  if [ ! -x "$verifier" ] || [ ! -x "$selftest" ]; then
    record_result root_of_trust_envelope FAIL \
      "root-of-trust envelope verifier or selftest is missing" ''
    return
  fi

  bash "$selftest" \
    >"$RAW_DIR/root-of-trust-selftest.stdout" \
    2>"$RAW_DIR/root-of-trust-selftest.stderr"
  local selftest_rc=$?
  if [ "$selftest_rc" -ne 0 ] || \
     ! grep -Fxq 'root-of-trust envelope selftest: 7 passed, 0 failed' \
       "$RAW_DIR/root-of-trust-selftest.stdout"; then
    record_result root_of_trust_envelope FAIL \
      "root-of-trust envelope sensitivity failed: rc=$selftest_rc" \
      "raw/root-of-trust-selftest.stderr"
    return
  fi
  if [ -z "$envelope" ] || [ -z "$expected_sha" ]; then
    record_result root_of_trust_envelope FAIL \
      "repository-external freeze envelope and digest were not supplied" ''
    return
  fi

  C30_EXPECTED_ROOT_OF_TRUST_ENVELOPE_SHA256="$expected_sha" \
    bash "$verifier" "$PROJECT_ROOT" "$envelope" \
      >"$RAW_DIR/root-of-trust.stdout" 2>"$RAW_DIR/root-of-trust.stderr"
  local verifier_rc=$?
  local receipts
  receipts="$(awk 'index($0, "ROOT_OF_TRUST freezeCommit=") == 1 { count += 1 } END { print count + 0 }' "$RAW_DIR/root-of-trust.stdout")"

  if [ "$verifier_rc" -eq 0 ] && [ "$receipts" -eq 1 ]; then
    cp "$envelope" "$RAW_DIR/root-of-trust-envelope.json"
    printf '%s  %s\n' "$expected_sha" root-of-trust-envelope.json \
      >"$RAW_DIR/root-of-trust-envelope.sha256"
    record_result root_of_trust_envelope PASS \
      "external envelope digest, frozen trees/files, and live tool identities agree" \
      "raw/root-of-trust-envelope.json"
  else
    record_result root_of_trust_envelope FAIL \
      "root-of-trust envelope rejected: rc=$verifier_rc receipts=$receipts" \
      "raw/root-of-trust.stderr"
  fi
}

# ---------------------------------------------------------------------------
# Clean no-local clone, locked offline replay
# ---------------------------------------------------------------------------

test_clean_clone_offline_replay() {
  local gate_ok=1
  local reason=''
  local tool_key=''
  local tool_path=''
  local expected_path=''
  local expected_hash=''
  local actual_hash=''
  local tools_tsv="$RAW_DIR/clean-clone-tools.tsv"
  : >"$tools_tsv"

  local git_bin
  local cargo_bin
  local rustc_bin
  git_bin="$("$JQ" -r '.toolchain.hostReplay.git.path // empty' "$BUILD_ENV_CONTRACT")"
  cargo_bin="$("$JQ" -r '.toolchain.hostReplay.cargo.path // empty' "$BUILD_ENV_CONTRACT")"
  rustc_bin="$("$JQ" -r '.toolchain.hostReplay.rustc.path // empty' "$BUILD_ENV_CONTRACT")"

  local tool_specs
  tool_specs="git|$git_bin
cargo|$cargo_bin
rustc|$rustc_bin
jq|$JQ
shasum|$SHASUM
timeout|$TIMEOUT_BIN
bash|$BASH"

  local old_ifs="$IFS"
  IFS='|'
  while read -r tool_key tool_path; do
    [ -n "$tool_key" ] || continue
    expected_path="$("$JQ" -r --arg key "$tool_key" \
      '.toolchain.hostReplay[$key].path // empty' "$BUILD_ENV_CONTRACT")"
    expected_hash="$("$JQ" -r --arg key "$tool_key" \
      '.toolchain.hostReplay[$key].sha256 // empty' "$BUILD_ENV_CONTRACT")"
    actual_hash=''
    if [ -f "$tool_path" ] && [ ! -L "$tool_path" ]; then
      actual_hash="$("$SHASUM" -a 256 "$tool_path" 2>/dev/null | awk '{print $1}')"
    fi
    printf '%s\t%s\t%s\t%s\n' \
      "$tool_key" "$tool_path" "$expected_hash" "$actual_hash" >>"$tools_tsv"
    if [ "$tool_path" != "$expected_path" ] || \
       ! printf '%s\n' "$expected_hash" | grep -Eq '^[0-9a-f]{64}$' || \
       [ "$actual_hash" != "$expected_hash" ]; then
      gate_ok=0
      reason="${reason:+$reason; }tool_identity:$tool_key"
    fi
  done <<EOF
$tool_specs
EOF
  IFS="$old_ifs"

  local source_status=''
  local source_commit=''
  local source_tree=''
  if [ "$gate_ok" -eq 1 ]; then
    source_status="$("$git_bin" -C "$PROJECT_ROOT" status --porcelain=v1 --untracked-files=no 2>/dev/null)"
    source_commit="$("$git_bin" -C "$PROJECT_ROOT" rev-parse HEAD 2>/dev/null)"
    source_tree="$("$git_bin" -C "$PROJECT_ROOT" rev-parse 'HEAD^{tree}' 2>/dev/null)"
    if [ -n "$source_status" ] || \
       ! printf '%s\n' "$source_commit" | grep -Eq '^[0-9a-f]{40}$' || \
       ! printf '%s\n' "$source_tree" | grep -Eq '^[0-9a-f]{40}$'; then
      gate_ok=0
      reason="${reason:+$reason; }source_tree_not_clean_or_resolved"
    fi
  fi

  local scratch=''
  local clone_dir=''
  local clone_commit=''
  local clone_tree=''
  local clone_rc=1
  local cargo_rc=1
  local post_status=''
  if [ "$gate_ok" -eq 1 ]; then
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/c30-clean-clone.XXXXXX")"
    if [ -z "$scratch" ] || [ ! -d "$scratch" ]; then
      gate_ok=0
      reason="${reason:+$reason; }scratch_creation_failed"
    else
      clone_dir="$scratch/repository"
      mkdir -p "$scratch/home" "$scratch/cargo-home" "$scratch/target"
      "$TIMEOUT_BIN" 60 "$git_bin" clone --quiet --no-local --no-hardlinks \
        "$PROJECT_ROOT" "$clone_dir" \
        >"$RAW_DIR/clean-clone-git.stdout" 2>"$RAW_DIR/clean-clone-git.stderr"
      clone_rc=$?
      if [ "$clone_rc" -ne 0 ]; then
        gate_ok=0
        reason="${reason:+$reason; }clone_rc=$clone_rc"
      else
        clone_commit="$("$git_bin" -C "$clone_dir" rev-parse HEAD 2>/dev/null)"
        clone_tree="$("$git_bin" -C "$clone_dir" rev-parse 'HEAD^{tree}' 2>/dev/null)"
        if [ "$clone_commit" != "$source_commit" ] || [ "$clone_tree" != "$source_tree" ]; then
          gate_ok=0
          reason="${reason:+$reason; }clone_identity_mismatch"
        fi
      fi
    fi
  fi

  if [ "$gate_ok" -eq 1 ]; then
    (
      cd "$clone_dir" || exit 2
      /usr/bin/env -i \
        PATH="$(dirname "$cargo_bin"):/usr/bin:/bin" \
        HOME="$scratch/home" \
        CARGO_HOME="$scratch/cargo-home" \
        CARGO_TARGET_DIR="$scratch/target" \
        CARGO_NET_OFFLINE=true \
        RUSTC="$rustc_bin" \
        LC_ALL=C \
        TZ=UTC \
        "$TIMEOUT_BIN" 300 "$cargo_bin" test \
          --workspace --all-targets --locked --offline
    ) >"$RAW_DIR/clean-clone-cargo.stdout" 2>"$RAW_DIR/clean-clone-cargo.stderr"
    cargo_rc=$?
    post_status="$("$git_bin" -C "$clone_dir" status --porcelain=v1 --untracked-files=no 2>/dev/null)"
    if [ "$cargo_rc" -ne 0 ] || [ -n "$post_status" ]; then
      gate_ok=0
      reason="${reason:+$reason; }offline_replay_rc=$cargo_rc"
    fi
  fi

  "$JQ" -n \
    --arg sourceCommit "$source_commit" \
    --arg sourceTree "$source_tree" \
    --arg cloneCommit "$clone_commit" \
    --arg cloneTree "$clone_tree" \
    --argjson cloneRc "$clone_rc" \
    --argjson cargoRc "$cargo_rc" \
    --arg cargoHomeState "fresh-empty" \
    --arg networkPolicy "CARGO_NET_OFFLINE=true + --offline" \
    '{schemaVersion:1,sourceCommit:$sourceCommit,sourceTree:$sourceTree,
      cloneCommit:$cloneCommit,cloneTree:$cloneTree,cloneRc:$cloneRc,cargoRc:$cargoRc,
      cargoHomeState:$cargoHomeState,networkPolicy:$networkPolicy}' \
    >"$RAW_DIR/clean-clone-receipt.json"

  if [ -n "$scratch" ]; then
    case "$scratch" in
      "${TMPDIR:-/tmp}"/c30-clean-clone.*) /bin/rm -rf "$scratch" ;;
      *) gate_ok=0; reason="${reason:+$reason; }unsafe_cleanup_target" ;;
    esac
  fi

  if [ "$gate_ok" -eq 1 ]; then
    record_result clean_clone_offline_replay PASS \
      "no-local clone at the same commit/tree passed locked offline workspace tests" \
      "raw/clean-clone-receipt.json"
  else
    record_result clean_clone_offline_replay FAIL \
      "clean clone replay failed: ${reason:-unknown}" "raw/clean-clone-*"
  fi
}

# ---------------------------------------------------------------------------
# External C11 lexer goldens and raw-byte diagnostics
# ---------------------------------------------------------------------------

test_lexer_full_goldens() {
  local contract="$PROJECT_ROOT/tests/contracts/lexer-c11-goldens.tsv"
  local expected_cases=141
  local gate_ok=1
  local reason=''
  local observed_cases=0
  local unique_ids=0
  local unique_inputs=0
  local unique_digests=0
  local xxd_bin
  local xxd_hash
  local cargo_bin
  local cargo_hash
  local rustc_bin
  local rustc_hash

  xxd_bin="$("$JQ" -r '.toolchain.hostReplay.xxd.path // empty' "$BUILD_ENV_CONTRACT")"
  xxd_hash="$("$JQ" -r '.toolchain.hostReplay.xxd.sha256 // empty' "$BUILD_ENV_CONTRACT")"
  cargo_bin="$("$JQ" -r '.toolchain.hostReplay.cargo.path // empty' "$BUILD_ENV_CONTRACT")"
  cargo_hash="$("$JQ" -r '.toolchain.hostReplay.cargo.sha256 // empty' "$BUILD_ENV_CONTRACT")"
  rustc_bin="$("$JQ" -r '.toolchain.hostReplay.rustc.path // empty' "$BUILD_ENV_CONTRACT")"
  rustc_hash="$("$JQ" -r '.toolchain.hostReplay.rustc.sha256 // empty' "$BUILD_ENV_CONTRACT")"

  if [ ! -f "$contract" ] || [ -L "$contract" ]; then
    gate_ok=0
    reason="golden_contract_missing_or_symlink"
  elif [ "$(sed -n '1p' "$contract")" != \
      $'case_id\tinput_hex\texpected_tokens\texpected_diagnostic' ]; then
    gate_ok=0
    reason="golden_header_invalid"
  fi

  if [ "$gate_ok" -eq 1 ]; then
    observed_cases="$(awk 'NR > 1 && NF { count += 1 } END { print count + 0 }' "$contract")"
    unique_ids="$(awk -F '\t' 'NR > 1 { print $1 }' "$contract" | sort -u | awk 'NF { count += 1 } END { print count + 0 }')"
    unique_inputs="$(awk -F '\t' 'NR > 1 { print $2 }' "$contract" | sort -u | awk '{ count += 1 } END { print count + 0 }')"
    if [ "$observed_cases" -ne "$expected_cases" ] || \
       [ "$unique_ids" -ne "$expected_cases" ] || \
       [ "$unique_inputs" -ne "$expected_cases" ]; then
      gate_ok=0
      reason="golden_denominator_or_uniqueness_invalid"
    fi
  fi

  if [ "$gate_ok" -eq 1 ]; then
    if [ ! -f "$xxd_bin" ] || [ -L "$xxd_bin" ] || \
       [ "$(sha256_file "$xxd_bin")" != "$xxd_hash" ] || \
       [ ! -f "$cargo_bin" ] || [ -L "$cargo_bin" ] || \
       [ "$(sha256_file "$cargo_bin")" != "$cargo_hash" ] || \
       [ ! -f "$rustc_bin" ] || [ -L "$rustc_bin" ] || \
       [ "$(sha256_file "$rustc_bin")" != "$rustc_hash" ]; then
      gate_ok=0
      reason="lexer_tool_identity_invalid"
    fi
  fi

  local digest_manifest="$RAW_DIR/lexer-input-digests.tsv"
  : >"$digest_manifest"
  if [ "$gate_ok" -eq 1 ]; then
    (
      awk -F '\t' 'NR > 1 { print $1 "\t" $2 }' "$contract" \
        | while IFS=$'\t' read -r case_id input_hex; do
        if ! printf '%s\n' "$input_hex" | grep -Eq '^([0-9a-f]{2})*$'; then
          exit 1
        fi
        local digest
        digest="$(printf '%s' "$input_hex" | "$xxd_bin" -r -p | "$SHASUM" -a 256 | awk '{print $1}')" || exit 1
        printf '%s\t%s\n' "$case_id" "$digest"
      done
    ) >"$digest_manifest"
    local digest_rc=$?
    unique_digests="$(awk -F '\t' '{ print $2 }' "$digest_manifest" | sort -u | awk 'NF { count += 1 } END { print count + 0 }')"
    if [ "$digest_rc" -ne 0 ] || [ "$unique_digests" -ne "$expected_cases" ]; then
      gate_ok=0
      reason="input_digest_recomputation_failed"
    fi
  fi

  local scratch=''
  local lexer_rc=1
  local diagnostic_rc=1
  if [ "$gate_ok" -eq 1 ]; then
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/c30-lexer-goldens.XXXXXX")"
    if [ -z "$scratch" ] || [ ! -d "$scratch" ]; then
      gate_ok=0
      reason="lexer_scratch_creation_failed"
    else
      mkdir -p "$scratch/home" "$scratch/cargo-home" "$scratch/target"
      /usr/bin/env -i \
        PATH="$(dirname "$cargo_bin"):/usr/bin:/bin" \
        HOME="$scratch/home" \
        CARGO_HOME="$scratch/cargo-home" \
        CARGO_TARGET_DIR="$scratch/target" \
        CARGO_NET_OFFLINE=true \
        RUSTC="$rustc_bin" \
        LC_ALL=C \
        TZ=UTC \
        "$TIMEOUT_BIN" 180 "$cargo_bin" test -p evocc-lex \
          --test c11_full_goldens --locked --offline -- --nocapture \
        >"$RAW_DIR/lexer-goldens.stdout" 2>"$RAW_DIR/lexer-goldens.stderr"
      lexer_rc=$?

      /usr/bin/env -i \
        PATH="$(dirname "$cargo_bin"):/usr/bin:/bin" \
        HOME="$scratch/home" \
        CARGO_HOME="$scratch/cargo-home" \
        CARGO_TARGET_DIR="$scratch/target" \
        CARGO_NET_OFFLINE=true \
        RUSTC="$rustc_bin" \
        LC_ALL=C \
        TZ=UTC \
        "$TIMEOUT_BIN" 180 "$cargo_bin" test -p evocc \
          --test lexer_diagnostics --locked --offline -- --nocapture \
        >"$RAW_DIR/lexer-diagnostics.stdout" 2>"$RAW_DIR/lexer-diagnostics.stderr"
      diagnostic_rc=$?
      if [ "$lexer_rc" -ne 0 ] || [ "$diagnostic_rc" -ne 0 ]; then
        gate_ok=0
        reason="lexer_goldens_rc=$lexer_rc diagnostic_rc=$diagnostic_rc"
      fi
    fi
  fi

  "$JQ" -n \
    --argjson expectedCases "$expected_cases" \
    --argjson observedCases "$observed_cases" \
    --argjson uniqueInputDigests "$unique_digests" \
    --argjson lexerRc "$lexer_rc" \
    --argjson diagnosticRc "$diagnostic_rc" \
    --arg contractSha256 "$(sha256_file "$contract" 2>/dev/null || true)" \
    --arg inputDigestManifestSha256 "$(sha256_file "$digest_manifest")" \
    '{schemaVersion:1,expectedCases:$expectedCases,observedCases:$observedCases,
      uniqueInputDigests:$uniqueInputDigests,lexerRc:$lexerRc,diagnosticRc:$diagnosticRc,
      contractSha256:$contractSha256,inputDigestManifestSha256:$inputDigestManifestSha256}' \
    >"$RAW_DIR/lexer-golden-receipt.json"

  if [ -n "$scratch" ]; then
    case "$scratch" in
      "${TMPDIR:-/tmp}"/c30-lexer-goldens.*) /bin/rm -rf "$scratch" ;;
      *) gate_ok=0; reason="unsafe_lexer_cleanup_target" ;;
    esac
  fi

  if [ "$gate_ok" -eq 1 ]; then
    record_result lexer_full_goldens PASS \
      "141 distinct normative inputs match kind, spelling, span, diagnostic, and EOF" \
      "raw/lexer-golden-receipt.json"
  else
    record_result lexer_full_goldens FAIL \
      "external lexer golden matrix failed: ${reason:-unknown}" "raw/lexer-*"
  fi
}

# ---------------------------------------------------------------------------
# Hand contract plus isolated EvoCC/GCC/Clang preprocessing observations
# ---------------------------------------------------------------------------

test_cpp_three_way_conformance() {
  local hand_selftest="$SCRIPT_DIR/cpp-hand-goldens-selftest.sh"
  local runner="$SCRIPT_DIR/cpp-three-way-conformance.sh"
  local gate_ok=1
  local reason=''

  if [ ! -x "$hand_selftest" ]; then
    record_result cpp_three_way_conformance FAIL \
      "CPP hand-golden selftest is missing or not executable" ''
    return
  fi
  bash "$hand_selftest" \
    >"$RAW_DIR/cpp-hand-selftest.stdout" 2>"$RAW_DIR/cpp-hand-selftest.stderr"
  local hand_rc=$?
  if [ "$hand_rc" -ne 0 ]; then
    record_result cpp_three_way_conformance FAIL \
      "CPP hand-golden sensitivity failed with rc=$hand_rc" \
      "raw/cpp-hand-selftest.stderr"
    return
  fi

  if [ ! -x "$runner" ]; then
    record_result cpp_three_way_conformance FAIL \
      "isolated three-way CPP runner is not implemented" \
      "tests/acceptance/cpp-three-way-conformance.sh"
    return
  fi

  local evidence_root="$PROJECT_ROOT/.evidence/cpp-three-way"
  local before="$RAW_DIR/cpp-three-way-before.txt"
  local after="$RAW_DIR/cpp-three-way-after.txt"
  local created="$RAW_DIR/cpp-three-way-created.txt"
  find "$evidence_root" -mindepth 2 -maxdepth 2 -name summary.json -type f \
    2>/dev/null | sort >"$before"

  bash "$runner" \
    >"$RAW_DIR/cpp-three-way.stdout" 2>"$RAW_DIR/cpp-three-way.stderr"
  local runner_rc=$?
  find "$evidence_root" -mindepth 2 -maxdepth 2 -name summary.json -type f \
    2>/dev/null | sort >"$after"
  comm -13 "$before" "$after" >"$created"

  local created_count
  created_count="$(awk 'NF { count += 1 } END { print count + 0 }' "$created")"
  local summary=''
  if [ "$created_count" -eq 1 ]; then
    summary="$(sed -n '1p' "$created")"
  fi
  local expected_ids
  expected_ids="$("$JQ" -R . "$PROJECT_ROOT/tests/contracts/cpp-required-fixtures.txt" | "$JQ" -s .)"
  local subject_manifest
  local oracle_image
  local gcc_hash
  local clang_hash
  subject_manifest="$("$JQ" -r '.buildImage.manifestDigest' "$BUILD_ENV_CONTRACT")"
  oracle_image="$("$JQ" -r '.buildImage.sealedImageId' "$BUILD_ENV_CONTRACT")"
  gcc_hash="$("$JQ" -r '.toolchain.gcc.binarySha256' "$BUILD_ENV_CONTRACT")"
  clang_hash="$("$JQ" -r '.toolchain.clang.binarySha256' "$BUILD_ENV_CONTRACT")"

  if [ "$runner_rc" -ne 0 ] || [ "$created_count" -ne 1 ] || \
     [ ! -f "$summary" ] || \
     ! "$JQ" -e \
       --argjson expected "$expected_ids" \
       --arg subjectManifest "$subject_manifest" \
       --arg oracleImage "$oracle_image" \
       --arg gccHash "$gcc_hash" \
       --arg clangHash "$clang_hash" '
       type == "object" and
       .schemaVersion == 1 and
       (.runId | type == "string" and length > 0) and
       (.sourceCommit | test("^[0-9a-f]{40}$")) and
       (.sourceTree | test("^[0-9a-f]{40}$")) and
       .subject.imageManifest == $subjectManifest and
       .subject.hostTriple == "aarch64-unknown-linux-musl" and
       .subject.compilerTarget == "aarch64-unknown-linux-gnu" and
       .subject.staticElf == true and
       .subject.compilerInventoryEmpty == true and
       .subject.loadedObjectClosure == [] and
       .subject.descendantExecs == [] and
       (.subject.binarySha256 | test("^[0-9a-f]{64}$")) and
       .oracles.imageId == $oracleImage and
       .oracles.gccSha256 == $gccHash and
       .oracles.clangSha256 == $clangHash and
       .total == 34 and
       .counts == {PASS:34} and
       .overall == "PASS" and
       (.results | type == "array" and length == 34) and
       ([.results[].file] | sort) == ($expected | sort) and
       ([.results[].file] | unique | length) == 34 and
       all(.results[];
         .status == "PASS" and
         .timedOut == false and
         .evoccSignal == 0 and .gccSignal == 0 and .clangSignal == 0 and
         (.handTokenSha256 | test("^[0-9a-f]{64}$")) and
         (.evoccTokenSha256 | test("^[0-9a-f]{64}$")) and
         (.gccTokenSha256 | test("^[0-9a-f]{64}$")) and
         (.clangTokenSha256 | test("^[0-9a-f]{64}$")) and
         .handTokenSha256 == .evoccTokenSha256 and
         .handTokenSha256 == .gccTokenSha256 and
         .handTokenSha256 == .clangTokenSha256 and
         (if .file == "025-include-missing.c"
          then .expectedRc == 1 and .evoccRc == 1 and .gccRc == 1 and .clangRc == 1 and
               .diagnostic == {code:"E0105",line:1,column:1,
                 category:"include-not-found",subject:"nonexistent.h"}
          else .expectedRc == 0 and .evoccRc == 0 and .gccRc == 0 and .clangRc == 0 and
               .diagnostic == null
          end)
       )
     ' "$summary" >/dev/null 2>&1; then
    gate_ok=0
    reason="runner_rc=$runner_rc created=$created_count or summary contract mismatch"
  fi

  if [ "$gate_ok" -eq 1 ]; then
    record_result cpp_three_way_conformance PASS \
      "hand contract, isolated EvoCC, GCC, and Clang agree on all 34 fixtures" "$summary"
  else
    record_result cpp_three_way_conformance FAIL \
      "three-way CPP conformance failed: $reason" "raw/cpp-three-way-*"
  fi
}

# ---------------------------------------------------------------------------
# Declarator, expression, EOF, and process-diagnostic parser observations
# ---------------------------------------------------------------------------

test_parser_matrix() {
  local matrix_stdout="$RAW_DIR/parser-matrix.stdout"
  local matrix_stderr="$RAW_DIR/parser-matrix.stderr"
  local diagnostics_stdout="$RAW_DIR/parser-diagnostics.stdout"
  local diagnostics_stderr="$RAW_DIR/parser-diagnostics.stderr"

  if [ ! -x "$TIMEOUT_BIN" ]; then
    record_result parser_matrix FAIL \
      "required timeout tool is unavailable: $TIMEOUT_BIN" ''
    return
  fi

  (
    cd "$PROJECT_ROOT" || exit 2
    "$TIMEOUT_BIN" 60 cargo test -p evocc-parser --test parser_matrix \
      --locked --offline -- --nocapture --test-threads=1
  ) >"$matrix_stdout" 2>"$matrix_stderr"
  local matrix_rc=$?

  (
    cd "$PROJECT_ROOT" || exit 2
    "$TIMEOUT_BIN" 60 cargo test -p evocc --test parser_diagnostics \
      --locked --offline -- --nocapture --test-threads=1
  ) >"$diagnostics_stdout" 2>"$diagnostics_stderr"
  local diagnostics_rc=$?

  local expression_receipts
  local declarator_receipts
  local positive_receipts
  local negative_receipts
  expression_receipts="$(awk 'index($0, "PARSER_MATRIX expressions=20 unique=20") { count += 1 } END { print count + 0 }' "$matrix_stdout")"
  declarator_receipts="$(awk 'index($0, "PARSER_MATRIX declarators=9 unique=9") { count += 1 } END { print count + 0 }' "$matrix_stdout")"
  positive_receipts="$(awk 'index($0, "PARSER_DIAGNOSTICS positive=3 unique=3") { count += 1 } END { print count + 0 }' "$diagnostics_stdout")"
  negative_receipts="$(awk 'index($0, "PARSER_DIAGNOSTICS negative=25 unique=25") { count += 1 } END { print count + 0 }' "$diagnostics_stdout")"

  if [ "$matrix_rc" -eq 0 ] && [ "$diagnostics_rc" -eq 0 ] && \
     [ "$expression_receipts" -eq 1 ] && [ "$declarator_receipts" -eq 1 ] && \
     [ "$positive_receipts" -eq 1 ] && [ "$negative_receipts" -eq 1 ]; then
    record_result parser_matrix PASS \
      "20 expression, 9 declarator, 3 positive process, and 25 exact negative cases passed" \
      "raw/parser-{matrix,diagnostics}.{stdout,stderr}"
  else
    record_result parser_matrix FAIL \
      "parser matrix failed: matrix_rc=$matrix_rc diagnostics_rc=$diagnostics_rc receipts=$expression_receipts/$declarator_receipts/$positive_receipts/$negative_receipts" \
      "raw/parser-{matrix,diagnostics}.{stdout,stderr}"
  fi
}

# ---------------------------------------------------------------------------
# Independent least-congruence oracle, mutation sensitivity, and proof guard
# ---------------------------------------------------------------------------

test_egraph_independent_checker() {
  local mutation_stdout="$RAW_DIR/egraph-mutation.stdout"
  local mutation_stderr="$RAW_DIR/egraph-mutation.stderr"
  local checker_stdout="$RAW_DIR/egraph-checker.stdout"
  local checker_stderr="$RAW_DIR/egraph-checker.stderr"
  local proof_stdout="$RAW_DIR/proof-semantics.stdout"
  local proof_stderr="$RAW_DIR/proof-semantics.stderr"

  if [ ! -x "$TIMEOUT_BIN" ]; then
    record_result egraph_independent_checker FAIL \
      "required timeout tool is unavailable: $TIMEOUT_BIN" ''
    return
  fi

  "$TIMEOUT_BIN" 300 bash "$SCRIPT_DIR/egraph-mutation-selftest.sh" \
    >"$mutation_stdout" 2>"$mutation_stderr"
  local mutation_rc=$?

  (
    cd "$PROJECT_ROOT" || exit 2
    "$TIMEOUT_BIN" 300 cargo test -p evocc-egraph --test independent_checker \
      --locked --offline -- --nocapture --test-threads=1
  ) >"$checker_stdout" 2>"$checker_stderr"
  local checker_rc=$?

  (
    cd "$PROJECT_ROOT" || exit 2
    "$TIMEOUT_BIN" 60 cargo test -p evocc-proof --test semantic_validation \
      --locked --offline -- --nocapture --test-threads=1
  ) >"$proof_stdout" 2>"$proof_stderr"
  local proof_rc=$?

  local mutation_receipts
  local campaign_receipts
  local exhaustive_receipts
  local control_receipts
  local proof_receipts
  mutation_receipts="$(awk '/^PASS (egraph_baseline_control|rebuild_noop|repair_noop|missing_parent_update|collapsed_constant)$/ { count += 1 } END { print count + 0 }' "$mutation_stdout")"
  campaign_receipts="$(awk 'index($0, "EGRAPH_CAMPAIGN seeds=100 actions=10000 ") { count += 1 } END { print count + 0 }' "$checker_stdout")"
  exhaustive_receipts="$(awk 'index($0, "EGRAPH_EXHAUSTIVE alphabet=5 max_length=6 sequences=19531 ") { count += 1 } END { print count + 0 }' "$checker_stdout")"
  control_receipts="$(awk 'index($0, "EGRAPH_CHECKER_CONTROLS baseline=1 faults=5") { count += 1 } END { print count + 0 }' "$checker_stdout")"
  proof_receipts="$(awk 'index($0, "PROOF_SEMANTICS ") { count += 1 } END { print count + 0 }' "$proof_stdout")"

  if [ "$mutation_rc" -eq 0 ] && [ "$checker_rc" -eq 0 ] && [ "$proof_rc" -eq 0 ] && \
     [ "$mutation_receipts" -eq 5 ] && [ "$campaign_receipts" -eq 1 ] && \
     [ "$exhaustive_receipts" -eq 1 ] && [ "$control_receipts" -eq 1 ] && \
     [ "$proof_receipts" -eq 4 ]; then
    record_result egraph_independent_checker PASS \
      "five mutants killed; 100x100 plus 19531-sequence oracle campaigns and four proof guards passed" \
      "raw/egraph-* raw/proof-semantics.*"
  else
    record_result egraph_independent_checker FAIL \
      "e-graph checker failed: rc=$mutation_rc/$checker_rc/$proof_rc receipts=$mutation_receipts/$campaign_receipts/$exhaustive_receipts/$control_receipts/$proof_receipts" \
      "raw/egraph-* raw/proof-semantics.*"
  fi
}

# ---------------------------------------------------------------------------
# Two independent no-local clones with isolated offline Cargo state
# ---------------------------------------------------------------------------

test_workspace_fresh_clone_replay() {
  local runner="$SCRIPT_DIR/workspace-fresh-clone-replay.sh"
  local output_dir="$RAW_DIR/workspace-fresh-clones"
  local receipt="$output_dir/fresh-clone-receipt.json"
  mkdir -p "$output_dir"

  if [ ! -x "$runner" ]; then
    record_result workspace_fresh_clone_replay FAIL \
      "two-clone replay runner is missing or not executable" ''
    return
  fi

  bash "$runner" "$PROJECT_ROOT" "$output_dir" \
    >"$RAW_DIR/workspace-fresh-clone.stdout" \
    2>"$RAW_DIR/workspace-fresh-clone.stderr"
  local runner_rc=$?
  local source_commit
  local source_tree
  source_commit="$(/usr/bin/git -C "$PROJECT_ROOT" rev-parse HEAD 2>/dev/null)"
  source_tree="$(/usr/bin/git -C "$PROJECT_ROOT" rev-parse 'HEAD^{tree}' 2>/dev/null)"

  if [ "$runner_rc" -eq 0 ] && [ -f "$receipt" ] && \
     "$JQ" -e --arg commit "$source_commit" --arg tree "$source_tree" '
       type == "object" and
       (keys == ["lanes","overall","schemaVersion","sourceCommit","sourceTree"]) and
       .schemaVersion == 1 and .sourceCommit == $commit and .sourceTree == $tree and
       .overall == "PASS" and
       (.lanes | type == "array" and length == 2) and
       ([.lanes[].lane] == ["one","two"]) and
       all(.lanes[];
         keys == ["cargoHomeFresh","clippyRc","cloneRc","commit","fmtRc","lane",
                  "offline","pass","postStatus","targetFresh","testRc","tree"] and
         .commit == $commit and .tree == $tree and .cloneRc == 0 and
         .testRc == 0 and .clippyRc == 0 and .fmtRc == 0 and
         .postStatus == "" and .pass == true and .cargoHomeFresh == true and
         .targetFresh == true and .offline == true)
     ' "$receipt" >/dev/null 2>&1; then
    record_result workspace_fresh_clone_replay PASS \
      "two no-local clones independently passed locked offline tests, Clippy, and fmt" \
      "raw/workspace-fresh-clones/fresh-clone-receipt.json"
  else
    record_result workspace_fresh_clone_replay FAIL \
      "two-clone replay failed: rc=$runner_rc or receipt mismatch" \
      "raw/workspace-fresh-clones/"
  fi
}

write_evidence_manifest() {
  local evidence_dir="$1"
  local manifest="$evidence_dir/evidence-manifest.sha256"
  local temporary="$evidence_dir/.evidence-manifest.tmp"
  (
    cd "$evidence_dir" || exit 1
    find . -type f ! -name evidence-manifest.sha256 ! -name .evidence-manifest.tmp | sort \
      | while IFS= read -r file; do
          "$SHASUM" -a 256 "$file"
        done
  ) >"$temporary"
  mv "$temporary" "$manifest"
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
test_root_of_trust_envelope
test_clean_clone_offline_replay
test_lexer_full_goldens
test_cpp_three_way_conformance
test_parser_matrix
test_egraph_independent_checker
test_workspace_fresh_clone_replay

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

write_evidence_manifest "$RUN_DIR"
FINAL_EVIDENCE_OK=1
FINAL_MANIFEST_SHA="$(sha256_file "$RUN_DIR/evidence-manifest.sha256")"
C30_EXPECTED_EVIDENCE_MANIFEST_SHA256="$FINAL_MANIFEST_SHA" \
  bash "$EVIDENCE_VERIFIER" "$RUN_DIR" >/dev/null 2>&1 || FINAL_EVIDENCE_OK=0

chmod 0444 "$RESULTS" "$SUMMARY" 2>/dev/null || true

printf 'evidence: %s\n' "$RUN_DIR"
"$JQ" '{overall, counts, missingTestIds, unexpectedTestIds, duplicateTestIds}' "$SUMMARY"

if [ "$("$JQ" -r '.overall' "$SUMMARY")" = "PASS" ] && [ "$FINAL_EVIDENCE_OK" -eq 1 ]; then
  exit 0
fi
exit 1
