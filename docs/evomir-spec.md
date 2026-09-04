# EvoMIR Specification

**Revision 0.5** | **Authoritative reference** | **2026-09-04**

EvoMIR is the machine-level intermediate representation of EvoCC. It sits
between the extracted EvoIR of a function (`docs/evoir-spec.md`, the EvoIR
specification, which this document cites by section) and the assembly text
handed to the platform assembler. Everything below the extraction boundary
is governed here; everything above it is governed by the EvoIR
specification, and where the two meet, the EvoIR specification wins.

This document follows the rules of its parent: a sentence about observable
behaviour is measured before it is written, a construct it does not admit is
refused with a named diagnostic rather than guessed, and an executor that
finds it silent stops and reports the gap. Revision 0.1 was implemented once
and stopped thirteen times; revision 0.2 rules every stop and records which
sentence was wrong.

## 1. Purpose and Position

EvoMIR exists so that backend engineering can be conventional. The e-graph
decides *which* semantics a function has; EvoMIR decides *how a machine
computes it*, with explicit registers, explicit frame slots and explicit
calling-convention constraints, and nothing else. Its first target is the
AArch64 scalar subset, first hosted as Darwin arm64 (the development machine,
so that a program can be compiled, run and compared on the spot) and then as
Linux AArch64 for the kernel. The two targets share every rule below except
the ones the calling convention and the symbol syntax name.

Three invariants carry over from the EvoIR specification and are restated
because a backend is where they are most easily lost:

- **Semantics are fixed before EvoMIR sees them.** The lowering of a
  function is a function of its extracted EvoIR: two runs over the same
  EvoIR text produce the same EvoMIR text, byte for byte.
- **Undefined behaviour stays explicit.** An `ub.assume` obligation lowers
  to a checked trap in the checked profile and is never dropped silently;
  the profile that omits the check is a named policy, not a default.
- **What is not lowered fails closed.** Every EvoIR form this revision does
  not lower has a diagnostic in Section 10, reported at the instruction, and
  the driver produces no assembly for that function.

The EvoIR specification's Section 10.2 invariant governs every EvoMIR form:
it implements the abstract semantics of its EvoIR source for **all defined
executions**. Where a machine instruction's behaviour outside defined
executions differs from EvoIR's (a shift by an out-of-range amount, a
wrapping address computation, an unmasked comparison), the difference is
admissible only when an EvoIR obligation the program has already asserted
makes it unobservable; Section 5 names each such case.

The revision 0.x series covers what the roadmap calls B1 through B4: EvoMIR
itself, a linear-scan register allocator, the Darwin arm64 scalar ABI, and
the native test suite. EvoMIR representations do not yet enter e-classes.

## 2. Values, Registers and Classes

An EvoMIR function is a list of basic blocks in the order of its EvoIR
source, the entry block first (block 0), each holding instructions in order
and ending in a terminator. Before register allocation, operands are
**virtual registers** `%vN` in block-argument SSA form, the same discipline
as EvoIR Section 2: a block's arguments are its only inputs, every value has
one definition, and every edge lists the values it passes. Block arguments
survive lowering and are removed by the out-of-SSA pass of Section 7, which
runs before allocation. After allocation, operands are **physical
registers** and **frame slots**.

Every virtual register has exactly one **register class**:

| Class | Holds | Physical registers |
|---|---|---|
| `gpr32` | `bits<N>` with `1 <= N <= 32` | `w0` to `w30` |
| `gpr64` | `bits<N>` with `33 <= N <= 64`, every pointer | `x0` to `x30` |
| `flags` | the result of a comparison or a flag-setting arithmetic form, consumed by the next conditional | `nzcv` |

`gpr32` and `gpr64` name the same physical register file viewed at two
widths; the allocator assigns one physical register per virtual register and
the class fixes the spelling. There is no floating-point class in this
revision; `f32`, `f64` and `vec` values are refused (Section 10).

A `flags` value may be consumed only by the instruction that immediately
follows its producer in the same block, and nothing between them may write
flags. This is a legality rule the verifier of Section 9 checks.

**Reserved physical registers.** `x18` is reserved by the platform on Darwin
and is never allocated; `x16` and `x17` are the intra-procedure-call
scratch registers, used only by emitted sequences and never live across an
instruction the allocator did not emit; `x29` is the frame pointer and
`x30` the link register, both saved in every prologue (Section 6); `sp` is
not a general register. The zero register `xzr`/`wzr` is an operand, never a
destination.

