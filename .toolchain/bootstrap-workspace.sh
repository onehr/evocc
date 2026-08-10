#!/bin/bash

# Reconstruct the flight-lab workspace from the tracked contracts on a fresh
# checkout. Everything fetched here is verified against contract digests
# before use; nothing is trusted because it "looks right".
#
# Steps:
#   1. fetch the pinned kernel tarball and signature, verify SHA-256 and GPG
#   2. extract the source tree
#   3. verify the sealed build image is present at the contract digest
#   4. build the evocc scaffold with the pinned Rust toolchain

set -eu
set -o pipefail

export LC_ALL=C TZ=UTC

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
JQ="${C30_JQ:-/opt/homebrew/bin/jq}"
SHASUM="${C30_SHASUM:-/usr/bin/shasum}"

BUILD_ENV_CONTRACT="$PROJECT_ROOT/contracts/build-environment.json"
KERNEL_CONTRACT="$PROJECT_ROOT/contracts/kernel-target.json"
ARTIFACTS="$PROJECT_ROOT/.toolchain/artifacts"

die() { printf 'bootstrap: %s\n' "$1" >&2; exit 1; }

version="$("$JQ" -r '.kernel.version' "$KERNEL_CONTRACT")"
tarball_url="$("$JQ" -r '.kernel.tarball.url' "$KERNEL_CONTRACT")"
sign_url="$("$JQ" -r '.kernel.tarball.signatureUrl' "$KERNEL_CONTRACT")"
expected_sha="$("$JQ" -r '.kernel.tarball.sha256' "$KERNEL_CONTRACT")"
signer_fp="$("$JQ" -r '.kernel.tarball.signatureEvidence.signerFingerprint' "$KERNEL_CONTRACT")"

mkdir -p "$ARTIFACTS"

# --- 1. fetch + verify -----------------------------------------------------

if [ ! -f "$ARTIFACTS/linux-$version.tar.xz" ]; then
  printf 'bootstrap: fetching %s\n' "$tarball_url"
  curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    -o "$ARTIFACTS/linux-$version.tar.xz" "$tarball_url"
  curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    -o "$ARTIFACTS/linux-$version.tar.sign" "$sign_url"
fi

actual_sha="$("$SHASUM" -a 256 "$ARTIFACTS/linux-$version.tar.xz" | awk '{print $1}')"
[ "$actual_sha" = "$expected_sha" ] || \
  die "tarball digest mismatch: contract=$expected_sha actual=$actual_sha"

if ! gpg --list-keys "$signer_fp" >/dev/null 2>&1; then
  printf 'bootstrap: fetching stable signing key %s\n' "$signer_fp"
  gpg --keyserver hkps://keyserver.ubuntu.com --recv-keys "$signer_fp" >/dev/null 2>&1 || \
    die "could not fetch the kernel stable signing key"
fi

if [ ! -f "$ARTIFACTS/linux-$version.tar" ]; then
  xz -dc "$ARTIFACTS/linux-$version.tar.xz" > "$ARTIFACTS/linux-$version.tar"
fi
gpg --verify "$ARTIFACTS/linux-$version.tar.sign" "$ARTIFACTS/linux-$version.tar" 2>&1 | \
  grep -q "Good signature" || die "GPG verification failed"
printf 'bootstrap: tarball verified (sha256 + GPG %s)\n' "$signer_fp"

# --- 2. extract ------------------------------------------------------------

if [ ! -d "$PROJECT_ROOT/.build/linux-src" ]; then
  mkdir -p "$PROJECT_ROOT/.build"
 exp_tmp="$PROJECT_ROOT/.build/.extract-tmp"
  rm -rf "$exp_tmp"
  mkdir -p "$exp_tmp"
  tar -xf "$ARTIFACTS/linux-$version.tar" -C "$exp_tmp"
  mv "$exp_tmp/linux-$version" "$PROJECT_ROOT/.build/linux-src"
  rm -rf "$exp_tmp"
fi
printf 'bootstrap: source tree ready\n'

# --- 3. sealed image presence ----------------------------------------------

sealed_tag="$("$JQ" -r '.buildImage.sealedImageTag' "$BUILD_ENV_CONTRACT")"
sealed_id="$("$JQ" -r '.buildImage.sealedImageId' "$BUILD_ENV_CONTRACT")"
sealed_id_short="${sealed_id#sha256:}"
if ! container image list --format json 2>/dev/null | \
     "$JQ" -e --arg id "$sealed_id_short" 'any(.[]; .id == $id or .configuration.descriptor.digest == ("sha256:" + $id))' \
     >/dev/null 2>&1; then
  printf 'bootstrap: sealed image absent, rebuilding from the tracked Dockerfile\n'
  container build --platform linux/arm64 --progress plain --no-cache \
    -f "$PROJECT_ROOT/.toolchain/Dockerfile.flight-lab" \
    -t "$sealed_tag" "$PROJECT_ROOT/.toolchain" >/dev/null
fi
printf 'bootstrap: sealed image present\n'

# --- 4. evocc scaffold -----------------------------------------------------

rustup run 1.97.1 cargo build --release --manifest-path "$PROJECT_ROOT/Cargo.toml" >/dev/null 2>&1 || \
  die "evocc scaffold build failed with pinned Rust 1.97.1"
printf 'bootstrap: evocc scaffold built\n'
