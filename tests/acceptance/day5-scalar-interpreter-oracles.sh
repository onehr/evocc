#!/bin/bash

# Sealed, defined-only GCC/Clang differential for the Day-5 scalar corpus.
# There is intentionally no subject-binary input and no native execution path
# for the 64 EXPECTED_REJECT programs.

set -eu
set -o pipefail

export LC_ALL=C
export TZ=UTC
export PYTHONDONTWRITEBYTECODE=1
export PATH=/usr/bin:/bin:/usr/sbin:/sbin
umask 022

if [ "$#" -ne 0 ]; then
  printf 'usage: %s\n' "$0" >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
CONTRACT="$PROJECT_ROOT/tests/contracts/day5-scalar-interpreter.json"
CORPUS="$SCRIPT_DIR/day5-scalar-interpreter.py"
ORACLE_HELPER="$SCRIPT_DIR/day5-scalar-interpreter-oracles.py"
BUILD_ENV="$PROJECT_ROOT/contracts/build-environment.json"
PYTHON_BIN="/usr/bin/python3"
EVIDENCE_ROOT="$PROJECT_ROOT/.evidence/day5-scalar-interpreter-oracles"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
BINDING_POLICY_VERSION="apple-container-local-alias-v1"

for path in "$CONTRACT" "$CORPUS" "$ORACLE_HELPER" "$BUILD_ENV"; do
  [ -f "$path" ] && [ ! -L "$path" ] || {
    printf 'required Day-5 oracle input is missing or symbolic: %s\n' "$path" >&2
    exit 2
  }
done
[ -x "$PYTHON_BIN" ] && [ -f "$PYTHON_BIN" ] && [ ! -L "$PYTHON_BIN" ] || {
  printf 'fixed Day-5 Python is unavailable, non-regular, or symbolic\n' >&2
  exit 2
}

build_value() {
  "$PYTHON_BIN" -I -c '
import json
import sys

value = json.load(open(sys.argv[1], encoding="utf-8"))
for component in sys.argv[2].split("."):
    value = value[component]
if not isinstance(value, str) or "\n" in value:
    raise SystemExit("invalid build authority field")
print(value)
' "$BUILD_ENV" "$1"
}

CONTAINER_BIN="$(build_value containerRuntime.cliPath)"
CONTAINER_SHA256="$(build_value containerRuntime.cliSha256)"
CONTAINER_VERSION="$(build_value containerRuntime.version)"
CONTAINER_COMMIT="$(build_value containerRuntime.commitSha)"
CONTAINER_COMMIT_SHORT="$(printf '%s' "$CONTAINER_COMMIT" | cut -c 1-7)"
JQ="$(build_value toolchain.hostReplay.jq.path)"
JQ_SHA256="$(build_value toolchain.hostReplay.jq.sha256)"
JQ_VERSION="$(build_value toolchain.hostReplay.jq.version)"
SHASUM="$(build_value toolchain.hostReplay.shasum.path)"
SHASUM_SHA256="$(build_value toolchain.hostReplay.shasum.sha256)"
TIMEOUT_BIN="$(build_value toolchain.hostReplay.timeout.path)"
TIMEOUT_SHA256="$(build_value toolchain.hostReplay.timeout.sha256)"
TIMEOUT_VERSION="$(build_value toolchain.hostReplay.timeout.version)"

for tool in "$CONTAINER_BIN" "$JQ" "$SHASUM" "$TIMEOUT_BIN"; do
  [ -x "$tool" ] && [ -f "$tool" ] && [ ! -L "$tool" ] || {
    printf 'frozen Day-5 oracle tool is unavailable, non-regular, or symbolic: %s\n' "$tool" >&2
    exit 2
  }
done

sha256_file() {
  "$SHASUM" -a 256 "$1" | awk '{print $1}'
}

[ "$(sha256_file "$CONTAINER_BIN")" = "$CONTAINER_SHA256" ] && \
[ "$(sha256_file "$JQ")" = "$JQ_SHA256" ] && \
[ "$(sha256_file "$SHASUM")" = "$SHASUM_SHA256" ] && \
[ "$(sha256_file "$TIMEOUT_BIN")" = "$TIMEOUT_SHA256" ] && \
[ "$($CONTAINER_BIN --version)" = "container CLI version $CONTAINER_VERSION (build: release, commit: $CONTAINER_COMMIT_SHORT)" ] && \
[ "$($JQ --version)" = "$JQ_VERSION" ] && \
[ "$($TIMEOUT_BIN --version | sed -n '1p')" = "$TIMEOUT_VERSION" ] || {
  printf 'frozen Day-5 host oracle tool identity differs\n' >&2
  exit 2
}

