# N08 to N11 analysis: block transport, records, inlining and return signing

This note answers the analysis questions of the performance batch: TODO N08,
N10, N09 and N11. It finds the causes of the measured excess in the code and
gives the Rust lane an ordered plan. It changes no code, specification,
receipt or task file.

## Base, scope and evidence

- Base: commit `d362bde0`. The cited parts of `crates/evocc-mir`,
  `crates/evocc-chir`, `crates/evocc-lower`, `lean/EvoIR/Alloc`,
  `lean/EvoIR/Asm`, `lean/EvoIR/MachineTransport*.lean`, `spec/evomir.md`
  (1.2.1) and `spec/verification.md` (1.1.1) had no uncommitted edits when
  read. `crates/evocc/src/main.rs` had edits by another agent, so its line
  numbers are those of `d362bde0`.
- Compiler: the frozen release `.build/frozen/366fd796/evocc-darwin`,
  SHA-256 `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04`
  (the same bytes as `target/frontend/release/evocc` at the time). Adapter:
  `target/frontend/debug/evo-ir-asm`, built at 09:01, SHA-256
  `93124f11b3883a9898bc712ff0b67b877e9addd5fea1e4c0c092540474c98e6b`.
  Both were copied to the session scratch directory before use. GCC:
  `gcc-14 (Debian 14.2.0-19)` in `evo-nexlab:sealed`, network off.
- Flags: `KERNEL_FLAGS` of `tools/k1/perf/compare_asm.py:25-28`, plus
  `GCC_ONLY` for GCC. `--emit=evoir` refuses `-fno-delete-null-pointer-checks`
  and `-fno-common`. The driver ignores both for `-S`, so the EvoIR was emitted
  without them. `-S` and `--emit=asm` give the same bytes (`cmp`). The GCC
  `-O2` lane adds `-fno-PIE`, because the `"i"` operand of the static-key
  `asm goto` needs a non-PIC address.
- Counting: static instructions along the hot path of the assembly
  (`tools/blocks.py`, `tools/transport.py`). They reproduce the tbprof counts
  of `tests/fixtures/kernel-tcg-instruction-profile.out:174-181`: 175 per loop
  iteration and 488 per `fault` call.
- Evidence: `.evidence/perf-analysis-20261014/` (1.1 MiB; `MANIFEST.txt` has
  every hash, `tools/` every script, `tables/` every count used here). No
  Cargo, no Lake, no kernel boot.
- Two read-only research agents read the inliner (N09) and the memory-object
  rules (N10). This note checked their main citations against the code.

**Claim ceiling.** This is code reading and static counts. A count "after a
change" comes from a twin: a copy of the reduction that avoids the cause with
today's compiler. No changed compiler ran. "Measured" names the command;
"inferred" marks a conclusion from code or arithmetic.

## 1. Causes

| ID | Cause | Stage | Code | Measured effect |
|---|---|---|---|---|
| T1 | One `asm.goto` turns identical-value transport off for the whole function. | SSA destruction | `crates/evocc-mir/src/generic/ssa.rs:212-219`; `isa/aarch64/insts.rs:715-717` | Loop 175 to 28 per iteration and `fault` 488 to 168 per call in the twins without `asm goto`. |
| T2 | A read-write or tied `asm` output, or a fixed binding, does the same. | Lowering and SSA destruction | `lower.rs:4879-4883`, `4936-4946`; `validate.rs:51-62`; `regalloc.rs:17-20`; `insts.rs:644` | Probe loop 12 to 28 per iteration. |
| T3 | Spill temporaries of a copy take the first free register, not the copy partner's. | Allocation | `generic/allocation.rs:919-1044`, `1568-1586` | 31 per iteration; `--regalloc-opt=copy-hints` removes them. |
| T4 | Slot compaction gives the source and the destination of a copy one slot, and nothing removes the pair. | Slot compaction | `allocation.rs:773-917`, `1653-1712` | 18 same-slot pairs (36 instructions) per iteration. |
| T5 | Phi copies stay, because a live range is one interval with no holes. | Allocation | `allocation.rs:1203-1333`, `1649-1651` | 4 of the 21 instructions over GCC left after T1. |
| T6 | Loop shape: the `asm goto` Boolean, the invariant shift guard, the block order, the induction. | Front-end shape, no mid-level pass | EvoIR `^b6`-`^b9`; `lower.rs:2116-2175` | 17 of the 21 instructions over GCC left after T1. |
| R1 | Records and arrays always live in memory; there is no promotion. | EvoIR lowering | `crates/evocc-lower/src/lib.rs:1243-1345` | `pte_offset_map` 115 to 63 with scalars. |
| R2 | A constant element address is `movz`, `sxtw`, `lsl`, `add`. | EvoMIR lowering | `lower.rs:3408-3437`, `1083-1110` | 120 of `theta`'s 329. |
| I1 | The inliner refuses a plain `static` helper with a local object. | C-HIR inliner | `crates/evocc-chir/src/inline.rs:989-1004`, `1022-1044` | `folio_state` 85 to 49 when inlined. |
| S1 | Protection keeps a frame record in every function, so every function signs. | Driver and emission | `emit.rs:1343-1353`, `1619-1627`, `1880-1901` | All 43 static leaves of two kernel objects sign. |

## 2. N08: why each live value travels through the stack

### 2.1 The EvoIR form is correct and costs nothing by itself

