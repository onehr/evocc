#!/bin/bash

# Boot the working-copy kernel and report PASS or FAIL.
#
# The Image under test is the one the last relink produced in the working copy,
# so this is the measurement's verdict: a kernel carrying EvoCC objects either
# reaches the initramfs and prints this flight's nonce, or it does not.
#
# Usage: tools/k1/boot.sh [image] [run-directory]

set -eu
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
. "$SCRIPT_DIR/lib.sh"

K1_ROOT="${K1_ROOT:-$REPO_ROOT/.build/k1}"
INITRAMFS="${K1_INITRAMFS:-$REPO_ROOT/.build/initramfs/initramfs.cpio}"
CONTRACT="${K1_KERNEL_CONTRACT:-$REPO_ROOT/contracts/kernel-target.json}"
EVIDENCE_ROOT="${K1_EVIDENCE_ROOT:-$REPO_ROOT/.evidence/k1}"
BOOT_TIMEOUT="${K1_BOOT_TIMEOUT:-60}"

IMAGE="${1:-$K1_ROOT/out/arch/arm64/boot/Image}"
RUN_DIR="${2:-$EVIDENCE_ROOT/boot-$(date -u '+%Y%m%dT%H%M%SZ')-$$}"

k1_require_tools
[ -f "$IMAGE" ] || k1_die "Image missing: $IMAGE"
[ -f "$INITRAMFS" ] || k1_die "initramfs missing: $INITRAMFS"

mkdir -p "$RUN_DIR"
LOG="$RUN_DIR/boot.log"
NONCE="$(k1_nonce)"
IMAGE_SHA="$(k1_sha256 "$IMAGE")"

VERDICT="$(k1_boot "$CONTRACT" "$IMAGE" "$INITRAMFS" "$NONCE" "$LOG" "$BOOT_TIMEOUT")"
LOG_SHA="$(k1_sha256 "$LOG")"

"$K1_JQ" -n \
  --arg verdict "$VERDICT" \
  --arg image "$IMAGE" \
  --arg imageSha256 "$IMAGE_SHA" \
  --arg nonce "$NONCE" \
  --arg bootLane "${K1_BOOT_LANE:-hvf-host}" \
  --arg selftestProgram "${K1_SELFTEST_COLLECTION:-}" \
  --arg selftestArguments "${K1_SELFTEST_ARGS:-}" \
  --arg serialLog "$LOG" \
  --arg serialLogSha256 "$LOG_SHA" \
  --argjson timeoutSeconds "$BOOT_TIMEOUT" \
  --argjson qemu "$(k1_qemu_facts "$CONTRACT")" \
  --arg flownAt "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
  '{verdict:$verdict,image:$image,imageSha256:$imageSha256,nonce:$nonce,bootLane:$bootLane,
    selftestProgram:$selftestProgram,selftestArguments:$selftestArguments,
    serialLog:$serialLog,serialLogSha256:$serialLogSha256,
    timeoutSeconds:$timeoutSeconds,qemu:$qemu,flownAt:$flownAt}' \
  > "$RUN_DIR/boot.json"

printf 'evidence: %s\n' "$RUN_DIR"
printf 'image sha256: %s\n' "$IMAGE_SHA"
printf 'serial log sha256: %s\n' "$LOG_SHA"
if [ "$VERDICT" = "PASS" ]; then
  printf 'PASS\n'
  exit 0
fi
printf 'FAIL %s\n' "$VERDICT"
exit 1