mkdir -p "$EVIDENCE_ROOT"
mkdir "$RUN_DIR"
mkdir "$RUN_DIR/lane-output-gcc" "$RUN_DIR/lane-output-clang"

# This receipt is authoritative until, and unless, all 256 cases complete in
# both independently identified compiler lanes and the comparator accepts them.
"$JQ" -n --arg runId "$RUN_ID" \
  '{schemaVersion:1,runId:$runId,status:"NOT_MEASURED",
    scope:"definedCasesOnly",requiredConsensus:256,
    expectedRejectNativeExecution:"PROHIBITED",
    reason:"sealed GCC and Clang lanes have not both completed"}' \
  >"$RUN_DIR/summary.json"

cp "$CONTRACT" "$RUN_DIR/contract.json"
cp "$CORPUS" "$RUN_DIR/day5-scalar-interpreter.py"
cp "$ORACLE_HELPER" "$RUN_DIR/day5-scalar-interpreter-oracles.py"
sha256_file "$CONTRACT" >"$RUN_DIR/contract.sha256"
sha256_file "$CORPUS" >"$RUN_DIR/corpus.sha256"
sha256_file "$ORACLE_HELPER" >"$RUN_DIR/oracle-helper.sha256"
sha256_file "$0" >"$RUN_DIR/oracle-runner.sha256"

"$PYTHON_BIN" -I "$CORPUS" audit \
  >"$RUN_DIR/corpus-audit.json" 2>"$RUN_DIR/corpus-audit.stderr"
"$PYTHON_BIN" -I "$CORPUS" self-test \
  >"$RUN_DIR/subject-comparator-selftest.json" \
  2>"$RUN_DIR/subject-comparator-selftest.stderr"
"$PYTHON_BIN" -I "$ORACLE_HELPER" self-test \
  >"$RUN_DIR/oracle-comparator-selftest.json" \
  2>"$RUN_DIR/oracle-comparator-selftest.stderr"
"$PYTHON_BIN" -I "$ORACLE_HELPER" materialize-defined "$RUN_DIR/lane-input" \
  >"$RUN_DIR/materialization.json" 2>"$RUN_DIR/materialization.stderr"
"$PYTHON_BIN" -I "$ORACLE_HELPER" audit-lane-input "$RUN_DIR/lane-input" \
  >"$RUN_DIR/lane-input-audit.json" 2>"$RUN_DIR/lane-input-audit.stderr"
sha256_file "$RUN_DIR/lane-input/manifest.tsv" >"$RUN_DIR/lane-input-manifest.sha256"

"$PYTHON_BIN" -I -c '
import json
import sys

contract = json.load(open(sys.argv[1], encoding="utf-8"))
build = json.load(open(sys.argv[2], encoding="utf-8"))
authority = contract["definedOracleAuthority"]
assert contract["denominators"]["defined"] == 256
assert contract["denominators"]["expectedReject"] == 64
assert authority["scope"] == "definedCasesOnly"
assert authority["expectedRejectNativeExecution"] == "PROHIBITED"
assert authority["imageId"] == build["buildImage"]["executionReference"]
assert authority["imageId"] == build["buildImage"]["sealedImageId"]
assert authority["compilerFlags"] == ["-std=gnu11", "-O0", "-g0", "-pipe"]
for lane in ("gcc", "clang"):
    tool = build["toolchain"][lane]
    expected = authority[lane]
    assert expected["path"] == tool["binaryPathInImage"]
    assert expected["sha256"] == tool["binarySha256"]
    assert expected["version"] == tool["resolvedVersion"]
print(json.dumps({"status": "PASS", "defined": 256,
                  "expectedReject": 64, "expectedRejectExecuted": 0},
                 indent=2, sort_keys=True))
' "$CONTRACT" "$BUILD_ENV" >"$RUN_DIR/authority-cross-check.json" \
  2>"$RUN_DIR/authority-cross-check.stderr"