## 3. Sub-width Values

EvoIR values of width `N < 32` live in `gpr32` registers with the bits above
`N` **unspecified** (`N < 64` in `gpr64` likewise). The EvoMIR interpreter
models those bits as unspecified, not as zero: an implementation that fills
them from a seeded source is conforming, and a lowering that reads them is
thereby caught. An instruction that reads such a value reads only its low
`N` bits, except that the following operations need a normalized form,
which the lowering inserts explicitly:

| Operation on `bits<N>` below the register width | Form needed | Inserted before |
|---|---|---|
| `add`, `sub`, `mul`, `and`, `or`, `xor`, `shl`, `trunc` | none: the low `N` bits of the result depend only on the low `N` bits of the operands | nothing |
| `icmp.eq`, `icmp.ne`, `lshr`, `udiv`, `urem`, `icmp.ult`, `icmp.ule`, `zext` | zero-extended to the register width | `and` with the mask `2^N - 1` (`uxtb`/`uxth` when `N` is 8 or 16); both operands of a comparison |
| `ashr`, `sdiv`, `srem`, `icmp.slt`, `icmp.sle`, `sext` | sign-extended to the register width | `sbfx` from bit `N - 1` (`sxtb`/`sxth` when `N` is 8 or 16); both operands of a comparison |
| shift amount operand `bits<M>` | the low `M` bits are the amount | `and` with `2^M - 1` when `M` is below the register width |
| a value stored with `store` of width `N` | none: the store writes `ceil(N / 8)` bytes and the bits above `N` in the last byte are those of the value, which EvoIR Section 5 defines as the representation | nothing |
| `cond_br` on `bits<1>` | bit 0 only | nothing: `tbnz`/`tbz` on bit 0 |

Revision 0.1 listed `icmp.eq` and `icmp.ne` in the first row; that was
wrong (a comparison reads every bit of the register, so `trunc.bits<3>` of
`0xffffffff` compared with the constant 7 answered 0 where EvoIR answers 1)
and the executor found it by hand before the campaign could.

**Shift amounts.** A 32-bit machine shift masks its amount modulo 32 and a
64-bit one modulo 64, never modulo the EvoIR width `N` of a narrower value.
EvoIR's raw shifts are total and answer 0 (or the sign fill) for an amount
of `N` or more, so the machine and EvoIR differ there. The difference is
unobservable in a defined execution because the front end asserts
`shift_count_defined` as an obligation before every shift (EvoIR Section
6); the lowering therefore inserts nothing beyond the mask of the row above,
and this is the first of the admissible differences Section 1 names.

`bits<1>` lives in a `gpr32` with bit 0 significant. Comparisons produce
their `bits<1>` result with `cset`, which writes 0 or 1 into the whole
register; a `bits<1>` produced by `trunc` or `and` has unspecified upper
bits, and the table above is what makes that safe.

Widths above 64 are not lowered (`E4003`).

## 4. Instruction Set

An EvoMIR instruction names an AArch64 operation, its destination register
class and its operands, or one of the four pseudo-forms this section marks
as such, which emission expands. The forms this revision admits are exactly
these; an operation absent from the table is not part of EvoMIR. Each row
states the semantics the EvoMIR interpreter (Section 9) implements, so that
the table is executable, not descriptive.

**Data movement.**

