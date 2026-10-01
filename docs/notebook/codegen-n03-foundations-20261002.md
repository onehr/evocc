# N03 preparation: original objects and read-only opportunity counts

Plans: [N03a](../../tests/contracts/codegen-n03a-20261002.json),
[A/B baselines](../../tests/contracts/codegen-n03a-baseline-a-20261002.json),
[N03b](../../tests/contracts/codegen-n03b-20261002.json),
[N03e](../../tests/contracts/codegen-n03e-20261002.json).
This checkpoint is preparation, not acceptance of any N03 transformation.

| Preparation | Evidence / limit |
| --- | --- |
| Corpus pre-link capture RED | Missing helper/interface; compile exit 101 |
| Corpus capture focused gate | 12 filesystem/interface tests PASS; both native harnesses compile |
| S5 retained-object RED | Six tests fail on absent optional interfaces; identity-after-emission-failure regression reproduced separately |
| S5 collector | Existing Capture/S1 path, optional A identity chain, retained first object and full-byte comparison; defaults unchanged |
| Opportunity scanner RED | Four tests reach the unimplemented scanner; earlier fixture parse errors are not compiler defects |
| Source retention RED | Existing `used` declaration incorrectly reported as new-inline permission; complete-chain `used`/`retain` now conservatively decline |
| Native objects / full workspace | NOT RUN for this checkpoint; no baseline or refactor acceptance inferred from helper tests |

Capture mode requires all 384 existing scalar/memory IDs, actual native and
reference outcomes, and two complete identical objects per case. Baselines are
published only after the entire population completes, never overwritten, and
retained only until cross-revision comparisons close. Default native tests keep
their original behavior. Six-TU collection reuses the N05 path; A/B and profiles
remain distinct and certified g0 stays a separate control.

The opportunity scanner does not transform IR. It distinguishes structural
candidates from whole-module/input admission. Original preprocessed source must
reproduce the driver's exact A IR before its declaration-chain permissions are
used; current B is counted separately. Raw generated IR cannot invent missing
source attributes. Opaque modules, absent execution inputs and the existing
volatile-initializer bridge gap remain excluded. A zero admitted population
closes the frozen slice negatively, not as an implemented optimizer.

**Claim ceiling:** helper correctness and read-only census preparation only;
no code-size improvement, native-corpus completion or N03a layering claim.
**Disk:** no native capture yet; test-owned scratch is removed. Shared Cargo
cache growth is reported at the next gate checkpoint, not assigned to each lane.
