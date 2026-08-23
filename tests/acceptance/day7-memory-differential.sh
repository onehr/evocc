#!/bin/bash

# Frozen GCC-14/Clang-19 oracle lanes for the Day-7 defined-memory corpus.

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
CORPUS="$SCRIPT_DIR/day7-memory-corpus.py"
HELPER="$SCRIPT_DIR/day7-memory-oracles.py"
BUILD_ENV="$PROJECT_ROOT/contracts/build-environment.json"
PYTHON_BIN=/usr/bin/python3
EVIDENCE_ROOT="$PROJECT_ROOT/.evidence/memory-differential"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
BINDING_POLICY_VERSION=apple-container-run-unique-alias-v1

for path in "$CORPUS" "$HELPER" "$BUILD_ENV"; do
  [ -f "$path" ] && [ ! -L "$path" ] || {
    printf 'required Day-7 oracle input is missing or symbolic: %s\n' "$path" >&2
    exit 2
  }
done

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
SHASUM="$(build_value toolchain.hostReplay.shasum.path)"
TIMEOUT_BIN="$(build_value toolchain.hostReplay.timeout.path)"
ORACLE_IMAGE="$(build_value buildImage.sealedImageId)"
ORACLE_TAG="$(build_value buildImage.sealedImageTag)"
ORACLE_GCC="$(build_value toolchain.gcc.binaryPathInImage)"
ORACLE_CLANG="$(build_value toolchain.clang.binaryPathInImage)"
GCC_SHA256="$(build_value toolchain.gcc.binarySha256)"
CLANG_SHA256="$(build_value toolchain.clang.binarySha256)"
GCC_VERSION="$(build_value toolchain.gcc.resolvedVersion)"
CLANG_VERSION="$(build_value toolchain.clang.resolvedVersion)"

sha256_file() {
  "$SHASUM" -a 256 "$1" | awk '{print $1}'
}

[ "$(sha256_file "$CONTAINER_BIN")" = "$CONTAINER_SHA256" ] && \
[ "$($CONTAINER_BIN --version)" = "container CLI version $CONTAINER_VERSION (build: release, commit: $CONTAINER_COMMIT_SHORT)" ] || {
  printf 'frozen container CLI identity differs\n' >&2
  exit 2
}

mkdir -p "$EVIDENCE_ROOT"
mkdir "$RUN_DIR"
mkdir "$RUN_DIR/lane-output-gcc" "$RUN_DIR/lane-output-clang"
"$JQ" -n --arg runId "$RUN_ID" \
  '{schemaVersion:1,runId:$runId,status:"NOT_MEASURED",definedRequired:128,
    invalidNativeExecution:"PROHIBITED",reason:"both frozen oracle lanes have not completed"}' \
  >"$RUN_DIR/summary.json"
cp "$CORPUS" "$RUN_DIR/day7-memory-corpus.py"
cp "$HELPER" "$RUN_DIR/day7-memory-oracles.py"
sha256_file "$CORPUS" >"$RUN_DIR/corpus.sha256"
sha256_file "$HELPER" >"$RUN_DIR/helper.sha256"
sha256_file "$0" >"$RUN_DIR/runner.sha256"

"$PYTHON_BIN" -I "$CORPUS" audit >"$RUN_DIR/corpus-audit.json"
"$PYTHON_BIN" -I "$HELPER" self-test >"$RUN_DIR/comparator-selftest.json"
"$PYTHON_BIN" -I "$HELPER" materialize-defined "$RUN_DIR/lane-input" \
  >"$RUN_DIR/materialization.json"
"$PYTHON_BIN" -I "$HELPER" audit-lane-input "$RUN_DIR/lane-input" \
  >"$RUN_DIR/lane-input-audit.json"

IDENTITY_SCRIPT='set -eu
/usr/bin/sha256sum "$1" "$2"
"$1" --version | /usr/bin/sed -n "1p"
"$2" --version | /usr/bin/sed -n "1p"'