| Form | Semantics |
|---|---|
| `mov rd, rn` | `rd := rn` |
| `mov rd, #imm` (`movz`/`movk`/`movn` sequence at emission) | `rd := imm`, any 32- or 64-bit constant |
| `adrp rd, @g@PAGE` + `add rd, rd, @g@PAGEOFF` | `rd := address of the global g`; `@f@FUNCPAGE` / `@f@FUNCPAGEOFF` for a function symbol, since EvoIR keeps globals and functions in two tables that may share a name and Section 8 gives both one platform prefix (Darwin spelling; Linux uses `:pg_hi21:` and `:lo12:`) |
| `ldr rd, [rn, #off]` / `ldrb` / `ldrh` / `ldrsb` / `ldrsh` / `ldrsw` | `rd := memory[rn + off]` of 8, 4, 2 or 1 bytes, zero- or sign-extended as the mnemonic says; `off` is a scaled unsigned 12-bit immediate or a signed 9-bit immediate |
| `ldr.p rd, [rn, #off]` / `ldr.pf` / `str.p rt, [rn, #off]` | an eight-byte load or store whose value is an object pointer (`ldr.p`), a function pointer (`ldr.pf`) or a stored pointer (`str.p`); emitted as `ldr`/`str`, but distinguished so that the interpreter reconstructs provenance exactly as EvoIR Sections 5.3 and 5.4 do and reports `E3013` on a partial shadow |
| `str rt, [rn, #off]` / `strb` / `strh` | `memory[rn + off] := low bytes of rt` |
| `ldp`/`stp rt1, rt2, [rn, #off]` | two registers, prologue and epilogue only (Section 6) |
| ` volatile` marker on any load or store | the access appears in the volatile trace (EvoIR Section 4.3); it adds nothing else on this target and forbids merging with a neighbouring access, which this revision never does |

**Arithmetic and logic.** Every form has a 32-bit (`w`) and a 64-bit (`x`)
spelling chosen by the destination class.

| Form | Semantics |
|---|---|
| `add rd, rn, rm` / `add rd, rn, #imm12` | wrapping addition |
| `sub rd, rn, rm` / `sub rd, rn, #imm12` | wrapping subtraction |
| `adds`, `subs rd, rn, rm` | as `add`/`sub`, and write `nzcv`; `v` is the signed-overflow flag, `c` the carry |
| `mul rd, rn, rm` | wrapping multiplication, low bits |
| `smull xd, wn, wm` / `smulh xd, xn, xm` | signed 32x32 to 64 product / the high 64 bits of a signed 64x64 product |
| `msub rd, rn, rm, ra` | `rd := ra - rn * rm` (remainders) |
| `sdiv rd, rn, rm` / `udiv rd, rn, rm` | signed / unsigned quotient; division by zero yields 0 on the machine, an admissible difference because `divisor_nonzero` has been asserted before (Section 5) |
| `and`, `orr`, `eor rd, rn, rm` / `#bitmask` | bitwise; the immediate form only for encodable bitmask immediates |
| `lsl`, `lsr`, `asr rd, rn, rm` / `#imm` | shifts; the register form uses the low 5 or 6 bits of `rm` |
| `sxtb`, `sxth`, `sxtw`, `uxtb`, `uxth` | extensions |
| `sbfx rd, rn, #lsb, #width` / `ubfx` | bitfield extract, sign- or zero-extending |
| `cmp rn, rm` / `cmp rn, #imm12` | sets `nzcv` from `rn - rm` |
| `cset rd, cond` | `rd := 1` if `cond` holds on `nzcv`, else 0 |
| `csel rd, rn, rm, cond` | `rd := cond ? rn : rm` |

**Control.** Pre-allocation EvoMIR has no fall-through: every conditional
form names both successors with their argument lists, and emission lays the
second successor out as the fall-through where block order allows.

| Form | Semantics |
|---|---|
| `b ^target(args)` | unconditional branch |
| `b.cond ^then(args), ^else(args)` | branch on `nzcv` |
| `cbz`/`cbnz rn, ^then(args), ^else(args)` | branch on zero / nonzero |
| `tbz`/`tbnz rn, #bit, ^then(args), ^else(args)` | branch on one bit |
| `bl @f(args) -> results` / `blr rn(args) -> results` | call; the argument and result registers are explicit operands so the allocator sees them as fixed (Section 7); writes `x30` |
| `ret rs...` | return with the result registers listed |
| `brk #imm16` | trap; the immediate is the decimal part of the EvoIR diagnostic the trap or assumption reason carries (`trap.unreachable_reached` is `brk #3024`), which fits sixteen bits and is an authority both documents share |

A conditional terminator does not name its `flags` operand: the parser recovers it from the instruction that Section 2's adjacency rule requires to precede it, while `cset` and `csel` name theirs. This asymmetry is intended (revision 0.3).

Condition codes used: `eq`, `ne`, `lt`, `le`, `gt`, `ge` (signed), `lo`,
`ls`, `hi`, `hs` (unsigned), `vs`, `vc` (signed overflow set / clear),
`cs`, `cc` (carry).

