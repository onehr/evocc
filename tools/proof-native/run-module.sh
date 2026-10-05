#!/bin/bash
# Assemble, link and run one case of tools/proof-native/proof_native.py in the
# existing sealed Linux AArch64 image (evo-flight-lab:sealed, Apple Container
# runtime), as the native tests in crates/evocc/tests/n03d_*.rs do.
#
# Run:  bash tools/proof-native/run-module.sh WORK_DIR/run/CASE
#
# The directory comes from `proof_native.py prepare`. It holds output.s (the
# checked text), driver.c, jobs.txt, symbols.txt and src/ (copies of the
# fixture C files). The script writes native.txt, native-llvm.txt, tools.txt,
# text.txt and subject.dis there. objcopy changes only the symbol table: it
# makes static entries global and renames main to subject_main.
set -euo pipefail
work=$(cd "$1" && pwd)
free_gib=$(df -k "$work" | awk 'NR==2 { print int($4 / 1048576) }')
if [ "$free_gib" -lt 25 ]; then
    echo "refused: free disk space ${free_gib} GiB is below 25 GiB" >&2
    exit 3
fi
exec /usr/local/bin/container run --rm --platform linux/arm64 --network none \
    --cpus 4 --memory 2G --volume "$work:/work" evo-flight-lab:sealed bash -c '
set -euo pipefail
cd /work
entries=$(sed -n "s/^ENTRIES //p" symbols.txt)
statics=$(sed -n "s/^STATIC //p" symbols.txt)
fixture=$(sed -n "s/^FIXTURE //p" symbols.txt)
{
    uname -srm
    gcc-14 --version | head -n 1
    as --version | head -n 1
    ld --version | head -n 1
    objcopy --version | head -n 1
    clang-19 --version | head -n 1
    ld.lld-19 --version | head -n 1
} > tools.txt
rm -f subject.o subject-llvm.o ref.o driver.o probe probe-llvm native.txt native-llvm.txt
# The checked text, assembled by GNU as (through gcc-14) and by clang-19.
gcc-14 -c output.s -o subject.o
clang-19 --target=aarch64-linux-gnu -c output.s -o subject-llvm.o
objcopy -O binary --only-section=.text subject.o text-gnu.bin
objcopy -O binary --only-section=.text subject-llvm.o text-llvm.bin
if cmp -s text-gnu.bin text-llvm.bin; then same=SAME; else same=DIFFERENT; fi
echo "text_bytes=$(stat -c %s text-gnu.bin) gnu_vs_llvm=$same" \
    "relocations_gnu=$(readelf -r subject.o | grep -c R_AARCH64 || true)" \
    "relocations_llvm=$(readelf -r subject-llvm.o | grep -c R_AARCH64 || true)" > text.txt
rm -f text-gnu.bin text-llvm.bin
objdump -d subject.o > subject.dis
fix=()
for s in $statics; do fix+=("--globalize-symbol=$s"); done
for s in $entries; do [ "$s" = main ] && fix+=(--redefine-sym main=subject_main); done
if [ ${#fix[@]} -gt 0 ]; then
    objcopy "${fix[@]}" subject.o
    objcopy "${fix[@]}" subject-llvm.o
fi
# The cross-check: the same C file compiled by gcc-14 -O0, symbols renamed ref_NAME.
gcc-14 -O0 -fno-builtin -w -c "src/$fixture.c" -o ref.o
globalize=(); rename=()
for s in $entries; do globalize+=("--globalize-symbol=$s"); rename+=(--redefine-sym "$s=ref_$s"); done
objcopy "${globalize[@]}" ref.o
objcopy "${rename[@]}" ref.o
gcc-14 -O0 -fno-builtin -c driver.c -o driver.o
gcc-14 driver.o subject.o ref.o -o probe
clang-19 -fuse-ld=lld driver.o subject-llvm.o ref.o -o probe-llvm
./probe < jobs.txt > native.txt
./probe-llvm C < jobs.txt > native-llvm.txt
rm -f probe probe-llvm subject.o subject-llvm.o ref.o driver.o
wc -l native.txt native-llvm.txt
'