oracle_image_id="$($JQ -r '.definedOracleAuthority.imageId' "$CONTRACT")"
oracle_gcc="$($JQ -r '.definedOracleAuthority.gcc.path' "$CONTRACT")"
oracle_clang="$($JQ -r '.definedOracleAuthority.clang.path' "$CONTRACT")"
gcc_hash="$($JQ -r '.definedOracleAuthority.gcc.sha256' "$CONTRACT")"
clang_hash="$($JQ -r '.definedOracleAuthority.clang.sha256' "$CONTRACT")"
gcc_version="$($JQ -r '.definedOracleAuthority.gcc.version' "$CONTRACT")"
clang_version="$($JQ -r '.definedOracleAuthority.clang.version' "$CONTRACT")"
oracle_tag="$(build_value buildImage.sealedImageTag)"
wrong_tag="$(build_value buildImage.reference)"

IDENTITY_INIT_SCRIPT='set -eu
/usr/bin/sha256sum "$1" "$2"
"$1" --version | /usr/bin/sed -n "1p"
"$2" --version | /usr/bin/sed -n "1p"'

LANE_INIT_SCRIPT='set -u
tool="$1"
lane="$2"
work="/output"
export TMPDIR="$work"
executable="$work/case-program"
while IFS="$(printf "\t")" read -r ordinal filename case_id source_sha; do
  source="/input/defined-cases/$filename"
  actual_sha="$(/usr/bin/sha256sum "$source" | /usr/bin/cut -d " " -f 1)"
  /usr/bin/rm -f "$executable"
  if [ "$actual_sha" != "$source_sha" ]; then
    printf "%s\t%s\t%s\t%s\t125\tNA\tNA\tNA\n" \
      "$ordinal" "$filename" "$case_id" "$actual_sha"
    continue
  fi
  "$tool" -std=gnu11 -O0 -g0 -pipe "$source" -o "$executable" \
    >"$work/$ordinal.compile.stdout" 2>"$work/$ordinal.compile.stderr"
  compile_rc=$?
  if [ "$compile_rc" -ne 0 ]; then
    printf "%s\t%s\t%s\t%s\t%s\tNA\tNA\tNA\n" \
      "$ordinal" "$filename" "$case_id" "$actual_sha" "$compile_rc"
    continue
  fi
  "$executable" >"$work/$ordinal.run.stdout" 2>"$work/$ordinal.run.stderr"
  run_rc=$?
  stdout_bytes="$(/usr/bin/wc -c <"$work/$ordinal.run.stdout")"
  stderr_bytes="$(/usr/bin/wc -c <"$work/$ordinal.run.stderr")"
  printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n" \
    "$ordinal" "$filename" "$case_id" "$actual_sha" "$compile_rc" \
    "$run_rc" "$stdout_bytes" "$stderr_bytes"
done </input/manifest.tsv
/usr/bin/rm -f "$executable"'

