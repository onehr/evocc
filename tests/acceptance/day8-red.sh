#!/usr/bin/env bash
set -u

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/evocc-day8-red.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
cd "$repo_root"

passed=0
total=0

run_gate() {
  local name="$1"
  shift
  total=$((total + 1))
  if "$@" >"$scratch/$name.log" 2>&1; then
    passed=$((passed + 1))
    printf 'DAY8_RED_GATE %s PASS\n' "$name"
  else
    printf 'DAY8_RED_GATE %s FAIL\n' "$name"
    tail -n 20 "$scratch/$name.log"
  fi
}

layering_contract() {
  local tree
  tree="$(cargo tree -p evocc-evoir --prefix none --offline)" || return 1
  if printf '%s\n' "$tree" | grep -Eq '^evocc-egraph([[:space:]]|$)'; then
    printf '%s\n' 'evocc-evoir still depends on evocc-egraph'
    return 1
  fi
}

run_gate layering layering_contract
run_gate syntax \
  cargo test -p evocc --offline --test day8_contract_red \
  day8_section9_contract_is_canonical_and_complete -- --exact
run_gate persistent-state \
  cargo test -p evocc-eqopt --offline --test day8_persistent_state_contract
run_gate proof-boundary \
  cargo test -p evocc-proofcheck --offline --test day8_proof_contract
run_gate storage-calls-effects \
  cargo test -p evocc --offline --test day8_contract_red \
  day8_storage_calls_effects_contract_is_semantically_equal -- --exact

printf 'DAY8_RED_SUMMARY %d/%d gates pass\n' "$passed" "$total"
test "$passed" -eq "$total"