EvoIR is block-local SSA. A value used in a block must be defined there or be
one of its arguments (`spec/evoir.md:431-441`). The front end therefore passes
each live value on each edge, also into blocks with one predecessor.
`loop-carried-transport.c` has 176 register block arguments in 16 blocks and
232 edge arguments; `asm-goto-live-transport.c` has 423 and 546 (measured:
`tools/evoir-edges.py`). EvoMIR keeps the form one to one (`lower.rs:1868-1874`,
`2047-2050`). Identical-value transport is the designed remedy
(`spec/evomir.md:1121-1127`). The twin `twins/loop-call.c` keeps the same CFG
and passes the Core machine export. Its post-SSA EvoMIR keeps 8 transport
moves of 195 edge arguments (measured: `--emit=mir-ssa-json`,
`--emit=mir-post-ssa-json`, `tools/mir-copies.py`).

### 2.2 EvoMIR lowering adds one full-width edge per guard

Lowering splits a block at each `ub.assume` (`lower.rs:2116-2175`). The
continuation receives every register that the rest of the block reads, its
edge arguments included (`live_across`, `lower.rs:2801-2823`). The shift-count
guard of `1 << order` in the loop header creates `.Lfree_ok_block19` with 12
arguments. With forwarding, they are free: the block has one predecessor.
Without forwarding they cost 21 instructions per iteration. At plain `-O2`,
three signed-overflow guards add three more such edges.

### 2.3 SSA destruction: one asm goto turns forwarding off

`forwarded_parameters` returns no alias when any terminator prevents
forwarding or any instruction has a fixed binding (`ssa.rs:212-219`). The
AArch64 adapter says yes for `Indirect` and for every `AsmGoto`
(`insts.rs:715-717`); EvoMIR Section 7.1 states the same exclusion. Both
reductions inline a static key, so each edge copies each argument. The copy
goes to the end of a one-successor predecessor, else to the top of the target
(`ssa.rs:107-128`). One iteration crosses 10 edges with 11 or 12 copies each:
112 copies.

History (measured: `git log -S`). The exclusion arrived with forwarding
(`6eafa9a0`, 2026-09-24): "Inline asm is deliberately excluded initially: its
tied/read-write constraints require a separate audit." `c4458630` (2026-09-26)
admitted ordinary `asm` without fixed bindings. The only recorded hazard is the
joint copy of indirect dispatch over a self-edge
(`tests/fixtures/loop-value-transport.out:11-14`, `ssa.rs:209-211`). `asm goto`
edges are explicit and split like `cond_br` (`ssa.rs:413` skips only computed
edges), so that hazard does not apply. No test or record shows an `asm goto`
failure. `crates/evocc-mir/tests/generic_out_of_ssa.rs:296-311` only asserts
the exclusion.

Two more admission rules turn forwarding off for a whole function:

- A read-write (`+r`) output or a digit-tied input. Lowering defines the output
  register twice: a `mov` before the `asm`, then the `asm` output
  (`lower.rs:4879-4883`, `4936-4946`). `validate_function` refuses the second
  definition (`validate.rs:51-62`), so `forwarding_input_valid` is false
  (`regalloc.rs:17-20`). Measured with `probes/rw-asm.c`: the loop runs 12
  instructions per iteration with `"=r"`/`"r"` and 28 with `"+r"`.
- A fixed register binding, as in `register long x0 asm("x0")` (`insts.rs:644`).
  Measured with `probes/fixed-asm.c`: 28 per iteration.

**Kernel.** Every K02 hot function of the retained EvoCC K16 objects contains
patch sites (measured: `tools/patch-sites.py`): `user_mem_abort` 17 cpucap
sites (9 `b`, 8 `nop`), `rmqueue_bulk` 4 static keys, `__free_one_page` 2,
`get_page_from_freelist` 11, `free_unref_folios` 5, `__free_pages_ok` 4. Each
static key (`arch/arm64/include/asm/jump_label.h:36-51`) and each cpucap test
(`alternative-macros.h:225-262`) is an `asm goto`. Every atomic operation
dispatches through `alternative_has_cap_likely(ARM64_HAS_LSE_ATOMICS)`
(`lse.h:19-21`), and both kernel configurations set `CONFIG_JUMP_LABEL=y` and
`CONFIG_ARM64_LSE_ATOMICS=y`. The LSE `cmpxchg` adds `"+r"(old)`
(`atomic_lse.h:257-258`). So forwarding is off in nearly every hot kernel
function (inferred). This matches the 29.6 percent transport share of K02.

### 2.4 Allocation: each block's arguments get new places

After SSA destruction, each argument of each block is its own virtual
register. Its interval runs from its first to its last position in block
order, stretched over each block where it is live on entry
(`allocation.rs:1211-1333`). An interval that spans a call takes one of the
ten callee-saved registers or spills (`allocation.rs:1568`;
`isa/aarch64/allocation.rs:49-60`; `regfile.rs:50`). The loop has 11 values
live across its calls, and each header argument covers the whole loop. So most
arguments spill. A spilled value is reloaded before each use and stored after
each definition (`allocation.rs:919-1044`). A copy `mov d, s` with `d` spilled
becomes `mov t, s; str t, [slot]`, and `t` takes the first free register, not
the register of `s`. The same happens to `ldr t, [slot]; mov d, t` when `s` is
spilled. That costs 31 instructions per iteration (24 move-store and 7
reload-move pairs). `--regalloc-opt=copy-hints` gives `t` the partner's
register and runs 144 (measured: adapter, protection lifted;
`--regalloc-opt=reload-reuse` alone runs 175). The allocator does no other
coalescing (`spec/evomir.md:1199-1201`).

