# N03d C5: bounded integer control selection

Implementation `d71b40c4`; clean measured subject/plans `21edc5e8`; RED
`d46e2210`; work plan `b9ce3937`; EvoMIR 0.64.
[Plan](../../tests/contracts/codegen-n03d-controls-20261003.json),
[receipt](../../tests/contracts/codegen-n03d-c5-20261003-receipt.json).
`--isel-opt=controls` selects two adjacent private-use integer idioms before
unchanged allocation: terminal CMP/CSET/Boolean branch, and a same-input
inclusive nonnegative imm12 interval. No motion, new operation, interpreter,
ABI, fact lattice, verifier, registry or general CCMP/CFG transformation.

**Closed with compile-cost misses, not admitted.** Full workspace
**3,065/0/5 existing ignored**, 398 suites; Clippy, format and regeneration
PASS. Focused Rust **19 PASS**, Python **60 PASS**. Native focused tests cover
304 MIR shapes / 608 before-after Darwin functions x 1,024 inputs (all also
assemble for Linux); public Linux C tests cover 65,536 inputs x three functions
x three variants against GCC14/Clang19. Both 384-case corpora execute and agree
OFF/ON; all 768 OFF hashes match C3, and ON changes zero corpus objects.
Actual pattern coverage comes from the focused tests, not those no-op objects.
Six frozen captures pass **72** selected-TU executions and **72** object twins;
zero completed proof rejections/bundle failures. All six IR/chain/bundle bindings
and 257 step rows per capture match C3. One sandbox-denied Linux startup executed
no cases: its failure log remains separate from the authorized native PASS.

B/unchecked `.text`; measured cumulative = frame+C1+extensions+copy+reload+C5,
**excluding C2**. All incremental savings classify as compare/select bytes,
not frame, P0, guard, DCE or spill savings. A/B and checked rows are in the receipt.

| TU | OFF | C5 only | Saving | Measured cumulative | Increment over C3 | /GCC-O2 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 0 | 128 | 0 | 1.333 |
| zlib deflate | 28,668 | 28,628 | 40 | 25,220 | 40 | 1.748 |
| zlib inflate | 26,520 | 26,460 | 60 | 24,568 | 60 | 2.472 |
| zstd compress | 120,592 | 120,040 | 552 | 95,804 | 552 | 1.965 |
| zstd decompress | 33,052 | 33,012 | 40 | 23,756 | 40 | 1.639 |
| cJSON | 29,384 | 29,340 | 44 | 24,548 | 44 | 1.705 |

All **48/48** text/text+rodata bounds pass; **45/48** compile-median bounds pass.
The frozen 1.25x ceiling is missed by A/cumulative unchecked zstd-compress
**1.391x**, A/cumulative checked cJSON **1.627x**, and B/C5-only checked inflate
**1.332x**. No resampling or causal claim about timing noise. Prior C3 remains
the admitted cumulative configuration; C5 and C2 are excluded, defaults OFF.
C4 predicted 109 branch + 25 interval windows; C5 selects 134 branches (including
25 interval results) and 25 intervals in B/unchecked. Those are producer counts,
not proofs. The total 736-byte saving is small, not a general optimizer result.

Only **70** step inputs are admitted; **187** remain opaque and legacy 1001/1002
source-event mismatch remains NOT_VALIDATED. Five library ratios still miss 1.4x.
Patterns are finite-tested, not SMT-proved; N07 E3's generator limitation stays
frozen. No default, full-application, kernel, latency or verified-EqSat claim.
The bounded N03d sequence closes here; broader transformations need new designs.

**Disk:** retired 792 hash-checked idle objects (1,829,464 logical bytes;
4,440,064 allocated), preserving all records/reproducers and current/prior
baselines. Exact intermediate runtimes are no longer retained. Shared cache
+231,657,472 bytes and retained evidence +6,623,232 give scoped net
**+238,280,704 bytes** (227.24 MiB), excluding tracked files/Git and unrelated
activity; **47.6 GiB free**. No new target tree, VM disk or disassembly dump.
