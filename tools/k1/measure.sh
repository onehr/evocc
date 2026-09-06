#!/bin/bash

# The whole mixed-link measurement, as one command.
#
#   K1_ALLOW_QEMU_DRIFT=1 tools/k1/measure.sh [options]
#
#     --run-id ID           name the evidence directory
#     --reuse-survey DIR    reuse an earlier run's survey instead of redoing it
#     --strict-symbols      substitute only units whose global symbols match
#                           the reference object's
#     --max-bisections N    give up after N reductions (default 3)
#     --jobs N              host compiler processes (default 10)
#
# The emulator digest is checked against the kernel contract; when the pinned
# binary has drifted on this machine the run refuses until the drift is
# acknowledged with K1_ALLOW_QEMU_DRIFT=1, and every receipt then carries both
# digests.
#
# What it does, in order:
#
#   1. makes sure the working copy of the reference object tree exists and
#      that the untouched reference kernel still boots on this machine;
#   2. puts every target C compilation of the pinned configuration through
#      EvoCC and the reference assembler, recording per unit whether it
#      reached assembly and which diagnostic codes stopped it otherwise;
#   3. substitutes every unit that produced an object, relinks, and boots;
#   4. on a failure, bisects the allowlist to the smallest guilty set,
#      removes it, and tries again, so the reported number is the largest
#      allowlist that booted rather than the first one that did not;
#   5. proves the surviving objects are in the linked image;
#   6. writes the receipt and prints the one-line summary.
#
# It is re-runnable with no hand steps: each run restores the substituted
# objects from the pristine copy before installing anything, and the pristine
# copy and the protected reference tree are never written to.

set -eu
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
. "$SCRIPT_DIR/lib.sh"

K1_ROOT="${K1_ROOT:-$REPO_ROOT/.build/k1}"
EVIDENCE_ROOT="${K1_EVIDENCE_ROOT:-$REPO_ROOT/.evidence/k1}"
PYTHON="${K1_PYTHON:-python3}"

RUN_ID="measure-$(date -u '+%Y%m%dT%H%M%SZ')"
REUSE_SURVEY=""
STRICT_SYMBOLS=0
MAX_BISECTIONS=3
JOBS=10

while [ "$#" -gt 0 ]; do
  case "$1" in
    --run-id) RUN_ID="$2"; shift 2 ;;
    --reuse-survey) REUSE_SURVEY="$2"; shift 2 ;;
    --strict-symbols) STRICT_SYMBOLS=1; shift ;;
    --max-bisections) MAX_BISECTIONS="$2"; shift 2 ;;
    --jobs) JOBS="$2"; shift 2 ;;
    -h|--help) sed -n '3,40p' "$0"; exit 0 ;;
    *) k1_die "unknown option: $1" ;;
  esac
done

k1_require_tools

RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
mkdir -p "$RUN_DIR"
SURVEY_DIR="$RUN_DIR/survey"
DRIFT_FLAG="--allow-symbol-drift"
[ "$STRICT_SYMBOLS" -eq 1 ] && DRIFT_FLAG=""

step() { printf '\n== %s ==\n' "$1"; }

# --- 1. the reference must still boot -------------------------------------
step "reference kernel"
"$SCRIPT_DIR/reproduce-reference.sh" | tee "$RUN_DIR/reference.log"
REFERENCE_BOOT="$(awk '/^evidence: /{print $2}' "$RUN_DIR/reference.log")/reference-boot.json"
[ -f "$REFERENCE_BOOT" ] || k1_die "the reference boot left no receipt at $REFERENCE_BOOT"

# --- 2. the per-unit survey -----------------------------------------------
if [ -n "$REUSE_SURVEY" ]; then
  [ -f "$REUSE_SURVEY/units.jsonl" ] || k1_die "no survey at $REUSE_SURVEY"
  step "survey: reusing $REUSE_SURVEY"
  SURVEY_DIR="$REUSE_SURVEY"
else
  step "survey: every target C compilation"
  "$PYTHON" "$SCRIPT_DIR/substitute.py" --all --no-install \
    --k1-root "$K1_ROOT" --evidence-root "$RUN_DIR" --run-id survey --jobs "$JOBS"
fi

ALLOWLIST="$RUN_DIR/allowlist.txt"
"$PYTHON" "$SCRIPT_DIR/plan.py" allowlist "$SURVEY_DIR/units.jsonl" > "$ALLOWLIST"
TOTAL="$(grep -c . < "$SURVEY_DIR/units.jsonl" || true)"
ASSEMBLING="$(grep -c . < "$ALLOWLIST" || true)"
printf 'k1: %s of %s units produced an object\n' "$ASSEMBLING" "$TOTAL"
[ "$ASSEMBLING" -gt 0 ] || k1_die "no unit produced an object; there is nothing to substitute"

