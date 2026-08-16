#!/bin/bash

set -u
set -o pipefail
export LC_ALL=C

if [ "$#" -ne 2 ]; then
  printf 'usage: %s CONTRACT REQUIRED_IDS\n' "$0" >&2
  exit 2
fi

CONTRACT="$1"
REQUIRED="$2"
JQ="${C30_JQ:-/opt/homebrew/Cellar/jq/1.8.2/bin/jq}"

[ -x "$JQ" ] || exit 2
[ -f "$CONTRACT" ] && [ ! -L "$CONTRACT" ] || exit 1
[ -f "$REQUIRED" ] && [ ! -L "$REQUIRED" ] || exit 1

"$JQ" -e '
  type == "object" and
  keys == ["dialect","fixtures","schemaVersion"] and
  .schemaVersion == 1 and
  .dialect == "gnu11" and
  (.fixtures | type == "array" and length == 34) and
  all(.fixtures[];
    type == "object" and
    keys == ["basis","diagnostic","expectedRc","expectsTokens","file","tokens"] and
    (.file | type == "string" and test("^[0-9]{3}-[a-z0-9-]+\\.c$")) and
    (.expectedRc == 0 or .expectedRc == 1) and
    (.expectsTokens | type == "boolean") and
    (.tokens | type == "array") and
    (.basis | type == "string" and length > 8) and
    all(.tokens[];
      type == "object" and
      keys == ["kind","spelling"] and
      (.kind == "identifier" or .kind == "pp-number" or
       .kind == "quoted" or .kind == "punctuator") and
      (.spelling | type == "string" and length > 0)
    ) and
    (if .expectsTokens then (.tokens | length > 0) else (.tokens | length == 0) end) and
    (if .expectedRc == 0
     then .diagnostic == null
     else
       .file == "025-include-missing.c" and
       .tokens == [] and
       .expectsTokens == false and
       .diagnostic == {
         code:"E0105",line:1,column:1,
         category:"include-not-found",subject:"nonexistent.h"
       }
     end)
  ) and
  ([.fixtures[] | select(.expectedRc == 1) | .file] == ["025-include-missing.c"]) and
  ([.fixtures[].file] | unique | length == 34)
' "$CONTRACT" >/dev/null || exit 1

ACTUAL="$(mktemp "${TMPDIR:-/tmp}/c30-cpp-hand-ids.XXXXXX")" || exit 2
trap '/bin/rm -f "$ACTUAL"' EXIT HUP INT TERM
"$JQ" -r '.fixtures[].file' "$CONTRACT" | sort >"$ACTUAL" || exit 1
cmp -s "$REQUIRED" "$ACTUAL" || exit 1

printf 'PASS cpp_hand_golden_contract fixtures=34\n'