**Pseudo-forms.** Emission expands these; the interpreter executes them as
stated.

| Form | Semantics |
|---|---|
| `frame.slot rd [origin=O, size=S, align=A]` | `rd := the address of a fresh object of S bytes at alignment A`, one per dynamic execution of the instruction, as EvoIR Section 3.7 requires of `alloca`; after allocation the slot is a fixed frame offset (Section 6), and the reuse of that memory by a re-executed `alloca` is sound because EvoIR ends the previous object's lifetime before the site is reached again |
| `mem.copy [dst], [src], #size, #align` | copies `size` bytes from `src` to `dst`, transferring per-byte initializedness and pointer fragments exactly as EvoIR Section 3.8's `copy` does (identical or disjoint ranges); emission expands it to `ldr`/`str` pairs of the largest aligned width, unrolled up to 64 bytes and a counted loop above; the loads of the expansion never report an uninitialized read, because they move representation, not values |
| `spill.store %v, slot` / `spill.load rd, slot` | the store of a virtual register to its spill slot and the reload from it, inserted by the allocator; emission expands them to `str`/`ldr` at the slot's offset from `sp` (revision 0.4) |
| the prologue and epilogue | synthesized at emission from Section 6 and never present as instructions: the interpreter's memory is EvoIR's object model, with no linear stack, no `sp` and no return address, so they are not interpretable; the allocated form is judged by the verifier and the native run (revision 0.4) |
| the register-offset addressing of `mem.copy`'s counted loop | emission's business, like the prologue's pre-indexed `stp` and `frame.slot`'s `add rd, sp, #off`: the loop counts an offset in a register, so the expansion uses the register-offset form of `ldr`/`str`, which the table's immediate rows do not admit and which only emission ever writes (revision 0.5) |

## 5. Lowering: EvoIR to EvoMIR

Lowering maps each extracted EvoIR instruction to a sequence of EvoMIR
instructions by the table below, in block order, keeping block arguments,
and never reorders memory operations: EvoIR's `mem` and `world` tokens are
consumed here, and the order they imposed becomes program order. A form not
in the table is refused with the diagnostic named in Section 10.