LANE_SCRIPT='set -u
tool="$1"
work=/output
export TMPDIR="$work"
executable="$work/case-program"
while IFS="$(printf "\t")" read -r ordinal filename case_id source_sha; do
  source="/input/$filename"
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
run_alias=''
cleanup() {
  for name in $cleanup_names; do
    "$CONTAINER_BIN" stop "$name" >/dev/null 2>&1 || true
    "$CONTAINER_BIN" delete "$name" >/dev/null 2>&1 || true
  done
  if [ "$alias_created" -eq 1 ] && [ -n "$run_alias" ]; then
    "$CONTAINER_BIN" image delete "$run_alias" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT HUP INT TERM

"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/images-before.json"
"$JQ" -e --arg tag "$ORACLE_TAG" --arg digest "$ORACLE_IMAGE" '
  any(.[]; .configuration.name == $tag and
    .configuration.descriptor.digest == $digest and
    any(.variants[]; .platform.os == "linux" and .platform.architecture == "arm64"))
' "$RUN_DIR/images-before.json" >/dev/null || {
  printf 'frozen Day-7 oracle image is unavailable\n' >&2
  exit 1
}

digest_prefix="$(printf '%s' "${ORACLE_IMAGE#sha256:}" | cut -c 1-16)"
run_token="$(printf '%s' "$RUN_ID" | tr '[:upper:]' '[:lower:]')"
run_alias="docker.io/library/c30-day7-memory-oracle-$digest_prefix:$run_token"
"$CONTAINER_BIN" image tag "$ORACLE_TAG" "$run_alias"
alias_created=1
"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/alias-before.json"
"$JQ" -e --arg name "$run_alias" --arg digest "$ORACLE_IMAGE" '
  any(.[]; .configuration.name == $name and .configuration.descriptor.digest == $digest)
' "$RUN_DIR/alias-before.json" >/dev/null

identity_name="c30-day7-memory-identity-$RUN_ID"
cleanup_names="$cleanup_names $identity_name"
set +e
"$TIMEOUT_BIN" 60 "$CONTAINER_BIN" run --name "$identity_name" \
  --platform linux/arm64 --network none --read-only --cap-drop ALL \
  "$run_alias" /bin/sh -c "$IDENTITY_SCRIPT" sh "$ORACLE_GCC" "$ORACLE_CLANG" \
  >"$RUN_DIR/oracle-identity.stdout" 2>"$RUN_DIR/oracle-identity.stderr"
identity_rc=$?
set -e
[ "$identity_rc" -eq 0 ] || exit 1
"$CONTAINER_BIN" list --all --format json >"$RUN_DIR/identity-container.json"
"$JQ" -e --arg name "$identity_name" --arg digest "$ORACLE_IMAGE" '
  any(.[]; .id == $name and .configuration.image.descriptor.digest == $digest and
    .configuration.readOnly == true and .configuration.networks == [] and
    .configuration.capDrop == ["ALL"] and .configuration.mounts == [])
' "$RUN_DIR/identity-container.json" >/dev/null
actual_gcc_sha="$(awk -v path="$ORACLE_GCC" '$2 == path {print $1}' "$RUN_DIR/oracle-identity.stdout")"
actual_clang_sha="$(awk -v path="$ORACLE_CLANG" '$2 == path {print $1}' "$RUN_DIR/oracle-identity.stdout")"
actual_gcc_version="$(sed -n '3p' "$RUN_DIR/oracle-identity.stdout")"
actual_clang_version="$(sed -n '4p' "$RUN_DIR/oracle-identity.stdout")"
[ "$actual_gcc_sha" = "$GCC_SHA256" ] && \
[ "$actual_clang_sha" = "$CLANG_SHA256" ] && \
[ "$actual_gcc_sha" != "$actual_clang_sha" ] && \
[ "$actual_gcc_version" = "$GCC_VERSION" ] && \
[ "$actual_clang_version" = "$CLANG_VERSION" ] || {
  printf 'frozen compiler identity differs\n' >&2
  exit 1
}
"$CONTAINER_BIN" delete "$identity_name" >/dev/null
cleanup_names="${cleanup_names% $identity_name}"

run_lane() {
  lane="$1"
  tool="$2"
  name="c30-day7-memory-$lane-$RUN_ID"
  cleanup_names="$cleanup_names $name"
  set +e
  "$TIMEOUT_BIN" 900 "$CONTAINER_BIN" run --name "$name" \
    --platform linux/arm64 --network none --read-only --cap-drop ALL \
    --mount "type=bind,source=$RUN_DIR/lane-input,target=/input,readonly" \
    --mount "type=bind,source=$RUN_DIR/lane-output-$lane,target=/output" \
    "$run_alias" /bin/sh -c "$LANE_SCRIPT" sh "$tool" \
    >"$RUN_DIR/$lane.observations.tsv" 2>"$RUN_DIR/$lane.stderr"
  lane_rc=$?
  set -e
  printf '%s\n' "$lane_rc" >"$RUN_DIR/$lane.rc"
  "$CONTAINER_BIN" list --all --format json >"$RUN_DIR/$lane-container.json"
  "$JQ" -e --arg name "$name" --arg digest "$ORACLE_IMAGE" '
    any(.[]; .id == $name and .configuration.image.descriptor.digest == $digest and
      .configuration.readOnly == true and .configuration.networks == [] and
      .configuration.capDrop == ["ALL"] and (.configuration.mounts | length) == 2)
  ' "$RUN_DIR/$lane-container.json" >/dev/null || return 1
  "$CONTAINER_BIN" delete "$name" >/dev/null
  cleanup_names="${cleanup_names% $name}"
  [ "$lane_rc" -eq 0 ]
}

run_lane gcc "$ORACLE_GCC" || exit 1
run_lane clang "$ORACLE_CLANG" || exit 1

"$PYTHON_BIN" -I "$HELPER" verify \
  "$RUN_DIR/gcc.observations.tsv" "$RUN_DIR/clang.observations.tsv" \
  "$RUN_DIR/oracle-consensus.tsv" >"$RUN_DIR/oracle-summary.json"
sha256_file "$RUN_DIR/oracle-consensus.tsv" >"$RUN_DIR/oracle-consensus.sha256"

"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/alias-after.json"
"$JQ" -e --arg name "$run_alias" --arg digest "$ORACLE_IMAGE" '
  any(.[]; .configuration.name == $name and .configuration.descriptor.digest == $digest)
' "$RUN_DIR/alias-after.json" >/dev/null
"$CONTAINER_BIN" image delete "$run_alias" >/dev/null
alias_created=0
"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/images-after.json"
cmp -s "$RUN_DIR/images-before.json" "$RUN_DIR/images-after.json" || {
  printf 'container image inventory changed during Day-7 oracle run\n' >&2
  exit 1
}

"$JQ" -n --slurpfile corpus "$RUN_DIR/corpus-audit.json" \
  --slurpfile materialization "$RUN_DIR/materialization.json" \
  --slurpfile inputAudit "$RUN_DIR/lane-input-audit.json" \
  --slurpfile oracle "$RUN_DIR/oracle-summary.json" \
  --arg runId "$RUN_ID" --arg image "$ORACLE_IMAGE" \
  --arg corpusSha "$(sha256_file "$CORPUS")" \
  --arg helperSha "$(sha256_file "$HELPER")" \
  --arg runnerSha "$(sha256_file "$0")" \
  --arg runAlias "$run_alias" --arg policy "$BINDING_POLICY_VERSION" \
  '{schemaVersion:1,runId:$runId,status:$oracle[0].status,
    corpusSha256:$corpusSha,helperSha256:$helperSha,runnerSha256:$runnerSha,
    corpus:$corpus[0],materialization:$materialization[0],laneInputAudit:$inputAudit[0],
    oracleExecution:{frozenImage:$image,runAlias:$runAlias,bindingPolicy:$policy,
      network:"none",rootFilesystem:"read-only",capabilitiesDropped:["ALL"],
      laneInput:"read-only",laneOutputs:"isolated",compilerIdentityChecked:true},
    oracles:$oracle[0]}' >"$RUN_DIR/summary.json"

"$JQ" '{status,defined:.corpus.defined,invalid:.corpus.invalid,
  uniqueSourceBytes:.corpus.uniqueSourceBytes,gccAccepted:.oracles.gccAccepted,
  clangAccepted:.oracles.clangAccepted,consensus:.oracles.consensus,
  oracleConflictCount:.oracles.oracleConflictCount,invalidExecuted:.oracles.invalidExecuted}' \
  "$RUN_DIR/summary.json"
printf 'evidence=%s\n' "$RUN_DIR"
