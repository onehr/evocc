# EvoMIR Specification

**Revision 0.1** | **Authoritative reference** | **2026-09-04**

EvoMIR is the machine-level intermediate representation of EvoCC. It sits
between the extracted EvoIR of a function (`docs/evoir-spec.md`, the EvoIR
specification, which this document cites by section) and the assembly text
handed to the platform assembler. Everything below the extraction boundary
is governed here; everything above it is governed by the EvoIR
specification, and where the two meet, the EvoIR specification wins.

This document follows the rules of its parent: a sentence about observable
behaviour is measured before it is written, a construct it does not admit is
refused with a named diagnostic rather than guessed, and an executor that
finds it silent stops and reports the gap.

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

The revision 0.x series covers what the roadmap calls B1 through B4: EvoMIR
itself, a linear-scan register allocator, the Darwin arm64 scalar ABI, and
the native test suite. The EvoMIR representations do not yet enter e-classes;
the invariant of EvoIR Section 10.2 applies the day they do and is not
exercised before.

## 2. Values, Registers and Classes

An EvoMIR function is a list of basic blocks in the order of its EvoIR
source, each holding instructions in order and ending in a terminator.
Before register allocation, operands are **virtual registers** `%vN` in
block-argument SSA form, the same discipline as EvoIR Section 2, so that a
block's arguments are its only inputs and every value has one definition.
After register allocation, operands are **physical registers** and **frame
slots**, and block arguments are gone (Section 7).

Every virtual register has exactly one **register class**:

| Class | Holds | Physical registers |
|---|---|---|
| `gpr32` | `bits<N>` with `1 <= N <= 32` | `w0` to `w30` |
| `gpr64` | `bits<N>` with `33 <= N <= 64`, every pointer | `x0` to `x30` |
| `flags` | the result of a comparison, consumed by the next conditional | `nzcv` |

`gpr32` and `gpr64` name the same physical register file viewed at two
widths; the allocator assigns one physical register per virtual register and
the class fixes the spelling. There is no floating-point class in this
revision; `f32`, `f64` and `vec` values are refused (Section 10).

A `flags` value may be consumed only by the instruction that immediately
follows its producer in the same block, and nothing between them may write
flags. This is a legality rule the verifier of Section 9 checks, not a hint.

**Reserved physical registers.** `x18` is reserved by the platform on Darwin
and is never allocated; `x16` and `x17` are the intra-procedure-call
scratch registers and are used only by emitted call sequences and never
hold a live value across an instruction the allocator did not emit; `x29`
is the frame pointer and `x30` the link register, both saved in every
prologue (Section 6); `sp` is not a general register. `x31` is `sp` or the
zero register `xzr`/`wzr` by position; the zero register is an operand,
never a destination.

## 3. Sub-width Values

EvoIR values of width `N < 32` live in `gpr32` registers with the bits above
`N` **unspecified**. An instruction that reads such a value reads only the
low `N` bits, except that the following operations first bring the value
into the form they need, and the lowering inserts the extension explicitly:

| Operation on `bits<N>`, `N < 32` (or `N < 64` in `gpr64`) | Form needed | Inserted before |
|---|---|---|
| `add`, `sub`, `mul`, `and`, `or`, `xor`, `shl`, `trunc`, `icmp.eq`, `icmp.ne` | none: the low `N` bits of the result depend only on the low `N` bits of the operands | nothing |
| `lshr`, `udiv`, `urem`, `icmp.ult`, `icmp.ule`, `zext` | zero-extended to the register width | `and` with the mask `2^N - 1` (`uxtb`/`uxth` when `N` is 8 or 16) |
| `ashr`, `sdiv`, `srem`, `icmp.slt`, `icmp.sle`, `sext` | sign-extended to the register width | `sbfx` from bit `N - 1` (`sxtb`/`sxth` when `N` is 8 or 16) |
| shift amount operand `bits<M>` | zero-extended, then masked to the width of the shifted value by the instruction itself | `and` with `2^M - 1` when `M < 32` |
| a value stored with `store` of width `N` | none: the store writes `ceil(N / 8)` bytes and the bits above `N` in the last byte are those of the value, which EvoIR Section 5 defines as the representation | nothing |
| `cond_br` on `bits<1>` | bit 0 only | nothing: `tbnz`/`tbz` on bit 0 |

