#!/bin/bash

set -u
set -o pipefail

export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
FIXTURES_DIR="$PROJECT_ROOT/tests/fixtures/cpp"
EVIDENCE_ROOT="${C30_CONTAINER_EVIDENCE_ROOT:-$PROJECT_ROOT/.evidence/cpp-differential}"
RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
EVOCC_RAW="$RUN_DIR/evocc"
GCC_RAW="$RUN_DIR/gcc"
RESULTS="$RUN_DIR/results.jsonl"
SUMMARY="$RUN_DIR/summary.json"
SERIALIZER="$SCRIPT_DIR/cpp-token-serializer.py"
CPP_REQUIRED="$PROJECT_ROOT/tests/contracts/cpp-required-fixtures.txt"
BUILD_ENV_CONTRACT="$PROJECT_ROOT/contracts/build-environment.json"

JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
EVOCC="${C30_EVOCC:-$PROJECT_ROOT/target/release/evocc}"
CONTAINER_BIN="${C30_CONTAINER_BIN:-/usr/local/bin/container}"
IMAGE="${C30_CPP_IMAGE:-}"
EXPECTED_IMAGE_ID=''
GCC_BIN="${C30_GCC_BIN:-gcc-14}"
SOURCE_DATE_EPOCH="${C30_SOURCE_DATE_EPOCH:-1786492800}"
TIMEOUT_BIN="${C30_TIMEOUT:-/opt/homebrew/bin/timeout}"

CONTAINER_AVAILABLE=0
EVOCC_AVAILABLE=0

mkdir -p "$RUN_DIR" "$EVOCC_RAW" "$GCC_RAW"
: >"$RESULTS"

if [ -z "${C30_EVOCC:-}" ]; then
  if ! (cd "$PROJECT_ROOT" && cargo build -p evocc --release --locked --offline) \
    >"$RUN_DIR/evocc-build.stdout" 2>"$RUN_DIR/evocc-build.stderr"; then
    printf 'failed to build EvoCC from the current source tree\n' >&2
    exit 2
  fi
fi

if [ ! -x "$JQ" ]; then
  printf 'required jq is unavailable: %s\n' "$JQ" >&2
  exit 2
fi
if [ ! -x "$TIMEOUT_BIN" ]; then
  printf 'required timeout is unavailable: %s\n' "$TIMEOUT_BIN" >&2
  exit 2
fi

if [ -z "$IMAGE" ]; then
  IMAGE="$("$JQ" -r '.buildImage.sealedImageTag // empty' "$BUILD_ENV_CONTRACT" 2>/dev/null)"
fi
EXPECTED_IMAGE_ID="$("$JQ" -r '.buildImage.executionReference // empty' "$BUILD_ENV_CONTRACT" 2>/dev/null)"
if ! printf '%s\n' "$EXPECTED_IMAGE_ID" | grep -Eq '^sha256:[0-9a-f]{64}$'; then
  printf 'CPP expected image identity is not an immutable digest: %s\n' "$EXPECTED_IMAGE_ID" >&2
  exit 2
fi

if [ ! -r "$CPP_REQUIRED" ]; then
  printf 'CPP fixture contract is unavailable: %s\n' "$CPP_REQUIRED" >&2
  exit 2
fi
ACTUAL_FIXTURES="$RUN_DIR/actual-fixtures.txt"
find "$FIXTURES_DIR" -maxdepth 1 -name '*.c' -type f -exec basename {} \; | sort >"$ACTUAL_FIXTURES"
if ! cmp -s "$CPP_REQUIRED" "$ACTUAL_FIXTURES"; then
  printf 'CPP fixture namespace differs from the frozen exact ID set\n' >&2
  exit 1
fi

if [ ! -f "$SERIALIZER" ]; then
  printf 'token serializer is unavailable: %s\n' "$SERIALIZER" >&2
  exit 2
fi

if ! bash "$SCRIPT_DIR/cpp-token-serializer-selftest.sh" >"$RUN_DIR/serializer-selftest.stdout" \
  2>"$RUN_DIR/serializer-selftest.stderr"; then
  printf 'token serializer self-test failed\n' >&2
  exit 2
fi

if [ ! -x "$EVOCC" ]; then
  printf 'evocc binary not found at %s\n' "$EVOCC" >&2
  EVOCC_AVAILABLE=0
else
  EVOCC_AVAILABLE=1
fi

if [ ! -x "$CONTAINER_BIN" ]; then
  printf 'container runtime not found at %s\n' "$CONTAINER_BIN" >&2
  CONTAINER_AVAILABLE=0
elif ! "$CONTAINER_BIN" image inspect "$IMAGE" \
    >"$RUN_DIR/image-inspect-before.stdout" 2>"$RUN_DIR/image-inspect-before.stderr" \
  || ! "$CONTAINER_BIN" image list --format json --verbose \
    >"$RUN_DIR/image-list-before.json" 2>"$RUN_DIR/image-list-before.stderr" \
  || ! "$JQ" -e --arg locator "$IMAGE" --arg id "$EXPECTED_IMAGE_ID" '
      any(.[];
        .configuration.descriptor.digest == $id and
        .configuration.name == $locator
      )
    ' "$RUN_DIR/image-list-before.json" >/dev/null 2>&1; then
  printf 'image %s not available in container runtime\n' "$IMAGE" >&2
  CONTAINER_AVAILABLE=0
