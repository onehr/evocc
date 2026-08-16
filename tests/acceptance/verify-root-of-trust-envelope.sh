#!/bin/bash

set -u
set -o pipefail

export LC_ALL=C

if [ "$#" -ne 2 ]; then
  printf 'usage: %s PROJECT_ROOT EXTERNAL_ENVELOPE\n' "$0" >&2
  exit 2
fi

PROJECT_ROOT="$(cd "$1" 2>/dev/null && pwd -P)" || exit 2
ENVELOPE_DIR="$(cd "$(dirname "$2")" 2>/dev/null && pwd -P)" || exit 2
ENVELOPE="$ENVELOPE_DIR/$(basename "$2")"
BUILD_ENV="$PROJECT_ROOT/contracts/build-environment.json"
EXPECTED_SHA="${C30_EXPECTED_ROOT_OF_TRUST_ENVELOPE_SHA256:-}"
JQ=/opt/homebrew/Cellar/jq/1.8.2/bin/jq
SHASUM=/usr/bin/shasum
GIT=/usr/bin/git

if [ ! -x "$JQ" ] || [ ! -x "$SHASUM" ] || [ ! -x "$GIT" ]; then
  printf 'root-of-trust bootstrap tools are unavailable\n' >&2
  exit 2
fi
if [ ! -f "$ENVELOPE" ] || [ -L "$ENVELOPE" ]; then
  printf 'external root-of-trust envelope is missing or symbolic\n' >&2
  exit 1
