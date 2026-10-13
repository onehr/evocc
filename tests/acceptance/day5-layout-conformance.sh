#!/bin/bash

# Deterministic Day-5 AArch64 layout differential. Oracle-only mode can
# measure/freeze evidence, but only a freshly built production subject can
# produce a conformance PASS.

set -u
set -o pipefail

export LC_ALL=C
export TZ=UTC
umask 022

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
MANIFEST="${C30_LAYOUT_MANIFEST:-$PROJECT_ROOT/tests/contracts/day5-layout-cases.json}"
CORPUS="$SCRIPT_DIR/day5-layout-corpus.py"
SUBJECT_OBSERVER="$SCRIPT_DIR/day5-layout-subject.py"
BUILD_ENV="$PROJECT_ROOT/contracts/build-environment.json"
EVIDENCE_ROOT="${C30_DAY5_LAYOUT_EVIDENCE_ROOT:-$PROJECT_ROOT/.evidence/day5-layout}"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"

PYTHON="${C30_PYTHON:-/usr/bin/python3}"
JQ="${C30_JQ:-/opt/homebrew/Cellar/jq/1.8.2/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"
CONTAINER_BIN="${C30_CONTAINER_BIN:-/usr/local/bin/container}"
TIMEOUT_BIN="${C30_TIMEOUT:-/opt/homebrew/Cellar/coreutils/9.11/bin/gtimeout}"
ORACLE_TAG="evo-nexlab:sealed"

usage() {
  printf 'usage: %s --oracles-only | --measure | --subject\n' "$0" >&2
  exit 2
}

[ "$#" -ge 1 ] || usage
mode="$1"
case "$mode" in
  --oracles-only)
    [ "$#" -eq 1 ] || usage
    ;;
  --measure)
    # First measurement of an unmeasured manifest: run both oracles and write
    # their consensus without comparing against a frozen hash, because none
    # exists yet. The consensus is then frozen into the manifest by the corpus
    # tool, and every later run uses --oracles-only against that hash.
    [ "$#" -eq 1 ] || usage
    ;;
  --subject)
    [ "$#" -eq 1 ] || usage
    ;;
  *) usage ;;
esac

for path in "$MANIFEST" "$CORPUS" "$SUBJECT_OBSERVER" "$BUILD_ENV"; do
  [ -f "$path" ] && [ ! -L "$path" ] || {
    printf 'required Day-5 layout input is missing or symbolic: %s\n' "$path" >&2
    exit 2
  }
done
for tool in "$PYTHON" "$JQ" "$SHASUM" "$CONTAINER_BIN" "$TIMEOUT_BIN"; do
  [ -x "$tool" ] || {
    printf 'required Day-5 layout tool is unavailable: %s\n' "$tool" >&2
    exit 2
  }
done
mkdir -p "$RUN_DIR/tmp-gcc" "$RUN_DIR/tmp-clang"

cleanup_names=''
alias_created=0
oracle_run_ref=''
cleanup() {
  cleanup_rc=0
  # Measurements and the subject digest are retained, not another compiled
  # dependency tree for every run. This directory contains only Cargo output.
  if [ -d "$RUN_DIR/cargo-target" ] && [ ! -L "$RUN_DIR/cargo-target" ]; then
    /bin/rm -rf -- "$RUN_DIR/cargo-target" || cleanup_rc=1
  fi
  for name in $cleanup_names; do
    "$CONTAINER_BIN" stop "$name" >/dev/null 2>&1 || true
    "$CONTAINER_BIN" delete "$name" >/dev/null 2>&1 || cleanup_rc=1
  done
  if [ "$alias_created" -eq 1 ] && [ -n "$oracle_run_ref" ]; then
    "$CONTAINER_BIN" image delete "$oracle_run_ref" >/dev/null 2>&1 || cleanup_rc=1
  fi
  return "$cleanup_rc"
}
trap cleanup EXIT HUP INT TERM

sha256_file() {
  "$SHASUM" -a 256 "$1" | awk '{print $1}'
}