else
  CONTAINER_AVAILABLE=1
fi

record_result() {
  local file="$1"
  local status="$2"
  local message="$3"
  local evocc_rc="${4:-null}"
  local gcc_rc="${5:-null}"
  local evocc_signal="${6:-null}"
  local gcc_signal="${7:-null}"
  local timed_out="${8:-false}"
  local evocc_hash="${9:-}"
  local gcc_hash="${10:-}"
  local diagnostic_code="${11:-}"

  "$JQ" -cn \
    --arg file "$file" \
    --arg status "$status" \
    --arg message "$message" \
    --argjson evoccRc "$evocc_rc" \
    --argjson gccRc "$gcc_rc" \
    --argjson evoccSignal "$evocc_signal" \
    --argjson gccSignal "$gcc_signal" \
    --argjson timedOut "$timed_out" \
    --arg evoccStdoutSha256 "$evocc_hash" \
    --arg gccStdoutSha256 "$gcc_hash" \
    --arg diagnosticCode "$diagnostic_code" \
    '{file:$file,status:$status,message:$message,
      evoccRc:$evoccRc,gccRc:$gccRc,
      evoccSignal:$evoccSignal,gccSignal:$gccSignal,timedOut:$timedOut,
      evoccStdoutSha256:$evoccStdoutSha256,gccStdoutSha256:$gccStdoutSha256,
      diagnosticCode:$diagnosticCode}' \
    >>"$RESULTS"
}

sha256_file() {
  /usr/bin/shasum -a 256 "$1" | awk '{print $1}'
}

signal_from_rc() {
  local rc="$1"
  if [ "$rc" -ge 128 ] && [ "$rc" -le 255 ]; then
    printf '%d\n' "$((rc - 128))"
  else
    printf '0\n'
  fi
}

