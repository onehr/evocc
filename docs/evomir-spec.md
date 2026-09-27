# EvoMIR Specification

**Revision 0.50** | **Authoritative reference** | **2026-09-27**

Function-placement addendum (revision 0.34): the associated validated EvoIR
module carries each definition's optional `section`, just as it carries alignment
and patchable-entry metadata. This association remains an explicit input through
lowering/allocation/native emission; standalone MIR text cannot replace it. Linux
selects `.section "name","ax",%progbits` before that function's alignment,
entry padding, symbol and complete body, or ordinary `.text` when absent.
Local block-address labels, CFI and `.size` remain in that same section, and
`__patchable_function_entries` retains SHF_LINK_ORDER association with the actual
code section. Aliases share the target symbol and allocate no body; section names
do not alter visibility or retention. The next function reselects its own section,
including the default, and strings are assembler-escaped rather than interpolated
as directives. Unsupported non-ELF function placement is E4009. Linker scripts
retain ownership of final placement/range checking; no warning suppression,
source-specific section inference or invented relocation replaces this contract.

Individual-section addendum (revision 0.49): Linux `-ffunction-sections` and
`-fdata-sections` independently select symbol-named ELF sections for definitions
without explicit placement. Each option and its `-fno-` form is reduced in written
order, last occurrence winning. Both default off; explicit section attributes
take precedence. These are native emission options, not new EvoIR object kinds
or permission changes. The validated module and effective option values remain
explicit emission inputs, including in any native artifact identity.

Functions use `.text.<symbol>` (AX, PROGBITS), with all local labels, alignment,
CFI, complete bodies and sizes in the same section. Automatic objects use
`.data.<symbol>` (AW, PROGBITS), `.bss.<symbol>` (AW, NOBITS) for mutable zero-fill,
or `.rodata.<symbol>` (A, PROGBITS) for read-only data, including all-zero
read-only data. Read-only objects with PIC relocations use
`.data.rel.ro.<symbol>` (AW, PROGBITS). TLS uses `.tdata.<symbol>` (AWT, PROGBITS)
or `.tbss.<symbol>` (AWT, NOBITS). Symbol size, alignment, linkage, visibility,
relocations and TLS type remain unchanged. Names use the existing assembler
escaping, not arbitrary directive interpolation. Aliases allocate no extra
section or body. Lifecycle registrations remain init/fini-array roots; patchable
entries retain SHF_LINK_ORDER to the selected code section. Module assembly is
still bracketed by default text selection, not rewritten to follow these flags.

These options permit linker garbage collection; they do not guarantee retention
of an unreferenced `used` symbol, reorder source effects, duplicate definitions
or change the existing reference/retention analysis. The linker owns reachability
and final layout. Unsupported non-ELF individual-section options fail E4009;
they must not be silently ignored. The disabled/default path retains its previous
assembly. No semantic or EqSat rule is added.

Native command-option addendum (2026-09-13): validated EvoIR `stack_protect`
marks select an extra private canary slot above local storage; emission loads
the configured global or sysreg-plus-byte-offset guard at entry and compares
it before every normal return, calling `__stack_chk_fail` on mismatch.
The original validated module and effective codegen options remain explicit
inputs to native emission, including through allocation; neither is inferred
from standalone MIR text. `pac-ret` signs LR with the incoming SP before saving
it and authenticates after restoring that same SP; return-address builtins
strip authentication bits. General-register-only emission omits SIMD vararg
saves; it does not enable floating-point lowering. Positive, failure and
over-aligned/dynamic-frame results are in `tests/fixtures/protected-frames.out`.

Patchable-entry addendum (2026-09-20): the associated validated EvoIR module
also supplies each emitted function's `patchable_entry=N,M`; standalone MIR
text is not a replacement for that metadata input. Linux emission places M
NOPs before the callable symbol and N-M immediately after it, before PAC,
frame setup or any user instruction. CFI begins at the callable symbol and
the function's size excludes its preceding pad. For N>0, an aligned 64-bit
relocation in `__patchable_function_entries` names the first NOP and uses
SHF_LINK_ORDER to associate the table entry with its code section; N=0 emits
neither NOPs nor an entry. Aliases retain the callable symbol's identity,
not a second patch region. The callable symbol still meets EvoIR's explicit
alignment: any required extra padding precedes the patch region. Nonzero
patching on Darwin is refused, not emitted without its ELF registration.

Strict-alignment addendum (2026-09-13): scalar MIR loads/stores retain the
EvoIR access guard's alignment as optional ` [align=N]` after the address;
N is a positive power of two, and omission retains natural alignment.
The abstract operation still transfers the whole value, including pointer
provenance. Under `-mstrict-align`, emission expands an insufficiently aligned
scalar access into byte loads/stores using reserved scratch registers;
128-bit pairs use two individually alignment-qualified halves. Ordinary
copy already chooses its largest safe width. `-mno-strict-align` restores
ordinary emission; the last option wins. Measurements are in
`tests/fixtures/strict-alignment.out`.

The effective tiny code model emits `adr` for direct symbol addresses and
literal GOT loads for interposable addresses. Requested unwind tables use
the stable x29 CFA, explicit saved-register locations (DWARF expressions for
over-aligned frames), per-return state restoration and PAC state toggles;
they do not depend on the dynamically moving SP. The last explicit table
enable/disable option wins. Native backtrace measurements are in
`tests/fixtures/unwind-and-tiny.out`.

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
| `simd32` | f32 | `s0` to `s31` |
| `simd64` | f64 or 8-byte integer vector | `d0` to `d31` |
| `simd128` | hosted Linux f128 or 16-byte integer vector | `q0` to `q31` |
| `flags` | the result of a comparison or a flag-setting arithmetic form, consumed by the next conditional | `nzcv` |

`gpr32` and `gpr64` name the same physical register file viewed at two
widths; the allocator assigns one physical register per virtual register and
the class fixes the spelling. A `bits<128>` EvoIR value has no class of its
own: lowering represents it as **two `gpr64` virtual registers**, the low
half first, and every operation on it becomes a sequence over the halves
(Section 5, revision 0.10). The allocator, the verifier and the interpreter
see only 64-bit registers; the pair is a fact of lowering and of the calling
convention (Section 6), never of allocation. Scalar FP and vector
classes share one physical bank, including all s/d/q aliases. v30/v31 are emission scratch; v8..v15 are
not allocated for vector temporaries because their upper halves are not
callee-saved. Live-across-call vectors spill at full width. Integer SIMD does
not bypass `-mgeneral-regs-only`; scalar FP is separately admitted by revision 0.33.

**Scalar FP (0.33).** `fp.OP.f32/f64` preserves the operation and format from
EvoIR 3.13; constants use exact GPR-to-SIMD bit moves. Scalar arithmetic uses
FADD/FSUB/FMUL/FDIV/FNEG, conversions FCVT/SCVTF/UCVTF/FCVTZS/FCVTZU, and
ordered/unordered comparisons FCMP plus the corresponding condition. Before
environment-dependent operations the baseline requires FPCR=0; otherwise
BRK #3064, without MSR FPCR. `fp.env.check` is the explicit machine check
before 128-bit conversion helpers, represented as ordinary ABI-visible calls
to `__float[un]ti{sf,df}` / `__fix[uns]{sf,df}ti`, not hidden emitter calls.
Invalid raw integer conversions yield zero; source UB obligations remain.
Spills/copies preserve exact width; FP predicates cannot invalidate a live
NZCV value because the existing immediate-consumer rule still applies.

**Strict scalar FP (0.45).** `fp.strict.OP.f32/f64` preserves the distinction
from baseline `fp.OP` through text, interpretation, allocation and emission.
Validated EvoIR world order determines instruction order before tokens are
erased; strict arithmetic is never scheduled as pure arithmetic. It uses the
same SIMD bank, call ABI, spills and copies. FP admission is a three-level must
analysis (unchecked, strict controls, fixed controls): fixed admission implies
strict admission, but not conversely; calls/opaque asm invalidate both. Strict
checks permit only the two rounding bits beyond the baseline FPCR=0 contract,
and never write FPCR/FPSR. Pure representation operations and the exact-domain
predicate do not depend on rounding. Ordered strict comparisons use FCMPE;
strict equality/inequality use FCMP. Explicit FMA uses FMADD; no multiply/add
contraction is permitted. Hardware accumulates sticky status for executed ops.
The `quiet_olt/quiet_ole/quiet_ogt/quiet_oge` variants also use FCMP, preserving
the source comparison builtins' signaling-NaN-only invalid effect. Quietness
survives canonical text and allocation; it is not inferred from result use.

The interpreter carries explicit FP state in its existing world snapshot.
Strict integer destinations retain their semantic width as
`fp.strict.to_signed.iN.f64` (and unsigned/f32 analogues), independent of the
GPR container width. Invalid raw conversions return zero and raise invalid;
narrow results are masked only after the checked conversion. Missing width
means the physical register width, not an inferred C type.
Strict 128-bit integer conversions on hosted Linux use the qualified eight
`__float[un]ti{sf,df}` / `__fix[uns]{sf,df}ti` helpers. `bl.strict_fp` preserves
this semantic distinction through text, validation, interpretation and allocation;
it admits the four rounding modes and accumulates flags in the current world.
Validation requires a known helper name and its exact GPR-pair/SIMD signature.
`fp.env.check.strict` admits dynamic rounding; ordinary `fp.env.check` still
requires the fixed environment. Checks do not modify FPCR/FPSR. A strict helper
call itself requires strict admission even if an explicit check is absent.
The ordinary ABI call remains visible to allocation, including clobbers.
Invalid raw FP-to-integer inputs are replaced with zero before the helper; a
separate strict conversion of selected NaN/zero raises INVALID only for the
invalid case. No runtime helper is invoked outside its defined conversion domain.
The C E3063 guard is unchanged. Darwin strict wide helpers remain refused until
their runtime is separately qualified. The Linux helper receipt is recorded in
`docs/notebook/strict-floating-implementation-20260927.md`.
**Hosted binary128 (0.48).** Linux `f128` uses the same SIMD128 bank, typed
Q loads/stores, full-width spills and parallel copies as existing vectors.
Constants, negation and representation conversion use exact two-word pack/
extract operations. Arithmetic uses qualified `__addtf3`, `__subtf3`, `__multf3`
and `__divtf3`; explicit IR FMA uses the audited hosted `fmal` contract. Format
conversions use `__extend{sf,df}tf2` and `__trunctf{sf,df}2`; integer conversions
use `__float[un]{si,di,ti}tf` and `__fix[uns]tf{si,di,ti}`. These are ordinary
ABI calls with allocator-visible inputs, outputs and clobbers, never fictitious
quad hardware opcodes. Source `fmal` remains an ordinary external library call;
this does not authorize a new `__builtin_fmal` source spelling.