`bits<1>` therefore lives in a `gpr32` with bit 0 significant. Comparisons
produce their `bits<1>` result with `cset`, which writes 0 or 1 into the
whole register; a `bits<1>` produced by `trunc` or `and` has unspecified
upper bits, and the table above is what makes that safe.

Widths above 64 are not lowered (`E4003`).

## 4. Instruction Set

An EvoMIR instruction names an AArch64 operation, its destination register
class and its operands. The forms this revision emits are exactly these; an
operation absent from the table is not part of EvoMIR and the lowering
never produces it. Each row states the semantics the EvoMIR interpreter
(Section 9) implements, so that the table is executable, not descriptive.

**Data movement.**

| Form | Semantics |
|---|---|
| `mov rd, rn` | `rd := rn` |
| `mov rd, #imm` (`movz`/`movk`/`movn` sequence) | `rd := imm`, any 32- or 64-bit constant, at most four instructions |
| `adrp rd, sym@PAGE` + `add rd, rd, sym@PAGEOFF` | `rd := address of the global sym` (Darwin spelling; Linux uses `:pg_hi21:` and `:lo12:`) |
| `ldr rd, [rn, #off]` / `ldrb` / `ldrh` / `ldrsb` / `ldrsh` / `ldrsw` | `rd := memory[rn + off]` of 8, 4, 2 or 1 bytes, zero- or sign-extended as the mnemonic says; `off` is a scaled unsigned 12-bit immediate or a signed 9-bit immediate |
| `str rd, [rn, #off]` / `strb` / `strh` | `memory[rn + off] := low bytes of rd` |
| `ldp`/`stp rt1, rt2, [rn, #off]` | two registers, prologue and epilogue only |

**Arithmetic and logic.** Every form has a 32-bit (`w`) and a 64-bit (`x`)
spelling chosen by the destination class.

| Form | Semantics |
|---|---|
| `add rd, rn, rm` / `add rd, rn, #imm12` | wrapping addition |
| `sub rd, rn, rm` / `sub rd, rn, #imm12` | wrapping subtraction |
| `mul rd, rn, rm` | wrapping multiplication, low bits |
| `msub rd, rn, rm, ra` | `rd := ra - rn * rm` (remainders) |
| `sdiv rd, rn, rm` / `udiv rd, rn, rm` | signed / unsigned quotient; division by zero yields 0 on the machine, which is why EvoIR's `divisor_nonzero` guard must have been asserted before this instruction is reached (Section 5) |
| `and`, `orr`, `eor rd, rn, rm` / `#bitmask` | bitwise; the immediate form only for encodable bitmask immediates |
| `lsl`, `lsr`, `asr rd, rn, rm` / `#imm` | shifts; the register form uses the low 5 or 6 bits of `rm` |
| `sxtb`, `sxth`, `sxtw`, `uxtb`, `uxth` | extensions |
| `sbfx rd, rn, #lsb, #width` / `ubfx` | bitfield extract, sign- or zero-extending |
| `cmp rn, rm` / `cmp rn, #imm12` | sets `nzcv` from `rn - rm` |
| `cset rd, cond` | `rd := 1` if `cond` holds on `nzcv`, else 0 |
| `csel rd, rn, rm, cond` | `rd := cond ? rn : rm` |

**Control.**

