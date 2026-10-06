# N03a result: behavior-preserving backend layering

Implementation: `42a003d2` (register description), `cb65735d`, `a1717dc1`,
`16cc9b2b` (instruction descriptors, SSA destruction, allocation and verifier),
`efcf8c57` (ABI tables, argument cursor, frame driver; RED `cc02556e`).
[Plan](../../../tests/contracts/codegen-n03a-20261002.json);
[receipt](../../../tests/contracts/codegen-n03a-20261002-receipt.json); step records
[1](codegen-n03a-regfile-20261002.md) and [2](codegen-n03a-machine-20261002.md).
`targets/aarch64/{regfile,insts,abi-darwin,abi-linux,profile-kernel}` generate
every migrated table; `src/generic/` holds SSA destruction, linear scan, spill
placement, slot compaction, the independent verifier, the argument cursor and
the frame driver, and names no ISA. Selection, the AArch64 aggregate/HFA/HVA/
variadic classifier, prologue/epilogue and assembly printing stay per-ISA.

| Check at `efcf8c57` | Observed result |
| --- | --- |
| Step-3 RED | Toy-ABI, ABI-binding and generic tests failed on absent interfaces (E0432); boundary test failed on a generic `sp` comment and 17 handwritten table sites; 13 generator tests errored |
| Focused Rust / Python | 180 tests in 21 suites, incl. 23-shape × 2-OS placement snapshot; Python 46/46; Clippy, format, regeneration PASS |
| Synthetic ISA | 12 forms; cursor/frame toy ABI (3+1 registers, 4-byte slots, 32-byte alignment); bank exhaustion, view-alias, clobber and forged-assignment rejections |
| Six original TUs, A and B, checked/unchecked | 24/24 object twins and complete original-byte comparisons identical |
| Retained g0 IR control | 12/12 identical; 10/10 standalone certificate replays |
| Darwin original corpus | 384/384 objects identical; 384 executed, 384 agree |
| Sealed Linux original corpus | 384/384 objects identical; 384 executed, 384 agree |
| Full workspace gate | 388 suites: 2,986 PASS / 0 FAIL / 4 ignored; Clippy, format, regeneration PASS |

All measured object deltas are **zero** at every step; no allocation-verifier
verdict changed and no proof rejection or bundle failure occurred.

**Observed, deliberately unchanged.** (1) A toy ABI whose argument registers
equal the whole 3-register pool made the shared allocator's spill retry grow
without settling (killed at 8.8 GB RSS); the test shape was changed. AArch64
has 8 argument and 26 allocatable registers. (2) `abi-darwin` records the
current, unmeasured Darwin rules as they are applied: AAPCS64 variadic save
areas, refused general stack arguments, unrefused vector stack placement.
(3) binary128 and strict-integer128 qualification remain OS checks in lowering.
(4) Prologue, epilogue and dynamic-stack printers keep their AArch64 16-byte
immediates. (5) The boundary test covers `evocc-mir/src/generic` only, not
Layer-1 crates.

**Claim ceiling:** a behavior-preserving three-layer backend for the existing
AArch64 Darwin and Linux targets on the stated population. Not a new target,
proved backend, optimization gain, ABI correction or N07 steps 4–6.
**Disk:** the 804 retained baseline objects (2,453,416 bytes) were removed once
every comparison closed; their hashes stay in the baseline receipts and logs.
Over this session the shared target grew 991,354,880 bytes and `.evidence`
shrank 2,605,056 bytes (net +988,749,824); free space is about 135 GiB.