fi
case "$ENVELOPE" in
  "$PROJECT_ROOT"|"$PROJECT_ROOT"/*)
    printf 'root-of-trust envelope must live outside the repository\n' >&2
    exit 1
    ;;
esac
if ! printf '%s\n' "$EXPECTED_SHA" | grep -Eq '^[0-9a-f]{64}$'; then
  printf 'external root-of-trust envelope digest is missing or malformed\n' >&2
  exit 2
fi

ACTUAL_SHA="$($SHASUM -a 256 "$ENVELOPE" | awk '{print $1}')"
if [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then
  printf 'root-of-trust envelope differs from the externally frozen digest\n' >&2
  exit 1
fi

REQUIRED_TREES='tests
crates/evocc-common/tests
crates/evocc-lex/tests
crates/evocc-parser/tests
crates/evocc-egraph/tests
crates/evocc-proof/tests
crates/evocc/tests'
REQUIRED_FILES='Cargo.toml
Cargo.lock
docs/roadmap.md
contracts/build-environment.json
contracts/kernel-target.json'
REQUIRED_TOOLS='bash
git
jq
shasum'

required_trees_json="$(printf '%s\n' "$REQUIRED_TREES" | "$JQ" -R . | "$JQ" -s .)"
required_files_json="$(printf '%s\n' "$REQUIRED_FILES" | "$JQ" -R . | "$JQ" -s .)"
required_tools_json="$(printf '%s\n' "$REQUIRED_TOOLS" | "$JQ" -R . | "$JQ" -s .)"

if ! "$JQ" -e \
    --argjson trees "$required_trees_json" \
    --argjson files "$required_files_json" \
    --argjson tools "$required_tools_json" '
    type == "object" and
    (keys == ["freezeCommit","freezeTree","frozenFiles","frozenTrees","schemaVersion","tools"]) and
    .schemaVersion == 1 and
    (.freezeCommit | test("^[0-9a-f]{40}$")) and
    (.freezeTree | test("^[0-9a-f]{40}$")) and
    (.frozenTrees | type == "array" and length == ($trees|length)) and
    ([.frozenTrees[].path] == $trees) and
    ([.frozenTrees[].path] | unique | length) == ($trees|length) and
    all(.frozenTrees[]; keys == ["oid","path"] and (.oid | test("^[0-9a-f]{40}$"))) and
    (.frozenFiles | type == "array" and length == ($files|length)) and
    ([.frozenFiles[].path] == $files) and
    ([.frozenFiles[].path] | unique | length) == ($files|length) and
    all(.frozenFiles[]; keys == ["path","sha256"] and (.sha256 | test("^[0-9a-f]{64}$"))) and
    (.tools | type == "array" and length == ($tools|length)) and
    ([.tools[].id] == $tools) and
    ([.tools[].id] | unique | length) == ($tools|length) and
    all(.tools[]; keys == ["id","path","sha256"] and
      (.path | startswith("/")) and (.sha256 | test("^[0-9a-f]{64}$")))
  ' "$ENVELOPE" >/dev/null 2>&1; then
  printf 'root-of-trust envelope schema or denominator is invalid\n' >&2
  exit 1
fi

FREEZE_COMMIT="$($JQ -r '.freezeCommit' "$ENVELOPE")"
FREEZE_TREE="$($JQ -r '.freezeTree' "$ENVELOPE")"
if ! "$GIT" -C "$PROJECT_ROOT" cat-file -e "$FREEZE_COMMIT^{commit}" 2>/dev/null || \
   [ "$($GIT -C "$PROJECT_ROOT" rev-parse "$FREEZE_COMMIT^{tree}" 2>/dev/null)" != "$FREEZE_TREE" ] || \
   ! "$GIT" -C "$PROJECT_ROOT" merge-base --is-ancestor "$FREEZE_COMMIT" HEAD; then
  printf 'freeze commit/tree is unavailable or is not an ancestor of HEAD\n' >&2
  exit 1
fi

if [ -n "$($GIT -C "$PROJECT_ROOT" status --porcelain=v1 --untracked-files=all -- \
    tests crates/evocc-common/tests crates/evocc-lex/tests \
    crates/evocc-parser/tests crates/evocc-egraph/tests \
    crates/evocc-proof/tests crates/evocc/tests Cargo.toml Cargo.lock \
    docs/roadmap.md contracts/build-environment.json contracts/kernel-target.json)" ]; then
  printf 'a frozen path has uncommitted drift\n' >&2
  exit 1
fi

while IFS=$'\t' read -r path expected_oid; do
  freeze_oid="$($GIT -C "$PROJECT_ROOT" rev-parse "$FREEZE_COMMIT:$path" 2>/dev/null)"
  current_oid="$($GIT -C "$PROJECT_ROOT" rev-parse "HEAD:$path" 2>/dev/null)"
  if [ "$freeze_oid" != "$expected_oid" ] || [ "$current_oid" != "$expected_oid" ]; then
    printf 'frozen tree drifted: %s\n' "$path" >&2
    exit 1
  fi
done <<EOF
$($JQ -r '.frozenTrees[] | [.path,.oid] | @tsv' "$ENVELOPE")
EOF

while IFS=$'\t' read -r path expected_sha; do
  if [ ! -f "$PROJECT_ROOT/$path" ] || [ -L "$PROJECT_ROOT/$path" ]; then
    printf 'frozen file is missing or symbolic: %s\n' "$path" >&2
    exit 1
  fi
  current_sha="$($SHASUM -a 256 "$PROJECT_ROOT/$path" | awk '{print $1}')"
  freeze_sha="$($GIT -C "$PROJECT_ROOT" show "$FREEZE_COMMIT:$path" | $SHASUM -a 256 | awk '{print $1}')"
  if [ "$current_sha" != "$expected_sha" ] || [ "$freeze_sha" != "$expected_sha" ]; then
    printf 'frozen file drifted: %s\n' "$path" >&2
    exit 1
  fi
done <<EOF
$($JQ -r '.frozenFiles[] | [.path,.sha256] | @tsv' "$ENVELOPE")
EOF

while IFS=$'\t' read -r id path expected_sha; do
  contract_path="$($JQ -r --arg id "$id" '.toolchain.hostReplay[$id].path // empty' "$BUILD_ENV")"
  contract_sha="$($JQ -r --arg id "$id" '.toolchain.hostReplay[$id].sha256 // empty' "$BUILD_ENV")"
  if [ "$path" != "$contract_path" ] || [ "$expected_sha" != "$contract_sha" ] || \
     [ ! -f "$path" ] || [ -L "$path" ] || \
     [ "$($SHASUM -a 256 "$path" | awk '{print $1}')" != "$expected_sha" ]; then
    printf 'frozen tool identity drifted: %s\n' "$id" >&2
    exit 1
  fi
done <<EOF
$($JQ -r '.tools[] | [.id,.path,.sha256] | @tsv' "$ENVELOPE")
EOF

printf 'ROOT_OF_TRUST freezeCommit=%s freezeTree=%s trees=7 files=5 tools=4\n' \
  "$FREEZE_COMMIT" "$FREEZE_TREE"