| Form | Semantics |
|---|---|
| `b label` | unconditional branch |
| `b.cond label` | branch on `nzcv` |
| `cbz`/`cbnz rn, label` | branch on zero / nonzero |
| `tbz`/`tbnz rn, #bit, label` | branch on one bit |
| `bl sym` / `blr rn` | call; writes `x30` |
| `ret` | return through `x30` |
| `brk #imm16` | trap; the immediate names the EvoIR trap or assumption reason (Section 5) |

Condition codes used: `eq`, `ne`, `lt`, `le`, `gt`, `ge` (signed), `lo`,
`ls`, `hi`, `hs` (unsigned).

## 5. Lowering: EvoIR to EvoMIR

Lowering maps each extracted EvoIR instruction to a sequence of EvoMIR
instructions by the table below, in block order, and never reorders memory
operations: EvoIR's `mem` and `world` tokens are consumed here, and the
order they imposed becomes program order. A form not in the table is refused
with the diagnostic named in Section 10.

| EvoIR | EvoMIR |
|---|---|
| `const.bits<N>` | `mov #imm` |
| `add`, `sub`, `mul`, `and`, `or`, `xor`, `shl`, `lshr`, `ashr` on `bits<N>`, `N <= 64` | the operation, with the extensions of Section 3 |
| `sdiv`, `udiv`, `srem`, `urem` | `sdiv`/`udiv`, remainders by `msub`; the EvoIR program has asserted `divisor_nonzero` and `signed_div_rem_no_overflow` before (they are `ub.assume` obligations and lower as such) |
| `sext`, `zext`, `trunc` | the extension of Section 3, or nothing for `trunc` |
| `icmp.*` on integers | `cmp` then `cset`; when the only consumer is a `cond_br` in the same block, `cmp` then `b.cond` with no `cset` |
| `icmp.eq`/`ne` on pointers | the same, 64-bit |
| `ptr.null` | `mov #0` |
| `global_addr @g` | `adrp` + `add` (Section 8 names the relocation) |
| `func_addr @f` | `adrp` + `add` of the function symbol |
| `ptr.offset` | `add` (scale by `lsl` first when the scale is a power of two, `mul` otherwise); the in-bounds guard has been asserted before |
| `alloca` (static size and alignment) | a frame slot (Section 6); the pointer is `add rd, x29, #slot` |
| `lifetime.end` | nothing |
| `pointer_valid`, `access_in_bounds` | nothing: these are the guards the front end already lowered into `ub.assume` obligations; the instruction itself has no machine effect |
| `load [volatile]` of `bits<N>`, `N <= 64`, or a pointer | `ldr` of the matching width; `volatile` adds nothing on this target but forbids merging with a neighbouring access (which this revision never does) |
| `store [volatile]` | `str` of the matching width |
| `copy` with static size | a loop of `ldr`/`str` pairs of the largest aligned width, unrolled up to 64 bytes, a counted loop above |
| `br ^b(args)` | parallel copies for the block arguments (Section 7) then `b` |
| `cond_br %c, ^t(...), ^e(...)` | `tbnz %c, #0` (or the `b.cond` fusion above), copies on each edge |
| `ret v...` | move results into `x0`.. (Section 6) then epilogue and `ret` |
| `trap.<reason>` | `brk #imm` with the reason's number |
| `ub.assume.<reason> %c` | checked profile: `tbz %c, #0, trap_label` where the label holds `brk #imm` for the reason; unchecked profile: nothing. The profile is a driver flag recorded in the receipt, default checked |
| `call @f(args, %m, %w)` / `call %p(...)` | argument moves, `bl`/`blr`, result moves (Section 6) |
| `uninitialized.bits<N>` | `mov #0` (any value is permitted; zero is chosen so the output is deterministic) |

