# N03c T2: conservative copy-location hints

Clean implementation `bb2f3b5f`; RED `e5a9361f`; plan `5a477d38`; EvoMIR 0.60.
[Plan](../../../tests/contracts/codegen-n03c-copy-hints-20261003.json),
[receipt](../../../tests/contracts/codegen-n03c-t2-20261003-receipt.json).
`--regalloc-opt=copy-hints` prefers an already assigned same-class copy peer's
location only after the original whole-interval conflict checks. Pools, fixed
and tied constraints, spills, T1's retry bound and the verifier are unchanged.
Protected profiles and every default remain OFF.

**Closed opt-in.** Full workspace **3,028/0/5 ignored**, 393 suites; Clippy,
format and regeneration PASS. Focused allocator/ABI/SSA/adapter tests: 69 PASS;
public-driver tests: 3 PASS, including 4,096 native inputs across six functions
and OFF/hint/cumulative variants against Clang. Eight schedule tests PASS.
Both native corpora execute/agree **384/384 ON and OFF**, with byte-identical
twins. All 768 OFF hashes match T1; ON changes none of these corpus objects.
Six A/B captures each pass 12 original checked/unchecked harness executions,
with 72 identical object pairs and zero proof rejection/bundle failure.
All six per-arm IR/chain/bundle bindings match T1. No full applications rerun.

B/unchecked `.text` bytes. Cumulative means frame+C1+copy, **excluding C2**.
The copy-only reduction is entirely N01's `move` category, including checked
and A controls; stack/frame and `.rodata` do not change in that comparison.

| TU | OFF | Copy only | Copy saving | Cumulative | Cumulative/GCC-O2 |
| --- | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 0 | 128 | 1.333 |
| zlib deflate | 28,668 | 28,624 | 44 | 25,272 | 1.751 |
| zlib inflate | 26,520 | 26,120 | 400 | 24,728 | 2.488 |
| zstd compress | 120,592 | 119,964 | 628 | 96,416 | 1.977 |
| zstd decompress | 33,052 | 32,904 | 148 | 23,808 | 1.643 |
| cJSON | 29,384 | 29,340 | 44 | 24,592 | 1.708 |

All **48/48** matched size/cost comparisons meet the frozen limits: no text
or text+rodata growth; maximum compile-median ratio **1.021x** versus 1.25x.
All 257 step/output records are unchanged, but only 70 are TV-admitted;
187 remain opaque, and integer-mix's legacy 1001/1002 source-event mismatch
remains NOT_VALIDATED. Earlier T1/C2 cost misses remain frozen. All five
libraries still miss 1.4x; this is not verified EqSat gain, a complete
allocator, ISA proof, kernel/latency acceptance or permission to change defaults.

An initial fixture assertion incorrectly expected the whole file's move count
to fall: its Linux assembly had 28 OFF versus 29 ON moves. `choose` loses one,
while wide call preparation adds two. The retained negative log motivated a
local-hit assertion plus native value checks, not a weakened six-TU size bound.

**Disk:** retired 1,608 hash-checked idle intermediate objects (4,750,144 bytes).
Exact historical runtime objects are no longer retained; logs, manifests,
receipts, classification summaries and reduced fixtures remain. Since T1's
ending snapshot, shared-cache growth is 226,525,184 allocated bytes and owned
evidence +4,255,744: scoped net **+230,780,928 bytes**, excluding tracked files
and unrelated activity. Free space is about 91 GiB.
