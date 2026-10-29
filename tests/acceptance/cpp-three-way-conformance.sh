#!/bin/bash

# Isolated three-way preprocessing observation. EvoCC runs in the pinned
# compiler-free base image; GCC and Clang run separately in the pinned oracle
# image. All three observations are compared with the independent hand-token
# contract.

set -u
set -o pipefail

export LC_ALL=C
umask 022

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
FIXTURES="$PROJECT_ROOT/tests/fixtures/cpp"
CONTRACT="$PROJECT_ROOT/tests/contracts/cpp-hand-goldens.json"
REQUIRED="$PROJECT_ROOT/tests/contracts/cpp-required-fixtures.txt"
BUILD_ENV="$PROJECT_ROOT/contracts/build-environment.json"
SERIALIZER="$SCRIPT_DIR/cpp-token-serializer.py"
CONTAINER_RUNNER="$SCRIPT_DIR/cpp-three-way-container-runner.sh"
EVIDENCE_ROOT="${C30_CPP_THREE_WAY_EVIDENCE_ROOT:-$PROJECT_ROOT/.evidence/cpp-three-way}"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
RESULTS="$RUN_DIR/results.jsonl"
SUMMARY="$RUN_DIR/summary.json"
JQ="${C30_JQ:-/opt/homebrew/Cellar/jq/1.8.2/bin/jq}"
CONTAINER_BIN="${C30_CONTAINER_BIN:-/usr/local/bin/container}"
TIMEOUT_BIN="${C30_TIMEOUT:-/opt/homebrew/Cellar/coreutils/9.11/bin/gtimeout}"
SHASUM=/usr/bin/shasum
PYTHON=/usr/bin/python3
SOURCE_DATE_EPOCH="${C30_SOURCE_DATE_EPOCH:-1786492800}"
PROD_HASH_FILES='crates/evocc-common/src/lib.rs crates/evocc-types/src/lib.rs crates/evocc-chir/src/interp/atomic_values.rs crates/evocc-chir/src/interp/bit_fields.rs crates/evocc-chir/src/interp/calls.rs crates/evocc-chir/src/interp/construction.rs crates/evocc-chir/src/interp/control_flow.rs crates/evocc-chir/src/interp/declarations.rs crates/evocc-chir/src/interp/discarded.rs crates/evocc-chir/src/interp/expressions.rs crates/evocc-chir/src/interp/functions.rs crates/evocc-chir/src/interp/identifier_values.rs crates/evocc-chir/src/interp/initializers.rs crates/evocc-chir/src/interp/machine.rs crates/evocc-chir/src/interp/memory_access.rs crates/evocc-chir/src/interp/objects.rs crates/evocc-chir/src/interp/places.rs crates/evocc-chir/src/interp/pointers.rs crates/evocc-chir/src/interp/requirements.rs crates/evocc-chir/src/interp/runtime_values.rs crates/evocc-chir/src/interp/scalar_evaluation.rs crates/evocc-chir/src/interp/scalar_updates.rs crates/evocc-chir/src/interp/statements.rs crates/evocc-chir/src/interp/static_storage.rs crates/evocc-chir/src/interp/symbols.rs crates/evocc-chir/src/interp/tests.rs crates/evocc-chir/src/interp/value_types.rs crates/evocc-chir/src/interp/varargs.rs crates/evocc-chir/src/interp/volatile_copies.rs crates/evocc-chir/src/lib.rs crates/evocc-lex/src/lib.rs crates/evocc-cpp/src/lib.rs crates/evocc-parser/src/assembly.rs crates/evocc-parser/src/assignments.rs crates/evocc-parser/src/atomic_builtins.rs crates/evocc-parser/src/attributes.rs crates/evocc-parser/src/builtin_calls.rs crates/evocc-parser/src/builtin_expressions.rs crates/evocc-parser/src/builtin_profiles.rs crates/evocc-parser/src/builtin_values.rs crates/evocc-parser/src/configuration.rs crates/evocc-parser/src/conversions.rs crates/evocc-parser/src/cursor.rs crates/evocc-parser/src/declaration_support.rs crates/evocc-parser/src/declarations.rs crates/evocc-parser/src/declarators.rs crates/evocc-parser/src/enums.rs crates/evocc-parser/src/expression_support.rs crates/evocc-parser/src/expression_types.rs crates/evocc-parser/src/functions.rs crates/evocc-parser/src/initializer_actions.rs crates/evocc-parser/src/initializer_paths.rs crates/evocc-parser/src/initializers.rs crates/evocc-parser/src/jump_labels.rs crates/evocc-parser/src/lexical_values.rs crates/evocc-parser/src/lib.rs crates/evocc-parser/src/operators.rs crates/evocc-parser/src/primary.rs crates/evocc-parser/src/records.rs crates/evocc-parser/src/specifiers.rs crates/evocc-parser/src/statements.rs crates/evocc-parser/src/symbols.rs crates/evocc-parser/src/tests.rs crates/evocc-egraph/src/lib.rs crates/evocc-proof/src/lib.rs crates/evocc-evoir/src/errors.rs crates/evocc-evoir/src/function_execution.rs crates/evocc-evoir/src/global_memory.rs crates/evocc-evoir/src/instruction_execution.rs crates/evocc-evoir/src/instruction_types.rs crates/evocc-evoir/src/instruction_values.rs crates/evocc-evoir/src/integer_kernel.rs crates/evocc-evoir/src/lib.rs crates/evocc-evoir/src/module_execution.rs crates/evocc-evoir/src/module_validation.rs crates/evocc-evoir/src/runtime_values.rs crates/evocc-evoir/src/tests.rs crates/evocc-evoir/src/tests/arithmetic.rs crates/evocc-evoir/src/tests/execution.rs crates/evocc-evoir/src/tests/uninitialized.rs crates/evocc-evoir/src/tests/validation.rs crates/evocc-evoir/src/validation.rs crates/evocc-evoir/src/value_aliases.rs crates/evocc-lower/src/assembly.rs crates/evocc-lower/src/assignments.rs crates/evocc-lower/src/binary.rs crates/evocc-lower/src/bit_fields.rs crates/evocc-lower/src/blocks.rs crates/evocc-lower/src/calls.rs crates/evocc-lower/src/checked_arithmetic.rs crates/evocc-lower/src/conditional_values.rs crates/evocc-lower/src/constant_values.rs crates/evocc-lower/src/effects.rs crates/evocc-lower/src/expressions.rs crates/evocc-lower/src/function_body.rs crates/evocc-lower/src/globals.rs crates/evocc-lower/src/initialization.rs crates/evocc-lower/src/lib.rs crates/evocc-lower/src/lifetimes.rs crates/evocc-lower/src/liveness.rs crates/evocc-lower/src/loops.rs crates/evocc-lower/src/memory_access.rs crates/evocc-lower/src/memory_updates.rs crates/evocc-lower/src/objects.rs crates/evocc-lower/src/predicates.rs crates/evocc-lower/src/references.rs crates/evocc-lower/src/signatures.rs crates/evocc-lower/src/statements.rs crates/evocc-lower/src/storage.rs crates/evocc-lower/src/switches.rs crates/evocc-lower/src/symbols.rs crates/evocc-lower/src/tests.rs crates/evocc-lower/src/unit.rs crates/evocc-lower/src/validation.rs crates/evocc-lower/src/value_ids.rs crates/evocc-lower/src/value_types.rs crates/evocc/src/main.rs Cargo.toml Cargo.lock'
SUBJECT_REF='docker.io/library/debian@sha256:c94f5ddd41327aa2d4a7cfba7889056c02936182fd76a513fec6160c97181fc0'
SUBJECT_MANIFEST='sha256:c94f5ddd41327aa2d4a7cfba7889056c02936182fd76a513fec6160c97181fc0'
ORACLE_REF='evo-nexlab:sealed'