run_all_evocc() {
  if [ "$EVOCC_AVAILABLE" -ne 1 ]; then
    return 1
  fi
  find "$FIXTURES_DIR" -maxdepth 1 -name '*.c' -type f -print0 | sort -z | while IFS= read -r -d '' src_file; do
    base="$(basename "$src_file")"
    ( cd "$FIXTURES_DIR" && SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH" "$TIMEOUT_BIN" 10 "$EVOCC" \
        -std=gnu11 -E -P -undef -nostdinc -I. -Iinclude "$base" ) \
        >"$EVOCC_RAW/${base}.out" \
        2>"$EVOCC_RAW/${base}.stderr"
    printf '%d\n' "$?" >"$EVOCC_RAW/${base}.rc"
  done
}

run_all_gcc() {
  if [ "$CONTAINER_AVAILABLE" -ne 1 ]; then
    return 1
  fi

  local gcc_runner="$RUN_DIR/gcc-runner.sh"
  cat >"$gcc_runner" <<'RUNNER_EOF'
#!/bin/sh
set -u
gcc_bin="$1"
src_dir="$2"
out_dir="$3"
source_date_epoch="$4"
cd "$src_dir" || exit 2
for f in ./*.c; do
  base="$(basename "$f")"
  SOURCE_DATE_EPOCH="$source_date_epoch" timeout 10 "$gcc_bin" \
    -std=gnu11 -E -P -undef -nostdinc -I. -Iinclude "$base" \
    >"$out_dir/${base}.out" 2>"$out_dir/${base}.stderr"
  rc=$?
  echo "$rc" >"$out_dir/${base}.rc"
done
RUNNER_EOF

  chmod +x "$gcc_runner"

  "$TIMEOUT_BIN" 180 "$CONTAINER_BIN" run --rm \
    --volume "$FIXTURES_DIR:/src:ro" \
    --volume "$gcc_runner:/runner.sh:ro" \
    --volume "$GCC_RAW:/out" \
    "$IMAGE" \
    /runner.sh "$GCC_BIN" /src /out "$SOURCE_DATE_EPOCH"

  local gcc_rc=$?
  if [ "$gcc_rc" -ne 0 ]; then
    printf 'gcc batch run failed with exit code %d\n' "$gcc_rc" >&2
    return 1
  fi
  if ! "$CONTAINER_BIN" image list --format json --verbose \
      >"$RUN_DIR/image-list-after.json" 2>"$RUN_DIR/image-list-after.stderr" \
    || ! "$JQ" -e --arg locator "$IMAGE" --arg id "$EXPECTED_IMAGE_ID" '
        any(.[];
          .configuration.descriptor.digest == $id and
          .configuration.name == $locator
        )
      ' "$RUN_DIR/image-list-after.json" >/dev/null 2>&1; then
    printf 'CPP image locator no longer resolves to the frozen identity\n' >&2
    return 1
  fi
}

printf 'Running evocc on all fixtures...\n'
run_all_evocc

printf 'Running gcc-14 on all fixtures (one container invocation)...\n'
run_all_gcc

printf 'Comparing results...\n'

find "$FIXTURES_DIR" -maxdepth 1 -name '*.c' -type f | sort | while IFS= read -r src_file; do
  base="$(basename "$src_file")"
  evocc_out="$EVOCC_RAW/${base}.out"
  gcc_out="$GCC_RAW/${base}.out"

  evocc_ok=1
  gcc_ok=1

  [ -f "$EVOCC_RAW/${base}.rc" ] || evocc_ok=0
  [ -f "$GCC_RAW/${base}.rc" ] || gcc_ok=0

  if [ "$evocc_ok" -ne 1 ] && [ "$gcc_ok" -ne 1 ]; then
    record_result "$base" "FAIL" "both evocc and gcc-14 unavailable"
    continue
  fi
  if [ "$evocc_ok" -ne 1 ]; then
    record_result "$base" "FAIL" "evocc unavailable"
    continue
  fi
  if [ "$gcc_ok" -ne 1 ]; then
    record_result "$base" "FAIL" "gcc-14 container unavailable"
    continue
  fi

  evocc_rc="$(cat "$EVOCC_RAW/${base}.rc" 2>/dev/null || echo 0)"
  gcc_rc="$(cat "$GCC_RAW/${base}.rc" 2>/dev/null || echo 0)"
  evocc_signal="$(signal_from_rc "$evocc_rc")"
  gcc_signal="$(signal_from_rc "$gcc_rc")"
  timed_out=false
  if [ "$evocc_rc" -eq 124 ] || [ "$gcc_rc" -eq 124 ]; then
    timed_out=true
  fi
  evocc_raw_hash="$(sha256_file "$evocc_out")"
  gcc_raw_hash="$(sha256_file "$gcc_out")"

  if [ "$evocc_rc" -ne "$gcc_rc" ]; then
    record_result "$base" "FAIL" "oracle conflict: exit codes differ: evocc=$evocc_rc gcc=$gcc_rc" \
      "$evocc_rc" "$gcc_rc" "$evocc_signal" "$gcc_signal" "$timed_out" \
      "$evocc_raw_hash" "$gcc_raw_hash"
    continue
  fi

  if [ "$evocc_rc" -ne 0 ]; then
    if [ "$base" = "025-include-missing.c" ] \
      && grep -F 'nonexistent.h' "$EVOCC_RAW/${base}.stderr" >/dev/null 2>&1 \
      && grep -F 'nonexistent.h' "$GCC_RAW/${base}.stderr" >/dev/null 2>&1; then
      record_result "$base" "PASS" "both implementations reject the missing include" \
        "$evocc_rc" "$gcc_rc" "$evocc_signal" "$gcc_signal" "$timed_out" \
        "$evocc_raw_hash" "$gcc_raw_hash" "E0105"
    else
      record_result "$base" "FAIL" "oracle conflict: unexpected controlled rejection: evocc=$evocc_rc gcc=$gcc_rc" \
        "$evocc_rc" "$gcc_rc" "$evocc_signal" "$gcc_signal" "$timed_out" \
        "$evocc_raw_hash" "$gcc_raw_hash"
    fi
    continue
  fi

  evocc_tokens="$EVOCC_RAW/${base}.tokens"
  gcc_tokens="$GCC_RAW/${base}.tokens"
  if ! /usr/bin/python3 "$SERIALIZER" <"$evocc_out" >"$evocc_tokens" \
    || ! /usr/bin/python3 "$SERIALIZER" <"$gcc_out" >"$gcc_tokens"; then
    record_result "$base" "FAIL" "oracle conflict: token serialization failed"
    continue
  fi

  if cmp -s "$evocc_tokens" "$gcc_tokens"; then
    evocc_token_hash="$(sha256_file "$evocc_tokens")"
    gcc_token_hash="$(sha256_file "$gcc_tokens")"
    record_result "$base" "PASS" "preprocessing token streams are identical" \
      "$evocc_rc" "$gcc_rc" "$evocc_signal" "$gcc_signal" "$timed_out" \
      "$evocc_token_hash" "$gcc_token_hash"
    continue
  fi

  record_result "$base" "FAIL" "oracle conflict: outputs differ after normalization"
done

"$JQ" -s \
  --arg runId "$RUN_ID" \
  --arg runDir "$RUN_DIR" \
  '
    . as $results |
    {
      runId: $runId,
      evidenceDir: $runDir,
      results: $results,
      counts: ($results | group_by(.status) | map({key: .[0].status, value: length}) | from_entries),
      total: ($results | length),
      overall: (
        if (($results | length) == 34 and all($results[]; .status == "PASS"))
        then "PASS"
        else "FAIL"
        end
      )
    }
  ' "$RESULTS" >"$SUMMARY"

printf '\nevidence: %s\n' "$RUN_DIR"
"$JQ" '{overall,counts,total}' "$SUMMARY"

if [ "$("$JQ" -r '.overall' "$SUMMARY")" = "FAIL" ]; then
  exit 1
fi
exit 0