| EvoIR | EvoMIR |
|---|---|
| `const.bits<N>` | `mov #imm` |
| `add`, `sub`, `mul`, `and`, `or`, `xor`, `shl`, `lshr`, `ashr` on `bits<N>`, `N <= 64` | the operation, with the extensions of Section 3 |
| `sdiv`, `udiv`, `srem`, `urem` | `sdiv`/`udiv`, remainders by `msub`; `divisor_nonzero` and `signed_div_rem_no_overflow` have been asserted before, which makes the machine's answers for the excluded operands unobservable |
| `divisor_nonzero %d` | zero-extend `%d` per Section 3, `cmp %d, #0` then `cset ne` |
| `signed_div_rem_no_overflow %a, %b` | sign-extend both per Section 3; `cmp %a, #INT_MIN_N` (the minimum of `bits<N>`, materialized), `cset ne` into `%p`; `cmp %b, #-1`, `cset ne` into `%q`; `orr %p, %q` |
| `signed_add_defined %a, %b` / `signed_sub_defined` | at the register width: `adds`/`subs` into a scratch, then `cset vc`; below it: sign-extend both operands, `add`/`sub` at the register width, and compare the result with the sign extension of its low `N` bits (`sbfx` then `cmp`, `cset eq`), because the `v` flag reports the register and never the source width (revision 0.3; `bits<3>`: 3 + 3 is 6, outside -4..3, so 0) |
| `signed_mul_defined %a, %b` | 32-bit: `smull` the sign-extended operands, compare with the sign extension of its low 32 bits, `cset eq`; 64-bit: `mul` and `smulh`, compare the high word with `asr` of the low word by 63, `cset eq`; any other `N` sign-extends to the next register width and compares the product with the sign extension of its low `N` bits (revision 0.3) |
| `shift_count_defined.<s> %x, %amt` | extend `%amt` by its declared signedness `<s>` to the register width, `cmp` with `#N`, `cset lo`: a sign-extended negative amount exceeds every bound up to 64 as an unsigned quantity, so one unsigned comparison serves both signednesses (revision 0.3; zero-extending a signed `bits<1>` amount of 1 had answered 1 where EvoIR decodes -1 and answers 0) |
| `signed_shl_defined.<s> %x, %amt` | the conjunction of four `bits<1>`: `shift_count_defined` as above; `%x` non-negative (`cmp` with `#0` after sign extension, `cset ge`); the round trip `(x << amt) >> amt == x` with an arithmetic right shift at the register width; and, below the register width, the shifted result representable in signed `bits<N>` (compare with the sign extension of its low `N` bits), as EvoIR 3.3.1 defines the predicate (revision 0.3) |
| `sext`, `zext`, `trunc` | the extension of Section 3, or nothing for `trunc` |
| `icmp.*` on integers | the extensions of Section 3, `cmp`, then `cset`; when the only consumer is a `cond_br` in the same block, `cmp` then `b.cond` with no `cset` |
| `icmp.eq`/`ne` on pointers | the same, 64-bit, no extension |
| `ptr.null` | `mov #0` |
| `global_addr @g` / `func_addr @f` | `adrp` + `add` with the symbol spellings of Section 4 |
| `ptr.offset.<direction>.<signedness>` | the index extended per the signedness, scaled by `lsl` when the scale is a power of two and `mul` otherwise, then `add` for the add direction and `sub` for the subtract direction; the machine wraps where EvoIR computes in unbounded arithmetic and reports `E3011`, an admissible difference because the pointer-formation guard has been asserted before |
| `alloca` (static size and alignment) | `frame.slot` at the instruction's program point |
| `lifetime.end` | nothing |
| `pointer_valid`, `access_in_bounds` | nothing: the guards are the obligations the front end already lowered into `ub.assume`; the instruction itself has no machine effect |
| `load [volatile]` of `bits<N>`, `N <= 64` | `ldr` of the matching width, with the marker |
| `load [volatile]` of a pointer | `ldr.p` / `ldr.pf` |
| `store [volatile]` | `str` of the matching width, `str.p` for a pointer |
| `copy` with static size | `mem.copy` |
| `br ^b(args)` | `b ^b(args)` |
| `cond_br %c, ^t(...), ^e(...)` | `tbnz %c, #0, ^t(...), ^e(...)` (or the `b.cond` fusion above) |
| `ret v...` | `ret` listing the results |
| `trap.<reason>` | `brk` with the reason's number |
| `ub.assume.<reason> %c` | checked profile: `tbz %c, #0, ^trap_reason(), ^continue(live...)`, where `^continue` is a fresh block whose arguments are exactly the registers the rest of the source block still reads, ordered by register number, and `^trap_reason` holds `brk` for the reason; one trap block per reason, appended after the source blocks in order of first use. Unchecked profile: nothing. The profile is a driver flag recorded in the receipt, default checked |
| `call @f(args, %m, %w)` / `call %p(...)` | `bl`/`blr` with explicit argument and result registers (Section 6) |
| `uninitialized.bits<N>` | `mov #0` (any value is permitted; zero is chosen so the output is deterministic) |

The UB predicates were absent from revision 0.1's table, which refused
19,410 of 20,000 generated functions; they are ordinary boolean
computations and lower whether or not a `ub.assume` consumes them.

Every other EvoIR form is refused: wide integers, floating point, vectors,
tuples, varargs, computed goto, frame address, aggregate arguments and
results, and pointer difference each have a code in Section 10.

## 6. Frame and Calling Convention (Darwin arm64, scalar)

The convention is Apple's AArch64 variant of AAPCS64, restricted to what
this revision lowers; the restriction is refused, never approximated.

**Arguments and results.** Integer and pointer arguments of at most 64 bits
go in `x0` to `x7` in order. Between two EvoCC-compiled functions, a
`bits<N>` argument with `N < 64` is passed in the low bits of its register
with the upper bits unspecified, and the callee applies Section 3 before
reading it as anything but a low-`N`-bit value. Across the host boundary the
platform rule governs instead: a caller extends an argument narrower than 32
bits to 32 bits, zero-extending an unsigned source type and sign-extending a
signed one, and a callee may assume the bits above its parameter's width are
that extension (Apple's AArch64 variant; revision 0.5, after the executor
found the two rules unreconciled and the probe passed only because each host
function masked its own argument). A cross-boundary call therefore inserts
the extension the platform requires; a call between EvoCC functions does
not. A ninth integer argument is refused (`E4010`); an aggregate argument
or result is refused (`E4011`); a variadic call or callee is refused
(`E4012`, the EvoIR `E3030` boundary). **Results:** an EvoMIR function
returns up to eight integer or pointer results in `x0` to `x7` in order.
C lowering never produces more than one, so the C ABI is unaffected; the
convention exists so that the machine lane of Section 9 can execute the
multi-result functions the randomized generator writes (revision 0.1's
single-result rule refused 16,926 of 20,000). A ninth result is `E4013`.
Measured against the reference compilers before this revision is first
used for a call across the boundary (Section 9 names the probe).

