#!/bin/bash

set -u
set -o pipefail

export LC_ALL=C

if [ "$#" -ne 2 ]; then
  printf 'usage: %s PROJECT_ROOT OUTPUT_DIR\n' "$0" >&2
  exit 2
fi

PROJECT_ROOT="$(cd "$1" 2>/dev/null && pwd -P)" || exit 2
OUTPUT_DIR="$2"
BUILD_ENV="$PROJECT_ROOT/contracts/build-environment.json"
JQ=/opt/homebrew/Cellar/jq/1.8.2/bin/jq
SHASUM=/usr/bin/shasum

if [ ! -d "$OUTPUT_DIR" ] || [ -L "$OUTPUT_DIR" ] || \
   [ ! -x "$JQ" ] || [ ! -x "$SHASUM" ]; then
  printf 'fresh-clone replay setup is invalid\n' >&2
  exit 2
fi
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd -P)"

GIT="$($JQ -r '.toolchain.hostReplay.git.path // empty' "$BUILD_ENV")"
CARGO="$($JQ -r '.toolchain.hostReplay.cargo.path // empty' "$BUILD_ENV")"
RUSTC="$($JQ -r '.toolchain.hostReplay.rustc.path // empty' "$BUILD_ENV")"
TIMEOUT="$($JQ -r '.toolchain.hostReplay.timeout.path // empty' "$BUILD_ENV")"

for id in git cargo rustc timeout; do
  path="$($JQ -r --arg id "$id" '.toolchain.hostReplay[$id].path // empty' "$BUILD_ENV")"
  expected="$($JQ -r --arg id "$id" '.toolchain.hostReplay[$id].sha256 // empty' "$BUILD_ENV")"
  if [ ! -f "$path" ] || [ -L "$path" ] || \
     [ "$($SHASUM -a 256 "$path" | awk '{print $1}')" != "$expected" ]; then
    printf 'fresh-clone tool identity drifted: %s\n' "$id" >&2
    exit 2
  fi
done

SOURCE_COMMIT="$($GIT -C "$PROJECT_ROOT" rev-parse HEAD 2>/dev/null)"
SOURCE_TREE="$($GIT -C "$PROJECT_ROOT" rev-parse 'HEAD^{tree}' 2>/dev/null)"
if [ -n "$($GIT -C "$PROJECT_ROOT" status --porcelain=v1 --untracked-files=no)" ] || \
   ! printf '%s\n' "$SOURCE_COMMIT" | grep -Eq '^[0-9a-f]{40}$' || \
   ! printf '%s\n' "$SOURCE_TREE" | grep -Eq '^[0-9a-f]{40}$'; then
  printf 'source repository is not a clean resolved commit\n' >&2
  exit 1
fi

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/c30-two-clones.XXXXXX")" || exit 2
ROWS="$OUTPUT_DIR/fresh-clone-rows.jsonl"
RECEIPT="$OUTPUT_DIR/fresh-clone-receipt.json"
: >"$ROWS"
cleanup() {
  case "$SCRATCH" in
    "${TMPDIR:-/tmp}"/c30-two-clones.*) /bin/rm -rf "$SCRATCH" ;;
    *) printf 'unsafe fresh-clone cleanup target\n' >&2; return 1 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

