# N03d C4: residual compare/control census

Clean subject `2ce2385c`; plan `757c92ea`; RED `14007276`.
[Plan](../../tests/contracts/codegen-n03d-control-20261003.json),
[receipt](../../tests/contracts/codegen-n03d-c4-20261003-receipt.json).
Read-only N01/S1 analysis of 48 retained OFF/cumulative A/B object rows,
checked and unchecked: 41 unique objects decoded, category totals reconcile.
No object rebuild, whole disassembly retention or timing run.

**Positive preflight, not implemented savings.** Full workspace: **3,051 PASS,
0 FAIL, 5 existing ignored**, 396 suites; Clippy, format and regeneration PASS.
Python: **16/16**; eight prior REDs failed on the missing measurement entry point.
Current cumulative B/unchecked (C2 excluded):

| TU | .text bytes | Compare/select bytes | Branch bytes | Boolean branch candidates | Compare combinations |
| --- | ---: | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 8 | 16 | 0 | 0 |
| deflate | 25,260 | 2,252 | 3,672 | 10 | 0 |
| inflate | 24,628 | 1,528 | 3,860 | 15 | 0 |
| zstd compress | 96,356 | 9,956 | 13,780 | 72 | 22 |
| zstd decompress | 23,796 | 2,512 | 3,692 | 10 | 0 |
| cJSON | 24,592 | 2,788 | 4,536 | 2 | 3 |

All four arms and both profiles have the same **109 branch / 25 combination**
syntactic counts; Boolean retest selects zero (27 raw shapes fail register
links). These counts do not prove virtual-register liveness or equivalence.
All 25 combinations were inspected: 22 unsigned zstd and three signed cJSON
inclusive intervals on one value, with ordered nonnegative imm12 endpoints.
Existing SUB/CMP/CSET can express a bounded follow-up; no general CCMP opcode
is needed for this measured population.

The retained `deflateGetDictionary` IR explains one branch opportunity:
four unused uninitialized definitions follow ICmp, preventing the existing
source-adjacency fusion. Shared baseline DCE later drops their zero
materializations, but branch selection is not retried. The object retains
CMP/CSET/TBZ. Existing compared-value CSEL, immediate comparisons, checked
flag branches and fallthrough selection remain baseline, not C4 gains.

Next is a separately frozen C5 exact-MIR slice with unique-use and adjacent
flags guards. **N03d remains open.** New optimized bytes are **zero** here;
no compiler, interpreter, registry, ABI, flags-lifetime or default change.
General conditional comparisons and CFG if-conversion are outside this census,
not proved unnecessary. C3's native/application evidence remains historical.

**Disk:** C4 retains **413,696 allocated bytes** of summaries, bounded examples,
scripts and logs; shared `target/frontend` is unchanged at 25,195,312 KiB.
Scoped net **+413,696 bytes**, excluding tracked files/Git and unrelated host
activity; about 33 GiB free at closure review. Existing baselines remain intact.
