#!/bin/bash

# Build the flight-lab initramfs: static reference init plus the minimal
# device and mount-point skeleton, packed deterministically with the kernel's
# own gen_init_cpio and the frozen timestamp.

set -eu
set -o pipefail

export LC_ALL=C TZ=UTC

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
JQ="${C30_JQ:-/opt/homebrew/bin/jq}"

sealed_image="$("$JQ" -r '.buildImage.sealedImageTag' "$PROJECT_ROOT/contracts/build-environment.json")"
sde="$("$JQ" -r '.reproducibility.SOURCE_DATE_EPOCH' "$PROJECT_ROOT/contracts/build-environment.json")"

RAMFS="$PROJECT_ROOT/.build/initramfs"
mkdir -p "$RAMFS/root"

# gen_init_cpio is a host tool produced by the reference build; lane choice is
# irrelevant because the tool's output depends only on the spec and timestamp.
GEN_INIT_CPIO="$PROJECT_ROOT/.build/reference/gcc-lane-1/usr/gen_init_cpio"
[ -x "$GEN_INIT_CPIO" ] || { printf 'initramfs: gen_init_cpio missing; run reference builds first\n' >&2; exit 1; }

cat > "$RAMFS/spec.txt" << EOF
file /init /work/ramfs/root/init 755 0 0
dir /dev 755 0 0
dir /proc 755 0 0
nod /dev/console 600 0 0 c 5 1
nod /dev/null 666 0 0 c 1 3
EOF

container run --rm --platform linux/arm64 --network none \
  -v "$PROJECT_ROOT/.toolchain/flight-lab-init.c:/work/init.c:ro" \
  -v "$RAMFS:/work/ramfs" \
  -v "$GEN_INIT_CPIO:/work/bin/gen_init_cpio:ro" \
  -w /work "$sealed_image" sh -c "
    set -eu
    gcc-14 -static -O2 -Wall -Wextra -o /work/ramfs/root/init /work/init.c
    /work/bin/gen_init_cpio -t $sde /work/ramfs/spec.txt > /work/ramfs/initramfs.cpio
    sha256sum /work/ramfs/initramfs.cpio
  " 2>/dev/null | grep -E '^[0-9a-f]{64} ' || { printf 'initramfs: build failed\n' >&2; exit 1; }