**Preserved registers.** `x19` to `x28`, `x29`, `x30` and `sp` are
callee-saved; `x0` to `x17` are caller-saved; `x18` is untouchable.

**Frame layout.** Every function keeps a frame pointer. The prologue is

```text
stp x29, x30, [sp, #-16]!
mov x29, sp
sub sp, sp, #frame          ; frame is a multiple of 16
stp/str of each used callee-saved register into its slot
```

and the epilogue reverses it. Below the saved pair, in order: the
callee-saved save area, then one slot per `frame.slot` (its declared size,
rounded to its declared alignment, laid out in origin order from the top of
the frame downward), then the spill area (one 8-byte slot per spilled
virtual register), then outgoing stack arguments (none in this revision).
`sp` stays 16-byte aligned at every call. The frame is static: no dynamic
`alloca` exists in EvoIR. Every slot (object, spill, save area) is addressed
from `sp` with a non-negative scaled offset, never from `x29`, so that frames
above 256 bytes stay inside the unsigned twelve-bit immediate form; `x29`
serves the unwinder and the debugger, not the code (revision 0.4).

**Stack arguments** (beyond `x7`) are not lowered in this revision; Apple's
rule packs them by natural alignment rather than in 8-byte slots, and that
rule enters with the measurement that enables it.

## 7. Out-of-SSA and Register Allocation

**Out-of-SSA.** Block arguments are removed by placing, on each edge, the
parallel copy from the values the terminator passes to the target's
arguments, resolved into a sequence of moves with at most one scratch: a
reserved virtual register that the allocator pins to `x16`, which is
excluded from the allocatable set, so that the pass stays on virtual
registers (revision 0.4). Where the copy goes is a rule, not a choice: at
the end of the predecessor when the predecessor has one successor; at the
top of the target when the target has one predecessor; otherwise the edge
is split by inserting a block. A copy placed at the end of a predecessor
with two successors would run on both edges, and a copy placed between a
`cmp` and its `b.cond` would break the adjacency rule of Section 2; the
allocation verifier found both before this sentence existed. The entry
block's arguments have no incoming edge and survive this pass as the
function's parameters; the allocator removes them by assigning `x0`
onwards. This happens before allocation, as its own pass; the lowering of
Section 5 does not do it.

**Linear scan.** Instructions are numbered in block order (the order of
Section 2); a virtual register's live interval is from its definition to
its last use, extended across any block in which it is live on entry to
cover the whole block. The allocator walks intervals by start point,
assigns a free physical register of the class, and on exhaustion spills the
interval with the furthest next use to a frame slot; a spilled value is
reloaded before each use and stored after its definition. Fixed operands
(argument and result registers at calls and at the function boundary,
`x16`/`x17` in emitted sequences) are pre-assigned intervals the allocator
must respect; a call clobbers every caller-saved register, so an interval
live across a call is assigned a callee-saved register or spilled. Ties are
broken by virtual register number, and among free registers the lowest-numbered
one of the class is taken, callee-saved only for an interval that spans a call
(revision 0.4), so the allocation is deterministic and the emitted text is a
stable digest. No coalescing beyond removing a move whose two operands got
the same register.

The output is verified, not trusted (Section 9): every physical register
read is defined on every path, no two simultaneously live values share a
register, and every `flags` producer is adjacent to its consumer.

## 8. Emission

The assembly text is the platform assembler's syntax. Functions are emitted
in module order, each as `.globl`/`.p2align 2`/label/body; globals as data
in `.data`/`.bss`/`.rodata` (`__DATA,__data` etc. on Darwin) with their
EvoIR initial bytes; symbols get the platform prefix (`_` on Darwin). The
only relocations this revision emits are the page-relative pair for
`global_addr` and `func_addr` and the branch relocation of `bl`; a pointer
inside static data (EvoIR `E3026`) is refused. The emitted text is
deterministic and is what the receipt digests.

