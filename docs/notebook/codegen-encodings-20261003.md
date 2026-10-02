# N07 E1: encoding facts and object-preserving migration

Implementation `4b945e9e`; RED `cdd3c01e`; plan `c9c37f5f`, completed-baseline
binding `2b534efe`; EvoMIR 0.61. [Plan](../../tests/contracts/codegen-n07-encodings-20261003.json),
[receipt](../../tests/contracts/codegen-n07-e1-20261003-receipt.json).
Existing immediate, memory/pair offset, mov-wide, logical-mask, shift and
TBZ/ADR reach facts now come from `targets/aarch64/encodings`. Public
`BinaryImm` validation rejects malformed forms already excluded by lowering.
Selection order, frame/relaxation policy, interpreters and defaults do not change.

**Closed, behavior-preserving.** Full workspace **3,032/0/5 ignored**, 394
suites; Clippy, format and regeneration PASS. Focused Rust **52 PASS** and
Python **45 PASS**. Independent logical-mask enumeration covers 1,302 W and
5,334 X masks; each of the ELF and Mach-O assembler lanes round-trips **20,142
instructions / 80,568 text bytes**, rejects 21 malformed forms and accepts
the four selected TBZ/ADR edge forms. These are encoding oracles, not ISA proofs.

Darwin and sealed Linux each execute/agree **384/384**, with identical object
twins and full-byte equality to the pre-migration objects. Six S5 TUs × A/B ×
checked/unchecked give **24 unchanged objects**, each built twice; all **24**
original harness executions pass. Independent replay has **zero completed
rejections and zero bundle failures**. All six IR/chain/bundle bindings per arm
and 257 step records are unchanged. Only 70 inputs are TV-admitted; 187 remain
opaque and the legacy integer-mix source-event mismatch remains NOT_VALIDATED.

Every number below is unchanged before → after; `.text` bytes, no new benefit.

| TU | A checked | A unchecked | B checked | B unchecked | Delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 128 | 128 | 0 |
| zlib deflate | 30,516 | 29,012 | 30,184 | 28,668 | 0 |
| zlib inflate | 28,404 | 26,664 | 28,264 | 26,520 | 0 |
| zstd compress | 124,024 | 121,652 | 122,972 | 120,592 | 0 |
| zstd decompress | 33,688 | 33,440 | 33,300 | 33,052 | 0 |
| cJSON | 30,228 | 30,116 | 29,496 | 29,384 | 0 |

**Claim ceiling:** encoding legality and unchanged existing behavior only.
Vendor-model execution and SMT remain N07 E2/E3; no default promotion, kernel,
latency, full-application, size-improvement or whole-backend proof claim.

**Disk:** retired 24 hash-checked idle duplicate objects (1,255,664 bytes).
Exact historical before-TU runtime objects are no longer retained; manifests,
receipts and logs remain. Keep 24 current TU and 768 native objects as the sole
open N07 migration baselines. Shared-cache growth 52,756,480 bytes plus owned
evidence 7,364,608 gives scoped net **+60,121,088 bytes**; excludes tracked files
and unrelated volume activity. Free space about 74 GiB. A no-op generator
rewrites unchanged files and touches Cargo inputs; the observed churn is
queued for E2's tooling fix, not hidden by a reduced workspace gate.
