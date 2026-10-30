# N03d C3: repeated integer extensions

Clean subject `2debb15c`; RED `3afaf704`; plan `14352b59`; measurement plans
`705869e0`; EvoMIR 0.63. [Plan](../../../tests/contracts/codegen-n03d-extensions-20261003.json),
[receipt](../../../tests/contracts/codegen-n03d-c3-20261003-receipt.json).
`--isel-opt=extensions` reuses exact block-local Extend, low Bfx and low-mask
AND results before allocation. Unique definitions, local non-edge uses and
effect fences are required. No new opcode, range inference, interpreter,
allocator, verifier, ABI or EqSat registry semantics. Defaults remain OFF.

**Closed opt-in, small benefit.** Full workspace **3,051/0/5 existing ignored**,
396 suites; Clippy, format and regeneration PASS. Focused Rust: 12 MIR/adapter
and four driver tests PASS; Python: 90+4 PASS. Direct MIR native coverage is
48 Darwin functions x 1,024 inputs; all 48 also assemble for Linux. Public
Linux narrow-argument calls run 65,536 inputs across two functions and three
variants against GCC14 and Clang19. Both native corpora execute and agree
**384/384 OFF and ON**; object twins match, all 768 OFF hashes match the retained
baseline, and ON changes zero corpus objects. Six frozen A/B captures pass
72 original selected-TU executions and 72 object-twin checks, with zero proof
rejection/bundle failure. All IR/chain/bundle bindings and 257 step rows match T3.

B/unchecked `.text` bytes; cumulative is frame+C1+copy+reload+extensions,
**excluding C2**. C3's incremental changes are solely extension/mask bytes,
not frame, P0, constants or spill savings. Checked rows are separate in the receipt.

| TU | OFF | C3 only | C3 saving | Cumulative | Increment over T3 | Cumulative/GCC-O2 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 0 | 128 | 0 | 1.333 |
| zlib deflate | 28,668 | 28,660 | 8 | 25,260 | 8 | 1.750 |
| zlib inflate | 26,520 | 26,516 | 4 | 24,628 | 4 | 2.478 |
| zstd compress | 120,592 | 120,592 | 0 | 96,356 | 0 | 1.976 |
| zstd decompress | 33,052 | 33,052 | 0 | 23,796 | 0 | 1.642 |
| cJSON | 29,384 | 29,384 | 0 | 24,592 | 0 | 1.708 |

All **48/48** matched size/cost comparisons meet the frozen bounds; maximum
compile-median ratio **1.107x**, ceiling 1.25x. No rodata/padding/pool increase.
Only **70** step inputs are TV-admitted; **187** remain opaque, and the legacy
1001/1002 source-event mismatch remains NOT_VALIDATED. All five libraries miss
1.4x. Three new pattern verdicts are finite-tested, **not SMT-proved**; N07 E3's
generator limitation and its historical inventory remain unchanged. No default,
full-application, kernel, latency, general instruction-selection or verified-EqSat claim.

The initial native driver test wrongly retargeted Linux signed-char IR to
Darwin: OFF and both ON paths returned 1920 for -128, versus Clang's -1920.
It remains **INVALID_CROSS_TARGET_PROBE, not PASS**. No production semantics or
expected result was changed; the replacement uses the supported Linux driver.
This does not establish a general Darwin C frontend ABI bridge. An optional
broad `cargo check` was interrupted; the complete gate above supplies acceptance.

**Disk:** 792 hash-checked idle intermediate objects retired (1,832,336 logical
bytes; 4,440,064 allocated). Exact intermediate runtimes are no longer retained;
current OFF/cumulative baselines, prior T3 baselines and all compact evidence
and minimal reproducers remain. Shared cache +318,644,224 bytes and retained
evidence +6,725,632 give scoped net **+325,369,856 bytes**, excluding tracked
files/Git and unrelated activity; about 32 GiB free. Next: a bounded disposition
of the remaining compare/control opportunities; C3 does not close unexamined
work or relax the 1.4 target.