verify_container_identity() {
  observed_name="$1"
  evidence_name="$2"
  "$CONTAINER_BIN" list --all --format json >"$RUN_DIR/$evidence_name-container.json" \
    2>"$RUN_DIR/$evidence_name-container.stderr" || return 1
  "$JQ" -e --arg name "$observed_name" --arg digest "$oracle_image_id" \
    --arg runRef "$oracle_run_ref" '
    any(.[]; .id == $name and
      .configuration.image.reference == $runRef and
      .configuration.image.descriptor.digest == $digest and
      .configuration.platform.os == "linux" and
      .configuration.platform.architecture == "arm64" and
      .configuration.readOnly == true and
      .configuration.networks == [] and
      .configuration.capDrop == ["ALL"])
  ' "$RUN_DIR/$evidence_name-container.json" >/dev/null
}

cp "$MANIFEST" "$RUN_DIR/case-manifest.json"
sha256_file "$RUN_DIR/case-manifest.json" >"$RUN_DIR/case-manifest.sha256"

unmeasured_flag=''
[ "$mode" = --measure ] && unmeasured_flag='--allow-unmeasured'
PYTHONDONTWRITEBYTECODE=1 "$PYTHON" "$CORPUS" validate-manifest "$MANIFEST" $unmeasured_flag \
  >"$RUN_DIR/manifest-validation.json" 2>"$RUN_DIR/manifest-validation.stderr" || exit 1
PYTHONDONTWRITEBYTECODE=1 "$PYTHON" "$CORPUS" emit-c "$MANIFEST" $unmeasured_flag \
  >"$RUN_DIR/layout-observer.c" 2>"$RUN_DIR/layout-observer.stderr" || exit 1
sha256_file "$RUN_DIR/layout-observer.c" >"$RUN_DIR/layout-observer.sha256"
if [ "$mode" != --measure ]; then
  # The conflict self-test perturbs a frozen consensus; an unmeasured
  # manifest has none to perturb, so it runs only once one exists.
  PYTHONDONTWRITEBYTECODE=1 "$PYTHON" "$CORPUS" self-test "$MANIFEST" \
    >"$RUN_DIR/oracle-conflict-selftest.json" \
    2>"$RUN_DIR/oracle-conflict-selftest.stderr" || exit 1
fi

"$JQ" -e --slurpfile manifest "$MANIFEST" '
  .buildImage.executionReference == $manifest[0].oracles.imageId and
  .toolchain.gcc.binaryPathInImage == $manifest[0].oracles.gcc.path and
  .toolchain.gcc.binarySha256 == $manifest[0].oracles.gcc.sha256 and
  .toolchain.gcc.resolvedVersion == $manifest[0].oracles.gcc.version and
  .toolchain.clang.binaryPathInImage == $manifest[0].oracles.clang.path and
  .toolchain.clang.binarySha256 == $manifest[0].oracles.clang.sha256 and
  .toolchain.clang.resolvedVersion == $manifest[0].oracles.clang.version
' "$BUILD_ENV" >"$RUN_DIR/contract-cross-check.stdout" \
  2>"$RUN_DIR/contract-cross-check.stderr" || exit 1

oracle_image_id="$("$JQ" -r '.oracles.imageId' "$MANIFEST")"
oracle_gcc="$("$JQ" -r '.oracles.gcc.path' "$MANIFEST")"
oracle_clang="$("$JQ" -r '.oracles.clang.path' "$MANIFEST")"
gcc_hash="$("$JQ" -r '.oracles.gcc.sha256' "$MANIFEST")"
clang_hash="$("$JQ" -r '.oracles.clang.sha256' "$MANIFEST")"
gcc_version="$("$JQ" -r '.oracles.gcc.version' "$MANIFEST")"
clang_version="$("$JQ" -r '.oracles.clang.version' "$MANIFEST")"
expected_observer_sha="$("$JQ" -r '.oracleEvidence.observerSourceSha256' "$MANIFEST")"
expected_consensus_sha="$("$JQ" -r '.oracleEvidence.consensusSha256' "$MANIFEST")"
binding_policy_version="$("$JQ" -r '.oracleEvidence.bindingPolicyVersion' "$MANIFEST")"
if [ "$mode" != --measure ]; then
  [ "$(sha256_file "$RUN_DIR/layout-observer.c")" = "$expected_observer_sha" ] || {
    printf 'generated layout observer differs from frozen oracle evidence\n' >&2
    exit 1
  }
