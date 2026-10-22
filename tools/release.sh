#!/bin/sh
# Build and package one EvoCC release archive on the current host.
#
# Usage: tools/release.sh [--target TRIPLE] [--out DIR] [--with-lean] [--skip-build]
#
#   --target TRIPLE  The release target: aarch64-apple-darwin or
#                    aarch64-unknown-linux-gnu. The default is the host. A build
#                    must run on its target, because the script runs evocc and
#                    builds evoir-sem there.
#   --out DIR        The output directory. The default is target/dist.
#   --with-lean      Rebuild evoir-sem even if lean/.lake/build/bin/evoir-sem exists.
#   --skip-build     Build nothing. Package the binaries that exist.
#
# The script builds evo, evocc and evocc-checker with Cargo, offline, in
# target/frontend using the dist profile. The dist profile removes the debug map
# and remaps the checkout, the Rust sysroot and the target directory. On Linux
# it builds them for aarch64-unknown-linux-musl, so they are static. That needs
# `rustup target add aarch64-unknown-linux-musl`. Without it the script builds
# for glibc and warns. evoir-sem comes from Lake and links glibc dynamically on
# Linux in all cases.
#
# The version is the second word of `evocc --version`. If that fails, the
# version comes from the package manifest or its inherited workspace version.
#
# The output is DIR/evocc-<version>-<target>.tar.gz. The archive holds one
# directory of the same name: bin/ with the four binaries, LICENSE, NOTICE,
# THIRD-PARTY-NOTICES.md, README.md and SHA256SUMS for those files. It also
# holds licenses/runtime/: the license texts of musl, libuv and GMP, which the
# binaries contain. Each text must have the SHA-256 that this script pins, or
# no archive is written. The archive also includes sysroot/ when the exported
# Linux headers exist: the headers, the license texts of their Debian packages
# in sysroot/licenses/ and the contract.
# Every header and license text must match the tracked contract and is listed
# in SHA256SUMS. The archive is deterministic. Entries are sorted, owner and
# group are 0, and every mtime is SOURCE_DATE_EPOCH, else the commit time of
# HEAD. The gzip header has no name and no time. The script also merges the
# archive digest into DIR/SHA256SUMS.
set -eu
START="$PWD"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

die() { echo "release: $*" >&2; exit 2; }
warn() { echo "release: warning: $*" >&2; }

target=""
out="target/dist"
with_lean=false
skip_build=false
while [ $# -gt 0 ]; do
    case "$1" in
        --target) [ $# -ge 2 ] || die "--target needs a value"; target="$2"; shift 2 ;;
        --target=*) target="${1#--target=}"; shift ;;
        --out) [ $# -ge 2 ] || die "--out needs a value"; out="$2"; out_given=true; shift 2 ;;
        --out=*) out="${1#--out=}"; out_given=true; shift ;;
        --with-lean) with_lean=true; shift ;;
        --skip-build) skip_build=true; shift ;;
        -h|--help) sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"; exit 0 ;;
        *) die "unknown option: $1" ;;
    esac