# --- 3 and 4. substitute, boot, and reduce a failure ----------------------
ATTEMPT=0
BISECTIONS=0
BISECTION_JSON=""
FAILING_UNITS="$RUN_DIR/failing-units.txt"
: > "$FAILING_UNITS"

while :; do
  ATTEMPT=$((ATTEMPT + 1))
  ATTEMPT_DIR="$RUN_DIR/attempt-$(printf '%02d' "$ATTEMPT")"
  mkdir -p "$ATTEMPT_DIR"
  cp "$ALLOWLIST" "$ATTEMPT_DIR/allowlist.txt"

  step "attempt $ATTEMPT: substitute $(grep -c . < "$ALLOWLIST") units and relink"
  substitution_ok=1
  "$PYTHON" "$SCRIPT_DIR/substitute.py" --allowlist "$ALLOWLIST" \
    --k1-root "$K1_ROOT" --evidence-root "$ATTEMPT_DIR" --run-id substitution \
    --jobs "$JOBS" $DRIFT_FLAG || substitution_ok=0

  boot_ok=0
  if [ "$substitution_ok" -eq 1 ]; then
    step "attempt $ATTEMPT: boot"
    "$SCRIPT_DIR/boot.sh" "$K1_ROOT/out/arch/arm64/boot/Image" "$ATTEMPT_DIR/boot" && boot_ok=1
  else
    printf 'k1: the relink failed; treating this allowlist as failing\n' >&2
  fi

  if [ "$boot_ok" -eq 1 ]; then
    SUBSTITUTION_JSON="$ATTEMPT_DIR/substitution/substitution.json"
    BOOT_JSON="$ATTEMPT_DIR/boot/boot.json"
    break
  fi

  if [ "$BISECTIONS" -ge "$MAX_BISECTIONS" ]; then
    k1_die "still failing after $BISECTIONS reductions; see $RUN_DIR"
  fi
  BISECTIONS=$((BISECTIONS + 1))

  step "attempt $ATTEMPT failed: reducing the allowlist"
  "$PYTHON" "$SCRIPT_DIR/bisect.py" "$ALLOWLIST" \
    --k1-root "$K1_ROOT" --evidence-root "$RUN_DIR" \
    --run-id "bisection-$BISECTIONS" --jobs "$JOBS" --assume-failing $DRIFT_FLAG
  BISECTION_JSON="$RUN_DIR/bisection-$BISECTIONS/bisect.json"
  "$K1_JQ" -r '.minimalFailingSet[]?' "$BISECTION_JSON" >> "$FAILING_UNITS"
  "$PYTHON" "$SCRIPT_DIR/plan.py" subtract "$ALLOWLIST" "$BISECTION_JSON" \
    > "$ALLOWLIST.next"
  if cmp -s "$ALLOWLIST" "$ALLOWLIST.next"; then
    k1_die "the reduction blamed nothing; refusing to loop"
  fi
  mv "$ALLOWLIST.next" "$ALLOWLIST"
  if [ "$(grep -c . < "$ALLOWLIST" || true)" -eq 0 ]; then
    k1_die "every unit was blamed; no allowlist boots"
  fi
done

# --- 5. proof --------------------------------------------------------------
step "proof of substitution"
"$PYTHON" "$SCRIPT_DIR/prove.py" --k1-root "$K1_ROOT" \
  --units "$K1_ROOT/substituted.json" --output "$RUN_DIR/proof.json" > /dev/null

# --- 6. receipt ------------------------------------------------------------
step "receipt"
RECEIPT="${K1_RECEIPT:-$REPO_ROOT/tests/contracts/k1-mixed-link-receipt.json}"
set -- --reference-boot "$REFERENCE_BOOT" \
       --survey "$SURVEY_DIR/substitution.json" \
       --survey-units "$SURVEY_DIR/units.jsonl" \
       --substitution "$SUBSTITUTION_JSON" \
       --boot "$BOOT_JSON" \
       --proof "$RUN_DIR/proof.json" \
       --failing-units "$FAILING_UNITS" \
       --output "$RECEIPT"
[ -n "$BISECTION_JSON" ] && set -- "$@" --bisection "$BISECTION_JSON"
"$PYTHON" "$SCRIPT_DIR/receipt.py" "$@"

printf '\nevidence: %s\n' "$RUN_DIR"