cleanup_names=''
alias_created=0
oracle_run_ref=''
cleanup() {
  for name in $cleanup_names; do
    "$CONTAINER_BIN" stop "$name" >/dev/null 2>&1 || true
    "$CONTAINER_BIN" delete "$name" >/dev/null 2>&1 || true
  done
  if [ "$alias_created" -eq 1 ] && [ -n "$oracle_run_ref" ]; then
    "$CONTAINER_BIN" image delete "$oracle_run_ref" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT HUP INT TERM

write_container_inventory() {
  observed_name="$1"
  evidence_name="$2"
  "$CONTAINER_BIN" list --all --format json \
    >"$RUN_DIR/$evidence_name-container.json" \
    2>"$RUN_DIR/$evidence_name-container.stderr"
}

verify_identity_container() {
  observed_name="$1"
  evidence_name="$2"
  write_container_inventory "$observed_name" "$evidence_name" || return 1
  "$JQ" -e --arg name "$observed_name" --arg digest "$oracle_image_id" \
    --arg runRef "$oracle_run_ref" --arg initScript "$IDENTITY_INIT_SCRIPT" \
    --arg gcc "$oracle_gcc" --arg clang "$oracle_clang" '
    any(.[]; .id == $name and
      .configuration.image.reference == $runRef and
      .configuration.image.descriptor.digest == $digest and
      .configuration.platform.os == "linux" and
      .configuration.platform.architecture == "arm64" and
      .configuration.readOnly == true and
      .configuration.networks == [] and
      .configuration.capDrop == ["ALL"] and
      .configuration.mounts == [] and
      .configuration.initProcess.executable == "/bin/sh" and
      .configuration.initProcess.arguments == ["-c",$initScript,"sh",$gcc,$clang])
  ' "$RUN_DIR/$evidence_name-container.json" >/dev/null
}

verify_lane_container() {
  observed_name="$1"
  evidence_name="$2"
  lane="$3"
  tool="$4"
  input_source="$RUN_DIR/lane-input"
  output_source="$RUN_DIR/lane-output-$lane"
  write_container_inventory "$observed_name" "$evidence_name" || return 1
  "$JQ" -e --arg name "$observed_name" --arg digest "$oracle_image_id" \
    --arg runRef "$oracle_run_ref" --arg initScript "$LANE_INIT_SCRIPT" \
    --arg lane "$lane" --arg tool "$tool" --arg inputSource "$input_source" \
    --arg outputSource "$output_source" '
    any(.[]; .id == $name and
      .configuration.image.reference == $runRef and
      .configuration.image.descriptor.digest == $digest and
      .configuration.platform.os == "linux" and
      .configuration.platform.architecture == "arm64" and
      .configuration.readOnly == true and
      .configuration.networks == [] and
      .configuration.capDrop == ["ALL"] and
      .configuration.mounts == [
        {destination:"/input",options:["ro"],source:$inputSource,
          type:{virtiofs:{}}},
        {destination:"/output",options:[],source:$outputSource,
          type:{virtiofs:{}}}
      ] and
      .configuration.initProcess.executable == "/bin/sh" and
      .configuration.initProcess.arguments == ["-c",$initScript,"sh",$tool,$lane])
  ' "$RUN_DIR/$evidence_name-container.json" >/dev/null
}

"$CONTAINER_BIN" image list --format json --verbose \
  >"$RUN_DIR/images-before.json" 2>"$RUN_DIR/images-before.stderr"
"$JQ" -e --arg tag "$oracle_tag" --arg digest "$oracle_image_id" '
  any(.[]; .configuration.name == $tag and
    .configuration.descriptor.digest == $digest and
    any(.variants[]; .platform.os == "linux" and .platform.architecture == "arm64"))
' "$RUN_DIR/images-before.json" >/dev/null || {
  printf 'pinned Day-5 scalar oracle image is unavailable\n' >&2
  exit 1
}

wrong_tag_digest="$($JQ -r --arg tag "$wrong_tag" \
  '.[] | select(.configuration.name == $tag) | .configuration.descriptor.digest' \
  "$RUN_DIR/images-before.json")"
if [ -z "$wrong_tag_digest" ] || [ "$wrong_tag_digest" = "$oracle_image_id" ]; then
  printf 'wrong-tag-swap mutant is absent or unexpectedly matches the oracle digest\n' >&2
  exit 1
fi
"$JQ" -n --arg candidate "$wrong_tag" --arg candidateDigest "$wrong_tag_digest" \
  --arg requiredDigest "$oracle_image_id" \
  '{status:"PASS",mutant:"wrong-tag-swap",candidate:$candidate,
    candidateDigest:$candidateDigest,requiredDigest:$requiredDigest,
    selectionOutcome:"REJECTED_BEFORE_EXECUTION"}' \
  >"$RUN_DIR/wrong-tag-swap-mutant.json"

digest_hex="${oracle_image_id#sha256:}"
digest_prefix="$(printf '%s' "$digest_hex" | cut -c 1-16)"
run_token="$(printf '%s' "$RUN_ID" | tr '[:upper:]' '[:lower:]')"
oracle_run_ref="docker.io/library/c30-day5-scalar-oracle-$digest_prefix:$run_token"
if "$JQ" -e --arg runRef "$oracle_run_ref" \
  'any(.[]; .configuration.name == $runRef)' "$RUN_DIR/images-before.json" >/dev/null; then
  printf 'run-unique oracle alias already exists: %s\n' "$oracle_run_ref" >&2
  exit 1
fi
"$CONTAINER_BIN" image tag "$oracle_tag" "$oracle_run_ref" \
  >"$RUN_DIR/alias-bind.stdout" 2>"$RUN_DIR/alias-bind.stderr"