mkdir -p "$RUN_DIR" "$RUN_DIR/evocc" "$RUN_DIR/gcc" "$RUN_DIR/clang" "$RUN_DIR/hand"
: >"$RESULTS"

for required_path in "$CONTRACT" "$REQUIRED" "$BUILD_ENV" "$SERIALIZER" \
  "$CONTAINER_RUNNER"; do
  [ -f "$required_path" ] && [ ! -L "$required_path" ] || {
    printf 'CPP three-way input is missing or symbolic: %s\n' "$required_path" >&2
    exit 2
  }
done
for required_tool in "$JQ" "$CONTAINER_BIN" "$TIMEOUT_BIN" "$SHASUM" "$PYTHON"; do
  [ -x "$required_tool" ] || {
    printf 'CPP three-way tool is unavailable: %s\n' "$required_tool" >&2
    exit 2
  }
done

cleanup_names=''
cleanup() {
  local name
  local cleanup_rc=0
  for name in $cleanup_names; do
    "$CONTAINER_BIN" stop "$name" >/dev/null 2>&1 || true
    "$CONTAINER_BIN" delete "$name" >/dev/null 2>&1 || cleanup_rc=1
  done
  return "$cleanup_rc"
}
trap cleanup EXIT HUP INT TERM

sha256_file() {
  "$SHASUM" -a 256 "$1" | awk '{print $1}'
}