fi

"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/images-before.json" \
  2>"$RUN_DIR/images-before.stderr" || exit 1
"$JQ" -e --arg tag "$ORACLE_TAG" --arg digest "$oracle_image_id" '
  any(.[]; .configuration.name == $tag and
    .configuration.descriptor.digest == $digest and
    any(.variants[]; .platform.os == "linux" and .platform.architecture == "arm64"))
' "$RUN_DIR/images-before.json" >/dev/null || {
  printf 'pinned Day-5 oracle image is unavailable\n' >&2
  exit 1
}

wrong_tag='docker.io/library/debian:13-slim'
wrong_tag_digest="$("$JQ" -r --arg tag "$wrong_tag" \
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
oracle_run_ref="docker.io/library/c30-day5-layout-oracle-$digest_prefix:$run_token"
if "$JQ" -e --arg runRef "$oracle_run_ref" \
  'any(.[]; .configuration.name == $runRef)' "$RUN_DIR/images-before.json" >/dev/null; then
  printf 'run-unique oracle alias already exists: %s\n' "$oracle_run_ref" >&2
  exit 1
fi
"$CONTAINER_BIN" image tag "$ORACLE_TAG" "$oracle_run_ref" \
  >"$RUN_DIR/alias-bind.stdout" 2>"$RUN_DIR/alias-bind.stderr" || exit 1
alias_created=1
"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/alias-before-run.json" \
  2>"$RUN_DIR/alias-before-run.stderr" || exit 1
"$JQ" -e --arg runRef "$oracle_run_ref" --arg digest "$oracle_image_id" '
  any(.[]; .configuration.name == $runRef and
    .configuration.descriptor.digest == $digest and
    any(.variants[]; .platform.os == "linux" and .platform.architecture == "arm64"))
' "$RUN_DIR/alias-before-run.json" >/dev/null || {
  printf 'run-unique alias does not bind the frozen oracle digest\n' >&2
  exit 1
}
"$JQ" -n --arg discoveryTag "$ORACLE_TAG" --arg runAlias "$oracle_run_ref" \
  --arg digest "$oracle_image_id" --arg policyVersion "$binding_policy_version" \
  '{digestAddressableLocalExecution:false,discoveryTag:$discoveryTag,
    runAlias:$runAlias,frozenDigest:$digest,bindingPolicyVersion:$policyVersion,
    policy:"verify discovery descriptor; bind run-unique alias; verify alias descriptor before and after use; verify every completed container descriptor; delete alias"}' \
  >"$RUN_DIR/alias-binding-policy.json"

identity_name="c30-day5-layout-identity-$RUN_ID"
cleanup_names="$cleanup_names $identity_name"
"$TIMEOUT_BIN" 60 "$CONTAINER_BIN" run --name "$identity_name" \
  --platform linux/arm64 --network none --read-only --cap-drop ALL \
  "$oracle_run_ref" /bin/sh -c '
    set -eu
    /usr/bin/sha256sum "$1" "$2"
    "$1" --version | /usr/bin/sed -n "1p"
    "$2" --version | /usr/bin/sed -n "1p"
  ' sh "$oracle_gcc" "$oracle_clang" >"$RUN_DIR/oracle-identity.stdout" \
  2>"$RUN_DIR/oracle-identity.stderr"