Machine calls distinguish `bl.fixed_fp`, `bl.strict_fp` and ordinary `bl`.
The first two require a qualified helper and exact signature. Fixed helpers
implement pure IR under RNE admission and do not alter the interpreter's
observable fenv; strict helpers observe dynamic rounding and accumulate status.
Ordinary modeled runtime calls observe and update the actual environment;
they must not inherit pure-IR semantics just because a symbol matches a helper.
All three retain the same physical ABI and call clobbers. The mode survives
text reconstruction and allocation. Fixed/strict helper calls require their
respective admission even if an explicit check marker was omitted.

Quad comparisons use the audited equality/unordered and ordered helper families,
testing signs rather than assuming unordered +/-1 encodings. Quiet ordered
predicates first establish unordered status through the signaling-NaN-only
helper, replace unordered inputs with zero for the ordered helper, then mask
the result false. C conversion definedness is a bit-only magnitude comparison;
it must not signal on sNaN. Invalid raw conversions select zero before invoking
a conversion helper, raising INVALID separately only in strict execution.
Helpers are never invoked outside their defined integer-conversion domains.

The existing AAPCS64 classifier assigns scalar quad arguments/results to q0..q7
and aligned 16-byte stack slots, and HFA1..4 to whole-register groups or wholly
to the shared stack. Quad varargs use the existing 16-byte SIMD save slots.
Full 128-bit values may not rely on v8..v15 upper halves surviving calls.
Darwin quad ABI/helpers and unqualified freestanding helper runtimes remain
outside this hosted contract. Numerical qualification and crossed ABI results
are recorded in the September 27 implementation notebook, not inferred from
type admission or a successful link.

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

A directly known scalar constant count may select the existing immediate form
of `lsl`, `lsr` or `asr` (revision 0.43). Decode it at its own `bits<M>` width,
with `M <= 64`, and require the resulting amount to be below the shifted value's
width `N <= 64`; do not mask a count modulo the machine width to select it.
The source count obligation and the operand's zero/sign extension remain in
place. Dynamic/out-of-range counts and 128-bit pair operations keep their
existing paths. This is instruction selection, not an EqSat rewrite.

For full-register `bits<32>`/`bits<64>` addition and subtraction, a directly
known right operand in `[0, 4095]` may select the existing unsigned twelve-bit
immediate form (revision 0.44). Addition may commute already evaluated SSA
operands to place such a constant on the right; subtraction may not. Preserve
all source checked-arithmetic obligations and evaluations. Narrow/pair values,
other operations, dynamic values and constants outside this bounded selection
keep their established paths; this adds no EqSat rule or flag-setting form.

`bits<1>` lives in a `gpr32` with bit 0 significant. Comparisons produce
their `bits<1>` result with `cset`, which writes 0 or 1 into the whole
register; a `bits<1>` produced by `trunc` or `and` has unspecified upper
bits, and the table above is what makes that safe.

Width 128 is lowered as a pair of halves (Sections 2, 5 and 6). Widths
from 65 to 127 and above 128 are not lowered (`E4003`).

## 4. Instruction Set

An EvoMIR instruction names an AArch64 operation, its destination register
class and its operands, or one of the four pseudo-forms this section marks
as such, which emission expands. The forms this revision admits are exactly
these; an operation absent from the table is not part of EvoMIR. Each row
states the semantics the EvoMIR interpreter (Section 9) implements, so that
the table is executable, not descriptive.

**Data movement.**

**Integer-vector representation forms (revision 0.32).**
`%v = vec.pack.N.B (%args)` and `%v = vec.extract.N.B (%src, %index)`
preserve lane width N and representation bytes B. B=8/16 uses simd64/simd128;
local B=1/2/4 uses gpr32. Pack/extract expand to lane moves and indexed
selection; an invalid index traps E3011 before hardware index masking.
Wider vectors are represented by local objects and scalarized operations.
The machine text, validator and interpreter retain both N and B.

**Linux ELF registrations and TLS (revision 0.32).** Lifecycle roots emit
aligned symbol relocations into `.init_array.NNNNN`/`.fini_array.NNNNN`,
or the unsuffixed default-priority sections. CRT/loader performs ordering
and lifetime processing; no hidden calls are inserted into ordinary main.
TLS emits SHF_TLS `.tdata`/`.tbss` (or the explicit TLS section), STT_TLS,
template bytes/relocations and declared alignment; the linker builds PT_TLS.
Global/local-dynamic addressing uses the standard TLSDESC sequence and
resolver clobbers, with TPIDR_EL0, never x18. Initial/local-exec use their
corresponding relocations; explicit model choices are preserved. Native
loader/runtime owns thread creation and late-loaded TLS instantiation.

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

**Scalar atomic memory operations (revision 0.31).** `atomic.load.K order
[volatile] [base]` and `atomic.store.K order [volatile] value, [base]`
retain order, volatile, width and pointer kind. `K` is `b`, `h`, `w`, `x`,
`p` or `pf`; the last two are provenance-carrying object/function pointers.
The base is gpr64, values are gpr32 for b/h/w and gpr64 otherwise, and the
access requires natural alignment. Validation accepts only the load/store
orders specified by EvoIR 3.8. The interpreter uses its shared atomic event
and memory rules, not a concurrent memory-model simulation.

On AArch64 Normal memory, relaxed b/h/w/x use LDRB/LDRH/LDR W/LDR X and
STRB/STRH/STR W/STR X. Consume/acquire/SC loads use LDARB/LDARH/LDAR W/LDAR X;
release/SC stores use STLRB/STLRH/STLR W/STLR X. Pointer accesses use the X
form. There is one source access; no additional DMB is inserted for relaxed
accesses, and the volatile marker does not select a second access. No pass
may replace these with ordinary accesses, split/merge them, or discard order.

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
form names both successors with their argument lists. After edge-copy placement
and allocation, emission uses either physically adjacent successor as the
fall-through: when only the taken successor is next, it inverts the condition
and branches to the untaken successor instead (revision 0.42). This changes
only instruction spelling, not block order, labels, arguments, effects or
logical CFG edges. Equal-target edges retain their distinct parameter-copy
routes. Long-branch relaxation applies after this layout choice, preserving
the selected fall-through and the other edge's reachability.