Every other EvoIR form is refused: division and shift guards that were not
asserted are not a lowering concern (they are the program's), but wide
integers, floating point, vectors, tuples, varargs, computed goto, frame
address, aggregate arguments and results, and pointer difference each have a
code in Section 10.

## 6. Frame and Calling Convention (Darwin arm64, scalar)

The convention is Apple's AArch64 variant of AAPCS64, restricted to what
this revision lowers; the restriction is refused, never approximated.

**Arguments and results.** Integer and pointer arguments of at most 64 bits
go in `x0` to `x7` in order; a `bits<N>` argument with `N < 64` is passed
in the low bits of its register with the upper bits unspecified, and the
callee applies Section 3 before reading it as anything but a low-`N`-bit
value. A ninth integer argument is refused (`E4010`); an aggregate argument
or result is refused (`E4011`); a variadic call or callee is refused
(`E4012`, the EvoIR `E3030` boundary). A single integer or pointer result
returns in `x0`; a function with more than one ordinary result is refused
(`E4013`). Measured against the reference compilers before this revision is
first used for a call across the boundary (Section 9 names the probe).

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
callee-saved save area, then one slot per `alloca` (its declared size,
rounded to its declared alignment, laid out in declaration order from the
top of the frame downward), then the spill area (one 8-byte slot per spilled
virtual register), then outgoing stack arguments (none in this revision).
`sp` stays 16-byte aligned at every call. The frame is static: no dynamic
`alloca` exists in EvoIR.

**Stack arguments** (beyond `x7`) are not lowered in this revision; Apple's
rule packs them by natural alignment rather than in 8-byte slots, and that
rule enters with the measurement that enables it.

## 7. Out-of-SSA and Register Allocation

**Out-of-SSA.** Block arguments are removed by placing, on each edge, the
parallel copy from the values the terminator passes to the target's
arguments, resolved into a sequence of moves with at most one scratch
register (`x16`) for cycles. Critical edges are split by inserting a block.
This happens before allocation, on virtual registers, so the allocator sees
ordinary moves.

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
broken by virtual register number, so the allocation is deterministic. No
coalescing beyond removing a move whose two operands got the same register.

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
   as the EvoIR interpreter and the two must agree on results and on the
   volatile trace. The randomized semantic gate of the middle end gains a
   third lane: EvoIR before, EvoIR after optimization, EvoMIR after
   lowering, all three agreeing or the program is a mismatch to shrink.
   This lane exists before any native run, so lowering defects are found
   without an assembler in the loop.
2. **The allocation verifier.** A separate pass over the allocated function
   checks the three properties of Section 7 by dataflow; the allocator does
   not certify itself.
3. **Native differential runs.** The assembled program is run on the host
   and its output compared with the reference compiler's build of the same C
   source (the layout oracle's GCC-14 on the sealed container for Linux;
   the host toolchain for Darwin, recorded by version). The native suite of
   the roadmap's B4 is the corpus; the receipt records every program's
   output digest under both compilers.

A probe precedes every calling-convention sentence that is used: before the
first cross-boundary call is emitted, a probe program compiled by the host
compiler calls EvoCC-compiled functions with each argument shape this
revision admits, and vice versa, and the receipt names it.

## 10. Diagnostics

All are reported at the EvoIR instruction that could not be lowered; the
driver emits no assembly for a function with any of them.

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
| `E4013` | `abi: multiple-results` | more than one ordinary result |
| `E4020` | `allocation: verifier-refused` | the allocation verifier found a violation; internal, must never reach a user of a released build |

## Appendix A. Textual Form

Before allocation:

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

The textual form is canonical (one rendering per function) and round-trips
through a parser, so that tests can state EvoMIR by hand and the receipt can
digest it.

## Appendix B. Revision History

| Revision | Date | Change |
|---|---|---|
| 0.1 | 2026-09-04 | First statement: purpose, register classes, sub-width representation, the AArch64 scalar instruction subset with executable semantics, the lowering table with fail-closed boundaries, the Darwin arm64 scalar calling convention and frame layout, out-of-SSA and linear scan, emission, three verification instruments, and the `E4xxx` diagnostics. Nothing implemented yet; the calling-convention sentences are to be probed before first use. |