### 2.5 Spill slots: the same slot, or a moving one

Each spilled value gets its own logical slot (`allocation.rs:294-297`).
Compaction colors the slots by conflict and takes the lowest free location
(`allocation.rs:773-917`). The source of a copy dies where its destination
starts, so the two do not conflict and often get one location. The pair then
loads and stores one address: 18 pairs per iteration. Otherwise the value
moves between slots from block to block (`zone` is at `#48` in `block7` and at
`#16` after `block11`). Nothing removes a same-slot pair:
`drop_identity_moves` compares registers only (`allocation.rs:1653-1712`).

### 2.6 Frame

The frame adds nothing per iteration. Per call, the prologue and epilogue run
14 + 14 instructions against GCC's 9 + 9: ten single `str` and `ldr`, because
the paired-save variant is suppressed (`emit.rs:1344`). The frame of
`free_ok` is 192 bytes: 16 for outgoing arguments, 88 for 11 spill slots, 80
for 10 saves and 8 of alignment.

### 2.7 One iteration, block by block

Hot path: `compound == 0`, key off. Slots are offsets from `sp`. Variables:
`i`, `page`, `order`, `fpi`, `zone`, `mt`, `init`, `poison`, `pfn`,
`compound`, `bad`. Listing: `evocc/loop-carried-transport.kernel.s`.

| EvoMIR block (EvoIR) | Instr. | Content | Stage |
|---|---:|---|---|
| `block1` (`^b1`) | 4 | `movz w0,#1`; reload `order` from `#32`; `cmp w1,#32`; `b.hs` | lowering: shift guard; allocation: `order` spilled |
| `block19` (guard continuation of `^b1`) | 25 | copy `i` `#24`→`w1`; `page` `#96`→`#24`; `bad` `#16`→`#96`; 8 same-slot pairs `#32`…`#88`; reload `order`; `lsl`, `cmp`, `b.ge` | lowering adds the edge; SSA destruction, slots |
| `block2` (`^b2`) | 25 | `i` `w1`→`#16`; 10 same-slot pairs; reload `compound`; `cmp`, `b.eq` | SSA destruction, slots |
| `block4` (`^b4`) | 22 | 11 reloads into `w11`-`w24` (copies `^b2`→`^b4`); 11 moves into `w0`-`w10` (copies `^b4`→`^b5`) | SSA destruction, allocation |
| `block5` (`^b5`) | 2 | `nop` (the key); `b block7` | block order |
| `block7` (`^b7`) | 21 | `movz w25,#0` (key result); 3 moves to `w26`-`w28`; 7 move-store; 1 store; `mov w0,w25`; `b` | SSA destruction, allocation, slots |
| `block9` (`^b9`) | 3 | `and x0,x0,#1`; `cmp x0,#0`; `b.eq` (the key result tested again) | front-end shape |
| `block11` (`^b11`) | 25 | 3 moves; 8 reloads; 1 store; 6 move-store; `b` | SSA destruction, slots |
| `block15` (`^b15`) | 25 | `sxtw`, `lsl`, `add`, `ldr`, `and`, `str` (the work); 4 moves; 7 reload-move; `b` | SSA destruction, allocation |
| `block14` (`^b14`) | 23 | `add w15,w4,#1`; 10 move-store and 1 store back to the header slots; `b block1` | SSA destruction, slots |
| **Total** | **175** | 147 copy, 3 reloads for use, 25 work and control | |

GCC 14 runs 8: `cbnz w20` (compound), `nop` (key), `ldr; and; str` (the
flags), `add x24, x24, 64; cmp x19, x24; bne` (a pointer induction). It keeps
the invariants in `x19`-`x28` and spills `pfn` and `mt` once, before the loop.

Attribution (counts measured, stages inferred from the code above):

- SSA destruction (T1) creates the 112 copies, which become 147 instructions.
- The allocator's per-block places (T3) double 31 of them.
- Slot compaction (T4) turns 18 of them into same-slot round trips.
- Lowering's guard edge accounts for 21 of the 147.
- The front end's block form and the frame cost nothing per iteration.

The twin `twins/loop-noasm.c` loads the key instead of using `asm goto`. It
keeps the CFG and runs 28 per iteration: no frame access, 5 moves (measured).
Under the adapter, with protection lifted, all existing variants together give
143 (measured: `tools/variants.sh`). They do not remove T1.

### 2.8 The asm-goto reduction

`fault` carries 20 values across three static keys. Each disabled key costs
about 115 instructions: `block2` 56, `block4` 3, `block6` 54, `block7` 2
(measured: `tables/asm-goto-call.txt`). The twin without `asm goto` runs 168
per call, so forwarding alone removes 320 (measured). Of the twin's excess
over GCC's 67: the element addresses of `values[20]` cost about 44
(`movz xK,#off; add xK,x1,xK` for 19 elements, plus 6 spills and reloads of
addresses; cause R2), and the ten single saves and restores cost 10 more than
GCC's pairs.

### 2.9 At -O2