OVERALL_OK=1
for lane in one two; do
  lane_root="$SCRATCH/$lane"
  clone_dir="$lane_root/repository"
  mkdir -p "$lane_root/home" "$lane_root/cargo-home" "$lane_root/target"

  "$TIMEOUT" 60 "$GIT" clone --quiet --no-local --no-hardlinks \
    "$PROJECT_ROOT" "$clone_dir" \
    >"$OUTPUT_DIR/$lane-git.stdout" 2>"$OUTPUT_DIR/$lane-git.stderr"
  clone_rc=$?
  clone_commit=''
  clone_tree=''
  test_rc=125
  clippy_rc=125
  fmt_rc=125
  post_status='CLONE_UNAVAILABLE'

  if [ "$clone_rc" -eq 0 ]; then
    clone_commit="$($GIT -C "$clone_dir" rev-parse HEAD 2>/dev/null)"
    clone_tree="$($GIT -C "$clone_dir" rev-parse 'HEAD^{tree}' 2>/dev/null)"
    common_env="PATH=$(dirname "$CARGO"):/usr/bin:/bin"

    (
      cd "$clone_dir" || exit 2
      /usr/bin/env -i "$common_env" \
        HOME="$lane_root/home" CARGO_HOME="$lane_root/cargo-home" \
        CARGO_TARGET_DIR="$lane_root/target" CARGO_NET_OFFLINE=true \
        RUSTC="$RUSTC" LC_ALL=C TZ=UTC \
        "$TIMEOUT" 300 "$CARGO" test --workspace --all-targets --locked --offline
    ) >"$OUTPUT_DIR/$lane-test.stdout" 2>"$OUTPUT_DIR/$lane-test.stderr"
    test_rc=$?

    (
      cd "$clone_dir" || exit 2
      /usr/bin/env -i "$common_env" \
        HOME="$lane_root/home" CARGO_HOME="$lane_root/cargo-home" \
        CARGO_TARGET_DIR="$lane_root/target" CARGO_NET_OFFLINE=true \
        RUSTC="$RUSTC" LC_ALL=C TZ=UTC \
        "$TIMEOUT" 300 "$CARGO" clippy --workspace --all-targets \
          --locked --offline -- -D warnings
    ) >"$OUTPUT_DIR/$lane-clippy.stdout" 2>"$OUTPUT_DIR/$lane-clippy.stderr"
    clippy_rc=$?

    (
      cd "$clone_dir" || exit 2
      /usr/bin/env -i "$common_env" \
        HOME="$lane_root/home" CARGO_HOME="$lane_root/cargo-home" \
        CARGO_TARGET_DIR="$lane_root/target" CARGO_NET_OFFLINE=true \
        RUSTC="$RUSTC" LC_ALL=C TZ=UTC \
        "$TIMEOUT" 120 "$CARGO" fmt --all -- --check
    ) >"$OUTPUT_DIR/$lane-fmt.stdout" 2>"$OUTPUT_DIR/$lane-fmt.stderr"
    fmt_rc=$?
    post_status="$($GIT -C "$clone_dir" status --porcelain=v1 --untracked-files=no 2>/dev/null)"
  fi

  lane_ok=false
  if [ "$clone_rc" -eq 0 ] && [ "$clone_commit" = "$SOURCE_COMMIT" ] && \
     [ "$clone_tree" = "$SOURCE_TREE" ] && [ "$test_rc" -eq 0 ] && \
     [ "$clippy_rc" -eq 0 ] && [ "$fmt_rc" -eq 0 ] && [ -z "$post_status" ]; then
    lane_ok=true
  else
    OVERALL_OK=0
  fi

  "$JQ" -cn \
    --arg lane "$lane" --arg commit "$clone_commit" --arg tree "$clone_tree" \
    --arg postStatus "$post_status" --argjson cloneRc "$clone_rc" \
    --argjson testRc "$test_rc" --argjson clippyRc "$clippy_rc" \
    --argjson fmtRc "$fmt_rc" --argjson pass "$lane_ok" \
    '{lane:$lane,commit:$commit,tree:$tree,cloneRc:$cloneRc,testRc:$testRc,
      clippyRc:$clippyRc,fmtRc:$fmtRc,postStatus:$postStatus,pass:$pass,
      cargoHomeFresh:true,targetFresh:true,offline:true}' >>"$ROWS"
done

"$JQ" -s \
  --arg sourceCommit "$SOURCE_COMMIT" --arg sourceTree "$SOURCE_TREE" \
  --arg overall "$(if [ "$OVERALL_OK" -eq 1 ]; then printf PASS; else printf FAIL; fi)" \
  '{schemaVersion:1,sourceCommit:$sourceCommit,sourceTree:$sourceTree,
    lanes:.,overall:$overall}' "$ROWS" >"$RECEIPT"

cleanup
trap - EXIT HUP INT TERM
printf 'FRESH_CLONE_REPLAY receipt=%s overall=%s\n' "$RECEIPT" \
  "$(if [ "$OVERALL_OK" -eq 1 ]; then printf PASS; else printf FAIL; fi)"
[ "$OVERALL_OK" -eq 1 ]