actual_fixtures="$RUN_DIR/actual-fixtures.txt"
find "$FIXTURES" -maxdepth 1 -name '*.c' -type f -exec basename {} \; | sort >"$actual_fixtures"
cmp -s "$REQUIRED" "$actual_fixtures" || {
  printf 'CPP fixture namespace differs from the exact contract\n' >&2
  exit 1
}
bash "$SCRIPT_DIR/verify-cpp-hand-goldens.sh" "$CONTRACT" "$REQUIRED" \
  >"$RUN_DIR/hand-contract.stdout" 2>"$RUN_DIR/hand-contract.stderr" || exit 1
bash "$SCRIPT_DIR/cpp-token-serializer-selftest.sh" \
  >"$RUN_DIR/serializer-selftest.stdout" 2>"$RUN_DIR/serializer-selftest.stderr" || exit 1

ORACLE_IMAGE_ID="$($JQ -r '.buildImage.executionReference' "$BUILD_ENV")"
ORACLE_GCC="$($JQ -r '.toolchain.gcc.binaryPathInImage' "$BUILD_ENV")"
ORACLE_CLANG="$($JQ -r '.toolchain.clang.binaryPathInImage' "$BUILD_ENV")"
GCC_HASH="$($JQ -r '.toolchain.gcc.binarySha256' "$BUILD_ENV")"
CLANG_HASH="$($JQ -r '.toolchain.clang.binarySha256' "$BUILD_ENV")"

"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/images-before.json" || exit 1
"$JQ" -e --arg ref "$SUBJECT_REF" --arg leaf "$SUBJECT_MANIFEST" '
  any(.[]; .configuration.name == $ref and any(.variants[]; .digest == $leaf and
    .platform.os == "linux" and .platform.architecture == "arm64"))
' "$RUN_DIR/images-before.json" >/dev/null || {
  printf 'pinned compiler-free subject image is unavailable\n' >&2
  exit 1
}
"$JQ" -e --arg ref "$ORACLE_REF" --arg digest "$ORACLE_IMAGE_ID" '
  any(.[]; .configuration.name == $ref and .configuration.descriptor.digest == $digest and
    any(.variants[]; .platform.os == "linux" and .platform.architecture == "arm64"))
' "$RUN_DIR/images-before.json" >/dev/null || {
  printf 'pinned oracle image is unavailable\n' >&2
  exit 1
}

RUST_LLD="${C30_RUST_LLD:-$(command -v rust-lld)}"
[ -x "$RUST_LLD" ] || {
  printf 'rust-lld is unavailable\n' >&2
  exit 2
}
if [ -n "$(git -C "$PROJECT_ROOT" status --porcelain=v1 --untracked-files=all -- \
    $PROD_HASH_FILES)" ]; then
  printf 'production inputs differ from the source commit\n' >&2
  exit 1
fi
SUBJECT_TARGET_DIR="$RUN_DIR/cargo-target"
[ ! -e "$SUBJECT_TARGET_DIR" ] || {
  printf 'fresh subject target directory already exists\n' >&2
  exit 1
}
(
  cd "$PROJECT_ROOT" || exit 2
  CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="$RUST_LLD" \
  CARGO_TARGET_DIR="$SUBJECT_TARGET_DIR" \
    cargo build -p evocc --release --target aarch64-unknown-linux-musl \
      --locked --offline
) >"$RUN_DIR/subject-build.stdout" 2>"$RUN_DIR/subject-build.stderr" || exit 1
SUBJECT_BIN="$SUBJECT_TARGET_DIR/aarch64-unknown-linux-musl/release/evocc"
[ -x "$SUBJECT_BIN" ] || exit 1
SUBJECT_HASH="$(sha256_file "$SUBJECT_BIN")"