done
# A relative --out is relative to the caller. The default is relative to the root.
if [ "${out_given:-false}" = true ]; then
    case "$out" in /*) ;; *) out="$START/$out" ;; esac
fi
if [ "$skip_build" = true ] && [ "$with_lean" = true ]; then
    die "--skip-build builds nothing, so --with-lean has no effect; give one of them"
fi

case "$(uname -s)/$(uname -m)" in
    Darwin/arm64) host="aarch64-apple-darwin" ;;
    Linux/aarch64|Linux/arm64) host="aarch64-unknown-linux-gnu" ;;
    *) host="unsupported" ;;
esac
[ -n "$target" ] || target="$host"
case "$target" in
    aarch64-apple-darwin|aarch64-unknown-linux-gnu) ;;
    *) die "unsupported target: $target" ;;
esac
if [ "$target" != "$host" ] && [ "$skip_build" = false ]; then
    die "the host is $host; build $target on its own host, or give --skip-build"
fi

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/frontend}"
case "$CARGO_TARGET_DIR" in
    /*) ;;
    *) CARGO_TARGET_DIR="$ROOT/$CARGO_TARGET_DIR" ;;
esac
export CARGO_TARGET_DIR
musl="aarch64-unknown-linux-musl"

# The Rust triple: musl on Linux when its standard library or its build exists.
rust_triple="$target"
if [ "$target" = "aarch64-unknown-linux-gnu" ]; then
    if [ "$skip_build" = true ]; then
        [ -x "$CARGO_TARGET_DIR/$musl/dist/evocc" ] && rust_triple="$musl"
    else
        libdir="$(rustc --print target-libdir --target "$musl" 2>/dev/null || true)"
        if [ -n "$libdir" ] && ls "$libdir"/libstd-*.rlib >/dev/null 2>&1; then
            rust_triple="$musl"
        fi
    fi
    if [ "$rust_triple" != "$musl" ]; then
        warn "no $musl build or standard library; evocc and evocc-checker link glibc dynamically"
    fi
fi
if [ "$rust_triple" = "$host" ]; then
    bin_dir="$CARGO_TARGET_DIR/dist"
    triple_arg=""
else
    bin_dir="$CARGO_TARGET_DIR/$rust_triple/dist"
    triple_arg="--target=$rust_triple"
fi
lean_bin="lean/.lake/build/bin/evoir-sem"

if [ "$skip_build" = false ]; then
    echo "== cargo build --profile dist ($rust_triple) =="
    rust_sysroot="$(rustc --print sysroot)"
    rust_commit="$(rustc -vV | sed -n 's/^commit-hash: //p' | head -n 1)"
    [ -n "$rust_commit" ] || rust_commit=unknown
    RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$ROOT=/build --remap-path-prefix=$rust_sysroot=/rustc/$rust_commit --remap-path-prefix=$CARGO_TARGET_DIR=/target"
    export RUSTFLAGS
    # shellcheck disable=SC2086 # triple_arg is empty or one word.
    CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo build --profile dist --offline --locked \
        $triple_arg -p evocc -p evocc-proofcheck
    if [ "$with_lean" = true ] || [ ! -x "$lean_bin" ]; then
        echo "== lake build evoir-sem ($(cat lean/lean-toolchain)) =="
        (cd lean && lake build evoir-sem)
        lean_built=true
    fi
fi

for file in "$bin_dir/evo" "$bin_dir/evocc" "$bin_dir/evocc-checker" "$lean_bin"; do
    [ -x "$file" ] || die "missing binary: $file"
done
for file in "$bin_dir/evo" "$bin_dir/evocc" "$bin_dir/evocc-checker" "$lean_bin"; do
    for path in "$ROOT" "$CARGO_TARGET_DIR"; do
        [ -n "$path" ] || continue
        if LC_ALL=C grep -a -F -q -- "$path" "$file"; then
            die "binary $file contains build path: $path"
        fi
    done
    if [ "$file" != "$lean_bin" ] && [ -n "$HOME" ] &&
        LC_ALL=C grep -a -F -q -- "$HOME" "$file"; then
        die "binary $file contains build path: $HOME"
    fi
done
if [ "${lean_built:-false}" = false ] &&
    [ -n "$(find lean -path lean/.lake -prune -o -name '*.lean' -newer "$lean_bin" -print | head -n 1)" ]; then
    warn "$lean_bin is older than a source in lean/; give --with-lean to rebuild it"
fi

version=""
if [ "$target" = "$host" ]; then
    version="$("$bin_dir/evocc" --version 2>/dev/null | sed -n 's/^evocc \([0-9][0-9A-Za-z.+-]*\).*/\1/p' | head -n 1 || true)"
fi
if [ -z "$version" ]; then
    version="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' crates/evocc/Cargo.toml | head -n 1)"
    [ -n "$version" ] || version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)"
    [ -n "$version" ] || die "no version from evocc --version or the package/workspace manifests"
    warn "evocc --version gave no version; using $version from the package/workspace manifests"
fi

# A package version must follow the current release policy and be documented.
printf '%s\n' "$version" | grep -Eq '^0\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' \
    || die "version must use 0.y.z: $version"
[ -f CHANGELOG.md ] || die "changelog is missing: CHANGELOG.md"
awk -v version="$version" 'index($0, "## [") == 1 && $2 == "[" version "]" { found = 1 }
    END { exit !found }' CHANGELOG.md \
    || die "changelog has no heading for version $version"

name="evocc-$version-$target"
mtime="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || echo 0)}"
mkdir -p "$out"
set -- "bin/evo=755=$bin_dir/evo" "bin/evocc=755=$bin_dir/evocc" "bin/evocc-checker=755=$bin_dir/evocc-checker" \
    "bin/evoir-sem=755=$lean_bin" "README.md=644=README.md"
for doc in LICENSE NOTICE THIRD-PARTY-NOTICES.md; do
    if [ -f "$doc" ]; then
        set -- "$@" "$doc=644=$doc"
    else
        warn "$doc is absent; the archive does not include it"
    fi
done

echo "== package $name.tar.gz =="
python3 - "$out" "$name" "$mtime" "$@" <<'PY'
import gzip, hashlib, io, json, sys, tarfile
from pathlib import Path

