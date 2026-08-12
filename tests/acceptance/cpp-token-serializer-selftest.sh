#!/bin/bash
set -eu
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
SERIALIZER="$SCRIPT_DIR/cpp-token-serializer.py"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/c30-cpp-serializer.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

serialize() {
  /usr/bin/python3 "$SERIALIZER" >"$2" <<EOF
$1
EOF
}

serialize 'int value = 1+2;' "$WORK/compact"
serialize 'int  value=1 + 2 ;' "$WORK/spaced"
cmp -s "$WORK/compact" "$WORK/spaced"

serialize 'int x;' "$WORK/two-identifiers"
serialize 'intx;' "$WORK/fused-identifier"
if cmp -s "$WORK/two-identifiers" "$WORK/fused-identifier"; then
  printf 'serializer conflated separate and fused identifiers\n' >&2
  exit 1
fi

serialize 'left / *ptr' "$WORK/two-punctuators"
if serialize 'left /*ptr' "$WORK/fused-comment" 2>/dev/null; then
  printf 'serializer accepted an unterminated fused comment\n' >&2
  exit 1
fi

serialize 'left/* discarded */right' "$WORK/comment"
serialize 'left right' "$WORK/comment-control"
cmp -s "$WORK/comment" "$WORK/comment-control"

printf 'cpp-token-serializer-selftest: PASS\n'
