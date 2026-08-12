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

JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
EVOCC="${C30_EVOCC:-$PROJECT_ROOT/target/release/evocc}"
CONTAINER_BIN="${C30_CONTAINER_BIN:-/usr/local/bin/container}"
IMAGE="${C30_CPP_IMAGE:-evo-flight-lab:sealed}"
GCC_BIN="${C30_GCC_BIN:-gcc-14}"
SOURCE_DATE_EPOCH="${C30_SOURCE_DATE_EPOCH:-1786492800}"

CONTAINER_AVAILABLE=0
EVOCC_AVAILABLE=0

mkdir -p "$RUN_DIR" "$EVOCC_RAW" "$GCC_RAW"

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
elif ! "$CONTAINER_BIN" image inspect "$IMAGE" >/dev/null 2>&1; then
  printf 'image %s not available in container runtime\n' "$IMAGE" >&2
  CONTAINER_AVAILABLE=0
else
  CONTAINER_AVAILABLE=1
fi

record_result() {
  local file="$1"
  local status="$2"
  local message="$3"

  "$JQ" -cn \
    --arg file "$file" \
    --arg status "$status" \
    --arg message "$message" \
    '{file:$file,status:$status,message:$message}' \
    >>"$RESULTS"
}

run_all_evocc() {
  if [ "$EVOCC_AVAILABLE" -ne 1 ]; then
    return 1
  fi
  find "$FIXTURES_DIR" -maxdepth 1 -name '*.c' -type f -print0 | sort -z | while IFS= read -r -d '' src_file; do
    base="$(basename "$src_file")"
    ( cd "$FIXTURES_DIR" && SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH" "$EVOCC" \
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
  SOURCE_DATE_EPOCH="$source_date_epoch" "$gcc_bin" \
    -std=gnu11 -E -P -undef -nostdinc -I. -Iinclude "$base" \
    >"$out_dir/${base}.out" 2>"$out_dir/${base}.stderr"
  rc=$?
  echo "$rc" >"$out_dir/${base}.rc"
done
RUNNER_EOF

  chmod +x "$gcc_runner"

  "$CONTAINER_BIN" run --rm \
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
    record_result "$base" "SKIPPED" "both evocc and gcc-14 unavailable"
    continue
  fi
  if [ "$evocc_ok" -ne 1 ]; then
    record_result "$base" "SKIPPED" "evocc unavailable"
    continue
  fi
  if [ "$gcc_ok" -ne 1 ]; then
    record_result "$base" "SKIPPED" "gcc-14 container unavailable"
    continue
  fi

  evocc_rc="$(cat "$EVOCC_RAW/${base}.rc" 2>/dev/null || echo 0)"
  gcc_rc="$(cat "$GCC_RAW/${base}.rc" 2>/dev/null || echo 0)"

  if [ "$evocc_rc" -ne "$gcc_rc" ]; then
    record_result "$base" "ORACLE_CONFLICT" "exit codes differ: evocc=$evocc_rc gcc=$gcc_rc"
    continue
  fi

  if [ "$evocc_rc" -ne 0 ]; then
    if [ "$base" = "025-include-missing.c" ] \
      && grep -F 'nonexistent.h' "$EVOCC_RAW/${base}.stderr" >/dev/null 2>&1 \
      && grep -F 'nonexistent.h' "$GCC_RAW/${base}.stderr" >/dev/null 2>&1; then
      record_result "$base" "PASS" "both implementations reject the missing include"
    else
      record_result "$base" "ORACLE_CONFLICT" "unexpected controlled rejection: evocc=$evocc_rc gcc=$gcc_rc"
    fi
    continue
  fi

  evocc_tokens="$EVOCC_RAW/${base}.tokens"
  gcc_tokens="$GCC_RAW/${base}.tokens"
  if ! /usr/bin/python3 "$SERIALIZER" <"$evocc_out" >"$evocc_tokens" \
    || ! /usr/bin/python3 "$SERIALIZER" <"$gcc_out" >"$gcc_tokens"; then
    record_result "$base" "ORACLE_CONFLICT" "token serialization failed"
    continue
  fi

  if cmp -s "$evocc_tokens" "$gcc_tokens"; then
    record_result "$base" "PASS" "preprocessing token streams are identical"
    continue
  fi

  record_result "$base" "ORACLE_CONFLICT" "outputs differ after normalization"
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
