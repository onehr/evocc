#!/bin/bash
# Miri conformance: run selected unsafe-free crates under Miri to detect
# undefined behavior. The arena allocator is the primary target because it
# does raw pointer arithmetic.
set -eu
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"

cd "$PROJECT_ROOT"

printf '=== arena (unsafe) miri conformance ===\n'
cargo +nightly miri test -p evocc-common -- \
  arena 2>&1 | grep -E '(running|test result)' | head -5

printf 'arena-miri-conformance: PASS\n'
