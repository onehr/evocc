#!/bin/bash

# Reference kernel builder for the flight lab.
#
# Builds the pinned kernel with two independent toolchains, twice each, inside
# the sealed --network=none image. Each lane must produce byte-identical
# declared artifacts across its two clean rebuilds; that identity is the
# reproducibility evidence, not an after-the-fact narrative.
#
# Lanes:
#   gcc-lane-1, gcc-lane-2     CC=gcc-14, GNU binutils
#   clang-lane-1, clang-lane-2 clang-19 + ld.lld-19 + llvm-* (no GNU compiler)
#
# Each lane family has its own frozen .config because Kconfig evaluates
# compiler-capability conditions; within a family the post-build .config must
# remain byte-identical to the frozen record.
#
# Provenance: every build runs with V=1 and keeps the full Kbuild command
# stream at kbuild-v1.log in the lane output directory, so every object can be
# traced to source path, argv, and compiler digest.

set -eu
set -o pipefail

export LC_ALL=C TZ=UTC
umask 022

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"

BUILD_ENV_CONTRACT="$PROJECT_ROOT/contracts/build-environment.json"
KERNEL_CONTRACT="$PROJECT_ROOT/contracts/kernel-target.json"

SRC_HOST="$PROJECT_ROOT/.build/linux-src"
OUT_ROOT_HOST="$PROJECT_ROOT/.build/reference"

LANES="gcc-lane-1 gcc-lane-2 clang-lane-1 clang-lane-2"
DECLARED_ARTIFACTS="arch/arm64/boot/Image vmlinux System.map"

die() { printf 'build-reference: %s\n' "$1" >&2; exit 1; }

[ -d "$SRC_HOST" ] || die "kernel source tree missing: $SRC_HOST"

lane_family() {
  case "$1" in
    gcc-lane-*) printf 'gcc' ;;
    clang-lane-*) printf 'clang' ;;
    *) die "unknown lane $1" ;;
  esac
}

# Per-lane toolchain selection. The Clang lane uses the full LLVM tool family
# so the two lanes share no compiler binary at all.
lane_make_vars() {
  case "$1" in
    gcc-lane-*)
      printf '%s' "CC=gcc-14"
      ;;
    clang-lane-*)
      printf '%s' "CC=clang-19 LD=ld.lld-19 AR=llvm-ar-19 NM=llvm-nm-19 OBJCOPY=llvm-objcopy-19 OBJDUMP=llvm-objdump-19 STRIP=llvm-strip-19"
      ;;
    *) die "unknown lane $1" ;;
  esac
}

# --- Contract re-verification: refuse to build from drifted inputs ---------

expected_tarball_sha="$("$JQ" -r '.kernel.tarball.sha256' "$KERNEL_CONTRACT")"
actual_tarball_sha="$("$SHASUM" -a 256 "$PROJECT_ROOT/.toolchain/artifacts/linux-6.18.44.tar.xz" | awk '{print $1}')"
[ "$actual_tarball_sha" = "$expected_tarball_sha" ] || \
  die "kernel tarball drifted: contract=$expected_tarball_sha actual=$actual_tarball_sha"

for family in gcc clang; do
  record="$PROJECT_ROOT/.toolchain/records/frozen-kernel-$family.config"
  [ -f "$record" ] || die "frozen $family config missing: $record"
  expected_sha="$("$JQ" -r ".config.lanes.$family.rawSha256" "$KERNEL_CONTRACT")"
  actual_sha="$("$SHASUM" -a 256 "$record" | awk '{print $1}')"
  [ "$actual_sha" = "$expected_sha" ] || \
    die "frozen $family config drifted: contract=$expected_sha actual=$actual_sha"
done

sealed_image="$("$JQ" -r '.buildImage.sealedImageTag' "$BUILD_ENV_CONTRACT")"
kbuild_ts="$("$JQ" -r '.reproducibility.KBUILD_BUILD_TIMESTAMP' "$BUILD_ENV_CONTRACT")"
sde="$("$JQ" -r '.reproducibility.SOURCE_DATE_EPOCH' "$BUILD_ENV_CONTRACT")"
build_user="$("$JQ" -r '.reproducibility.KBUILD_BUILD_USER' "$BUILD_ENV_CONTRACT")"
build_host="$("$JQ" -r '.reproducibility.KBUILD_BUILD_HOST' "$BUILD_ENV_CONTRACT")"