## 9. Verification

Three instruments, each independent of the lowering:

1. **The EvoMIR interpreter.** Every Section 4 form has executable
   semantics; an interpreter executes an EvoMIR function on the same inputs
   as the EvoIR interpreter and the two must agree on results, traps and the
   volatile trace. The randomized semantic gate of the middle end runs a
   third lane: EvoIR before, EvoIR after optimization, EvoMIR after
   lowering, all three agreeing or the program is a mismatch to shrink. A
   lowering refusal is counted by code, not compared. This lane exists
   before any native run, so lowering defects are found without an
   assembler in the loop.
2. **The allocation verifier.** A separate pass over the allocated function
   checks the three properties of Section 7 by dataflow; the allocator does
   not certify itself.
3. **Native differential runs.** The assembled program is run on the host
   and its output compared with the reference compiler's build of the same C
   source (the layout oracle's GCC-14 on the sealed container for Linux;
   the host toolchain for Darwin, recorded by version). The native suite of
   the roadmap's B4 is the corpus; the receipt records every program's
   output digest under both compilers.

**The native corpus.** The randomized generator's seed space is pinned by
the middle-end campaigns (a seed names one module forever), so the native
lane does not change it. It draws from a second generator mode with its own
seed space, `native-main`, whose `main` takes no arguments and returns one
`bits<32>`, and its programs are compared by exit status modulo 256, the
eight bits a process status carries (revision 0.4).

A probe precedes every calling-convention sentence that is used: before the
first cross-boundary call is emitted, a probe program compiled by the host
compiler calls EvoCC-compiled functions with each argument shape this
revision admits, and vice versa, and the receipt names it.

## 10. Diagnostics

All are reported at the EvoIR instruction that could not be lowered; the
driver emits no assembly for a function with any of them. Five codes
(`E4005` to `E4008`, `E4012`) name forms a validated EvoIR module cannot
contain at revision 1.9.56, because EvoIR's own fail-closed codes exclude
them earlier; they are defined so that the boundary is named from both
sides, and are never emitted today.

| Code | Name | Meaning |
|---|---|---|
| `E4001` | `not-lowered: floating-point` | `f32`/`f64` operand or result |
| `E4002` | `not-lowered: vector` | `vec` operand or result |
| `E4003` | `not-lowered: wide-integer` | `bits<N>` with `N > 64` |
| `E4004` | `not-lowered: tuple` | tuple-typed value |
| `E4005` | `not-lowered: static-pointer-data` | a pointer inside static initial bytes |
| `E4006` | `not-lowered: indirect-branch` | computed goto (EvoIR `E3029` boundary) |
| `E4007` | `not-lowered: frame-address` | EvoIR `E3031` boundary |
| `E4008` | `not-lowered: pointer-difference` | EvoIR `E3032` boundary |
| `E4009` | `not-lowered: unsupported-form` | any EvoIR instruction absent from Section 5 |
| `E4010` | `abi: too-many-register-arguments` | more than eight integer arguments |
| `E4011` | `abi: aggregate-by-value` | aggregate argument or result |
| `E4012` | `abi: variadic` | variadic callee or caller |
| `E4013` | `abi: too-many-results` | more than eight ordinary results |
| `E4020` | `allocation: verifier-refused` | the allocation verifier found a violation; internal, must never reach a user of a released build |

## Appendix A. Textual Form

The textual form is canonical (one rendering per function, the EvoIR
Section 9.4 byte rules: four-space indent, `", "` between operands, one
blank line between blocks and between functions, one final newline) and
round-trips through a parser, so that tests can state EvoMIR by hand and
the receipt can digest it.

