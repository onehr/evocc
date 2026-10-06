#!/bin/bash

set -u
set -o pipefail

export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
VERIFIER="$SCRIPT_DIR/verify-root-of-trust-envelope.sh"
BUILD_ENV="$PROJECT_ROOT/contracts/build-environment.json"
JQ=/opt/homebrew/Cellar/jq/1.8.2/bin/jq
SHASUM=/usr/bin/shasum
GIT=/usr/bin/git

if [ ! -x "$VERIFIER" ] || [ ! -x "$JQ" ] || [ ! -x "$SHASUM" ] || \
   [ ! -x "$GIT" ]; then
  printf 'root-of-trust envelope selftest setup is incomplete\n' >&2
  exit 2
fi

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/c30-root-envelope.XXXXXX")" || exit 2
INSIDE="$PROJECT_ROOT/.root-envelope-selftest.$$.json"
cleanup() {
  /bin/rm -f "$INSIDE"
  case "$SCRATCH" in
    "${TMPDIR:-/tmp}"/c30-root-envelope.*) /bin/rm -rf "$SCRATCH" ;;
    *) printf 'unsafe root-envelope cleanup target\n' >&2; return 1 ;;
  esac
}
trap cleanup EXIT HUP INT TERM

PASSED=0
FAILED=0
pass() {
  PASSED=$((PASSED + 1))
  printf 'PASS %s\n' "$1"
}
fail() {
  FAILED=$((FAILED + 1))
  printf 'FAIL %s: %s\n' "$1" "$2" >&2
}
digest() {
  "$SHASUM" -a 256 "$1" | awk '{print $1}'
}
verify() {
  local project="$1"
  local envelope="$2"
  local expected="$3"
  C30_EXPECTED_ROOT_OF_TRUST_ENVELOPE_SHA256="$expected" \
    bash "$VERIFIER" "$project" "$envelope" >/dev/null 2>&1
}

TREES="$SCRATCH/trees.jsonl"
FILES="$SCRATCH/files.jsonl"
TOOLS="$SCRATCH/tools.jsonl"
ENVELOPE="$SCRATCH/envelope.json"
: >"$TREES"
: >"$FILES"
: >"$TOOLS"

for path in \
  tests \
  crates/evocc-common/tests \
  crates/evocc-lex/tests \
  crates/evocc-parser/tests \
  crates/evocc-egraph/tests \
  crates/evocc-proof/tests \
  crates/evocc/tests; do
  "$JQ" -cn --arg path "$path" \
    --arg oid "$($GIT -C "$PROJECT_ROOT" rev-parse "HEAD:$path")" \
    '{path:$path,oid:$oid}' >>"$TREES"
done

for path in Cargo.toml Cargo.lock docs/archive/roadmap.md \
  contracts/build-environment.json contracts/kernel-target.json; do
  "$JQ" -cn --arg path "$path" --arg sha256 "$(digest "$PROJECT_ROOT/$path")" \
    '{path:$path,sha256:$sha256}' >>"$FILES"
done

for id in bash git jq shasum; do
  "$JQ" -cn --arg id "$id" \
    --arg path "$($JQ -r --arg id "$id" '.toolchain.hostReplay[$id].path' "$BUILD_ENV")" \
    --arg sha256 "$($JQ -r --arg id "$id" '.toolchain.hostReplay[$id].sha256' "$BUILD_ENV")" \
    '{id:$id,path:$path,sha256:$sha256}' >>"$TOOLS"
done

"$JQ" -n \
  --arg freezeCommit "$($GIT -C "$PROJECT_ROOT" rev-parse HEAD)" \
  --arg freezeTree "$($GIT -C "$PROJECT_ROOT" rev-parse 'HEAD^{tree}')" \
  --slurpfile frozenTrees "$TREES" --slurpfile frozenFiles "$FILES" \
  --slurpfile tools "$TOOLS" \
  '{schemaVersion:1,freezeCommit:$freezeCommit,freezeTree:$freezeTree,
    frozenTrees:$frozenTrees,frozenFiles:$frozenFiles,tools:$tools}' >"$ENVELOPE"
ENVELOPE_SHA="$(digest "$ENVELOPE")"

if verify "$PROJECT_ROOT" "$ENVELOPE" "$ENVELOPE_SHA"; then
  pass intact_envelope
else
  fail intact_envelope 'valid envelope was rejected'
fi

if verify "$PROJECT_ROOT" "$ENVELOPE" \
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; then
  fail wrong_external_digest 'wrong external digest was accepted'
else
  pass wrong_external_digest
fi

WRONG_TREE="$SCRATCH/wrong-tree.json"
"$JQ" '.frozenTrees[0].oid = "0000000000000000000000000000000000000000"' \
  "$ENVELOPE" >"$WRONG_TREE"
if verify "$PROJECT_ROOT" "$WRONG_TREE" "$(digest "$WRONG_TREE")"; then
  fail wrong_tree_oid 'wrong tree oid was accepted'
else
  pass wrong_tree_oid
fi

MISSING_TREE="$SCRATCH/missing-tree.json"
"$JQ" '.frozenTrees = .frozenTrees[0:-1]' "$ENVELOPE" >"$MISSING_TREE"
if verify "$PROJECT_ROOT" "$MISSING_TREE" "$(digest "$MISSING_TREE")"; then
  fail missing_tree 'missing frozen tree was accepted'
else
  pass missing_tree
fi

DUPLICATE_TREE="$SCRATCH/duplicate-tree.json"
"$JQ" '.frozenTrees[6] = .frozenTrees[0]' "$ENVELOPE" >"$DUPLICATE_TREE"
if verify "$PROJECT_ROOT" "$DUPLICATE_TREE" "$(digest "$DUPLICATE_TREE")"; then
  fail duplicate_tree 'duplicate frozen tree substituted for a required tree'
else
  pass duplicate_tree
fi

WRONG_TOOL="$SCRATCH/wrong-tool.json"
"$JQ" '.tools[1].path = "/tmp/not-git"' "$ENVELOPE" >"$WRONG_TOOL"
if verify "$PROJECT_ROOT" "$WRONG_TOOL" "$(digest "$WRONG_TOOL")"; then
  fail wrong_tool_path 'wrong tool path was accepted'
else
  pass wrong_tool_path
fi

cp "$ENVELOPE" "$INSIDE"
if verify "$PROJECT_ROOT" "$INSIDE" "$(digest "$INSIDE")"; then
  fail repository_local_envelope 'repository-local envelope was accepted'
else
  pass repository_local_envelope
fi
/bin/rm -f "$INSIDE"

CLONE="$SCRATCH/repository"
if "$GIT" clone --quiet --no-local --no-hardlinks "$PROJECT_ROOT" "$CLONE" && \
   printf '\nmutated\n' >>"$CLONE/tests/contracts/foundation-required-ids.txt"; then
  if verify "$CLONE" "$ENVELOPE" "$ENVELOPE_SHA"; then
    fail live_frozen_tree_drift 'modified frozen test tree was accepted'
  else
    pass live_frozen_tree_drift
  fi
else
  printf 'root-of-trust selftest could not construct drift clone\n' >&2
  exit 2
fi

if [ $((PASSED + FAILED)) -ne 8 ]; then
  printf 'root-of-trust envelope selftest denominator changed\n' >&2
  exit 2
fi
printf 'root-of-trust envelope selftest: %d passed, %d failed\n' "$PASSED" "$FAILED"
[ "$FAILED" -eq 0 ]