At plain `-O2`, EvoCC runs 187 per iteration and 475 per call; GCC 14 runs 8
and 52 (measured). The twins run 30 and 155. The difference to the kernel
flags is the signed-overflow guards, because `-fno-strict-overflow` is absent.

## 3. N08: the smallest changes

### 3.1 A0, a precondition: check SSA destruction in every function

No checker covers SSA destruction of a function with `asm` or `asm.goto`
today:

- The allocation verifier checks the allocated program against itself, not
  against the SSA program (`generic/allocverify.rs:1-4`, `300-428`).
- The EvoMIR interpreter refuses `asm` and `asm.goto` (`spec/evomir.md:1417-1426`).
- The Lean transport check runs only on `core-v1`, which has no `asm`
  (`spec/verification.md:70-74`).
- The N02 runner refuses functions with volatile, atomic, I/O,
  floating-environment or unwind effects, which `free_ok` and `fault` have
  (`crates/evocc-passes/src/lib.rs:275-296`). It runs only `main`
  (`main.rs:2016-2019`), and the EvoIR interpreter refuses `asm` with E3021
  (`tests/fixtures/asm-value-transport.out:18`).

New rule: a Rust validator of each (SSA, post-SSA) pair, the twin of
`Transport.checkStructure` (`lean/EvoIR/MachineTransport.lean:381-390`):
symbolic origins at block entry, retained instructions with equal definitions
(`MachineTransportInstruction.lean:21-34`), administrative blocks of moves
only (`MachineTransport.lean:372-379`), and each edge argument followed
through the copies (`MachineTransport.lean:236-262`). It treats `asm` and
`asm.goto` as opaque instructions with uses and definitions. Run it beside
`verify_allocation_for` (`native.rs:298-303`) with its own refusal code. No
output changes.

Without A0, changes A and B are covered only by tests, the native differential
lane (Section 9.4) and kernel runs, as the `asm.goto` path is today. That
keeps today's coverage and does not raise it. Conservative choice: A0 first.

### 3.2 A: admit asm.goto to identical-value transport

- Code: `insts.rs:715-717`. New rule: `Indirect` prevents forwarding; an
  `AsmGoto` prevents it only when its body has a fixed register binding, the
  rule that `has_fixed_binding` applies to `MInst::Asm` (`insts.rs:644`). The
  generic layer keeps no ISA name.
- Soundness: forwarding identifies a parameter with its single distinct
  incoming value (`ssa.rs:244-276`). That is a fact about SSA values, not about
  the terminator. `asm.goto` edges are explicit, split when critical
  (`ssa.rs:387-440`) and copied by the rules of Section 7.1. An `asm.goto`
  output is a definition. The forwarding map has parameter keys only, so the
  operand rewrite of the terminator (`ssa.rs:157-159`; `insts.rs:550-555`)
  never changes an output (inferred).
- Specification: `spec/evomir.md:1121-1127`, minor revision 1.3.0. New text:
  "…functions with a fixed-register assembly operand, in an `asm` or an
  `asm.goto`, or with indirect dispatch are excluded."
- Checkers: the allocation verifier is unchanged and checks the result. A0
  checks the destruction. Lean: no change, because `core-v1` has no `asm` and
  `proposeAliases` (`MachineTransportProposal.lean:80-85`) never meets one.
- Count: loop about 29 per iteration (twin 28, with the asm-goto Boolean in
  place of the 6-instruction key load); `fault` about 171 per call (inferred).

### 3.3 B: admit read-write and tied asm outputs

