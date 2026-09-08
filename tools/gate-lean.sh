#!/bin/sh
# The Lean proof layer's own gate. It is deliberately separate: nothing in the
# kernel path, the workspace gate or the ladder depends on it, and a failure
# here does not fail a compiler build. Section 1 of docs/lean-plan.md fixes
# that boundary; this script is the whole of the Lean lane's checking.
#
# Usage: tools/gate-lean.sh
#
# Four steps, in order:
#   1. regenerate lean/EvoIR/Registry.lean from the specification block, which
#      refuses to run unless the block hashes to the digest stated beside it,
#      and confirm the checked-in file is what the generator produces;
#   2. run the generator's own Python tests;
#   3. lake build, on the pinned toolchain;
#   4. the evaluator self-test;
#   5. the differential vectors, this side only: the Rust runners write
#      <name>.rust.jsonl beside these files and tools/lean/diff.py compares
#      them; here the named vectors are checked against their own expected
#      values, which needs no second runner;
#   6. the six-step audit and its receipt.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== 1. registry generator =="
python3 tools/lean/registry_to_lean.py --check

echo "== 2. generator tests =="
python3 -m unittest discover -s tools/lean/tests -p 'test_*.py' -t tools/lean/tests

echo "== 3. lake build =="
cd "$ROOT/lean"
echo "toolchain: $(cat lean-toolchain)"
lean --version
start=$(date +%s)
lake build
end=$(date +%s)
echo "lake build wall time: $((end - start))s"

echo "== 4. evaluator self-test =="
lake exe evoir-sem --selftest

echo "== 5. differential vectors, this side =="
cd "$ROOT"
python3 tools/lean/vectors.py
cd "$ROOT/lean"
for name in transfers-small lattice-small values-width8 random named; do
  lake exe evoir-sem "vectors/$name.jsonl" "vectors/$name.lean.jsonl"
done
cd "$ROOT"
python3 tools/lean/diff.py --expect lean/vectors/named.lean.jsonl

echo "== 6. audit and receipt =="
python3 tools/lean/audit.py

echo "gate-lean: all steps passed"