| Form | Semantics |
|---|---|
| `b ^target(args)` | unconditional branch |
| `br rn [^a(args), ^b(args), ...]` | indirect transfer through a `gpr64` code address, with the complete ordered successor edges from EvoIR; simultaneously binds only the selected edge's parameters; invalid interpreted target is E3048 |
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
| `stack.alloc rd, rn [origin=O, align=A]` | fresh uninitialized object of the runtime unsigned `gpr64` byte count `rn`, with distinct origin O and power-of-two alignment A; `rd` is `gpr64`; preserves fixed-frame storage and earlier live dynamic objects, with a disjoint outgoing-argument area below the new object (Section 6) |
| `stack.release rn` | ends the dynamic object's logical lifetime in the object interpreter; native emission reclaims stack bytes only if this object is still the top dynamic allocation, otherwise retaining the bytes until later frame restoration; never releases an intervening live function-lifetime allocation |
| `mem.copy [dst], [src], #size, #align` | copies `size` bytes from `src` to `dst`, transferring per-byte initializedness and pointer fragments exactly as EvoIR Section 3.8's `copy` does (identical or disjoint ranges); emission expands it to `ldr`/`str` pairs of the largest aligned width, unrolled up to 64 bytes and a counted loop above; the loads of the expansion never report an uninitialized read, because they move representation, not values |
| `spill.store %v, slot` / `spill.load rd, slot` | the store of a virtual register to its spill slot and the reload from it, inserted by the allocator; emission expands them to `str`/`ldr` at the stable frame slot, from `sp` only when it remains fixed and otherwise using the entry-frame base (Section 6) |
| the prologue and epilogue | synthesized at emission from Section 6 and never present as instructions: the interpreter's memory is EvoIR's object model, with no linear stack, no `sp` and no return address, so they are not interpretable; the allocated form is judged by the verifier and the native run (revision 0.4) |
| the register-offset addressing of `mem.copy`'s counted loop | emission's business, like the prologue's pre-indexed `stp` and `frame.slot`'s `add rd, sp, #off`: the loop counts an offset in a register, so the expansion uses the register-offset form of `ldr`/`str`, which the table's immediate rows do not admit and which only emission ever writes (revision 0.5) |
| `asm` / `asm.goto` | the EvoIR instruction of its specification's Section 3.11, carried through unchanged: the template, its operands with their constraints, its clobbers and, for `asm.goto`, its label successors. The interpreter does not execute it (it reports `E3021`); emission writes the template with each operand reference replaced by the operand's assigned register, memory address or immediate |

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
| `sdiv`, `udiv`, `srem`, `urem`, width <= 64 | `sdiv`/`udiv`, remainders by `msub`; `divisor_nonzero` and `signed_div_rem_no_overflow` have been asserted before, which makes the machine's answers for the excluded operands unobservable |
| `sdiv`, `udiv`, `srem`, `urem`, width 128 | the already-approved S08 hosted path (EvoIR Appendix D): ordinary calls to `__divti3`, `__udivti3`, `__modti3`, `__umodti3`, respectively, with low/high argument pairs in `x0:x1` and `x2:x3` and the result in `x0:x1`; allocation sees ordinary call clobbers and preserves live values. The hosted target must supply libgcc/compiler-rt helpers; no freestanding dependency is silently supplied. The original divide/remainder/overflow guards remain before the call. |
| `divisor_nonzero %d` | zero-extend `%d` per Section 3, `cmp %d, #0` then `cset ne` |
| `signed_div_rem_no_overflow %a, %b` | sign-extend both per Section 3; `cmp %a, #INT_MIN_N` (the minimum of `bits<N>`, materialized), `cset ne` into `%p`; `cmp %b, #-1`, `cset ne` into `%q`; `orr %p, %q` |
| `signed_add_defined %a, %b` / `signed_sub_defined` | at the register width: `adds`/`subs` into a scratch, then `cset vc`; below it: sign-extend both operands, `add`/`sub` at the register width, and compare the result with the sign extension of its low `N` bits (`sbfx` then `cmp`, `cset eq`), because the `v` flag reports the register and never the source width (revision 0.3; `bits<3>`: 3 + 3 is 6, outside -4..3, so 0) |
| `signed_mul_defined %a, %b` | 32-bit: `smull` the sign-extended operands, compare with the sign extension of its low 32 bits, `cset eq`; 64-bit: `mul` and `smulh`, compare the high word with `asr` of the low word by 63, `cset eq`; any other `N` sign-extends to the next register width and compares the product with the sign extension of its low `N` bits (revision 0.3) |
| `shift_count_defined.<s> %x, %amt` | extend `%amt` by its declared signedness `<s>` to the register width, `cmp` with `#N`, `cset lo`: a sign-extended negative amount exceeds every bound up to 64 as an unsigned quantity, so one unsigned comparison serves both signednesses (revision 0.3; zero-extending a signed `bits<1>` amount of 1 had answered 1 where EvoIR decodes -1 and answers 0) |
| `signed_shl_defined.<s> %x, %amt` | the conjunction of four `bits<1>`: `shift_count_defined` as above; `%x` non-negative (`cmp` with `#0` after sign extension, `cset ge`); the round trip `(x << amt) >> amt == x` with an arithmetic right shift at the register width; and, below the register width, the shifted result representable in signed `bits<N>` (compare with the sign extension of its low `N` bits), as EvoIR 3.3.1 defines the predicate (revision 0.3) |
| `sext`, `zext`, `trunc` | the extension of Section 3, or nothing for `trunc` |
| `bits<128>` forms (revision 0.10; the pair is `lo`, `hi`) | `add`: `adds lo` then `adc hi`; `sub`: `subs lo` then `sbc hi`; `mul`: `mul lo, alo, blo`, `umulh hi, alo, blo`, `madd hi, alo, bhi, hi`, `madd hi, ahi, blo, hi`; `and`, `or`, `xor`: halfwise; `zext` to 128: `hi = 0`; `sext` to 128: `hi = asr lo, #63`; `trunc` from 128: the low half, then Section 3; `shl`, `lshr`, `ashr` by an amount `s` (the front end has asserted `s < 128`): the branch-free form the oracles emit, that is the 64-bit shifts of both halves, the cross term `lsl`/`lsr` of the other half by `63 - s` after a one-bit pre-shift, combined by `orr`, and two `csel` on `tst s, #64` selecting the 64-or-more case (`p35-int128.c`, `shl_var`, `lshr_var`, both compilers); `icmp.eq`/`ne`: `eor` each half pair, `orr`, compare with zero; ordered `icmp`: `cmp alo, blo` then `sbcs xzr, ahi, bhi` and `cset` with the 64-bit form's condition (`lo` for `ult`, `lt` for `slt`; `ule` and `sle` compute the swapped subtraction and take `hs` and `ge`); `load`/`store`: `ldp`/`stp` of the halves at the address, which is not single-copy atomic, exactly as both oracles emit for a `volatile u128` (the cohort's atomic 128-bit accesses are `asm`); `shift_count_defined` compares the amount with `#128`; `copy` is unchanged; the UB predicates and hosted division/remainder follow their current rows in this table |
| 128-bit UB predicates (revision 0.26; the pair is `lo`, `hi`) | `divisor_nonzero`: `orr t, lo, hi`, `cmp t, #0`, `cset ne`. `signed_div_rem_no_overflow`: `%a` is `MIN` when `lo == 0` and `hi == 0x8000000000000000`, `%b` is `-1` when both halves are all ones; the predicate is the negation of their conjunction, computed with `cmp`/`ccmp` and one `cset`. `signed_add_defined`: `adds t, alo, blo`, `adcs t, ahi, bhi`, `cset vc`; `signed_sub_defined`: `subs`, `sbcs`, `cset vc`. `signed_mul_defined`: take the magnitudes of both operands (negate the pair when `hi` is negative), form the 256-bit product of the magnitudes from the four 64-bit limb products with `mul`/`umulh` and carries, and answer true exactly when the high 128 bits are zero and the low 128 bits do not exceed `2^127 - 1` (operand signs equal) or `2^127` (signs differ). `shift_count_defined` with a 128-bit shifted value: as at narrower widths with bound `#128`. `signed_shl_defined` at 128: the conjunction of the count predicate, `ahi` non-negative, and the round trip through the pair shift sequences of revision 0.10 (`(x << amt) >> amt == x` with the arithmetic right shift). Under EvoIR 1.9.88 the front end no longer emits `signed_shl_defined`, so the row serves older modules. |
| `va_start %ap` | in a variadic function only: store to the list object at `%ap` the five fields of Section 6 (`stack`, `gr_top`, `vr_top`, `gr_offs`, `vr_offs`) |
| `va_arg.T %ap` | the register-then-stack procedure of Section 6 for `T`'s size and alignment, ending in a load of `T` from the computed address |
| `va_end %ap` | nothing |
| `icmp.*` on integers | the extensions of Section 3, `cmp`, then `cset`; when the only consumer is a `cond_br` in the same block, `cmp` then `b.cond` with no `cset` |
| `icmp.eq`/`ne` on pointers | the same, 64-bit, no extension |
| `ptr.null` | `mov #0` |
| `global_addr @g` / `func_addr @f` | `adrp` + `add` with the symbol spellings of Section 4 |
| `ptr.offset.<direction>.<signedness>` | the index extended per the signedness, scaled by `lsl` when the scale is a power of two and `mul` otherwise, then `add` for the add direction and `sub` for the subtract direction; the machine wraps where EvoIR computes in unbounded arithmetic and reports `E3011`, an admissible difference because the pointer-formation guard has been asserted before |
| `alloca` (static size and alignment) | `frame.slot` at the instruction's program point |
| `alloca.dynamic` | `stack.alloc` at the instruction's program point, preserving byte-count operand, origin and alignment |
| `lifetime.end` | `stack.release` for an address originating from dynamic allocation, including explicit edge transport; fixed frame slots remain reserved and need no native instruction |
| `pointer_valid`, `access_in_bounds` | nothing: the guards are the obligations the front end already lowered into `ub.assume`; the instruction itself has no machine effect |
| `load [volatile]` of `bits<N>`, `N <= 64` | `ldr` of the matching width, with the marker; bounded immediate address selection below preserves access attributes |
| `load [volatile]` of a pointer | `ldr.p` / `ldr.pf` |
| `store [volatile]` | `str` of the matching width, `str.p` for a pointer; bounded immediate address selection below preserves access attributes |
| `atomic.load/store order [volatile]` | corresponding `atomic.load/store.K` with identical order/volatile and scalar/pointer kind |
| `copy` with static size | `mem.copy` |
| `copy.volatile.read/write/readwrite` | `mem.copy.volatile.read/write/readwrite`; validated size 1/2/4/8 and alignment >= size guarantee exactly one source load and one destination store, with only the specified sides observable; initializedness and pointer metadata follow ordinary `mem.copy` |
| `br ^b(args)` | `b ^b(args)` |
| `cond_br %c, ^t(...), ^e(...)` | `tbnz %c, #0, ^t(...), ^e(...)` (or the `b.cond` fusion above) |
| `br.indirect %p [^b(args), ...]` | `br %p [^b(args), ...]`, retaining the complete ordered CFG and address arguments; effect ordering is preserved as above, and Section 7 implements simultaneous entry bindings |
| `ret v...` | `ret` listing the results |
| `trap.<reason>` | `brk` with the reason's number |
| `ub.assume.<reason> %c` | checked profile: `tbz %c, #0, ^trap_reason(), ^continue(live...)` (or the equivalent adjacent-flags selection below), where `^continue` is a fresh block whose arguments are exactly the registers the rest of the source block still reads, ordered by register number, and `^trap_reason` holds `brk` for the reason; one trap block per reason, appended after the source blocks in order of first use. Unchecked profile: nothing. The profile is a driver flag recorded in the receipt, default checked |
| `call @f(args, %m, %w)` / `call %p(...)` | `bl`/`blr` with explicit argument and result registers (Section 6) |
| `uninitialized.bits<N>` | `mov #0` (any value is permitted; zero is chosen so the output is deterministic) |
| `ptr.to_int`, `ptr.from_int.*` | nothing: the value is the same 64-bit register (revision 0.12) |
| `block_addr ^b` | `adr rd, <label of block b>` (revision 0.15); a static relocation against a block uses a local symbol `.L<function>.<block>` the emitter places at the block's first instruction |
| `return_address` | `ldr rd, [x29, #8]`: the saved link register of this frame record, valid anywhere in the body because every function keeps a frame pointer (revision 0.17) |
| `frame_address` | `mov rd, x29` (revision 0.17) |
| `ctlz`, `cttz`, `ctpop`, `bswap` (revision 0.14) | `ctlz`: zero-extend per Section 3, `clz` at the register width, then subtract `32 - N` or `64 - N` for a narrower `N`; `cttz`: `rbit` then `clz`, and for a narrower `N` the result capped at `N` with `cmp`/`csel`, since the unspecified upper bits may hold a set bit; `ctpop`: the general-register sequence both oracles use without SIMD (`p39.out`, Clang-19; GCC-14 calls `__popcountdi2`, which the kernel does not provide): `x - ((x >> 1) & 0x5555...)`, `(x & 0x3333...) + ((x >> 2) & 0x3333...)`, `(x + (x >> 4)) & 0x0f0f...`, multiply by `0x0101...` and `lsr` by the register width minus 8, after zero extension; `bswap`: `rev16` (16, upper bits unspecified per Section 3), `rev` at 32 or 64 |