- Code, smallest form: let `regalloc::out_of_ssa_with_dce` (`regalloc.rs:17-20`)
  accept one non-SSA form: a register defined by a `Mov` and then, later in the
  same block, as the read-write or tied output of an `asm` or of the block's
  `asm.goto`. Every other double definition still refuses forwarding. Such a
  register is never a block argument, and its last definition precedes every
  use outside the pair (inferred). Larger alternative: lower `+r` and digit
  ties to SSA, with a fresh input tied to the output (Section 7.3: "the two
  live ranges are one").
- Fixed bindings stay excluded. `pin_assembly` separates untied fixed inputs
  (`allocation.rs:463-492`), but tied operands still refuse contradictory
  bindings (`allocation.rs:497-510`; the E0004 case in
  `tests/fixtures/asm-value-transport.out:4-6`). That exclusion needs its own
  audit.
- Specification: one Section 7.1 sentence for the admitted form.
- Checkers: as for A.
- Count: probe 28 to about 12 per iteration (inferred). No reduction changes.

### 3.4 C: keep a copy's partners in one register

Two allocator rules, in this order.

- C1, copy hints by default. They already exist behind
  `--regalloc-opt=copy-hints` (`allocation.rs:1480-1490`, `1573-1586`;
  Section 7.6). Measured on the original: 175 to 144. After A, few spilled
  copies remain, so the gain on the reductions is small (twin: 28 to 28).
- C2, live ranges with holes. After A, 4 phi moves per iteration remain
  (`bad` three times, `i` once). Hints cannot remove them: an interval is one
  hull (`allocation.rs:1203-1210`, `1649-1651`), so a loop phi covers the whole
  loop and overlaps its own copy partners. New rule: each interval keeps its
  live segments per block, from block entry or definition to last use or block
  end, from the existing `liveness` (`allocation.rs:1104-1137`). `overlaps` and
  the `taken` set (`allocation.rs:1550-1563`) test segments; expiry keeps the
  hull end (`allocation.rs:1540`). Determinism is unchanged.

Not in SSA destruction. TODO N08 says "coalesce block arguments with their
sources in out-of-SSA". A pass that renames definitions there breaks the
proof-required path. `checkRetained` demands equal definitions
(`MachineTransportInstruction.lean:31-34`), and the proposer aligns
instructions by identity (`MachineTransportProposal.lean:87-100`). Each
`core-v1` program with a coalesced phi would then end with exit 3. That form
is not acceptable unless the Lean transport check changes first.

Checkers for C: a dropped copy is a rename, which `checkAllocation` accepts
(`lean/EvoIR/Alloc/Check.lean:12-16`, `222-224`, `254-292`). The hint producer
is generic (`verify_alloc.rs:657-709`). The Rust verifier checks register
contents by dataflow and needs no change (`allocverify.rs:300-428`). `checkAsm`
sees fewer `mov` lines, each with its template. The proof-required path uses
the same allocator (`verify_alloc.rs:746-762`), so its outputs change.

Count: loop about 25 per iteration; `fault` about 165 per call (inferred).
Risk: a default change re-baselines every object, and compile time can grow
(N03c T1 recorded misses up to 1.456x).

### 3.5 D: spill-slot copies

After A, the loops of both reductions have no frame access (measured: twins),
so N08's exit does not need D. For functions that still spill across joins:
let compaction prefer a copy source's location for the copy destination, then
drop `ldr t, [k]; str t, [k]` when `t` has no other use
(`allocation.rs:773-917`).

Checker gap: the Rust verifier tracks registers only (`allocverify.rs:179`,
`472-500`). A reload defines its temporary, and no rule checks what the slot
holds. Today's compaction is therefore unchecked outside `core-v1`; only Lean
tracks slots (`Alloc/Check.lean:75-81`: spill stores and reloads copy the
equations of a `spill` place). D is not
acceptable before the Rust verifier tracks slots, with places as in
`verify_alloc.rs:23-31`.

### 3.6 E: what allocation cannot fix

After A and C, the loop runs about 25 per iteration. The exit needs at most 16
(2 × 8,236 per call over 1,023 iterations). The rest, from the twin listing
(inferred):

- The `asm goto` result: 0 or 1 in two blocks, a join, then `and`, `cmp`,
  `b.eq` (about 5).
- The invariant `1 << order` and its shift guard in the header: `movz`, `cmp`,
  `b.hs`, `lsl` (4).
- Three `b` to a block that is not the next one (3).
- `cmp w, #0; b.eq` where GCC uses `cbnz` (1).
- `sxtw; lsl #6; add` where GCC steps a pointer (3).
- The test at the top of the loop and the `b` back to it, where GCC tests at
  the bottom (1).

Block order and `cbz` are checkable. `checkAsm` follows the allocated block
order (`branchTo`, `lean/EvoIR/Asm/Check.lean:168-169`), and the other two Lean
checkers find blocks by identity; the entry block must stay first. Threading
the Boolean and hoisting the guard are CFG and loop transforms in a function
with `asm.goto`. No checker can validate them today: the EvoIR interpreter
refuses `asm`, so N02 cannot run them, and `core-v1` excludes `asm`. They need
a design with its own checker.

### 3.7 Counts after each change (inferred unless marked)

| Change | Loop, per iteration | `fault`, per call |
|---|---:|---:|
| Today (measured) | 175 | 488 |
| A | ~29 | ~171 |
| A + C | ~25 | ~165 |
| + R2 address fold (Section 5) | ~25 | ~125 |
| + block order and `cbz` (E, checkable) | ~21 | ~120 |
| + Boolean threading and guard hoist (E, no checker yet) | ~12 | ~110 |
| GCC 14 (measured) | 8 | 67 |
| Exit: twice GCC 14 | 16 | 134 |

So N08's exit is reachable for `asm-goto-live-transport.c` with A and R2. For
`loop-carried-transport.c` it is not reachable with checkable changes alone.

## 4. protects_codegen()

### 4.1 Why it exists

It is a validation-scope rule, not a repair of a known miscompile. The quality
plan says: "The kernel profile and any frame-pointer-requesting option keep
today's output until the kernel gates have been run with the new code"
(`docs/archive/codegen-quality-plan.md:502-507`). `718f155f` (2026-10-02,
N03f) amended the plan: frame-pointer requests and kernel markers suppress all
three frame variants, not only leaf frames (the handoff amendment in
`tests/contracts/codegen-n03f-20261002.json`). `a1b0d7f0`, 24 minutes later,
put that condition into `emit_function`. `bb2f3b5f` (2026-10-03, N03c T2)
moved it to `CodegenOptions::protects_codegen` (`emit.rs:70-79`), so that
selection and allocation variants share it (`native.rs:245`). `21f4c2a8` and
`f82d5e61` only describe it. `-mgeneral-regs-only`, the system-register guard
and a nonzero patchable entry serve as kernel markers; arm64 Kbuild always
passes the first. EvoMIR Section 1.4 says "the variants are not validated
there" (`spec/evomir.md:160-165`). Today the guard changes no kernel object:
the kernel command projection passes no variant (measured: no `--frame-opt`,
`--isel-opt` or `--regalloc-opt` in `tools/k1/cmdrecord.py`).

### 4.2 What it keeps off, and what makes each safe there

| Variant | Suppressed at | Kernel interaction | Needed before lifting |
|---|---|---|---|
| F1 paired saves | `emit.rs:1344` | None: slots, size and order are unchanged (`emit.rs:80-84`). | Kernel build, boot and tests with F1 on. Proof-required mode keeps it off: the Lean prologue has single `str` saves (`Asm/Check.lean:160-166`). |
| F2 shared epilogue | `emit.rs:1345` | None known: patch sites are at entry. | Kernel validation; off in proof-required mode (each `ret` has its own epilogue template, `Asm/Check.lean:190`). |
| F3 leaf frames | `emit.rs:1352-1353` | `-fno-omit-frame-pointer` asks for frame records. GCC 14 still omits the record of a leaf at the kernel flags (measured: `gcc/leaf-helper-call.kernel.s`). A stack guard already blocks F3 (`emit.rs:1475`). Patch NOPs come before the prologue (`emit.rs:1367-1391`). | Let only `-mno-omit-leaf-frame-pointer` block F3. Kernel tests of stack traces, ftrace and kprobes through a leaf. Off in proof-required mode (`Asm/Check.lean:160-162` always pushes the record). Needed by N11. |
| C1 constants | `emit.rs:1346` | None known. | Kernel validation; off in proof-required mode (no template for the compact forms). |
| C2 local selection | `emit.rs:1347` | None known. | It stays off anyway: compile-cost miss (N03d). |
| C3 extension reuse | `native.rs:246` | None known; the allocation verifier checks it. | Kernel validation. |
| C5 controls | `native.rs:270` | None known. | It stays off anyway: compile-cost misses (N03d). |
| T2 copy hints, T3 reload reuse | `native.rs:289-293` | None known; the verifier checks registers. | Kernel validation. T2 is change C1 above. |

The smallest structural change: make protection a per-variant set, not one
Boolean, and lift one variant at a time after a kernel lane builds with it.
Each lift needs an EvoMIR Section 1.4 revision.

## 5. N10: one-word records and small arrays

Hot path per call (measured by the N10 agent with a block walker; both equal
the TODO counts): `pte_offset_map` 115 against 23, `theta` 329 against 94.

Causes:

- R1, records and arrays in memory. `collect_memory_symbols` keeps an object
  in memory for any array, vector, struct, union, `va_list` or complex type
  (`crates/evocc-lower/src/lib.rs:1252-1263`) and for any `&x`
  (`lib.rs:1340-1345`). No promotion pass exists (measured: no mem2reg or SROA
  in `crates/`). The inliner also creates one struct object for each by-value
  argument and result (`crates/evocc-chir/src/inline.rs:3879-3900`), so each
  helper costs a slot and a copy: 37 slot-copy instructions in
  `pte_offset_map`.
- R2, constant element addresses. EvoMIR `PtrOffset` always emits `movz`,
  `sxtw`, `lsl`, `add` and records the constant (`lower.rs:3408-3437`).
  `memory_address` uses the record only when nothing but `MovImm` follows
  (`lower.rs:1083-1110`). `theta` pays 30 × 4 = 120.
- R3, zero fill. A memory object gets zero stores over all its bytes for
  `-ftrivial-auto-var-init=zero` (`lib.rs:8467-8499`). An SSA local gets a
  zero binding and no store (`lib.rs:6404-6426`). Each element is written
  before any read, so the stores are dead (inferred).
- R4, canary. `stack_protect` comes from source facts: an array-containing
  type or `&x` (`lib.rs:4282-4309`). 11 instructions per call in both.
- R5, Boolean results: `cset; and #1; sxtw; cmp #0; b.eq` (14 in
  `pte_offset_map`).
- R6, constant shift guards: `rol64(x, 1)` keeps guards on constant amounts
  (28 in `theta`, 15 in `folio_state`). Lowering discharges only a literal
  true condition (`lower.rs:2088-2110`).

Smallest changes:

1. Promote an object of at most 8 bytes, or a small scalar array, when each
   access is a non-volatile load or store at a constant in-bounds offset and
   the address never escapes. As an EvoIR `ModulePass`, N02 can check it where
   it admits the function: a one-TU `theta` driver is admitted, and
   `pte_offset_map` is refused for its volatile load (N10 agent, measured with
   `--opt-pass=identity`). Inside lowering only the structural validator sees
   it. Removing an object shifts interpreter addresses (`crates/evocc-memory/src/lib.rs:1220-1242`).
   Measured proxies: scalar sources run 63 (`pte_offset_map`) and 275 (`theta`).
2. Decide `stack_protect` after promotion: protect only if an object with an
   array-containing type or an exposed address stays in memory. No checker
   sees it; `stack_protect` has no EvoIR semantics, and EvoMIR reads only the
   flag (`emit.rs:1348-1351`). GCC also decides late.
3. Fold constant offsets: in the `PtrOffset` arm emit `add xD, xB, #imm`
   (one instruction, nothing moves); then let `memory_address` use the
   per-block record across other instructions. The second step contradicts
   the comment at `lower.rs:1094-1096`; the owner decides. Only the allocation
   verifier sees either step; `core-v1` has no pointers.
4. A bounded default form of C5 for Boolean branches (cost re-measure first).
5. Skip a shift guard whose amount is a constant below the width. For
   `core-v1` (which admits `<<` on unsigned values), `checkLowering` must then
   accept the missing guard; test it against the proof fixtures (inferred).

Counts (inferred): `pte_offset_map` 63 after 1 and 2; the exit (46) also
needs 4 and 5. `theta` about 175 after 1 to 3, within its exit of 188.
`core-v1` refuses structs, arrays, address-of, the stack protector and zero
initialization (`spec/verification.md:40-41`, `70-74`), so the Lean path never
sees N10.

## 6. N09: small static helpers

Causes (N09 agent; checked at `inline.rs:985-1044`):

- Inlining happens in C-HIR, inside lowering (`inline.rs:881-1288`;
  `crates/evocc-lower/src/lib.rs:573-583`). No option or `-O` level controls
  it, and `protects_codegen` does not reach it.
- `const_folio_flags` is a plain `static` with the local `page`. The private
  leaf class refuses local objects (`inline.rs:995`). The size-bounded class
  needs an `inline` spelling (`inline.rs:1031`), and so does the single-site
  class (`inline.rs:1066`).
- The call stays specialized: every call passes 0, so the body reads the
  literal (`common_parameter_values`, `inline.rs:706-814`). The callee keeps
  `movz w1,#0; and; lsl; add`, a frame record and a PAC pair.

Smallest change: let the size-bounded class (`inline.rs:1031-1039`) admit an
internal function without an `inline` spelling, with its own node budget and
the same barriers. Today's barriers: `noinline`, `noipa`, `noclone`, `used`,
`weak`, address-taken, variadic, recursion and `&&label`. Recorded but not
honored: `section(...)` and `patchable_function_entry` (kernel `notrace`).
No consumer: `no_instrument_function`, `cold`, `target(...)`, `returns_twice`.
The new class should refuse `returns_twice`, `target(...)` and a section that
differs from the caller's (conservative). There is no total growth cap
(`inline.rs:1465-1483`); N09's exit asks that total code size does not grow,
so add a per-unit cap.

What the inliner can reuse: the call graph in C-HIR `Uses.calls` and the
callers map (`inline.rs:50-58`, `1052-1057`), the size in `Uses.nodes`
(`inline.rs:34`), and `emitted_functions` (`crates/evocc-mir/src/linkage.rs:30-95`).

Proof-required path: `core-v1` admits a `static` definition and refuses
`inline` and every attribute (`crates/evocc-chir/src/core_profile.rs:153-160`).
The source export uses the unit before substitution, and `source-to-evoir`
runs the callee body on the source side. An inlined call is accepted: the N09
agent measured exit 0 and `CHECKED_TO_FINAL_ASSEMBLY` for a small inlined
helper. Each emitted function must be an entry (`proof_required.rs:475-495`).
N02 cannot see a C-HIR inliner, and it runs only `main`; kernel units get no
translation validation.

Count: with the helper spelled `static inline`, `folio_state` runs 49 per call
(measured: `twins/leaf-inline.c`; today 85, GCC 14 9). The exit (18) also
needs R2 (12 of the 49 are `movz #0; and; lsl; add`), R6 (15 guard
instructions) and either a frameless unsigned leaf (N11, 5) or the removal of
`mov w, w` (3) (inferred).

## 7. N11: return signing

Where it is decided:

- The driver sets `codegen.sign_returns` for both `pac-ret` and
  `pac-ret+leaf`. The scope reaches only the ACLE macro (`main.rs:874-890`,
  `1356`).
- Emission signs when `frame.sign_returns` is set (`emit.rs:1341`):
  `paciasp` first (`emit.rs:1620-1625`), `autiasp` after the frame pop
  (`emit.rs:1895-1900`).
- A function pushes `stp x29, x30` unless `frame.frameless`
  (`emit.rs:1626-1632`). That needs `--frame-opt=leaf-frames`, no protection
  and `is_frameless_leaf` (`emit.rs:1352-1353`, `1466-1494`).

"The return address never leaves x30" means here: the function is frameless.
`is_frameless_leaf` already requires no frame, no call, no `asm` or
`asm.goto`, no frame address and no stack guard. `bl` and `blr` overwrite
`x30`, and the frame record stores it. GCC 14 signs exactly the functions that
save the link register.

With the kernel flags every function is protected, so no function is
frameless and every function stores `x30`. All 19 leaves of `mmu.o` and all 24
of `page_alloc.o` sign and push the record (measured:
`tools/signed-leaves.py`). The kernel asks for `pac-ret`, not `pac-ret+leaf`
(`arch/arm64/Makefile:84-87`; it probes with `pac-ret+leaf`, `Kconfig:2034-2036`).

Smallest change:

1. Carry the scope into `CodegenOptions` (a new `sign_leaf_returns`).
2. In `emit_function`, after `frame.frameless` is known: sign when
   `sign_returns && (sign_leaf_returns || !frame.frameless)`.
3. Let F3 apply under `-fno-omit-frame-pointer` (Section 4.2). Without step 3,
   steps 1 and 2 change no kernel function.

Checkers: none in the native path; the change is a template choice. The
proof-required AST has no PAC line, and `export_core_asm` takes no driver
option (`proof_required.rs:567`), so a published text with `paciasp` cannot
match it (inferred). F3 must stay off there. Tests to change:
`crates/evocc/tests/compilation_options.rs:921-956` (both spellings sign both
functions) and `crates/evocc/tests/n03f_frame_variants.rs:482-512` (a signed
frameless leaf). New test: every function whose text stores `x30` has
`paciasp` before the store.

## 8. Order of work, tests and receipts

| Step | Work | RED and pinning tests | Receipts and tests that change |
|---|---|---|---|
| 1 | A0 SSA-destruction validator | A seeded wrong copy refused; every existing compilation accepted | None |
| 2 | A, `asm.goto` forwarding | RED: `crates/evocc-mir/tests/loop_value_transport.rs` case with an `asm.goto` loop, no transport copy for invariants; C test: no `[sp` access inside the loop of `loop-carried-transport.c` | `generic_out_of_ssa.rs:296-311` (assert the fixed-binding exclusion instead); kernel objects; the informative hash in `tests/fixtures/assembly-static-key-toggle.out`. Native checks: `section4_inline_assembly.rs:974`, `section9_native_differential.rs:306`, and a new looping `asm goto` with permuted values against GCC 14. Watch `inline_assembly_lowering.rs:85-97` (`spilled >= 12`): its 12 SIMD values span a call with no SIMD across-call register, so it should hold (inferred). |
| 3 | B, read-write and tied `asm` | RED: `probes/rw-asm.c` shape | Kernel objects. Ladder sources use `"+r"` only under ARMCRC32 (zlib) or x86 (zstd) (measured: grep) |
| 4 | R2: `add #imm` for a constant `PtrOffset` (N10 change 3, first step) | MIR test of a constant `PtrOffset` | Six-unit and native-corpus hashes, ladder sizes |
| 5 | R6 constant shift guards | Proof fixture with a constant shift | Same as 4; proof fixtures |
| 6 | N09 inline class | `private_leaf_substitution.rs:260-290` (16 sources keep `call @f`; line 276 is the `const_folio_flags` shape) | `inline_definition_roles.rs:131-138`; `lean/EvoIR/Core/SourceCallTests.lean:183-187` pins a caller body; N05 trend; ladder sizes |
| 7 | N10 promotion and late canary | Interpreter tests with address-dependent output | Most objects; interpreter step counts |
| 8 | C, allocator (owner-approved default) | `section7_linear_scan.rs`, `generic_allocation.rs` | Every object: n03a, N03c, N03d, N03f and N07 OFF hashes; ladder size receipts; proof-required outputs |
| 9 | N11 with F3 for the kernel flags | Section 7 | Kernel objects |
| 10 | E | New design with its own checker | Kernel objects |

Unchanged by steps 1 to 3: the six-unit and 768 native-corpus hashes and the
ladder sizes (no `asm goto` in `.build/ladder` sources: measured, grep; the
native generator emits no `asm`), and the proof fixtures (`core-v1` has no
`asm`). After steps 2 and 3, rerun the K02 profile with
`tools/k1/perf/reduction-counts.py` and boot the Image.

## 9. Open risks

- No proof checker covers `asm.goto` functions before or after A; A0 is the
  only new independent check.
- N08's exit for `loop-carried-transport.c` needs E, which has no checker.
- N09's exit needs four changes, not one; N10's `pte_offset_map` exit needs
  R5 and R6 as well.
- C2 and N10 change every object; compile time and determinism need their own
  bounds.
- The Rust allocation verifier does not track spill slots (Section 3.5).
- F3 and N11 change kernel unwinding; they need kernel stack-trace and ftrace
  tests.

## 10. Other defects found

- `naked` has no consumer: the out-of-line body gets a frame record, and a
  `static inline` naked body is inlined with its `ret` (N09 agent, measured).
- Specialization from caller constants (`inline.rs:706-814`) makes a
  proof-required entry fail: `g(a,3)+g(a+1,3)` with entries `g` and `f` gives
  exit 3 for `g`; the control with 3 and 4 gives exit 0 (N09 agent, measured).

## 11. Proposed TODO and CHANGELOG text

N08, next action (replaces the current one):
"Admit `asm.goto` without fixed bindings to identical-value transport
(`isa/aarch64/insts.rs:715`), then read-write and tied `asm` outputs; first add
an independent SSA-destruction check. Turn copy hints on and give live ranges
holes in the allocator, not in SSA destruction (the Lean transport check needs
equal definitions). The 2× exit for `loop-carried-transport.c` also needs
Boolean threading and guard hoisting, which need their own checker. Analysis:
docs/archive/notebook/perf-n08-n11-analysis-20261014.md."

N10, next action: "Promote one-word objects and small constant-indexed arrays
in an EvoIR pass, decide the canary after it, fold constant `PtrOffset` into
`add #imm`, and skip constant shift guards."

N09, next action: "Admit internal functions without `inline` to the
size-bounded inline class, with a per-unit growth cap; the exit also needs the
N10 offset fold and constant shift guards."

N11, next action: "Carry the signing scope into `CodegenOptions`, leave
frameless leaves unsigned under `pac-ret`, and let leaf-frame elision apply
under `-fno-omit-frame-pointer`."

CHANGELOG: this note changes no behavior, so it needs no entry. Proposed text
for change A when it lands (Keep a Changelog, `### Changed`): "A function with
`asm goto` now forwards unchanged block arguments, as other functions do.
Kernel code with static keys and LSE atomics no longer copies every live
value through the stack at each join."
