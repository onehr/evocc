#!/bin/sh
# Run the workspace gate only for the crates a change touched, plus every crate
# that depends on them. Unchanged crates keep their last green run; their test
# binaries are not rebuilt, so macOS does not re-assess them at launch either.
#
# Usage: tools/gate-changed.sh <base-commit> [extra cargo test args...]
#        tools/gate-changed.sh --full-workspace [extra cargo test args...]
# R01a/R02 acceptance requires --full-workspace, not a selected-crate gate.
#   base-commit   the last commit that had a full green gate (usually main's)
#
# Prints the crate set it chose, then runs: cargo test -p <set>, cargo clippy
# --all-targets for the same set, and cargo fmt --check for the workspace.
# A workspace Cargo manifest/lock or Rust toolchain change runs every crate. Other changes
# outside crates also select consumers of Rust include!/include_str!/include_bytes!
# inputs, including nested includes. Unrelated external changes run no cargo
# tests; run the relevant Python tests or renderer check by hand. A nonliteral
# include cannot be resolved here, so external changes conservatively select its
# owner instead of silently skipping a possible dependency.
set -eu
BASE="${1:?base commit required}"; shift || true
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
python3 tools/check-spec-pins.py
if [ "$BASE" = "--full-workspace" ]; then
    jobs_value=false
    for option in "$@"; do
        if [ "$jobs_value" = true ]; then
            case "$option" in ''|*[!0-9]*) echo "invalid full-workspace job count" >&2; exit 2;; esac
            jobs_value=false
        else
            case "$option" in
                --offline|--locked|--frozen|-j[0-9]*|--jobs=*) ;;
                -j|--jobs) jobs_value=true ;;
                *) echo "full-workspace acceptance forbids selection/filter option: $option" >&2; exit 2 ;;
            esac
        fi
    done
    if [ "$jobs_value" = true ]; then echo "missing job count" >&2; exit 2; fi
    python3 tools/generate-target.py --check
    cargo test --workspace --no-fail-fast "$@"
    cargo clippy --workspace --all-targets -- -D warnings
    cargo fmt --all -- --check
    echo "gate-changed: full workspace green"
    exit 0
fi
python3 tools/generate-target.py --check
changed="$(git diff --name-only "$BASE"...HEAD; git status --short | awk '{print $2}')"
crates="$(python3 - "$changed" <<'PY'
import json
from pathlib import Path
import re
import sys

root = Path.cwd()
paths = {Path(path) for path in sys.argv[1].splitlines() if path}
owners = {path.parts[1] for path in paths
          if len(path.parts) > 2 and path.parts[0] == "crates"}
external = {path.resolve() for path in paths if path.parts[0] != "crates"}
invocation = re.compile(r'\b(include_str|include_bytes|include)\s*!\s*([({\[])\s*')
literal = re.compile(r'r(?P<hashes>#{0,255})"(?P<raw>.*?)"(?P=hashes)|"(?:[^"\\]|\\.)*"', re.S)
closing = {"(": ")", "{": "}", "[": "]"}

def uses_changed_input(source, visited):
    source = source.resolve()
    if source in visited:
        return False
    visited.add(source)
    try:
        contents = source.read_text(encoding="utf-8")
    except (OSError, UnicodeError):
        # A missing/unreadable nested Rust include must not conceal dependencies.
        return True
    for call in invocation.finditer(contents):
        value = literal.match(contents, call.end())
        if value is None:
            return True
        tail = contents[value.end():].lstrip()
        if tail.startswith(","):
            tail = tail[1:].lstrip()
        if not tail.startswith(closing[call[2]]):
            return True
        try:
            name = value["raw"] if value["raw"] is not None else json.loads(value[0])
            dependency = (source.parent / name).resolve()
        except (ValueError, OSError):
            # Unhandled Rust escapes/expressions over-select, never under-select.
            return True
        if dependency in external:
            return True
        if call[1] == "include" and uses_changed_input(dependency, visited):
            return True
    return False

if external:
    for crate in sorted((root / "crates").iterdir()):
        if crate.is_dir() and crate.name not in owners:
            visited = set()
            if any(uses_changed_input(source, visited) for source in crate.rglob("*.rs")):
                owners.add(crate.name)
print("\n".join(sorted(owners)))
PY
)"
if printf '%s\n' "$changed" | grep -qE '^(Cargo\.(toml|lock)|rust-toolchain(\.toml)?)$'; then crates="$(ls crates)"; echo "workspace manifest/toolchain changed: testing every crate"; fi
if [ -z "$crates" ]; then echo "no crate changed since $BASE; nothing to test"; exit 0; fi
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