alias_created=1
"$CONTAINER_BIN" image list --format json --verbose \
  >"$RUN_DIR/alias-before-run.json" 2>"$RUN_DIR/alias-before-run.stderr"
"$JQ" -e --arg runRef "$oracle_run_ref" --arg digest "$oracle_image_id" '
  any(.[]; .configuration.name == $runRef and
    .configuration.descriptor.digest == $digest and
    any(.variants[]; .platform.os == "linux" and .platform.architecture == "arm64"))
' "$RUN_DIR/alias-before-run.json" >/dev/null || {
  printf 'run-unique alias does not bind the frozen oracle digest\n' >&2
  exit 1
}
"$JQ" -n --arg discoveryTag "$oracle_tag" --arg runAlias "$oracle_run_ref" \
  --arg digest "$oracle_image_id" --arg policyVersion "$BINDING_POLICY_VERSION" \
  '{digestAddressableLocalExecution:false,discoveryTag:$discoveryTag,
    runAlias:$runAlias,frozenDigest:$digest,bindingPolicyVersion:$policyVersion,
    policy:"verify discovery descriptor; bind run-unique alias; verify alias descriptor before and after use; verify every completed container descriptor; delete alias"}' \
  >"$RUN_DIR/alias-binding-policy.json"

identity_name="c30-day5-scalar-identity-$RUN_ID"
cleanup_names="$cleanup_names $identity_name"
set +e
"$TIMEOUT_BIN" 60 "$CONTAINER_BIN" run --name "$identity_name" \
  --platform linux/arm64 --network none --read-only --cap-drop ALL \
  "$oracle_run_ref" /bin/sh -c "$IDENTITY_INIT_SCRIPT" \
  sh "$oracle_gcc" "$oracle_clang" >"$RUN_DIR/oracle-identity.stdout" \
  2>"$RUN_DIR/oracle-identity.stderr"
identity_rc=$?
set -e
printf '%s\n' "$identity_rc" >"$RUN_DIR/oracle-identity.rc"
[ "$identity_rc" -eq 0 ] || exit 1
verify_identity_container "$identity_name" identity || exit 1
actual_gcc_hash="$(awk -v path="$oracle_gcc" '$2 == path {print $1}' "$RUN_DIR/oracle-identity.stdout")"
actual_clang_hash="$(awk -v path="$oracle_clang" '$2 == path {print $1}' "$RUN_DIR/oracle-identity.stdout")"
actual_gcc_version="$(sed -n '3p' "$RUN_DIR/oracle-identity.stdout")"
actual_clang_version="$(sed -n '4p' "$RUN_DIR/oracle-identity.stdout")"
if [ "$actual_gcc_hash" != "$gcc_hash" ] || \
   [ "$actual_clang_hash" != "$clang_hash" ] || \
   [ "$actual_gcc_hash" = "$actual_clang_hash" ] || \
   [ "$actual_gcc_version" != "$gcc_version" ] || \
   [ "$actual_clang_version" != "$clang_version" ]; then
  printf 'pinned Day-5 scalar oracle identity differs\n' >&2
  exit 1
fi
"$CONTAINER_BIN" delete "$identity_name" >/dev/null
cleanup_names="${cleanup_names% $identity_name}"

run_lane() {
  lane="$1"
  tool="$2"
  name="c30-day5-scalar-$lane-$RUN_ID"
  cleanup_names="$cleanup_names $name"
  set +e
  "$TIMEOUT_BIN" 900 "$CONTAINER_BIN" run --name "$name" \
    --platform linux/arm64 --network none --read-only --cap-drop ALL \
    --mount "type=bind,source=$RUN_DIR/lane-input,target=/input,readonly" \
    --mount "type=bind,source=$RUN_DIR/lane-output-$lane,target=/output" \
    "$oracle_run_ref" /bin/sh -c "$LANE_INIT_SCRIPT" \
    sh "$tool" "$lane" >"$RUN_DIR/$lane.observations.tsv" \
    2>"$RUN_DIR/$lane.stderr"
  lane_rc=$?
  set -e
  printf '%s\n' "$lane_rc" >"$RUN_DIR/$lane.rc"
  verify_lane_container "$name" "$lane" "$lane" "$tool" || return 1
  "$CONTAINER_BIN" delete "$name" >/dev/null 2>&1 || return 1
  cleanup_names="${cleanup_names% $name}"
  [ "$lane_rc" -eq 0 ]
}