identity_rc=$?
printf '%s\n' "$identity_rc" >"$RUN_DIR/oracle-identity.rc"
[ "$identity_rc" -eq 0 ] || exit 1
verify_container_identity "$identity_name" identity || exit 1
actual_gcc_hash="$(awk -v path="$oracle_gcc" '$2 == path {print $1}' "$RUN_DIR/oracle-identity.stdout")"
actual_clang_hash="$(awk -v path="$oracle_clang" '$2 == path {print $1}' "$RUN_DIR/oracle-identity.stdout")"
actual_gcc_version="$(sed -n '3p' "$RUN_DIR/oracle-identity.stdout")"
actual_clang_version="$(sed -n '4p' "$RUN_DIR/oracle-identity.stdout")"
if [ "$actual_gcc_hash" != "$gcc_hash" ] || [ "$actual_clang_hash" != "$clang_hash" ] || \
   [ "$actual_gcc_hash" = "$actual_clang_hash" ] || \
   [ "$actual_gcc_version" != "$gcc_version" ] || \
   [ "$actual_clang_version" != "$clang_version" ]; then
  printf 'pinned Day-5 oracle identity differs\n' >&2
  exit 1
fi
"$CONTAINER_BIN" delete "$identity_name" >/dev/null || exit 1
cleanup_names="${cleanup_names% $identity_name}"

run_lane() {
  lane="$1"
  tool="$2"
  name="c30-day5-layout-$lane-$RUN_ID"
  cleanup_names="$cleanup_names $name"
  "$TIMEOUT_BIN" 120 "$CONTAINER_BIN" run --name "$name" \
    --platform linux/arm64 --network none --read-only --cap-drop ALL \
    --volume "$RUN_DIR:/out" "$oracle_run_ref" /bin/sh -c '
      set -eu
      tool="$1"
      lane="$2"
      export TMPDIR="/out/tmp-$lane"
      "$tool" -std=gnu11 -O0 -g0 -pipe -fno-short-enums -fno-common \
        -Wall -Wextra -Werror /out/layout-observer.c -o "/out/$lane-observer"
      "/out/$lane-observer"
    ' sh "$tool" "$lane" >"$RUN_DIR/$lane.observations.tsv" \
    2>"$RUN_DIR/$lane.stderr"
  lane_rc=$?
  printf '%s\n' "$lane_rc" >"$RUN_DIR/$lane.rc"
  verify_container_identity "$name" "$lane" || return 1
  "$CONTAINER_BIN" delete "$name" >/dev/null 2>&1 || return 1
  cleanup_names="${cleanup_names% $name}"
  [ "$lane_rc" -eq 0 ]
}

run_lane gcc "$oracle_gcc" || exit 1
run_lane clang "$oracle_clang" || exit 1

PYTHONDONTWRITEBYTECODE=1 "$PYTHON" "$CORPUS" verify-observations $unmeasured_flag \
  "$MANIFEST" "$RUN_DIR/gcc.observations.tsv" \
  >"$RUN_DIR/gcc-validation.json" 2>"$RUN_DIR/gcc-validation.stderr" || exit 1
PYTHONDONTWRITEBYTECODE=1 "$PYTHON" "$CORPUS" verify-observations $unmeasured_flag \
  "$MANIFEST" "$RUN_DIR/clang.observations.tsv" \
  >"$RUN_DIR/clang-validation.json" 2>"$RUN_DIR/clang-validation.stderr" || exit 1

PYTHONDONTWRITEBYTECODE=1 "$PYTHON" "$CORPUS" compare-oracles $unmeasured_flag \
  "$MANIFEST" "$RUN_DIR/gcc.observations.tsv" "$RUN_DIR/clang.observations.tsv" \
  >"$RUN_DIR/oracle-summary.json" 2>"$RUN_DIR/oracle-summary.stderr"
oracle_rc=$?
[ "$oracle_rc" -eq 0 ] || {
  cat "$RUN_DIR/oracle-summary.json" >&2
  exit 1
}
cp "$RUN_DIR/gcc.observations.tsv" "$RUN_DIR/oracle-consensus.tsv"
sha256_file "$RUN_DIR/oracle-consensus.tsv" >"$RUN_DIR/oracle-consensus.sha256"
if [ "$mode" != --measure ]; then
  [ "$(sha256_file "$RUN_DIR/oracle-consensus.tsv")" = "$expected_consensus_sha" ] || {
    printf 'live oracle consensus differs from frozen oracle evidence hash\n' >&2
    exit 1
  }