run_lane() {
  lane="$1"
  family="$(lane_family "$lane")"
  frozen_config="$PROJECT_ROOT/.toolchain/records/frozen-kernel-$family.config"
  frozen_config_sha="$("$JQ" -r ".config.lanes.$family.rawSha256" "$KERNEL_CONTRACT")"
  out_host="$OUT_ROOT_HOST/$lane"
  rm -rf "$out_host"
  mkdir -p "$out_host"
  cp "$frozen_config" "$out_host/.config"

  make_vars="$(lane_make_vars "$lane")"
  # The container CLI writes [N/6] progress lines on stdout; probes must
  # filter for the expected output shape.
  ncpu="$(container run --rm --platform linux/arm64 --network none -c 10 -m 24G "$sealed_image" nproc 2>/dev/null | grep -E '^[0-9]+$' | tail -1)"

  printf '== lane %s (%s): clean build (jobs=%s) ==\n' "$lane" "$family" "$ncpu"
  # V=1 keeps the full Kbuild command stream as provenance. olddefconfig with
  # the frozen per-lane config must leave the file byte-identical: the probe
  # environment in the lane equals the probe environment at freeze time.
  container run --rm --platform linux/arm64 --network none \
    -c 10 -m 24G \
    -v "$SRC_HOST:/work/src:ro" \
    -v "$out_host:/work/out" \
    -w /work/out \
    "$sealed_image" \
    sh -c "
      set -eu
      umask 022
      export KBUILD_BUILD_TIMESTAMP='$kbuild_ts' SOURCE_DATE_EPOCH='$sde' \
             KBUILD_BUILD_USER='$build_user' KBUILD_BUILD_HOST='$build_host'
      make -C /work/src O=/work/out ARCH=arm64 olddefconfig $make_vars
      make -C /work/src O=/work/out ARCH=arm64 -j\"$ncpu\" V=1 $make_vars Image vmlinux 2>&1 | tee /work/out/kbuild-v1.log | tail -5
    " > "$out_host/build-console.log" 2>&1 || {
      printf 'lane %s FAILED; console tail:\n' "$lane" >&2
      tail -20 "$out_host/build-console.log" >&2
      exit 1
    }

  # Byte-identical config check: same compiler probing means any byte
  # difference is drift or nondeterminism, both of which fail the lane.
  post_config_sha="$("$SHASUM" -a 256 "$out_host/.config" | awk '{print $1}')"
  [ "$post_config_sha" = "$frozen_config_sha" ] || \
    die "lane $lane .config diverged from the frozen $family record (build=$post_config_sha frozen=$frozen_config_sha)"

  # Record declared artifact digests for this lane.
  : > "$out_host/artifact-digests.tsv"
  for artifact in $DECLARED_ARTIFACTS .config; do
    sha="$("$SHASUM" -a 256 "$out_host/$artifact" | awk '{print $1}')"
    size="$(stat -f '%z' "$out_host/$artifact")"
    printf '%s\t%s\t%s\n' "$artifact" "$sha" "$size" >> "$out_host/artifact-digests.tsv"
  done
  printf '== lane %s complete ==\n' "$lane"
  cat "$out_host/artifact-digests.tsv"
}

for lane in $LANES; do
  run_lane "$lane"
done

# --- Same-lane byte-identity verdict ---------------------------------------

identity_ok=1
for pair in "gcc-lane-1 gcc-lane-2" "clang-lane-1 clang-lane-2"; do
  set -- $pair
  for artifact in $DECLARED_ARTIFACTS .config; do
    sha_a="$(awk -v a="$artifact" '$1 == a {print $2}' "$OUT_ROOT_HOST/$1/artifact-digests.tsv")"
    sha_b="$(awk -v a="$artifact" '$1 == a {print $2}' "$OUT_ROOT_HOST/$2/artifact-digests.tsv")"
    if [ "$sha_a" != "$sha_b" ] || [ -z "$sha_a" ]; then
      printf 'MISMATCH %s vs %s: %s\n' "$1" "$2" "$artifact" >&2
      identity_ok=0
    fi
  done
done

if [ "$identity_ok" -eq 1 ]; then
  printf 'REPRODUCIBLE: same-lane declared artifacts are byte-identical\n'
else
  printf 'NOT REPRODUCIBLE: see mismatches above\n' >&2
  exit 1
fi