out, name, mtime = Path(sys.argv[1]), sys.argv[2], int(sys.argv[3])
entries = []
for spec in sys.argv[4:]:
    arcname, mode, path = spec.split("=", 2)
    entries.append((arcname, int(mode, 8), Path(path).read_bytes()))
# The license texts of the third-party code inside the binaries.
# THIRD-PARTY-NOTICES.md, section 1, gives the source of each text.
runtime = {
    "gmp/gpl-3.0.txt": "3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986",
    "gmp/lgpl-3.0.txt": "e3a994d82e644b03a792a930f574002658412f62407f5fee083f2555c5f23118",
    "libuv/LICENSE": "16de0c32b265cb7d46a6d3bd614f259dd4d693a5e26b3407b04aae8d73041f0c",
    "libuv/LICENSE-extra": "262c44bd2cdba037e6d2a82fba15f5800d292bc993a6f5d6b6ea487744d02836",
    "libuv/inet.c-notice": "94c9ea5ff8756fbd01e307e0a3d44f2a0369df9d7ddc86abb278790e4a63ba0d",
    "libuv/tree.h-notice": "05504284dee75c47d712644e7b06c6c1b1ebbaee5d03dd660646716c8be3e5c3",
    "musl/COPYRIGHT": "f9bc4423732350eb0b3f7ed7e91d530298476f8fec0c6c427a1c04ade22655af",
}
tree = Path("licenses/runtime")
found = {path.relative_to(tree).as_posix(): path.read_bytes()
         for path in tree.rglob("*") if path.is_file()}
if {path: hashlib.sha256(data).hexdigest() for path, data in found.items()} != runtime:
    raise SystemExit("release: licenses/runtime/ differs from the pinned license texts; "
                     "no archive written")
entries.extend((f"licenses/runtime/{path}", 0o644, data) for path, data in sorted(found.items()))
sysroot = Path("sysroot/aarch64-unknown-linux-gnu")
if sysroot.exists():
    contract_path = Path("contracts/sysroot-aarch64-unknown-linux-gnu.json")
    contract = json.loads(contract_path.read_text())
    # The headers, and the license texts of the Debian packages that own them.
    for tree, pinned, kind in ((sysroot, contract["files"], "headers"),
                               (Path("sysroot/licenses/aarch64-unknown-linux-gnu"),
                                contract.get("licenses"), "header licenses")):
        found = {path.relative_to(tree).as_posix(): path.read_bytes()
                 for path in tree.rglob("*") if path.is_file()}
        actual = {name: hashlib.sha256(data).hexdigest() for name, data in found.items()}
        if not pinned or actual != pinned:
            raise SystemExit(f"release: Linux {kind} differ from their contract; no archive written. "
                             "Run tools/export-sysroot.sh again into an empty directory")
        entries.extend((f"{tree.as_posix()}/{path}", 0o644, data)
                       for path, data in sorted(found.items()))
    entries.append(("sysroot/aarch64-unknown-linux-gnu.json", 0o644, contract_path.read_bytes()))
sums = "".join(f"{hashlib.sha256(data).hexdigest()}  {arc}\n"
               for arc, _, data in sorted(entries))
entries.append(("SHA256SUMS", 0o644, sums.encode()))

def info(arcname, mode, size, kind=tarfile.REGTYPE):
    item = tarfile.TarInfo(arcname)
    item.type, item.mode, item.size, item.mtime = kind, mode, size, mtime
    item.uid = item.gid = 0
    item.uname = item.gname = ""
    return item

archive = out / f"{name}.tar.gz"
with open(archive, "wb") as handle, \
        gzip.GzipFile(filename="", mode="wb", fileobj=handle, mtime=0, compresslevel=9) as gz, \
        tarfile.open(fileobj=gz, mode="w", format=tarfile.USTAR_FORMAT) as tar:
    tar.addfile(info(name, 0o755, 0, tarfile.DIRTYPE))
    for arc, mode, data in sorted(entries):
        tar.addfile(info(f"{name}/{arc}", mode, len(data)), io.BytesIO(data))

digest = hashlib.sha256(archive.read_bytes()).hexdigest()
listing = out / "SHA256SUMS"
lines = {}
if listing.exists():
    for line in listing.read_text().splitlines():
        if "  " in line:
            value, file = line.split("  ", 1)
            lines[file] = value
lines[archive.name] = digest
listing.write_text("".join(f"{lines[f]}  {f}\n" for f in sorted(lines)))
print(sums, end="")
print(f"{digest}  {archive.name}")
PY
tar -tvzf "$out/$name.tar.gz"
echo "== wrote $out/$name.tar.gz and $out/SHA256SUMS =="