fi

"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/alias-after-run.json" \
  2>"$RUN_DIR/alias-after-run.stderr" || exit 1
"$JQ" -e --arg runRef "$oracle_run_ref" --arg digest "$oracle_image_id" '
  any(.[]; .configuration.name == $runRef and
    .configuration.descriptor.digest == $digest)
' "$RUN_DIR/alias-after-run.json" >/dev/null || {
  printf 'run-unique alias descriptor changed during oracle execution\n' >&2
  exit 1
}
"$CONTAINER_BIN" image delete "$oracle_run_ref" >"$RUN_DIR/alias-delete.stdout" \
  2>"$RUN_DIR/alias-delete.stderr" || exit 1
alias_created=0

"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/images-after.json" \
  2>"$RUN_DIR/images-after.stderr" || exit 1
cmp -s "$RUN_DIR/images-before.json" "$RUN_DIR/images-after.json" || {
  printf 'container image inventory changed during Day-5 layout observation\n' >&2
  exit 1
}

if [ "$mode" = --subject ]; then
  cargo="$("$JQ" -r '.toolchain.hostReplay.cargo.path' "$BUILD_ENV")"
  cargo_sha="$("$JQ" -r '.toolchain.hostReplay.cargo.sha256' "$BUILD_ENV")"
  rustc="$("$JQ" -r '.toolchain.hostReplay.rustc.path' "$BUILD_ENV")"
  rustc_sha="$("$JQ" -r '.toolchain.hostReplay.rustc.sha256' "$BUILD_ENV")"
  for tool in "$cargo" "$rustc"; do
    [ -x "$tool" ] && [ ! -L "$tool" ] || {
      printf 'pinned Day-5 Rust tool is unavailable or symbolic: %s\n' "$tool" >&2
      exit 2
    }
  done
  [ "$(sha256_file "$cargo")" = "$cargo_sha" ] || {
    printf 'pinned Day-5 Cargo identity differs\n' >&2
    exit 1
  }
  [ "$(sha256_file "$rustc")" = "$rustc_sha" ] || {
    printf 'pinned Day-5 rustc identity differs\n' >&2
    exit 1
  }

  mkdir -p "$RUN_DIR/cargo-home" "$RUN_DIR/cargo-target"
  /usr/bin/env -i \
    PATH=/usr/bin:/bin LC_ALL=C TZ=UTC \
    CARGO_HOME="$RUN_DIR/cargo-home" \
    CARGO_TARGET_DIR="$RUN_DIR/cargo-target" \
    RUSTC="$rustc" \
    "$cargo" build --manifest-path "$PROJECT_ROOT/Cargo.toml" \
      -p evocc-parser --example day5_layout_probe --locked --offline \
      >"$RUN_DIR/subject-build.stdout" 2>"$RUN_DIR/subject-build.stderr" || exit 1
  subject_probe="$RUN_DIR/cargo-target/debug/examples/day5_layout_probe"
  [ -x "$subject_probe" ] && [ ! -L "$subject_probe" ] || {
    printf 'fresh Day-5 subject probe was not produced\n' >&2
    exit 1
  }
  sha256_file "$subject_probe" >"$RUN_DIR/subject-probe.sha256"
  cp "$SUBJECT_OBSERVER" "$RUN_DIR/day5-layout-subject.py"
  sha256_file "$RUN_DIR/day5-layout-subject.py" >"$RUN_DIR/day5-layout-subject.sha256"
  PYTHONDONTWRITEBYTECODE=1 "$PYTHON" "$SUBJECT_OBSERVER" \
    "$MANIFEST" "$subject_probe" >"$RUN_DIR/subject.observations.tsv" \
    2>"$RUN_DIR/subject.observations.stderr" || exit 1
  sha256_file "$RUN_DIR/subject.observations.tsv" >"$RUN_DIR/subject.observations.sha256"
  PYTHONDONTWRITEBYTECODE=1 "$PYTHON" "$CORPUS" compare-subject \
    "$MANIFEST" "$RUN_DIR/subject.observations.tsv" >"$RUN_DIR/subject-summary.json" \
    2>"$RUN_DIR/subject-summary.stderr"
  subject_rc=$?
  "$JQ" -n --slurpfile oracle "$RUN_DIR/oracle-summary.json" \
    --slurpfile subjectSummary "$RUN_DIR/subject-summary.json" \
    --arg runId "$RUN_ID" --arg manifestSha "$(sha256_file "$MANIFEST")" \
    --arg sourceSha "$(sha256_file "$RUN_DIR/layout-observer.c")" \
    --arg consensusSha "$(sha256_file "$RUN_DIR/oracle-consensus.tsv")" \
    --arg subjectProbeSha "$(sha256_file "$subject_probe")" \
    --arg subjectObserverSha "$(sha256_file "$SUBJECT_OBSERVER")" \
    --arg cargoPath "$cargo" --arg cargoSha "$cargo_sha" \
    --arg rustcPath "$rustc" --arg rustcSha "$rustc_sha" \
    --arg executionRef "$oracle_image_id" --arg discoveryTag "$ORACLE_TAG" \
    --arg runAlias "$oracle_run_ref" --arg policyVersion "$binding_policy_version" \
    '{schemaVersion:1,runId:$runId,mode:"subject",status:$subjectSummary[0].status,
      manifestSha256:$manifestSha,observerSourceSha256:$sourceSha,
      oracleConsensusSha256:$consensusSha,
      subjectExecution:{freshCargoTarget:true,probeSha256:$subjectProbeSha,
        observerSha256:$subjectObserverSha,
        cargo:{path:$cargoPath,sha256:$cargoSha},rustc:{path:$rustcPath,sha256:$rustcSha}},
      oracleExecution:{frozenDigest:$executionRef,discoveryTag:$discoveryTag,
        runAlias:$runAlias,digestAddressableLocalExecution:false,
        bindingPolicyVersion:$policyVersion,
        aliasBindingPolicy:"pre/post descriptor checks plus every completed container descriptor",
        completedContainerDescriptorChecks:true,wrongTagSwapMutant:"PASS"},
      oracles:$oracle[0],subject:$subjectSummary[0]}' \
    >"$RUN_DIR/summary.json"
  "$JQ" '{status,mode,oracles:.oracles.status,total:.subject.total,mismatchCount:.subject.mismatchCount}' \
    "$RUN_DIR/summary.json"
  exit "$subject_rc"