**Bounded immediate addresses (revision 0.40).** An ordinary typed load/store
may use the existing `[base, #offset]` form for one preceding constant
`ptr.offset` in the same machine block. Decode the index at its own width and
signedness, then apply scale/direction; the exact displacement must fit `i64`
and the access's signed-unscaled or unsigned-scaled immediate. Preserve kind,
width, alignment, volatility and access position; when strict alignment can
expand the access, every resulting byte displacement must fit too. Only
constant materializations may intervene: do not select through an access, call,
assembly, other computation or guard/CFG boundary, or combine offset chains.
Pointer-validity/access checks and explicit continuation arguments remain;
atomic operations are unchanged. Unmatched addresses retain their original
register computation. Ordinary dead-computation removal may subsequently
remove the now-unused address expression, without removing a memory operation.

The UB predicates were absent from revision 0.1's table, which refused
19,410 of 20,000 generated functions; they are ordinary boolean
computations and lower whether or not a `ub.assume` consumes them.

Every form outside this table and the later explicitly admitted families is
refused; scalar f32/f64 is admitted by revision 0.33, not by the legacy E4001 row.

**Compared-value selection (0.36).** Instruction selection may replace the
contiguous full-width integer chain `c=icmp(x,y); e=zext(c); mask=0-e;
d=a xor b; masked=d and mask; result=b xor masked` with `cmp x,y; csel a,b`,
using the same signed/unsigned predicate, when a/b are x/y in either order.
All arithmetic values have the same `bits<32>` or `bits<64>` type; every removed
intermediate has exactly one use. The zero may be an existing constant or a
single-use constant between extension and subtraction. Otherwise the ordinary
rows apply. No intervening instruction, effect, guard, operand evaluation or
block binding is removed or moved; source symbolic-address operands are excluded.
The comparison's flags have their one adjacent CSEL consumer under Section 2.
This selects existing machine instructions, without adding an EvoIR operation,
expanding the EqSat allowlist, or authorizing speculative conditional arms.

**Checked flag branches (0.37).** When an obligation is the only use of the
immediately preceding EvoIR instruction's Boolean result, and that result's
machine definition is a final `cset c, flags, condition` adjacent to its flags
producer, selection may remove the CSET and use `b.<inverse-condition>` to the
same reason's trap. The original success continuation and every live edge
argument remain; the flags producer and branch are adjacent under Section 2.
Other uses, including successor arguments or a second obligation, require the
Boolean to remain materialized. Intervening source instructions prevent this
selection, and unmatched forms retain TBZ. This is only selection of existing
instructions: no predicate, effect, trap, source-width test or EqSat restriction
is removed, and the allocator's coalescing policy is unchanged.

## 6. Frame and Calling Convention (Darwin arm64, scalar)

**Linux integer-vector ABI (revision 0.32).** 8/16-byte vectors use v0..v7,
independently of x0..x7; the ninth and later vectors use the shared argument
stack with 8/16-byte alignment. All argument classes share that stack cursor,
so a 128-bit integer or aligned record after a stacked 8-byte vector must
round the physical stack offset, not the count of integer arguments.
Homogeneous vector aggregates of one to four identical short vectors retain
`hva=Cxvec<L, bits<N>>` in byval/sret marks. They use consecutive SIMD
registers or go wholly to stack if the group does not fit; return uses v0..v3.
Nonhomogeneous and wider-than-16-byte vectors use the ordinary by-reference
record convention. Variadic vectors use the VR save area (16-byte slots),
with HVA register slots gathered into their contiguous object layout;
stack-resident HVAs use their packed object layout, not save-area stride.
Mixed-compiler acceptance is required; self-calls alone cannot validate ABI.

**Linux scalar FP/HFA ABI (0.33).** f32/f64 and HFA members consume the same
v0..v7 cursor as integer vectors/HVAs. HFA groups go wholly to stack when they
do not fit, with contiguous object bytes (not one eight-byte slot per float);
return uses v0..v3. Anonymous float is promoted to double; double/HFA va_arg
uses VR save slots when available and the packed stack object otherwise.
HFA stack transport consumes the natural-member alignment recorded by EvoIR
(`abi_align=16`, or 8 when omitted), not the complete object's overalignment.
Caller placement, callee entry and `va_arg` use that same cursor alignment;
copies from the stack may not claim a stronger source alignment than it supplies.
This does not import Linux variadic rules into Darwin.

The convention is Apple's AArch64 variant of AAPCS64, restricted to what
this revision lowers; the restriction is refused, never approximated.

**Records (revision 0.18; withdraws `E4011`).** EvoIR passes a record as a
pointer to a copy marked `byval(size, align)` and returns one through a
destination pointer marked `sret(size, align)` (EvoIR 10.1.3). The machine
applies AAPCS64 to those marks on both targets, so an EvoCC object agrees
with a GCC or clang object on either side of the call: a `byval` record of
at most 16 bytes is loaded from the copy into the next one or two general
registers as 8-byte chunks (a record whose alignment is 16 starts at an
even register on Linux, as a `bits<128>` does) and, in the callee, stored
into a frame slot that becomes the parameter's place; a `byval` record
above 16 bytes stays a pointer to the caller's copy in the next register or
stack slot, which is what AAPCS64 does too. An `sret` result of at most 16
bytes is loaded by the callee into `x0` and `x1` before `ret` and stored by
the caller into its destination; above 16 bytes the destination pointer
travels in `x8`, not in an argument register, and the callee writes through
`x8`. Measured shapes: `.evidence/builtin-probes/20260902-2c23684c/p35-int128.c`
(`take_union`, `take_pair`) and the zstd fault (`tests/contracts/ladder-zstd-receipt.json`).

**Arguments and results.** Integer and pointer arguments of at most 64 bits
go in `x0` to `x7` in order. Between two EvoCC-compiled functions, a
`bits<N>` argument with `N < 64` is passed in the low bits of its register
with the upper bits unspecified, and the callee applies Section 3 before
reading it as anything but a low-`N`-bit value. Across the host boundary the
platform rule governs instead: a caller extends an argument narrower than 32
bits to 32 bits and a callee may assume the bits above its parameter's width
are that extension (Apple's AArch64 variant; revision 0.5, after the
executor found the two rules unreconciled and the probe passed only because
each host function masked its own argument; eighteen of forty comparisons
failed once the probe stopped masking and was built at `-O2`). A call is
**cross-boundary** when the callee's definition is not emitted beside the
caller, which is a fact emission holds and EvoIR cannot spell, since every
call EvoIR admits names a function the module defines (revision 0.7).

EvoIR carries no signedness on a parameter, so the extension EvoMIR inserts
is a **zero extension**: the value of a `bits<N>` is its low `N` bits, and a
front end that must pass a signed narrow argument emits `sext` to
`bits<32>` before the call, which is a lowering rule of the EvoIR
specification, not a machine one (revision 0.7). A **result** narrower than 32 bits is
zero-extended to 32 bits by the *callee*, on every return, not only where a
boundary is known: a callee cannot know its caller, and the internal rule
leaves those bits unspecified, so specifying them breaks nothing. Measured
at revision 0.9, before the extension existed: a function returning
`bits<8>` of the value 1 computed by negation returned 4294967295 where the
host compiler's build of the same function returned 255, at every width
tried; after it, 597 comparisons over three widths agree. `E4014` is
therefore withdrawn. A ninth integer argument is refused (`E4010`); an aggregate argument
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
virtual register), then the register save area of a variadic function
(Section 6, 64 bytes), then outgoing stack arguments (Linux only, revision 0.10).
`sp` stays 16-byte aligned at every call. Fixed slots remain in this frame;
EvoIR's approved `alloca.dynamic` additionally moves `sp` at runtime.