run_lane gcc "$oracle_gcc" || exit 1
run_lane clang "$oracle_clang" || exit 1

"$PYTHON_BIN" -I "$ORACLE_HELPER" verify \
  "$RUN_DIR/gcc.observations.tsv" "$RUN_DIR/clang.observations.tsv" \
  "$RUN_DIR/oracle-consensus.tsv" >"$RUN_DIR/oracle-summary.json" \
  2>"$RUN_DIR/oracle-summary.stderr"
sha256_file "$RUN_DIR/oracle-consensus.tsv" >"$RUN_DIR/oracle-consensus.sha256"

"$CONTAINER_BIN" image list --format json --verbose \
  >"$RUN_DIR/alias-after-run.json" 2>"$RUN_DIR/alias-after-run.stderr"
"$JQ" -e --arg runRef "$oracle_run_ref" --arg digest "$oracle_image_id" '
  any(.[]; .configuration.name == $runRef and
    .configuration.descriptor.digest == $digest)
' "$RUN_DIR/alias-after-run.json" >/dev/null || {
  printf 'run-unique alias descriptor changed during oracle execution\n' >&2
  exit 1
}
"$CONTAINER_BIN" image delete "$oracle_run_ref" \
  >"$RUN_DIR/alias-delete.stdout" 2>"$RUN_DIR/alias-delete.stderr"
alias_created=0

"$CONTAINER_BIN" image list --format json --verbose \
  >"$RUN_DIR/images-after.json" 2>"$RUN_DIR/images-after.stderr"
cmp -s "$RUN_DIR/images-before.json" "$RUN_DIR/images-after.json" || {
  printf 'container image inventory changed during Day-5 scalar oracle run\n' >&2
  exit 1
}

"$JQ" -n --slurpfile oracle "$RUN_DIR/oracle-summary.json" \
  --slurpfile materialization "$RUN_DIR/materialization.json" \
  --slurpfile laneInputAudit "$RUN_DIR/lane-input-audit.json" \
  --slurpfile crossCheck "$RUN_DIR/authority-cross-check.json" \
  --arg runId "$RUN_ID" --arg contractSha "$(sha256_file "$CONTRACT")" \
  --arg corpusSha "$(sha256_file "$CORPUS")" \
  --arg helperSha "$(sha256_file "$ORACLE_HELPER")" \
  --arg runnerSha "$(sha256_file "$0")" \
  --arg executionRef "$oracle_image_id" --arg discoveryTag "$oracle_tag" \
  --arg runAlias "$oracle_run_ref" --arg policyVersion "$BINDING_POLICY_VERSION" \
  '{schemaVersion:1,runId:$runId,status:$oracle[0].status,
    productionConformance:"NOT_MEASURED_BY_THIS_ORACLE_RUN",
    scope:"definedCasesOnly",expectedRejectNativeExecution:"PROHIBITED",
    expectedRejectExecuted:0,contractSha256:$contractSha,corpusSha256:$corpusSha,
    oracleHelperSha256:$helperSha,oracleRunnerSha256:$runnerSha,
    oracleExecution:{frozenDigest:$executionRef,discoveryTag:$discoveryTag,
      runAlias:$runAlias,digestAddressableLocalExecution:false,
      bindingPolicyVersion:$policyVersion,
      completedContainerDescriptorChecks:true,exactMountAndInitChecks:true,
      laneInputMount:"READ_ONLY",laneOutputMount:"PER_LANE_WRITABLE",
      mountedGoldenFields:0,wrongTagSwapMutant:"PASS"},
    materialization:$materialization[0],laneInputAudit:$laneInputAudit[0],
    authorityCrossCheck:$crossCheck[0],
    oracles:$oracle[0]}' >"$RUN_DIR/summary.json"
"$JQ" '{status,scope,productionConformance,
  expectedRejectExecuted,required:.oracles.required,
  gccAccepted:.oracles.gccAccepted,clangAccepted:.oracles.clangAccepted,
  consensus:.oracles.consensus}' "$RUN_DIR/summary.json"