inspect_name="c30-cpp-inspect-$RUN_ID"
cleanup_names="$cleanup_names $inspect_name"
"$TIMEOUT_BIN" 60 "$CONTAINER_BIN" run --name "$inspect_name" \
  --platform linux/arm64 --network none --read-only --cap-drop ALL \
  --volume "$SUBJECT_BIN:/tool/evocc:ro" "$ORACLE_REF" \
  /usr/bin/readelf -h -l -d /tool/evocc \
  >"$RUN_DIR/subject-readelf.txt" 2>"$RUN_DIR/subject-readelf.stderr"
inspect_rc=$?
[ "$inspect_rc" -eq 0 ] || exit 1
grep -Eq 'Machine:[[:space:]]+AArch64' "$RUN_DIR/subject-readelf.txt" || exit 1
grep -Fq 'There is no dynamic section' "$RUN_DIR/subject-readelf.txt" || exit 1
if grep -Fq 'INTERP' "$RUN_DIR/subject-readelf.txt"; then
  printf 'subject ELF has a program interpreter\n' >&2
  exit 1
fi
"$CONTAINER_BIN" delete "$inspect_name" >/dev/null || exit 1
cleanup_names="${cleanup_names% $inspect_name}"

hash_name="c30-cpp-hashes-$RUN_ID"
cleanup_names="$cleanup_names $hash_name"
"$TIMEOUT_BIN" 60 "$CONTAINER_BIN" run --name "$hash_name" \
  --platform linux/arm64 --network none --read-only --cap-drop ALL \
  "$ORACLE_REF" /usr/bin/sha256sum "$ORACLE_GCC" "$ORACLE_CLANG" \
  >"$RUN_DIR/oracle-hashes.txt" 2>"$RUN_DIR/oracle-hashes.stderr"
hash_rc=$?
[ "$hash_rc" -eq 0 ] || exit 1
actual_gcc_hash="$(awk -v path="$ORACLE_GCC" '$2 == path { print $1 }' "$RUN_DIR/oracle-hashes.txt")"
actual_clang_hash="$(awk -v path="$ORACLE_CLANG" '$2 == path { print $1 }' "$RUN_DIR/oracle-hashes.txt")"
if [ "$actual_gcc_hash" != "$GCC_HASH" ] || [ "$actual_clang_hash" != "$CLANG_HASH" ] || \
   [ "$actual_gcc_hash" = "$actual_clang_hash" ]; then
  printf 'oracle binary identity differs from the contract\n' >&2
  exit 1
fi
"$CONTAINER_BIN" delete "$hash_name" >/dev/null || exit 1
cleanup_names="${cleanup_names% $hash_name}"

inventory_name="c30-cpp-inventory-$RUN_ID"
cleanup_names="$cleanup_names $inventory_name"
"$TIMEOUT_BIN" 60 "$CONTAINER_BIN" run --name "$inventory_name" \
  --platform linux/arm64 --network none --read-only --cap-drop ALL \
  "$SUBJECT_REF" sh -c '
    set -eu
    for tool in gcc gcc-14 g++ cc c++ cpp clang clang-19 clang++; do
      command -v "$tool" >/dev/null 2>&1 && exit 1
    done
    if find /usr /lib -type f \( -name "libclang*" -o -name "libLLVM*" \
      -o -name "libbfd*" -o -name "libopcodes*" \) -print -quit 2>/dev/null | grep .; then
      exit 1
    fi
    printf "PASS compiler_inventory_empty\n"
  ' >"$RUN_DIR/subject-inventory.txt" 2>"$RUN_DIR/subject-inventory.stderr"
inventory_rc=$?
[ "$inventory_rc" -eq 0 ] && grep -Fxq 'PASS compiler_inventory_empty' \
  "$RUN_DIR/subject-inventory.txt" || exit 1
"$CONTAINER_BIN" delete "$inventory_name" >/dev/null || exit 1
cleanup_names="${cleanup_names% $inventory_name}"

