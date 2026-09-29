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

Initial status: **NOT RUN**. Commit this protocol before collection. The
frozen population has no AI candidates and authorizes no paid search.

## Remaining R02 implementation

- Performance manifests with actual source/preprocessed/data, build/link and
  effective UB/lowering identities; object provenance and fresh result predicates.
- Native metric collection, result validation for each sample, A/A calibration,
  interleaving/statistics and original application adapters.
- EqSat-off identity chains and common verified-IR checked/unchecked fan-out;
  the latter still requires the R02d backend profile.
- Full R02b application cap table and selected-pair, rule and shortcut censuses.
  This collector leaves all three track verdicts NOT_RUN.
