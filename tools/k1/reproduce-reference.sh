#!/bin/bash

# Gate zero of the mixed-link measurement: the untouched reference kernel must
# boot on this machine before any substituted kernel means anything.
#
# The GCC-built reference tree is copied once into two trees: .build/k1/out,
# which every later step mutates, and .build/k1/pristine, which nothing writes
# to. The gate boots the pristine Image, so it says the same thing whether or
# not a substituted kernel is currently installed in the working tree, and
# .build/reference stays untouched evidence either way.
#
# Usage: tools/k1/reproduce-reference.sh [--refresh]
#   --refresh  re-copy the reference tree even if the copy already exists

set -eu
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
. "$SCRIPT_DIR/lib.sh"

K1_ROOT="${K1_ROOT:-$REPO_ROOT/.build/k1}"
REFERENCE="${K1_REFERENCE:-$REPO_ROOT/.build/reference/gcc-lane-1}"
INITRAMFS="${K1_INITRAMFS:-$REPO_ROOT/.build/initramfs/initramfs.cpio}"
CONTRACT="${K1_KERNEL_CONTRACT:-$REPO_ROOT/contracts/kernel-target.json}"
EVIDENCE_ROOT="${K1_EVIDENCE_ROOT:-$REPO_ROOT/.evidence/k1}"
BOOT_TIMEOUT="${K1_BOOT_TIMEOUT:-60}"

REFRESH=0
[ "${1:-}" = "--refresh" ] && REFRESH=1

k1_require_tools
[ -d "$REFERENCE" ] || k1_die "reference tree missing: $REFERENCE"
[ -f "$INITRAMFS" ] || k1_die "initramfs missing: $INITRAMFS"
[ -f "$CONTRACT" ] || k1_die "kernel contract missing: $CONTRACT"

OUT="$K1_ROOT/out"
PRISTINE="$K1_ROOT/pristine"

# The reference tree is about 1 GiB and is copied twice: one mutable working
# tree and one pristine copy the bisection restores from. Refuse to start
# without room for both plus the relink's own output.
avail_gib="$(df -g "$REPO_ROOT" | awk 'NR==2 {print $4}')"
if [ "${avail_gib:-0}" -lt 6 ] && { [ "$REFRESH" -eq 1 ] || [ ! -d "$OUT" ]; }; then
  k1_die "only ${avail_gib} GiB free; the two tree copies need about 3 GiB plus relink headroom"
fi

if [ "$REFRESH" -eq 1 ] || [ ! -d "$OUT" ]; then
  printf 'k1: copying the reference tree to %s\n' "$OUT"
  mkdir -p "$OUT" "$PRISTINE"
  rsync -a --delete "$REFERENCE/" "$OUT/"
  rsync -a --delete "$REFERENCE/" "$PRISTINE/"
else
  printf 'k1: reusing the existing copy at %s (pass --refresh to re-copy)\n' "$OUT"
fi

REF_IMAGE="$REFERENCE/arch/arm64/boot/Image"
OUT_IMAGE="$PRISTINE/arch/arm64/boot/Image"
ref_sha="$(k1_sha256 "$REF_IMAGE")"
out_sha="$(k1_sha256 "$OUT_IMAGE")"
[ "$ref_sha" = "$out_sha" ] || k1_die "the pristine copy's Image differs from the reference Image"
[ -d "$OUT" ] || k1_die "the working copy is missing: $OUT"

RUN_ID="$(date -u '+%Y%m%dT%H%M%SZ')-$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
mkdir -p "$RUN_DIR"
LOG="$RUN_DIR/reference-boot.log"
NONCE="$(k1_nonce)"

printf 'k1: booting the reference Image (nonce %s, timeout %ss)\n' "$NONCE" "$BOOT_TIMEOUT"
VERDICT="$(k1_boot "$CONTRACT" "$OUT_IMAGE" "$INITRAMFS" "$NONCE" "$LOG" "$BOOT_TIMEOUT")"
log_sha="$(k1_sha256 "$LOG")"

"$K1_JQ" -n \
  --arg runId "$RUN_ID" \
  --arg verdict "$VERDICT" \
  --arg image "$OUT_IMAGE" \
  --arg imageSha256 "$out_sha" \
  --arg referenceImage "$REF_IMAGE" \
  --arg referenceImageSha256 "$ref_sha" \
  --arg initramfs "$INITRAMFS" \
  --arg initramfsSha256 "$(k1_sha256 "$INITRAMFS")" \
  --arg nonce "$NONCE" \
  --arg serialLog "$LOG" \
  --arg serialLogSha256 "$log_sha" \
  --argjson timeoutSeconds "$BOOT_TIMEOUT" \
  --argjson qemu "$(k1_qemu_facts "$CONTRACT")" \
  --arg flownAt "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
  '{runId:$runId,verdict:$verdict,image:$image,imageSha256:$imageSha256,
    referenceImage:$referenceImage,referenceImageSha256:$referenceImageSha256,
    initramfs:$initramfs,initramfsSha256:$initramfsSha256,nonce:$nonce,
    serialLog:$serialLog,serialLogSha256:$serialLogSha256,
    timeoutSeconds:$timeoutSeconds,qemu:$qemu,flownAt:$flownAt}' \
  > "$RUN_DIR/reference-boot.json"

printf 'evidence: %s\n' "$RUN_DIR"
printf 'image sha256: %s\n' "$out_sha"
printf 'serial log sha256: %s\n' "$log_sha"

if [ "$VERDICT" = "PASS" ]; then
  printf 'PASS reference kernel boots\n'
  exit 0
fi
printf 'FAIL reference kernel did not boot: %s\n' "$VERDICT" >&2
exit 1
