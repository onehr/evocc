#!/bin/bash

# End-to-end Day-1 nexlab gate: reconstruct, build, boot, and accept.
#
# This orchestrator is what the different-absolute-path gate runs on a fresh
# clone. Every step fails closed; a green result means the whole pipeline
# actually ran here, not that it ran somewhere else once.

set -eu
set -o pipefail

export LC_ALL=C TZ=UTC

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"

cd "$PROJECT_ROOT"

printf '== 1/5 bootstrap workspace ==\n'
bash .toolchain/bootstrap-workspace.sh

printf '== 2/5 reference kernel builds (4 clean lanes) ==\n'
bash .toolchain/build-reference-kernels.sh

printf '== 3/5 nexlab initramfs ==\n'
bash .toolchain/build-nexlab-initramfs.sh

printf '== 4/5 boot flights (2 compilers x 2 CPU lanes) ==\n'
boot_flight() {
  image="$1"; lane="$2"; log="$3"
  nonce="flight-$(date -u +%Y%m%dT%H%M%SZ)-$(head -c 4 /dev/urandom | od -A n -t x1 | tr -d ' \n')"
  bash .toolchain/boot-reference-kernel.sh "$image" .build/initramfs/initramfs.cpio "$lane" "$nonce" "$log"
}
boot_flight .build/reference/gcc-lane-1/arch/arm64/boot/Image hvf-host .build/reference/gcc-lane-1/boot-hvf.log
boot_flight .build/reference/gcc-lane-1/arch/arm64/boot/Image tcg-cortex-a72 .build/reference/gcc-lane-1/boot-tcg.log
boot_flight .build/reference/clang-lane-1/arch/arm64/boot/Image hvf-host .build/reference/clang-lane-1/boot-hvf.log
boot_flight .build/reference/clang-lane-1/arch/arm64/boot/Image tcg-cortex-a72 .build/reference/clang-lane-1/boot-tcg.log

printf '== 5/5 artifact manifest + acceptance ==\n'
cd "$PROJECT_ROOT/.build/reference"
find gcc-lane-1 gcc-lane-2 clang-lane-1 clang-lane-2 -type f \
  \( -name 'Image' -o -name 'vmlinux' -o -name 'System.map' -o -name '.config' \
     -o -name 'artifact-digests.tsv' -o -name 'kbuild-v1.log' -o -name 'boot-*.log' \) \
  | sort | xargs /usr/bin/shasum -a 256 > artifact-manifest.sha256
cd "$PROJECT_ROOT"

bash tests/acceptance/nexlab/run.sh