# No production crate may launch an oracle or another executable. Tests are
# deliberately excluded because they are process-level harnesses.
if grep -R -E 'std::process::Command|Command::new|execve|posix_spawn|dlopen' \
    "$PROJECT_ROOT/crates"/*/src >"$RUN_DIR/subject-exec-scan.txt" 2>&1; then
  printf 'production source contains an executable-delegation path\n' >&2
  exit 1
fi

run_lane() {
  local lane="$1"
  local image="$2"
  local tool="$3"
  local output="$RUN_DIR/$lane"
  local name="c30-cpp-$lane-$RUN_ID"
  cleanup_names="$cleanup_names $name"
  local binary_mount=''
  if [ "$lane" = evocc ]; then
    binary_mount="--volume $SUBJECT_BIN:/tool/evocc:ro"
    tool=/tool/evocc
  fi
  # shellcheck disable=SC2086
  "$TIMEOUT_BIN" 180 "$CONTAINER_BIN" run --name "$name" \
    --platform linux/arm64 --network none --read-only --cap-drop ALL \
    --volume "$CONTAINER_RUNNER:/runner.sh:ro" \
    --volume "$FIXTURES:/src:ro" --volume "$output:/out" \
    $binary_mount "$image" /runner.sh "$lane" "$tool" /src /out "$SOURCE_DATE_EPOCH" \
    >"$RUN_DIR/$lane-container.stdout" 2>"$RUN_DIR/$lane-container.stderr"
  local lane_rc=$?
  "$CONTAINER_BIN" list --all --format json >"$RUN_DIR/$lane-container.json" || return 1
  if [ "$lane" = evocc ]; then
    "$JQ" -e --arg name "$name" --arg ref "$SUBJECT_REF" '
      any(.[]; .id == $name and .configuration.image.reference == $ref and
        .configuration.platform.os == "linux" and
        .configuration.platform.architecture == "arm64" and
        .configuration.readOnly == true and .configuration.networks == [] and
        .configuration.capDrop == ["ALL"])
    ' "$RUN_DIR/$lane-container.json" >/dev/null || return 1
  else
    "$JQ" -e --arg name "$name" --arg ref "$ORACLE_REF" --arg digest "$ORACLE_IMAGE_ID" '
      any(.[]; .id == $name and .configuration.image.reference == $ref and
        .configuration.image.descriptor.digest == $digest and
        .configuration.platform.os == "linux" and
        .configuration.platform.architecture == "arm64" and
        .configuration.readOnly == true and .configuration.networks == [] and
        .configuration.capDrop == ["ALL"])
    ' "$RUN_DIR/$lane-container.json" >/dev/null || return 1
  fi
  [ "$lane_rc" -eq 0 ] || return 1
  "$CONTAINER_BIN" delete "$name" >/dev/null || return 1
  cleanup_names="${cleanup_names% $name}"
}

run_lane evocc "$SUBJECT_REF" /tool/evocc || exit 1
run_lane gcc "$ORACLE_REF" "$ORACLE_GCC" || exit 1
run_lane clang "$ORACLE_REF" "$ORACLE_CLANG" || exit 1

while IFS= read -r file; do
  [ -n "$file" ] || continue
  expected_rc="$($JQ -r --arg file "$file" '.fixtures[] | select(.file==$file) | .expectedRc' "$CONTRACT")"
  "$JQ" -c --arg file "$file" '.fixtures[] | select(.file==$file) | .tokens[] | {kind,spelling}' \
    "$CONTRACT" >"$RUN_DIR/hand/$file.tokens"
  status=PASS
  for lane in evocc gcc clang; do
    rc="$(sed -n '1p' "$RUN_DIR/$lane/$file.rc" 2>/dev/null)"
    [ "$rc" = "$expected_rc" ] || status=FAIL
    "$PYTHON" "$SERIALIZER" <"$RUN_DIR/$lane/$file.out" \
      >"$RUN_DIR/$lane/$file.tokens" 2>"$RUN_DIR/$lane/$file.serializer.stderr" || status=FAIL
    cmp -s "$RUN_DIR/hand/$file.tokens" "$RUN_DIR/$lane/$file.tokens" || status=FAIL
  done
  diagnostic_json=null
  if [ "$file" = '025-include-missing.c' ]; then
    grep -Eq ':1:1: error\[E0105\]: include-not-found: nonexistent\.h$' \
      "$RUN_DIR/evocc/$file.stderr" || status=FAIL
    grep -Fq 'nonexistent.h' "$RUN_DIR/gcc/$file.stderr" || status=FAIL
    grep -Fq 'nonexistent.h' "$RUN_DIR/clang/$file.stderr" || status=FAIL
    diagnostic_json='{"code":"E0105","line":1,"column":1,"category":"include-not-found","subject":"nonexistent.h"}'
  fi
  evocc_rc="$(sed -n '1p' "$RUN_DIR/evocc/$file.rc")"
  gcc_rc="$(sed -n '1p' "$RUN_DIR/gcc/$file.rc")"
  clang_rc="$(sed -n '1p' "$RUN_DIR/clang/$file.rc")"
  evocc_signal=0; gcc_signal=0; clang_signal=0; timed_out=false
  [ "$evocc_rc" -lt 128 ] || evocc_signal=$((evocc_rc - 128))
  [ "$gcc_rc" -lt 128 ] || gcc_signal=$((gcc_rc - 128))
  [ "$clang_rc" -lt 128 ] || clang_signal=$((clang_rc - 128))
  if [ "$evocc_rc" -eq 124 ] || [ "$gcc_rc" -eq 124 ] || [ "$clang_rc" -eq 124 ]; then
    timed_out=true; status=FAIL
  fi
  "$JQ" -cn --arg file "$file" --arg status "$status" \
    --argjson expectedRc "$expected_rc" --argjson evoccRc "$evocc_rc" \
    --argjson gccRc "$gcc_rc" --argjson clangRc "$clang_rc" \
    --argjson evoccSignal "$evocc_signal" --argjson gccSignal "$gcc_signal" \
    --argjson clangSignal "$clang_signal" --argjson timedOut "$timed_out" \
    --arg handTokenSha256 "$(sha256_file "$RUN_DIR/hand/$file.tokens")" \
    --arg evoccTokenSha256 "$(sha256_file "$RUN_DIR/evocc/$file.tokens")" \
    --arg gccTokenSha256 "$(sha256_file "$RUN_DIR/gcc/$file.tokens")" \
    --arg clangTokenSha256 "$(sha256_file "$RUN_DIR/clang/$file.tokens")" \
    --argjson diagnostic "$diagnostic_json" \
    '{file:$file,status:$status,expectedRc:$expectedRc,evoccRc:$evoccRc,gccRc:$gccRc,
      clangRc:$clangRc,evoccSignal:$evoccSignal,gccSignal:$gccSignal,
      clangSignal:$clangSignal,timedOut:$timedOut,handTokenSha256:$handTokenSha256,
      evoccTokenSha256:$evoccTokenSha256,gccTokenSha256:$gccTokenSha256,
      clangTokenSha256:$clangTokenSha256,diagnostic:$diagnostic}' >>"$RESULTS"
done <"$REQUIRED"

"$CONTAINER_BIN" image list --format json --verbose >"$RUN_DIR/images-after.json" || exit 1
cmp -s "$RUN_DIR/images-before.json" "$RUN_DIR/images-after.json" || {
  printf 'image inventory changed during CPP observation\n' >&2
  exit 1
}

SOURCE_COMMIT="$(git -C "$PROJECT_ROOT" rev-parse HEAD)"
SOURCE_TREE="$(git -C "$PROJECT_ROOT" rev-parse 'HEAD^{tree}')"
"$JQ" -s --arg runId "$RUN_ID" --arg sourceCommit "$SOURCE_COMMIT" \
  --arg sourceTree "$SOURCE_TREE" --arg subjectManifest "$SUBJECT_MANIFEST" \
  --arg subjectSha "$SUBJECT_HASH" --arg oracleImage "$ORACLE_IMAGE_ID" \
  --arg gccSha "$GCC_HASH" --arg clangSha "$CLANG_HASH" '
  . as $results |
  {schemaVersion:1,runId:$runId,sourceCommit:$sourceCommit,sourceTree:$sourceTree,
   subject:{imageManifest:$subjectManifest,hostTriple:"aarch64-unknown-linux-musl",
     compilerTarget:"aarch64-unknown-linux-gnu",staticElf:true,
     compilerInventoryEmpty:true,loadedObjectClosure:[],descendantExecs:[],
     binarySha256:$subjectSha},
   oracles:{imageId:$oracleImage,gccSha256:$gccSha,clangSha256:$clangSha},
   total:($results|length),
   counts:($results|group_by(.status)|map({key:.[0].status,value:length})|from_entries),
   overall:(if (($results|length)==34 and all($results[];.status=="PASS")) then "PASS" else "FAIL" end),
   results:$results}
' "$RESULTS" >"$SUMMARY"

"$JQ" '{overall,counts,total,runId}' "$SUMMARY"
[ "$("$JQ" -r '.overall' "$SUMMARY")" = PASS ]
