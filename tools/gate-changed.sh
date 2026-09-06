#!/bin/sh
# Run the workspace gate only for the crates a change touched, plus every crate
# that depends on them. Unchanged crates keep their last green run; their test
# binaries are not rebuilt, so macOS does not re-assess them at launch either.
#
# Usage: tools/gate-changed.sh <base-commit> [extra cargo test args...]
#   base-commit   the last commit that had a full green gate (usually main's)
#
# Prints the crate set it chose, then runs: cargo test -p <set>, cargo clippy
# --all-targets for the same set, and cargo fmt --check for the workspace.
# A change outside any crate (tools/, docs/, tests/contracts/) runs no cargo
# tests; run the relevant Python tests or renderer check by hand.
set -eu
BASE="${1:?base commit required}"; shift || true
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
changed="$(git diff --name-only "$BASE"...HEAD; git status --short | awk '{print $2}')"
crates="$(printf '%s\n' "$changed" | sed -n 's|^crates/\([^/]*\)/.*|\1|p' | sort -u)"
if [ -z "$crates" ]; then echo "no crate changed since $BASE; nothing to test"; exit 0; fi
if printf '%s\n' "$changed" | grep -qE '^(Cargo\.(toml|lock))$'; then crates="$(ls crates)"; echo "workspace manifest changed: testing every crate"; fi
# reverse dependencies inside the workspace, via cargo metadata
set_="$(python3 - "$crates" <<'PY'
import json, subprocess, sys
changed = set(sys.argv[1].split())
meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], capture_output=True, text=True, check=True).stdout)
names = {p["name"] for p in meta["packages"]}
deps = {p["name"]: {d["name"] for d in p["dependencies"] if d["name"] in names} for p in meta["packages"]}
result = set(c for c in changed if c in names)
grew = True
while grew:
    grew = False
    for name, ds in deps.items():
        if name not in result and ds & result:
            result.add(name); grew = True
print(" ".join(sorted(result)))
PY
)"
echo "testing crates: $set_"
pkgs=""; for c in $set_; do pkgs="$pkgs -p $c"; done
cargo test $pkgs --no-fail-fast "$@"
cargo clippy $pkgs --all-targets -- -D warnings
cargo fmt --all -- --check
echo "gate-changed: green for [$set_] against $BASE"