```text
function        := "func @" ident " {" newline block+ "}" newline
block           := "^" ident "(" params ")" ":" newline (indent instruction newline)* indent terminator newline
params          := [ "%v" int ":" class ("," " " "%v" int ":" class)* ]
class           := "gpr32" | "gpr64" | "flags"
instruction     := "%v" int ":" class " = " op operands
                 | "%v" int ":" class ", %v" int ":flags = " ("adds"|"subs") ".w"|".x" operands   ; a register and the flags
                 | "%v" int ":" class " = " op ".w"|".x" operands        ; width suffix on two-width forms
                 | "%v" int ":" class " = frame.slot [origin=" ident ", size=" int ", align=" int "]"
                 | "mem.copy [" reg "], [" reg "], #" int ", #" int
                 | store-form [" volatile"]
                 | call-form
terminator      := "b ^" ident "(" args ")"
                 | ("b." cond | "cbz " reg | "cbnz " reg | "tbz " reg ", #" int | "tbnz " reg ", #" int) ", ^" ident "(" args ")" ", ^" ident "(" args ")"
                 | "ret" [" " reg ("," " " reg)*]
                 | "brk #" int
call-form       := "bl @" ident "(" args ")" [" -> " results] | "blr " reg "(" args ")" [" -> " results]
symbol          := "@" ident "@PAGE" | "@" ident "@PAGEOFF" | "@" ident "@FUNCPAGE" | "@" ident "@FUNCPAGEOFF"
```

Before allocation (the entry block's label is spelled, as every block's is):

```text
func @dot {
^entry(%v0:gpr64, %v1:gpr64):
    %v2:gpr32 = ldr.w [%v0, #0]
    %v3:gpr32 = ldr.w [%v1, #0]
    %v4:gpr32 = mul.w %v2, %v3
    ret %v4
}
```

After allocation the same function names physical registers and frame
slots, and block arguments are gone:

```text
func @dot {
entry:
    w2 = ldr.w [x0, #0]
    w3 = ldr.w [x1, #0]
    w0 = mul.w w2, w3
    ret
}
```

## Appendix B. Revision History

| Revision | Date | Change |
|---|---|---|
| 0.5 | 2026-09-04 | 256 generated programs assembled, linked, run and agreeing with the interpreter on exit status; a fourth gate lane; a sixteen-shape calling-convention probe, 32 comparisons, 0 failures. States the cross-boundary argument extension the platform requires, distinct from the internal rule, and records that emission may write addressing forms the Section 4 tables do not admit. |
| 0.4 | 2026-09-04 | First native runs: five assembled programs agree with the interpreter on exit status. Rules the allocation and emission findings: the out-of-SSA scratch is a pinned virtual register, copy placement is a three-way rule, entry arguments become parameters the allocator assigns, the lowest free register is taken, spill accesses are pseudo-forms, frame slots are addressed from `sp`, the prologue is synthesized at emission and not interpretable; the native lane uses a second generator mode so pinned seeds stay pinned. |
| 0.3 | 2026-09-04 | Second implementation: 20,000 of 20,000 generated programs lowered and judged by the three-lane gate with 0 mismatches. Rules the executor's five predicate-row findings: overflow flags report the register, so narrower widths compare with the sign extension of the low N bits (add, sub, mul); shift amounts extend by their declared signedness; the signed left-shift predicate conjoins count, sign, round trip and representability; the division predicates extend first; the grammar spells a two-definition adds/subs and states the conditional-terminator asymmetry. |
| 0.2 | 2026-09-04 | Rules the thirteen stops of the first implementation: lowering rows for every UB predicate; `icmp.eq`/`ne` moved to the zero-extended row after a measured counterexample; `frame.slot` as a per-execution pseudo-form reconciling `alloca` with EvoIR 3.7; `ldr.p`/`ldr.pf`/`str.p` for provenance; a grammar for the textual form with the executor's spellings (symbols, the volatile marker, explicit call registers, two-successor conditionals, spelled entry labels); the `ub.assume` continuation-block construction; `brk` immediates as the diagnostic's decimal part; shift amounts and `ptr.offset` wrapping as obligation-covered differences; `mem.copy` as a pseudo-form transferring initializedness and provenance; block arguments kept until the Section 7 pass; up to eight results in `x0`..`x7` for the machine lane; the five unreachable codes named as such. |
| 0.1 | 2026-09-04 | First statement: purpose, register classes, sub-width representation, the AArch64 scalar instruction subset with executable semantics, the lowering table with fail-closed boundaries, the Darwin arm64 scalar calling convention and frame layout, out-of-SSA and linear scan, emission, three verification instruments, and the `E4xxx` diagnostics. |
