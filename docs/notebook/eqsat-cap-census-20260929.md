# EqSat cap census: diagnostic collector checkpoint

This is an implementation step toward R02a/R02b, not closure of either task.
R01a landed in `6cae306`. The remaining application preflight still requires
fresh correctness admission, post-lowering opportunity inventory, selected-pair
traces, separate policy/rule/shortcut verdicts and native materialization.

## Frozen collector smoke

[Manifest](../../tests/contracts/eqsat-cap-smoke-20260929.json): 32 **previously
exposed regression** seeds, 9300 through 9331, argument seed 0, 200,000 interpreter
steps, default instruction-count policy, current v4 registry. The four coupled
limits remain search 100,000 bridge nodes / 1,024 rewrite events and admission
100,000 proof nodes / 1,024 regions; chain limit eight transactions. This is
generated EvoIR, not C lowering, original application execution or held-out data.
Do not infer default-cap behavior for zlib/zstd/Lua from it.

The actual evaluator/checker binaries and generator source are hash-pinned.
The runner requires the manifest bytes to be committed before execution,
uses the existing `.build/bench.lock`, refuses subject drift and missing cases,
and bounds execution to 180 seconds with an 8 MiB combined stdout/stderr limit
and at least 1 GiB free disk. Capture files are independently limited to half
the output allowance. Attempts are exclusive directories, never old-receipt
replacement. Interrupted and failed started attempts remain recorded.

```sh
python3 tools/bench.py --eqsat-cap-smoke tests/contracts/eqsat-cap-smoke-20260929.json
python3 tools/bench.py --eqsat-census PATH/TO/policy.json
```

The second command only validates/reduces retained **producer telemetry**; it
does not replay proof bundles. During the first command the pinned evaluator
uses R01a's shared independently checked chain and compares interpreted
observations before/after every transaction. Neither command is native or
performance admission. Missing/rejected chains and execution failures have
unknown cap-hit results, not zero hits. Exit-2 malformed bundles remain separate
from exit-3 proof rejection; matcher work remains null/NOT_MEASURED.

The protocol was committed as `33e49db` **before** collection. The frozen
population has no AI candidates and authorizes no paid search.

## Observed collector smoke

Capture `.evidence/eqsat-cap-smoke/20260929T045519Z-1790657719371114000` completed:
**32/32 interpreted observations agreed; 74/74 transaction proofs replayed**.
No undefined input, interpreter fuel stop or refused chain was reported.
Thirty-one programs changed IR; the 1,480 retained regions are summed across
transaction inputs, not unique-source coverage or native instruction savings.

| Scope | Default limit | Largest transaction observation | Programs hitting cap |
| --- | ---: | ---: | ---: |
| Transient bridge nodes | 100,000 | 560 | 0/32 |
| Transient rewrite events | 1,024 | 121 | 0/32 |
| Admitted regions | 1,024 | 103 | 0/32 |
| Admitted proof nodes | 100,000 | 617 | 0/32 |
| Chain transactions | 8 | See per-program retained record | 0/32 |

Maximum recorded EVP firing fuel was 33 (bound 256), maximum epoch marker 5,
and maximum before-witness `n` 14. Actual matcher work remains NOT_MEASURED.
The result is **NO_HITS_IN_THIS_POPULATION**, not FINITE_ENUMERATION_SUFFICIENT
or NO_OBSERVABLE_EFFECT. All three track preflight verdicts remain NOT_RUN.

The complete raw report and collector receipt are retained in
[the fixture directory](../../tests/fixtures/certificates/cap-census-20260929/).
All capture files together occupy 224,038 bytes. The collector/bench/chain/TODO
Python gate passed 93 tests before this run. A new imported-CLI test initially
exposed a module-path error; fixing package selection made both direct CLI and
imported entry-point tests pass. Failure tests use synthetic child processes,
not forged compiler performance evidence.

## Remaining R02 implementation

- Performance manifests with actual source/preprocessed/data, build/link and
  effective UB/lowering identities; object provenance and fresh result predicates.
- Native metric collection, result validation for each sample, A/A calibration,
  interleaving/statistics and original application adapters.
- Common verified-IR checked/unchecked fan-out; the unchecked backend profile
  still requires R02d. The EqSat-off identity-chain prerequisite is described below.
- Full R02b application cap table and selected-pair, rule and shortcut censuses.
  This collector leaves all three track verdicts NOT_RUN.

## EqSat-off identity-chain prerequisite

`--eq-proof-dir=NEW_DIR` without `-Oeq-scalar` now exports an explicit identity
chain. The driver validates and renders the same module it then sends to
interpretation or codegen; it does not rerun lowering or enable EqSat. Existing
lowering optimizations and source flags are unchanged. Only EvoIR emission,
assembly emission and EvoIR interpretation accept the export option.

Canonical `evocc-identity-chain-v1` records bind identical `input.evo` and
`output.evo` bytes, zero attempted transactions and an empty transaction list.
The publication label is `IDENTITY_VERIFIED`, deliberately distinct from an EVP
rewrite proof. The shared exclusive-directory publisher refuses overwrite.

```sh
target/frontend/debug/evocc --emit=evoir -O2 --eq-proof-dir=NEW_DIR source.c
python3 tools/bench.py --verify-eqsat-chain NEW_DIR
```

The independent consumer checks byte identity and record hashes, rejects changed
IR, a fake EVP artifact, nonzero transaction counts, unknown fields and a supplied
checker. It reports one identity check and **zero proof successes**. Optimized
chains still require the standalone checker; changing a schema cannot bypass
the equality-of-bytes requirement. Identity verification is not a C correctness,
native-code or performance claim. R02's future manifest must additionally bind
the declared variant/source and actual codegen input; the identity record alone
does not establish those outer provenance claims.

Tests compare actual driver IR/assembly output with and without export, replay
both identity and optimized chains, mutate each load-bearing identity field,
and reject inactive flags and existing destinations. The Darwin ARM64 native
driver test also lowers/runs the exact identity IR alongside the optimized IR.
This closes the identity-record prerequisite, not R02a or checked/unchecked
native twins.

The final identity/chain/census/bench/TODO Python gate passed **100 tests**;
the Rust verified-chain/driver gate passed **16 tests**, including both Darwin
native executions. Three affected packages passed all-target Clippy with
`-D warnings`; formatting, whitespace and 261 local inline Markdown links passed.
These are overlapping regression gates, not a new application population.
An actual CLI identity export is retained at `.evidence/eqsat-identity-20260929`;
its independent audit reported one identity check and zero proof successes.
