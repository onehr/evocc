#!/bin/bash
set -eu
set -o pipefail

export LC_ALL=C

if [ "$#" -ne 1 ]; then
  printf 'usage: %s EVIDENCE_DIR\n' "$0" >&2
  exit 2
fi

EVIDENCE_DIR="$(cd "$1" 2>/dev/null && pwd -P)" || {
  printf 'evidence directory is unavailable: %s\n' "$1" >&2
  exit 1
}
MANIFEST="$EVIDENCE_DIR/evidence-manifest.sha256"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"

[ -f "$MANIFEST" ] || {
  printf 'evidence manifest is missing\n' >&2
  exit 1
}
[ -x "$SHASUM" ] || exit 2

if find "$EVIDENCE_DIR" -type l -print | grep -q .; then
  printf 'evidence contains a symbolic link\n' >&2
  exit 1
fi

ACTUAL="$(mktemp "${TMPDIR:-/tmp}/c30-evidence-actual.XXXXXX")"
DECLARED="$(mktemp "${TMPDIR:-/tmp}/c30-evidence-declared.XXXXXX")"
trap 'rm -f "$ACTUAL" "$DECLARED"' EXIT

(cd "$EVIDENCE_DIR" && find . -type f ! -name evidence-manifest.sha256 | sort) >"$ACTUAL"
awk '{print $2}' "$MANIFEST" | sort >"$DECLARED"
cmp -s "$ACTUAL" "$DECLARED" || {
  printf 'evidence namespace differs from the manifest\n' >&2
  exit 1
}

(cd "$EVIDENCE_DIR" && "$SHASUM" -a 256 -c evidence-manifest.sha256 --quiet) || {
  printf 'evidence content differs from the manifest\n' >&2
  exit 1
}

printf 'foundation evidence: VERIFIED\n'
