# N03c T3: local resident spill reloads

Clean subject `f79373da`; RED `e44ed429`; plan `22d8e3d1`; EvoMIR 0.62.
[Plan](../../tests/contracts/codegen-n03c-reload-reuse-20261003.json),
[receipt](../../tests/contracts/codegen-n03c-t3-20261003-receipt.json).
`--regalloc-opt=reload-reuse` removes only a repeated logical-slot reload
whose exact-class value is still in the same physical register. Unique
definitions and local non-edge uses are required. Physical clobbers, stores,
calls, assembly, ordinary memory and reserved scratch prevent reuse. No
interval, allocation choice, spill policy or independent verifier changes.

**Closed opt-in; N03c's bounded T1/T2/T3 work is dispositioned.** Full workspace
**3,039/0/5 existing ignored**, 394 suites; Clippy, format and regeneration
PASS. Seven focused suites: 73 PASS; public-driver tests: 4 PASS, including
4,096 native inputs across seven functions and five EvoCC variants against
Clang. Python: 48 PASS. Both native corpora execute/agree **384/384 ON and
OFF**, with identical object twins; all 768 OFF hashes match E2, and ON
changes no corpus object. The six A/B captures pass 72 original selected-TU
harness executions and 72 object-twin checks, with zero proof rejection or
bundle failure. IR/chain/bundle bindings and 257 step records match T2.

B/unchecked `.text` bytes. Cumulative is frame+C1+copy+reload, **excluding C2**.
Reload-only and cumulative increments over T2 are entirely stack-access bytes;
no new frame, move, constant, padding, literal-pool or `.rodata` reduction.

| TU | OFF | Reload only | Reload saving | Cumulative | Increment over T2 | Cumulative/GCC-O2 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 0 | 128 | 0 | 1.333 |
| zlib deflate | 28,668 | 28,664 | 4 | 25,268 | 4 | 1.751 |
| zlib inflate | 26,520 | 26,424 | 96 | 24,632 | 96 | 2.478 |
| zstd compress | 120,592 | 120,528 | 64 | 96,356 | 60 | 1.976 |
| zstd decompress | 33,052 | 33,040 | 12 | 23,796 | 12 | 1.642 |
| cJSON | 29,384 | 29,384 | 0 | 24,592 | 0 | 1.708 |

All **48/48** matched size/cost comparisons meet the frozen bounds; maximum
compile-median ratio **1.232x** versus 1.25x. Only **70** step inputs are
TV-admitted; **187** stay opaque, and integer-mix's legacy 1001/1002 event
mismatch stays NOT_VALIDATED. All five libraries still miss 1.4x. In
particular, cumulative inflate still has **7,300 stack-access bytes**: the
96-byte local reduction does not close its broader spill-quality gap. No
global allocator, universal backend proof, verified-EqSat gain, full-app,
kernel, latency or default admission is claimed. T1/C2 cost misses and N07's
negative SMT result remain frozen; no further allocator redesign in this slice.

One release-tool command used the wrong package for the existing `corpus`
example; Cargo refused it. The corrected `evocc-passes` build passed before
measurement. Both logs remain; no measurement or compiler result was rerun.

**Disk:** retired 816 hash-checked idle objects (3,087,376 logical bytes);
exact historical runtimes are no longer retained, but current OFF/cumulative
baselines and all compact evidence remain. Shared-cache growth +249,446,400
allocated bytes, new evidence +6,598,656, retired prior E2 allocation
−1,294,336: scoped net **+254,750,720 bytes**, excluding tracked files and
unrelated activity. Free space about **53 GiB**. Next: bounded N03d selection.