**Dynamic frames (revision 0.41, synchronization with EvoIR Appendix D).**
Each `stack.alloc` evaluates its existing size operand once, subtracts that
unsigned byte count from the current SP and rounds the object address down to
`max(A, 16)` alignment. A private 16-byte header below the object records the
previous SP; below that header, emission reserves the aligned outgoing-argument
area. The header and outgoing arguments are not bytes of the user object.
Fixed objects, callee-save slots and spills use addresses derived from the
stable entry frame (`x29`, including the fixed frame's realignment), not the
moving SP. Returning restores SP from `x29` before restoring the saved frame
record; recursion therefore retains distinct activations.

`stack.release` restores the saved SP only when its object address identifies
the current top allocation (current SP plus the outgoing area and header).
Otherwise it leaves native stack storage reserved: ending a VLA scope must
not pop a later function-lifetime `alloca`. Logical lifetime still ends,
including in the interpreter, so retaining bytes does not authorize later
access. Allocation and release use reserved scratch registers and do not
clobber live NZCV. No stack-overflow guarantee or general non-top reclamation
is introduced. These are the already-approved function-lifetime and VLA
rules, not a new dynamic-storage model.

Physical frame bytes may be reused by distinct logical origins only when the
address has one definition and every use is a direct, bounded, nonvolatile
load/store/copy in the defining block, with disjoint closed access intervals
(different blocks cannot overlap under this nonescape condition).
Reuse requires identical size and alignment; address transport, arithmetic,
calls, assembly, spills and repeated definitions retain separate storage.
The origin-to-offset table retains every origin, object identity and lifetime
semantics are unchanged, and simultaneous copy operands never overlap merely
because their access instructions have the same index.

**Over-aligned frames (revision 0.26).** When any `frame.slot` of the
function declares an alignment `A` greater than 16, the frame is realigned:
the prologue's `sub sp, sp, #frame` uses a frame size rounded up to a
multiple of `A` plus `A - 16` of slack, and is followed by `mov x16, sp`,
`and x16, x16, #-A`, `mov sp, x16`, using the reserved scratch register.
The logical instruction cannot read `sp`: both sealed assemblers reject
`and sp, sp, #-256` and accept this scratch-register sequence,
so that `sp` itself is a multiple of `A`; every slot keeps its non-negative
offset from `sp`, and a slot whose alignment is `A'` sits at an offset that
is a multiple of `A'`, which now makes its address a multiple of `A'` for
every `A' <= A`. The epilogue restores `sp` from the frame pointer, `mov sp,
x29`, before the `ldp` of the saved pair, since the realigned `sp` is no
longer `x29 - frame`. Incoming stack arguments are still read from `x29`,
the frame chain is unchanged, and a function without such a slot is emitted
exactly as before. Both oracles realign this way and the probe checks the
real address and the live bytes of 32 recursive `aligned(256)` objects
(`.evidence/builtin-probes/20260909-alignment-specifiers/stack.c`); the
subject's earlier emission kept `sp` at 16 and returned a wrong value, the
first silent native error the alignment work found. Every slot (object, spill, save area) is addressed
from `sp` with a non-negative scaled offset, never from `x29`, so that frames
above 256 bytes stay inside the unsigned twelve-bit immediate form; `x29`
serves the unwinder and the debugger, not the code (revision 0.4).

**Stack arguments** (beyond `x7`). On the Linux target (revision 0.10) an
argument that finds no register goes to the stack in an 8-byte slot, slots
assigned in argument order at increasing addresses from the caller's `sp` at
the call, a `bits<128>` in a 16-byte-aligned 16-byte slot; a narrow value
occupies the low bytes of its slot with the rest unspecified (both oracles
`str w` a promoted `char` into an 8-byte slot, `p34-callsite.c`). The
caller reserves the outgoing area below its frame (Section 6 frame layout,
"outgoing stack arguments"), and the callee reads slot `i` at `x29 + 16 +
8 * i`, since the prologue saves the pair first. On Darwin the platform
packs stack arguments by natural alignment instead; that rule is still
unmeasured, so more than eight integer arguments stay `E4010` there.

**128-bit arguments and results (revision 0.10; Darwin corrected at
0.13).** On Linux a `bits<128>` argument takes two consecutive registers
`x(2k)`, `x(2k+1)`, the low half in the even one: the next register number
is rounded up to even first, so after one 64-bit argument it is `x2`:`x3`
with `x1` skipped, after two it is `x2`:`x3` with nothing skipped, and
after seven it goes to the stack, whole, 16-aligned, with `x7` unused. On
Darwin the platform does **not** round: the argument takes the next two
registers whatever their parity, `x1`:`x2` after one 64-bit argument and
`x3`:`x4` after three, and after seven it goes to the stack (measured with
Apple clang, `.evidence/builtin-probes/20260905-b6/b6-darwin-abi.c`,
`b6-darwin-abi.out`, functions `take_after_one`, `take_after_three`,
`take_after_seven`; the executor found the revision 0.10 sentence
unmeasured on Darwin and stopped rather than guess). A `bits<128>` result is `x0`:`x1`, low half in
`x0`. Every row is measured with both oracles (`p35-int128.c`, functions
`ret128`, `take_after_one`, `take_after_two`, `take_after_seven`; `p34-p35.out`
and `p34-p35-clang.out`). Records are never passed by value in EvoIR (its
10.1.3 sends a pointer to a copy), so the oracles' rule for 16-byte-aligned
records (`take_union`, even pair) is recorded and unused.

**Variadic functions (revision 0.10; withdraws `E4012`).** A call passes
anonymous arguments exactly as it passes named ones: same registers, same
slots, no distinction (`call_v` and `call_f` in `p34-callsite.c` are
instruction-for-instruction identical under both oracles), and the front
end has already promoted narrow values, so nothing is inserted. A variadic
**callee** reserves a 64-byte, 16-byte-aligned **register save area** in its
frame and stores `x_k` to `x7` into it at offsets `8 * k` to `56` in the
prologue, where `k` is the number of argument registers its named
parameters consumed (a `bits<128>` parameter consumes two and the rounding
above). The list object is 32 bytes: `stack` (a pointer, offset 0),
`gr_top` (offset 8), `vr_top` (offset 16), `gr_offs` (a signed 32-bit
integer, offset 24), `vr_offs` (offset 28). `va_start` stores
`stack = x29 + 16 + named_stack_bytes`, where `named_stack_bytes` is the
extent of named incoming stack arguments, including argument-alignment
padding and rounded up to 8 bytes, but excluding final caller stack-area
padding. With no named stack arguments it is zero. This is the AAPCS64
`va_start` rule, measured with both sealed oracles in
`.evidence/builtin-probes/20260907-variadic-stack/results.txt` and covered by
`tests/fixtures/variadic-named-stack.c`. The remaining fields are `gr_top = save area +
64`, `gr_offs = -8 * (8 - k)`; `k = 8` gives `gr_offs = 0` and every
general-register argument comes from the stack.

**Vector register save area (revision 0.28).** A variadic callee also
reserves a 128-byte, 16-byte-aligned vector save area and stores `q0` to
`q7` into it at offsets `16 * j` in the prologue (`stp q0, q1, ...`
pairs), and `va_start` stores `vr_top = vector save area + 128` and
`vr_offs = -128`. It does so even though this profile executes no
floating point: the callee's own code never reads those registers, but a
caller compiled by a reference compiler passes a `double` anonymous
argument in `q0` to `q7`, and a `va_list` this function hands to another
function (`vsnprintf`) is read by that function's `va_arg` of `double`,
which takes `vr_offs < 0` as "the value is in the save area". Both sealed
oracles store all eight registers and write `-128` at `-O0` and `-O2`
(`.evidence/builtin-probes/20260909-variadic-vector-save/wrapper.c`,
`results.txt`, printing `7 1e+09 11`); the earlier text, which set
`vr_offs = 0` and called the field never read, described a program built
entirely without floating point and was wrong for a mixed link. Measured
consequence: with `vr_offs = 0`, every musl variadic wrapper EvoCC compiled
(`snprintf`, `swprintf`, `fprintf`) made the floating-point formatting
tests of libc-test fail (`printf-fmt-g-*`, `printf-1e9-oob`, `snprintf`,
`swprintf`, `strtod_simple`: 12 cases in run `20260909T070307Z`), and the
bisection could not separate them, which capped the passing build at 498
of 995 assembled units. Named scalar FP, HFAs and integer vectors/HVAs consume vector
registers: `vr_offs = -16 * (8 - named_vector_registers)` with the count
capped at eight. Revision 0.33 admits `va_arg.f64` and marked scalar HFAs.
Darwin passes every anonymous
argument on the stack and has no save areas (revision 0.20's note). `va_arg` of a type of size `S` (rounded up to a
multiple of 8 as `S8`) and alignment `A` performs the procedure of the
AAPCS64 Appendix B restricted to general registers:

```text
offs = gr_offs
if offs < 0:
    if A == 16: offs = (offs + 15) & ~15
    next = offs + S8
    if next <= 0: gr_offs = next; address = gr_top + offs; done
    gr_offs = 0
address = stack
if A == 16: address = (address + 15) & ~15
stack = address + S8
```

then loads `T` from `address`. This procedure, written by hand in
`p34-varargs.c`, agrees with GCC-14's and Clang-19's own `va_arg` on
fourteen consecutive items covering `int`, `long`, `__int128` (the
16-byte rounding of `gr_offs` and of `stack`), a 16-byte record in the
save area, a 24-byte record by reference, and the crossing from registers
to stack for `k` of 1, 7 and 8. `va_end` emits nothing. Only the promoted
scalar types of EvoIR 3.10 reach `va_arg`; the record rows above are
measured for the day a unit needs them.

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

**Identical-value transport (revision 0.38).** At the validated SSA boundary,
an ordinary block parameter may be identified with its sole distinct incoming
value when all other incoming bindings pass that same parameter unchanged.
Repeat this rule through forwarding chains; a phi with two distinct incoming
definitions is not a trivial phi, even if their constant bits happen to agree.
Counters, exchanged values and ordinary merges retain their parallel copies.
Complete incoming edges are inspected, including different bindings from the
same predecessor/target pair. Entry parameters remain caller bindings; entry
backedges and functions containing fixed-register assembly operands, `asm.goto`
or indirect dispatch are initially excluded from this reduction. Ordinary
assembly in validated SSA form admits input/address-use rewriting, without
altering its outputs, constraints, clobbers, effects or position (revision 0.39).
Read/write or tied forms that redefine a virtual register do not satisfy that
SSA precondition. In particular, an indirect union-copy
self-edge may overwrite the source needed by another target, whose independent
receiving location must not be removed by a direct-edge identity argument.
Remove only the proven transport
copies and rewrite their uses during SSA destruction, never in the local-SSA
input form. No instruction, memory access, guard, lifecycle action, label or
successor is moved or removed by this identity rule. Remaining copies follow
the placement/cycle rules above and the independent allocation verifier.

**Indirect entries (revision 0.29).** A computed edge cannot be split merely
by changing its CFG target: `block_addr` and static relocations still denote
the original label. Its explicit SSA parameters require non-conflicting
receiving locations across all candidate targets. Before allocation, form
one joint parallel-copy batch from all edges into distinct destination
parameter virtual registers. Keep all destinations live at dispatch through
the complete CFG, so allocation assigns independent registers or private
spill slots; only proven identical address payloads may share a location.
These spills implement SSA transport and are not new EvoIR storage objects.
Resolve copy cycles using the existing scratch rule. If a self-edge's copy
overwrites the virtual register holding the target, first capture the target
in a fresh ordinary virtual register; keep it live through the final `br`.
Direct, conditional and assembly edges use the same destination parameters.
The final transfer emits `br xn`, not a call, with no ABI clobber or link-register
write. Effect tokens disappear only after their ordering constraints have
been respected; real address operands do not disappear with them.

This joint batch prepares receiving locations only: it cannot perform object
reads, scope entry/exit or other effects for unselected edges. Such actions
must be routed on the selected path before dispatch by explicit control flow
as specified in EvoIR 3.9. Label identities and every CFG successor survive
lowering, allocation and emission, including labels referenced only by a
static relocation. Missing edges are never repaired by assuming a likely
target.

**Assembly operands.** A constraint fixes what an `asm` operand may be, and
the allocator must satisfy it before it satisfies anything else:

| Constraint | Requirement on the allocator |
|---|---|
| `r`, `=r`, `+r` | any register of the operand's class |
| `=&r`, `+&r` | a register no input operand of the same `asm` holds (early clobber) |
| `rZ` | as `r`, except that an operand known to be the constant zero is the zero register and consumes no allocation |
| `Q`, `=Q`, `+Q`, `Qo` | a register holding the operand's address, and the emitted reference is `[reg]` |
| a digit | the same register as the output that digit names (a tied operand); the two intervals are one |
| `I`, `J`, `K`, `L`, `i`, `n` | no register: the operand is an immediate the emitter writes literally, and the lowering has already refused a non-constant |
| `p` | a register holding the address |

A clobber names a register the `asm` writes: the allocator treats the whole
instruction as defining it, so no live value may occupy it across the
instruction, and `cc` clobbers the flags, which ends any `flags` value's
adjacency. `memory` constrains nothing here; the maximal effects of the
EvoIR instruction already order the `asm` against every other access.
Defined at revision 0.6.

**Fixed input occurrences (revision 0.39).** The same source value may supply
independent operands bound to different physical registers. Materialize their
distinct boundary copies before applying fixed locations; identical value and
location requirements may share a copy. A tied input still shares its output's
location, and contradictory fixed requirements within that tie remain invalid.
This does not pin the original value's entire lifetime or change asm effects.

For EvoIR 1.9.80, an `i` operand may instead be a symbol plus a signed
64-bit addend. It occupies no register and allocates no object. Lowering
derives it from the existing address definitions, pointer casts and constant
64-bit additions/subtractions; an unresolvable immediate is refused, never
replaced by a register or zero. EvoMIR text writes `@symbol+addend` (the
addend includes its sign and is always present, including `+0`); a label
target is `@function^bN+addend`. Emission uses the same platform symbol or
block spelling as static relocations, and `%c` suppresses any immediate prefix.
Only `i` admits symbolic operands. The interpreter's opaque-assembly refusal
and the allocator's register constraints are unchanged (revision 0.25).

**Approved SIMD assembly (integer vectors 2026-09-20; scalar FP 0.50).**
The `w`, `=w`, `+w`, `=&w` and `+&w` constraints carry `simd64`/`simd128`
integer/polynomial vectors and scalar f32/f64 as `simd32`/`simd64` in the existing
shared SIMD register bank; a tied input inherits its output's class and
location, including an early-clobber output, while independent inputs and
distinct outputs must not overlap forbidden locations. Input copies precede
the asm, output stores follow it (on each selected `asm.goto` successor),
and existing spill/parallel-copy machinery preserves all 32/64/128 operand bits and
live source values. The allocator excludes every aliased clobber and its
reserved scratch registers; the independent checker validates the constraint
bank and assigned physical bank as well as overlap and dataflow. Default
references emit `vN`; `%b/%h/%s/%d/%q` select that register's views, with
accepted `vN/dN/sN` clobbers normalized to the same physical identity.
`-mgeneral-regs-only` still rejects generated SIMD use; this extension does
not admit f128, half, floating-vector or SVE asm or pretend that interpreters
execute asm. Scalar default references still print `vN`; explicit `%s/%d`
select the usual scalar instruction views. Transport does not quiet NaNs or
raise FP status. The opaque instruction retains `fenv=true`; FP admission
facts are invalidated across ordinary/goto assembly, and subsequent numeric
operations recheck their own required controls. No new register bank, spill
format, memory opcode or implicit floating-environment channel is introduced.

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
stable digest. Physical allocation performs no coalescing beyond removing a
move whose two operands got the same register; the preceding identical-value
SSA transport reduction is independent of physical locations.

The output is verified, not trusted (Section 9): every physical register
read is defined on every path, no two simultaneously live values share a
register, and every `flags` producer is adjacent to its consumer.

## 8. Emission

**The two targets.** They differ in four places and nowhere else, each
measured by emitting one module both ways (revision 0.9):

| | Darwin arm64 | Linux AArch64 |
|---|---|---|
| symbol prefix | `_main` | `main` |
| page-relative pair | `_g@PAGE`, `_g@PAGEOFF` | `:pg_hi21:g`, `:lo12:g` |
| sections | `.section __TEXT,__text,…`, `.section __DATA,__data` | `.text`, `.data` |
| local labels | `Lmain_block0` | `.Lmain_block0` |

The target is a parameter of emission (`darwin-arm64`, `linux-aarch64`), not
a build-time choice.

The assembly text is the platform assembler's syntax. Functions are emitted
in module order, each as `.globl` (external functions only, revision 0.16)/`.p2align log2(align)` (the function's `align=N`, EvoIR 1.9.88, 4 when absent; revision 0.26)/label/body; globals as data
in `.data`/`.bss`/`.rodata` (`__DATA,__data` etc. on Darwin) with their
EvoIR initial bytes; symbols get the platform prefix (`_` on Darwin).
Static pointer relocations are supported: revision 0.12 emits each
`(offset, @symbol, addend)` as `.xword symbol+addend` at its specified byte
offset, preserving surrounding data; revision 0.15 also permits the
function/block local symbols of Section 5. This is in addition to address
materialization and call relocations; the later external-symbol, linkage,
TLS and relative-label rules specify their own forms. A static pointer is
not itself a refusal (`E4005` was withdrawn at 0.12); frontend `E3026`
remains the boundary for unsupported initializer expressions, not all
symbolic addresses. In `%l[...]` the bracket holds the label's position in the declared list, counting from zero, or `bN` naming that position; a label has no name of its own in either representation (revision 0.8, taken at the point of use and to be revisited when a C front end lowers a real `asm goto`). An `asm` is emitted as its template with every operand reference replaced:
`%0` to `%9` and `%[name]` by the operand's register in its natural width,
`%w0` and `%x0` by the 32-bit and 64-bit spelling of that register, a `Q`
operand by `[reg]`, an immediate operand by its value, `%l[label]` by the
block label, and `%%` by one percent. The template's own text is copied
verbatim and never parsed. The emitted text is
deterministic and is what the receipt digests.

**External symbols (revision 0.24).** A `call @f` whose `@f` is an EvoIR
`declare` uses the platform's `bl` symbol spelling. The address of an EvoIR
`extern` object or `declare` function, whether strong or weak, uses GOT indirection: Linux emits
`adrp reg, :got:symbol` followed by `ldr reg, [reg, #:got_lo12:symbol]`;
Darwin uses `_symbol@GOTPAGE` and `_symbol@GOTPAGEOFF`. This reads the
linker's address slot, not the C object's contents, and adds no EvoIR memory
operation. An absent weak definition has address zero. Defined objects and
functions keep the direct page-relative recipe; calls are unchanged. The object file carries
undefined symbol references resolved by the linker, including in a PIE without
requiring a direct relocation against external data. The minimal `environ`
program is measured against GCC-14 and Clang-19 in
`.evidence/builtin-probes/20260908-external-object-pie/results.txt`; both source
builds run with exit zero, while the former direct-address subject fails PIE
linking. This body change is authorized by the user's 2026-09-08 benchmark
closure instruction. Every such call is cross-boundary in the sense of Section 6, so the
platform's extension rule applies to its narrow arguments. The native lane
of Section 9 may link a program against the host C library for the 10.1.14
library set and against reference-compiled objects for anything else; the
interpreter lanes report the EvoIR codes `E3039`/`E3040` and the native
result decides, exactly as for assembly (revision 0.6).

The first replay exposed the same relocation defect for an external function
address (`longjmp` in TinyCC). Both sealed source builds of
`tests/fixtures/external-function-pie.c` pass PIE linking and execution, whereas
the direct subject reference to `puts` fails linking under both; see
`.evidence/builtin-probes/20260908-external-object-pie/function-results.txt`.
The address rule is consequently shared by both undefined-symbol tables,
not specialized to the first failing symbol kind.

**Linkage (revision 0.16).** An EvoIR function or global whose
`visibility` is `internal` is emitted without `.globl`, so its symbol is
local to the object. Before emission, every internal function that no
`call`, `func_addr`, `block_addr` relocation or other emitted function
references is dropped, and the check repeats until no internal function
is dropped; an internal global is kept, since data is cheap and a
relocation may name it. Module assembly may reference any symbol, so a
symbol named in a module assembly entry counts as referenced. This is
what lets two EvoCC objects that share a header link together (EvoIR
1.9.70).

**Linkage marks (revision 0.22).** A function or global with `linkage=weak`
(EvoIR 1.9.79) is emitted with `.weak name` where `.globl name` would stand;
on Darwin the definition is `.globl name` followed by `.weak_definition name`.
An `extern` or `declare` with `linkage=weak` is emitted as `.weak name`
(Darwin `.weak_reference name`) with no definition, so the linker resolves the
reference to zero when nothing defines it. An `alias @name = @target` is
emitted directly after the target's definition as `.globl name` (`.weak name`
when the alias is weak, nothing when it is internal), `.type name, %function`
or `%object` following the target's kind on ELF, and `.set name, target`, both
symbols carrying the platform prefix. An alias counts as a reference to its
target, so an internal target of an emitted alias is never dropped. Nothing is
copied: the alias has no bytes and no body of its own, and a symbol size,
where the emitter states one, is stated for the target only. The Linux target
is the one this revision measures; the Darwin spellings are stated, not yet
measured.

**ELF visibility (revision 0.30).** Carry the validated EvoIR symbol marks
through allocation to emission. Hidden and protected definitions remain
global/weak according to their independent strength; emit `.hidden` or
`.protected` for definitions, aliases and actually referenced declarations.
An unused undefined declaration must not create a new linker dependency.
Protected binding applies to the final executable/DSO, not this module;
strong hidden/protected references may use direct addresses, whereas
default-visible PIC references use the GOT even for a same-module definition.
Weak references retain GOT lookup and unresolved-null behavior. Direct calls
retain named relocations so the linker applies final component binding.
Source visibility("internal") already arrives as hidden, never as C static.
Darwin has no protected equivalent: native compilation refuses that mark,
not silently replacing it with hidden. The ELF component probes are
`tests/fixtures/symbol-component-*.c`; `.protected` has the GNU assembler's
[component-binding semantics](https://sourceware.org/binutils/docs/as/Protected.html).

**Data spelling (revision 0.20).** A global whose bytes are all zero,
whose EvoIR 2.1 `initialized=` bits are all set and which carries no
relocation is emitted in the target's zero-filled section (`.bss`;
`.section __DATA,__bss,zerofill` on Darwin) as alignment, symbol and one
`.zero N`, and emission returns to the text section after the last global
when that global was zero-filled. Every other global is emitted in the data
section, where a run of at least sixteen zero bytes is one `.zero N` and
the bytes between runs are `.byte` lists of at most sixteen; a run never
crosses a relocation, which keeps the `.xword symbol+addend` at its own
offset. A byte whose `initialized=` bit is clear is emitted as the zero
EvoIR stores for it, and a global that is not fully initialized is never
zero-filled. Measured: a 1,000,001-byte zero array went from 1,000,003
assembly lines to 4 and the static image to within 56 bytes of GCC's.

**Darwin variadic note (revision 0.20).** Apple's AArch64 variant passes
every anonymous argument on the stack, where Linux uses the same registers
as for named arguments (Section 6, revision 0.10). The Darwin lane still
judges by exit status only; `printf` through a Darwin build prints wrong
values until a Darwin variadic rule is measured and written.

**Module assembly (revision 0.14).** Every entry of the EvoIR module
assembly table (EvoIR 2.1, revision 1.9.66) is emitted verbatim, in table
order, after the last function and the last global of the unit. Emission
does not parse an entry and substitutes nothing into it; it precedes the
first entry with a `.text` directive and follows the last with one, so an
entry that switches sections cannot change where a later revision's
trailing output lands.

## 9. Verification

Three instruments, each independent of the lowering:

A function containing an `asm` is outside instrument one: neither
interpreter executes one, so such a function is judged by the allocation
verifier and the native run alone. That is the price of admitting assembly
and it is stated here rather than discovered later.

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

Active refusals are reported at the EvoIR instruction that could not be
lowered; the driver emits no assembly for a function with any of them.
Withdrawn codes remain named so historical receipts can be read. In particular,
the former static-pointer-data, indirect-branch, level-zero frame-address and
pointer-difference boundaries are implemented; the old 1.9.61 impossibility
description is not the current supported profile.

| Code | Name | Meaning |
|---|---|---|
| `E4001` | legacy `not-lowered: floating-point` | Withdrawn for admitted scalar f32/f64; deferred formats remain frontend refusals |
| `E4002` | `not-lowered: vector` | Unsupported vector execution or external ABI; supported integer vectors are admitted. Sub-8-byte native arguments/results remain refused after conflicting cross-link controls (`integer-vector-small*.c`), without rejecting local operations. |
| `E4003` | `not-lowered: wide-integer` | `bits<N>` with `64 < N < 128` or `N > 128`; width 128 includes the UB predicates and hosted division/remainder |
| `E4004` | `not-lowered: tuple` | tuple-typed value |
| `E4005` | `not-lowered: static-pointer-data` | withdrawn at revision 0.12: a relocation `(offset, @symbol, addend)` is emitted as `.xword symbol+addend` at its offset, the surrounding bytes as before |
| `E4006` | legacy `not-lowered: indirect-branch` | withdrawn for the complete `br.indirect` edge-binding path in revision 0.29 (EvoIR 1.9.92); foreign/invalid targets and malformed successor bindings retain their independent checks |
| `E4007` | `not-lowered: frame-address` | withdrawn at revision 0.17 (EvoIR 1.9.72 lowers level zero) |
| `E4008` | legacy `not-lowered: pointer-difference` | withdrawn with EvoIR 1.9.67: `ptr.to_int` operands, 64-bit subtraction and exact signed division lower through existing scalar forms; the C-HIR same-object check is unchanged |
| `E4009` | `not-lowered: unsupported-form` | any EvoIR instruction absent from Section 5 |
| `E4010` | `abi: too-many-register-arguments` | more than eight integer arguments on Darwin (Linux lowers stack arguments, revision 0.10) |
| `E4011` | `abi: aggregate-by-value` | withdrawn at revision 0.18: `byval`/`sret` records follow AAPCS64 (Section 6) |
| `E4012` | `abi: variadic` | withdrawn at revision 0.10: variadic callees and callers lower (Section 6); the name is kept so old receipts read |
| `E4013` | `abi: too-many-results` | more than eight ordinary results |
| `E4020` | `allocation: verifier-refused` | the allocation verifier found a violation; internal, must never reach a user of a released build |

## Appendix A. Textual Form

**`indirect.result` (revision 0.19).** A zero-operand marker that names the
virtual register carrying an `sret` destination above 16 bytes, so that
allocation pins it to `x8` for the call it belongs to (Section 6, revision
0.18). It is written `%v = indirect.result` at the call's program point,
emits no instruction, and is the executor's spelling recorded here after the
record-convention window, as revision 0.8 did for assembly.

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
                 | "%v" int ":gpr64 = stack.alloc " reg ", [origin=" int ", align=" int "]"
                 | "stack.release " reg
                 | "mem.copy" [".volatile.read" | ".volatile.write" | ".volatile.readwrite"] " [" reg "], [" reg "], #" int ", #" int
                 | store-form [" volatile"]
                 | "%v" int ":" class " = atomic.load." atomic-kind " " atomic-order [" volatile"] " [" reg "]"
                 | "atomic.store." atomic-kind " " atomic-order [" volatile"] " " reg ", [" reg "]"
                 | call-form
atomic-kind     := "b" | "h" | "w" | "x" | "p" | "pf"
atomic-order    := "relaxed" | "consume" | "acquire" | "release" | "seq_cst"
terminator      := "b ^" ident "(" args ")"
                 | "br " reg " [" indirect-edge (", " indirect-edge)* "]"
                 | ("b." cond | "cbz " reg | "cbnz " reg | "tbz " reg ", #" int | "tbnz " reg ", #" int) ", ^" ident "(" args ")" ", ^" ident "(" args ")"
                 | "ret" [" " reg ("," " " reg)*]
                 | "brk #" int
indirect-edge   := "^" ident "(" args ")"
call-form       := ("bl @" | "bl.strict_fp @") ident "(" args ")" [" -> " results] | "blr " reg "(" args ")" [" -> " results]
asm-form        := "%v" int ":" class " = asm" [".volatile"] " " string
                   " outputs [" asm-out ("," " " asm-out)* "]"
                   " inputs [" asm-in ("," " " asm-in)* "]"
                   " clobbers [" string ("," " " string)* "]"
                 ; a register output is the instruction's result, as the higher IR writes it;
                 ; a memory output names its address register in the list, since it is not a result
asm-goto-form   := "asm.goto" [".volatile"] " " string " outputs [...] inputs [...] clobbers [...]"
                   " labels [^" ident "()" ("," " ^" ident "()")* "]"
                   " fallthrough ^" ident "()"
asm-out         := string "(" ident ")"
asm-in          := string "(" ident ")" ": " (reg | "#" int | "#zero:" class | asm-symbol)
asm-symbol      := "@" ident ["^b" int] ("+" int | "-" positive-int)
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
| 0.50 | 2026-09-27 | Admit exact scalar f32/f64 SIMD assembly bindings through existing allocation, spills, selected edges and floating-environment invalidation. |
| 0.49 | 2026-09-27 | Emit independently selected per-function/data ELF sections, preserving explicit placement, TLS/storage kind, aliases, lifecycle/patch roots and written option overrides. |
| 0.48 | 2026-09-27 | Hosted Linux binary128 via qualified helpers; shared SIMD128 scalar/HFA/varargs ABI, exact strict status and conversion/NaN adapters. |
| 0.47 | 2026-09-27 | Qualified Linux strict integer128 conversion calls, explicit strict admission and preserved helper semantics. |
| 0.46 | 2026-09-27 | Preserve quiet relational predicate identity and use FCMP rather than the signaling FCMPE path. |
| 0.45 | 2026-09-27 | Add strict FP operations, machine strictness and logical integer conversion widths, dynamic-rounding admission, FPSR effects and explicit FMA; retain the shared SIMD bank. |
| 0.44 | 2026-09-26 | Select existing unsigned twelve-bit add/sub immediates for directly known full-register constants; commute only already-evaluated ADD operands, preserving source checks and nonmatching paths. |
| 0.43 | 2026-09-26 | Select existing immediate scalar shifts for directly known in-range counts decoded at their own width; retain source obligations, operand extensions and dynamic/pair paths. |
| 0.42 | 2026-09-26 | Select either adjacent conditional successor as fall-through after parameter-copy placement; invert only the emitted condition, retaining logical edges, labels and long-branch relaxation. |
| 0.41 | 2026-09-26 | Errata: synchronize dynamic allocation/release forms, grammar and frame rules with the existing EvoIR Appendix D contract and implementation; fixed storage remains stable, only the top dynamic allocation can be popped, and function-lifetime allocations are not reclaimed at VLA scope exit. No new execution scope. |
| 0.40 | 2026-09-26 | Select bounded same-block constant pointer offsets into existing ordinary load/store immediates, preserving access attributes, strict-alignment encodability, guards and effect boundaries. |
| 0.39 | 2026-09-24 | Forward direct-edge identities through ordinary validated-SSA assembly without changing its effects; distinguish independent fixed input occurrences of the same value while preserving genuine tied-operand conflicts. |
| 0.38 | 2026-09-24 | Remove only proven trivial block-argument transport during SSA destruction, including unchanged loop cycles; retain nontrivial merges, complete edges, original effects and deterministic physical allocation. |
| 0.37 | 2026-09-24 | Select an adjacent single-use checked Boolean directly with its producer's flags and an inverted conditional trap branch; retain every guard, effect and continuation argument, with ordinary TBZ for unmatched forms. |
| 0.36 | 2026-09-23 | Select the bounded, single-use full-width compared-value bit mux with existing adjacent CMP/CSEL. Preserve all prior evaluations, guards, effects and block bindings; unmatched chains retain ordinary lowering. |
| 0.35 | 2026-09-23 | Errata: synchronize 128-bit division/remainder lowering and E4003 with the approved hosted runtime path; mark E4006/E4008 withdrawn under existing indirect-edge/pointer-difference rules and remove the stale 1.9.61 diagnostic boundary claim. No new execution scope or waived guard. |
| 0.34 | 2026-09-21 | Preserve validated function-section placement through native emission, including aliases, code labels, CFI and link-ordered patch registrations; restore each following function's own placement. |
| 0.31 | 2026-09-15 | Carry bounded scalar atomic order/volatile/pointer metadata through validation, text, allocation and interpretation; emit the natural-width AArch64 LDR/STR or LDAR/STLR mapping. |
| 0.32 | 2026-09-15 | Add integer SIMD representation/classes, real fixed/variadic vector and HVA ABI, ELF lifecycle registrations, and all four Linux TLS addressing models. |
| 0.33 | 2026-09-20 | Add shared-bank scalar FP, fail-closed FPCR checks, visible 128-bit conversion calls and scalar/HFA ABI. |
| 0.30 | 2026-09-15 | Emit the reviewed protected/hidden ELF binding without changing C linkage or weak strength; preserve interposition for same-module default-visible PIC definitions and retain all symbol marks through native emission. |
| 0.29 | 2026-09-12 | Specifies computed branches with complete CFG edges, joint pre-allocation parameter transport, cycle/spill handling and live target capture, preserving direct-entry and code-label identity. Implements the approved design boundary, not a native acceptance claim. |
| 0.28 | 2026-09-09 | A variadic callee saves `q0` to `q7` in a 128-byte vector save area and writes `vr_top`/`vr_offs = -128`, as both oracles do; the earlier `vr_offs = 0` lost every `double` a reference-compiled caller passed through an EvoCC-compiled wrapper, the cause of musl's floating-point formatting failures in the mixed link. |
| 0.27 | 2026-09-09 | Corrects the stack-realignment instruction sequence to use the reserved scratch register, because logical instructions cannot read SP. Both sealed assemblers reject the previous spelling and accept the replacement. Authorized review correction; this changes no compiler code. |
| 0.26 | 2026-09-09 | Specifies frame realignment (instruction spelling corrected in 0.27) when a slot's alignment exceeds 16 and restores `sp` from `x29`; emits a function's `align=N` as `.p2align`; lowers the 128-bit UB predicates (flags for add/sub, a magnitude product for mul, pair compares for division and shift counts), leaving only 128-bit division and remainder under `E4003`. |
| 0.25 | 2026-09-08 | Carry EvoIR 1.9.80 symbolic assembly immediates without a register, with canonical signed-addend text and target-correct symbol/block emission. This revision changes the contract, not compiler code. |
| 0.24 | 2026-09-08 | Apply the measured PIE address rule to strong undefined functions as well as objects; direct calls and defined symbols remain unchanged. The original TinyCC replay exposed longjmp immediately after environ. This revision changes the contract, not compiler code. |
| 0.23 | 2026-09-08 | Define PIE-safe GOT addressing for all undefined external objects; retain direct defined-object addressing and calls, and the GOT recipe for unresolved weak function addresses. Authorized benchmark closure, measured by the sealed environ probe. This revision changes the contract, not compiler code. |
| 0.22 | 2026-09-08 | Emits `linkage=weak` as `.weak` (Darwin `.weak_definition`/`.weak_reference`) and `alias` records as `.set name, target` after the target, with the alias's own linkage directive and type (EvoIR 1.9.79). |
| 0.21 | 2026-09-07 | Corrects the variadic stack start to follow named stack arguments, using the same ABI placement as calls; the nine-named-argument probe agrees under both sealed oracles. |
| 0.20 | 2026-09-07 | Zero data goes to the zero-filled section and zero runs are `.zero` (the record of the emitter's run encoding); notes the Darwin variadic gap. |
| 0.19 | 2026-09-07 | Records the `indirect.result` marker the record-convention window introduced for the `x8` destination register. |
| 0.18 | 2026-09-06 | Records marked `byval`/`sret` follow AAPCS64: up to 16 bytes in general registers, larger by pointer, results in `x0`/`x1` or through `x8` (EvoIR 1.9.73). |
| 0.17 | 2026-09-06 | The return address is the saved link register of the frame record and the frame address is the frame pointer (EvoIR 1.9.72). |
| 0.16 | 2026-09-06 | Internal functions and globals are emitted as local symbols, and internal functions nothing references are dropped before emission (EvoIR 1.9.70). |
| 0.15 | 2026-09-06 | A label address is `adr` of the block's local symbol, and a static relocation may name that symbol (EvoIR 1.9.68). |
| 0.14 | 2026-09-06 | Bit counting and byte swap lower to `clz`, `rbit`, `rev` and the general-register population-count sequence; module assembly is emitted verbatim after all code and data (EvoIR 1.9.66). |
| 0.13 | 2026-09-05 | Corrects the 128-bit argument rule for Darwin: Apple clang does not round the register number to even, so the pair takes the next two registers; Linux keeps the AAPCS64 rounding. Measured by the executor of the register-pair window. |
| 0.12 | 2026-09-05 | Pointer-integer conversions lower to nothing; static relocations are emitted as `.xword symbol+addend` inside the global's bytes (`E4005` withdrawn), matching EvoIR 1.9.65. |
| 0.11 | 2026-09-05 | External functions and objects (EvoIR 1.9.64 `declare`/`extern`) are emitted as undefined symbol references with the defined-symbol spellings; the linker resolves them, and the native lane may link against the host library for the library set. |
| 0.10 | 2026-09-05 | Opens the two forms the sixth lowering survey put first: `bits<128>` as a pair of `gpr64` halves with measured sequences for every scalar form but division, the even-aligned pair convention for arguments and `x0`:`x1` for results; variadic callees and callers on Linux, with the register save area, the five-field list object and the general-registers-only `va_arg` procedure that agrees with both oracles on fourteen items; stack arguments in 8-byte slots on Linux. `E4012` withdrawn, `E4003` and `E4010` narrowed. The executor's spellings enter Appendix A with the implementation, as revision 0.8 did for assembly. |
| 0.9 | 2026-09-04 | The same 384 C programs compile, link, run and agree on both targets: on the host under Apple clang, and inside the sealed container under GCC-14 for Linux AArch64. Settles the narrow result by probe (the callee extends; `E4014` withdrawn) and states the four places the two targets differ. |
| 0.8 | 2026-09-04 | Inline assembly runs on the machine: eleven constraint rows and both `asm.goto` edges assembled, linked, run and agreeing with host-compiled equivalents. Gives Appendix A its assembly productions, in the executor's spelling, and fixes `%l[...]` as the label's position in the declared list. |
| 0.7 | 2026-09-04 | 384 C programs compiled by the whole pipeline, run natively, and agreeing with the host compiler's build of the same source. Defines a cross-boundary call as one whose callee emission does not define, makes the argument extension a zero extension because EvoIR carries no parameter signedness, and refuses a narrow result across the boundary as `E4014` until a probe settles it. |
| 0.6 | 2026-09-04 | Carries inline assembly through the machine representation: the constraint classes the allocator must satisfy, including early clobber, tied operands, address operands and register clobbers; emission substitutes operand references into the template and copies its text verbatim; a function containing an `asm` is judged by the verifier and the native run, not by the interpreter. |
| 0.5 | 2026-09-04 | 256 generated programs assembled, linked, run and agreeing with the interpreter on exit status; a fourth gate lane; a sixteen-shape calling-convention probe, 32 comparisons, 0 failures. States the cross-boundary argument extension the platform requires, distinct from the internal rule, and records that emission may write addressing forms the Section 4 tables do not admit. |
| 0.4 | 2026-09-04 | First native runs: five assembled programs agree with the interpreter on exit status. Rules the allocation and emission findings: the out-of-SSA scratch is a pinned virtual register, copy placement is a three-way rule, entry arguments become parameters the allocator assigns, the lowest free register is taken, spill accesses are pseudo-forms, frame slots are addressed from `sp`, the prologue is synthesized at emission and not interpretable; the native lane uses a second generator mode so pinned seeds stay pinned. |
| 0.3 | 2026-09-04 | Second implementation: 20,000 of 20,000 generated programs lowered and judged by the three-lane gate with 0 mismatches. Rules the executor's five predicate-row findings: overflow flags report the register, so narrower widths compare with the sign extension of the low N bits (add, sub, mul); shift amounts extend by their declared signedness; the signed left-shift predicate conjoins count, sign, round trip and representability; the division predicates extend first; the grammar spells a two-definition adds/subs and states the conditional-terminator asymmetry. |
| 0.2 | 2026-09-04 | Rules the thirteen stops of the first implementation: lowering rows for every UB predicate; `icmp.eq`/`ne` moved to the zero-extended row after a measured counterexample; `frame.slot` as a per-execution pseudo-form reconciling `alloca` with EvoIR 3.7; `ldr.p`/`ldr.pf`/`str.p` for provenance; a grammar for the textual form with the executor's spellings (symbols, the volatile marker, explicit call registers, two-successor conditionals, spelled entry labels); the `ub.assume` continuation-block construction; `brk` immediates as the diagnostic's decimal part; shift amounts and `ptr.offset` wrapping as obligation-covered differences; `mem.copy` as a pseudo-form transferring initializedness and provenance; block arguments kept until the Section 7 pass; up to eight results in `x0`..`x7` for the machine lane; the five unreachable codes named as such. |
| 0.1 | 2026-09-04 | First statement: purpose, register classes, sub-width representation, the AArch64 scalar instruction subset with executable semantics, the lowering table with fail-closed boundaries, the Darwin arm64 scalar calling convention and frame layout, out-of-SSA and linear scan, emission, three verification instruments, and the `E4xxx` diagnostics. |
