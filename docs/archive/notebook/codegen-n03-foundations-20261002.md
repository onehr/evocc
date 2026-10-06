# N03a preparation: original object baselines

Implementation: `856dfcd3`; A/B plans: `eff2235e`; retained-g0 plan: `4a03735f`.
Plan: [N03a](../../../tests/contracts/codegen-n03a-20261002.json).
Receipt: [baseline capture](../../../tests/contracts/codegen-n03a-baseline-20261002-receipt.json).
This checkpoint is preparation, not acceptance of a backend refactor.

| Gate | Evidence / limit |
| --- | --- |
| Corpus pre-link capture RED | Missing helper/interface; compile exit 101 |
| Corpus capture focused gate | 12 filesystem/interface tests PASS; both native harnesses compile |
| S5 retained-object RED | Six tests fail on absent optional interfaces; identity-after-emission-failure regression reproduced separately |
| S5 collector | Existing Capture/S1 path, optional A identity chain, retained first object and full-byte comparison; defaults unchanged |
| Darwin original corpus | 384 objects, 384 executions agreeing with reference; 384 byte-identical build pairs |
| Sealed Linux original corpus | 384 objects, 384 executions agreeing with GCC; 384 byte-identical build pairs |
| Six-TU A/B | 24 checked/unchecked object pairs identical; zero completed proof rejection or bundle failure |
| Retained g0 IR | 12 object pairs identical; 10 standalone current-checker certificate replays exit 0 |
| Full workspace attempt | 379 suites; 2,938 PASS / 0 FAIL / 4 ignored; Clippy FAIL on capture-test unwrap; full recheck pending |

Object `.text + .text.*` bytes (checked / unchecked):

| TU | A | B | Retained-g0 control |
| --- | ---: | ---: | ---: |
| integer-mix | 128 / 128 | 128 / 128 | 128 / 128 |
| zlib deflate | 30,516 / 29,012 | 30,184 / 28,668 | 29,960 / 28,668 |
| zlib inflate | 28,404 / 26,664 | 28,264 / 26,520 | 28,232 / 26,520 |
| zstd compress | 124,024 / 121,652 | 122,972 / 120,592 | 121,988 / 120,592 |
| zstd decompress | 33,688 / 33,440 | 33,300 / 33,052 | 33,300 / 33,052 |
| cJSON | 30,228 / 30,116 | 29,496 / 29,384 | 29,468 / 29,384 |

The initial g0 attempt failed before checker execution because the chain binder
requires the historical checker identity. The corrected frozen procedure replays
each original certificate directly with the current checker; it does not rewrite
or re-admit the historical chain under a different identity. This retained-IR
control is not fresh source/application acceptance or a new guard saving.

Complete native populations must pass before baseline publication. Only first
objects remain for cross-revision byte comparison; native scratch, duplicate
objects and assembly are removed. Default capture behavior is unchanged. The
separate N03b/e census has no transformation or size claim.

**Claim ceiling:** pre-refactor baseline only; cross-revision identity and N03a
layering remain pending. Existing P0/g0 differences are not N03 gains.
**Disk:** retained native objects 576,736 bytes; A/B captures 2,043,349 bytes;
g0 capture 687,452 bytes. Shared Cargo cache delta +866,865,152 bytes,
recorded once for the joint preparation checkpoint. The Clippy failure is
preserved; its explicit `if let` repair is rechecked with step 1, not waived.