fi

oracle_status="$("$JQ" -r '.status' "$RUN_DIR/oracle-summary.json")"
"$JQ" -n --slurpfile oracle "$RUN_DIR/oracle-summary.json" \
  --arg runId "$RUN_ID" --arg mode "${mode#--}" \
  --arg manifestSha "$(sha256_file "$MANIFEST")" \
  --arg sourceSha "$(sha256_file "$RUN_DIR/layout-observer.c")" \
  --arg consensusSha "$(sha256_file "$RUN_DIR/oracle-consensus.tsv")" \
  --arg executionRef "$oracle_image_id" --arg discoveryTag "$ORACLE_TAG" \
  --arg runAlias "$oracle_run_ref" --arg policyVersion "$binding_policy_version" \
  '{schemaVersion:1,runId:$runId,mode:$mode,status:$oracle[0].status,
    productionConformance:"NOT_MEASURED",manifestSha256:$manifestSha,
    observerSourceSha256:$sourceSha,oracleConsensusSha256:$consensusSha,
    oracleExecution:{frozenDigest:$executionRef,discoveryTag:$discoveryTag,
      runAlias:$runAlias,digestAddressableLocalExecution:false,
      bindingPolicyVersion:$policyVersion,
      aliasBindingPolicy:"pre/post descriptor checks plus every completed container descriptor",
      completedContainerDescriptorChecks:true,wrongTagSwapMutant:"PASS"},
    oracles:$oracle[0]}' >"$RUN_DIR/summary.json"
"$JQ" '{status,mode,productionConformance,total:.oracles.total}' "$RUN_DIR/summary.json"
[ "$oracle_status" = ORACLE_CONSENSUS ]
