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
EXPECTED_MANIFEST_SHA="${C30_EXPECTED_EVIDENCE_MANIFEST_SHA256:-}"

[ -f "$MANIFEST" ] || {
  printf 'evidence manifest is missing\n' >&2
  exit 1
}
[ -x "$SHASUM" ] || exit 2
if ! printf '%s\n' "$EXPECTED_MANIFEST_SHA" | grep -Eq '^[0-9a-f]{64}$'; then
  printf 'external evidence-manifest digest is missing or malformed\n' >&2
  exit 2
fi

ACTUAL_MANIFEST_SHA="$("$SHASUM" -a 256 "$MANIFEST" | awk '{print $1}')"
[ "$ACTUAL_MANIFEST_SHA" = "$EXPECTED_MANIFEST_SHA" ] || {
  printf 'evidence manifest differs from the externally frozen digest\n' >&2
  exit 1
}

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
