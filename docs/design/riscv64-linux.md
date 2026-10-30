# RISC-V64 Linux userspace backend

This is the design of R07 in [TODO.md](../../TODO.md). Its numbered sections
match the [x86-64 design](x86-64-linux.md), so readers can compare the targets.
It specifies future work. EvoCC does not yet compile for RISC-V.

## Base, scope and evidence

- Source base: `2ff5cf38`. Paths below refer to that tree, before any backend
  restructuring. R04a will move some of these definitions.
- Scope: RV64GC, LP64D, little-endian Linux userspace. No kernel backend.
- Measurements: GCC 14 cross-compilation and QEMU user execution on 2026-10-28.
  The image and commands are in Section 5. This work changes no compiler code.
- Probe sources, output, package versions and executable hashes are retained in
  `OUT/riscv-design/probes/`, where `OUT` is
  `/Users/onehrxn/dev/30days/evocc-runs/team2-goals/out/foundation-0.2.0/`.
  A probe name below identifies both its source and its text output there.
- Proposed instructions and algorithms below are requirements, not results of
  an implemented backend. Assembly observations are not a formal ABI proof.

**Claim ceiling.** The probes establish facts about one pinned GCC, binutils,
glibc and QEMU environment. They do not establish hardware conformance, memory
model correctness, complete helper semantics or EvoCC program correctness.

## 1. Decisions

| # | Decision | Section |
|---|---|---|
| D1 | Use the shared EvoMIR. Keep RISC-V descriptions and emission under `isa/riscv64/`; do not add a second machine IR. | 4.1 |
| D2 | Separate ISA, OS ABI and object format. Add RISC-V only after R04a supplies those boundaries. | 4.2 |
| D3 | Keep the EvoMIR W-value contract. Normalize RV64 word results to zero-extended 32-bit values internally; apply LP64D extension separately at calls. | 4.4 |
| D4 | Use single-consumer adjacent flag chains. Keep a reserved software NZCV value; fuse comparisons only when exact. | 4.6 |
| D5 | Classify records into ordered integer/FP pieces. Support mixed-bank records and integer fallback; `hfa=` alone is insufficient. | 4.7 |
| D6 | Keep real binary128 `long double`. Legalize values to integer pairs or frame-backed storage and use audited helpers before allocation. | 3.4 |
| D7 | Use PIC/PIE and an explicit PC-relative code model. Emit range-safe branches; do not expect the linker to repair every out-of-range jump. | 3.7, 4.8 |
| D8 | Parameterize NaNs, tininess and fenv encoding. Do not reuse the AArch64 numerical adapter unchanged. | 3.10 |
| D9 | No RVV, Zbb, AES or other optional ISA instructions in the initial backend. No implicit FP contraction. | 4.8 |
| D10 | Use `ebreak` for a trap. Carry its reason separately; it has no immediate reason field. | 3.12 |
| D11 | Once the driver recognizes this target, refuse proof-required compilation with exit 6 before export. Lean stays unchanged. | 8 |
| D12 | Run GCC and QEMU in an ARM64 container. RISC-V is the program target, not the container's platform. | 5 |

## 2. The current target abstraction

### 2.1 What is shared

| Layer | Current files | Required treatment |
|---|---|---|
| C types and layout | `crates/evocc-types/src/lib.rs` | Pass the selected `TargetDataLayout`, including wide-character and varargs facts. |
| C-HIR and EvoIR | `crates/evocc-chir/`, `crates/evocc-evoir/` | Keep semantic values and obligations; add reviewed target ABI descriptors. |
| EqSat | `crates/evocc-eqopt/`, `crates/evocc-egraph/` | Preserve registry checks and target layout identity. No new rewrite domain. |
| Machine data, text and validation | `crates/evocc-mir/src/{lib,text,text_parse,validate}.rs` | Keep shared forms and their interpreter meaning. |
| SSA destruction and allocation | `crates/evocc-mir/src/generic/` | Supply target traits; do not insert ISA names here. |
| Allocation validation | `generic/allocverify.rs` | Validate the same location, clobber and ABI facts that production uses. |
| Frame and argument cursors | `generic/{frame,abi}.rs` | Extend placement and incoming-frame facts without changing AArch64 output. |

### 2.2 What is AArch64-specific

`Target { Darwin, Linux }` in `emit.rs` does not identify an ISA. The current
tables, register adapters and emission live under `isa/aarch64/` and
`targets/aarch64/`. Record classification, variadic calls and target assembly
still have AArch64-specific code in `lower.rs`. Scalar floating emission lives
in `floating.rs`. TLS and memory-barrier templates also contain AArch64 text.
R04a must separate these sites before R07b starts.

### 2.3 What already generalizes

The traits in `generic/{machine,allocation,ssa}.rs` describe instruction uses,
definitions, copies, pins, clobbers and successors. RISC-V uses three-address
integer instructions and needs no general tied-operand allocator. The existing
copy and spill algorithms can be reused through a RISC-V adapter.

Do not equate the `Simd32` and `Simd64` classes with vector support. They can
describe scalar views of one FP register bank. `Simd128` cannot be assigned to
one RISC-V FP register: FLEN is 64 in this ABI.

### 2.4 AArch64 assumptions outside the backend

R04a must pass the driver's layout into the interpreters, pointer arithmetic,
lifecycle handling and EqSat cache identity. It must separate a pointer
`va_list` from stack-only anonymous arguments. RISC-V needs the first property
but not the second. `wide_character_rank` must select signed `int` here,
although AArch64 Linux selects unsigned `int`.

Do not change the Core proof gate's AArch64 layout into a permissive target
test. The RISC-V execution path and the proof profile are separate.

## 3. The RISC-V64 Linux userspace slice

### 3.1 Identity

Use `riscv64-unknown-linux-gnu`, `-march=rv64gc` and `-mabi=lp64d`.
The probe compiler's own `-dumpmachine` prints `riscv64-linux-gnu`; that is a
toolchain prefix, not a reason to expose a different EvoCC target contract.
The executable is ELF64 little-endian, RISC-V, double-float ABI, with RVC
permitted (`elf-header.txt`, `elf-attributes.txt`, `dumpmachine.txt`).

Start with uncompressed emitted instructions using `.option norvc`. This is a
valid subset of RV64GC, not a different ABI. Reference libraries can still use
compressed instructions. Do not derive whole-program instruction counts by
dividing `.text` by four.

### 3.2 Data layout

`layout.c` / `layout.txt` measured these size/alignment pairs in bytes:

| Type | Size / alignment |
|---|---|
| `_Bool`, `char` | 1 / 1 |
| `short` | 2 / 2 |
| `int`, `float`, `wchar_t` | 4 / 4 |
| `long`, `long long`, pointer, `size_t`, `ptrdiff_t`, `double` | 8 / 8 |
| `__int128`, `long double` | 16 / 16 |
| `va_list` | 8 / 8; a pointer |
| `max_align_t` | 32 / 16 |

Plain `char` is unsigned. `wchar_t` is signed. Byte order is little-endian.
`FLT_MANT_DIG`, `DBL_MANT_DIG` and `LDBL_MANT_DIG` are 24, 53 and 113;
`FLT_EVAL_METHOD` is zero. LP64 and binary128 storage match AArch64 Linux;
wide-character signedness and the variadic-list representation do not.

Bit-fields, packed records and explicit over-alignment must use the shared
layout engine with this target profile. Full cross-compiler record-layout
coverage is not measured by this scalar probe; it is a required R07c lane.

### 3.3 Calling convention (LP64D)

Use the eight integer argument registers `a0` to `a7` and the eight FP argument
registers `fa0` to `fa7`. Results use up to `a0/a1` and/or `fa0/fa1` according
to classification, not the current EvoMIR limit of eight general results.
Use the integer convention when FP classification cannot use all its required
registers. This is not the AAPCS64 HFA algorithm.

`abi_shapes.{h,c,s}`, `abi_main.c` and `abi.txt` measure separate caller/callee
units with integer/FP records, arrays, unions, packed records and bank
exhaustion. These are GCC-to-GCC observations; EvoCC cross-calls remain future
tests.

| Shape | Observed placement in `abi_shapes.s` |
|---|---|
| `{ double f; long i; }` | `fa0` and `a0`; next integer uses `a1`. |
| `{ long i; double f; }` | `a0` and `fa0`; declaration order survives flattening. |
| Two `float` or two `double` fields | Two FP registers. |
| `float values[2]` | Two FP registers after flattening the array. |
| Empty C struct followed by `float` | Empty field contributes no argument piece. |
| Packed `{ char c; double d; }` | Integer plus FP registers despite the unaligned field. Do not copy an x86 MEMORY rule. |
| Union of `double` and `long` | Integer convention, not FP flattening. |
| Three `long` fields | Indirect argument. |
| Return `{ double; long; }` | `fa0` plus `a0`. |
| Return three `long` fields | Hidden result pointer in the integer argument stream. |
| Seven integer arguments then two-word named record | First word in `a7`, second on stack; next argument on stack. |
| Seven integer arguments then named `__int128` | Low half in `a7`, high half on stack; no anonymous even-register rule. |
| Integer bank exhausted before mixed record | Whole record uses integer fallback on stack; a later independent FP scalar still uses an FP register. |
| FP bank exhausted before mixed record | Whole record uses integer fallback, not a partial FP plan. |

The runtime checks produce the expected sums and returned fields in `abi.txt`.
They do not exhaust nested records or padding layouts. R07c must test those
without reading indeterminate padding as C values.

For integer scalars narrower than XLEN, separate the ABI extension from the
internal bit-vector value. `narrow.c` passes and returns through leaf assembly:
unsigned 32-bit `0x80000000` arrives as `ffffffff80000000`; signed `-1` also has
all high bits set; unsigned byte 255 arrives as 255. LP64D first widens a
narrow integer according to its type to 32 bits, then sign-extends to XLEN.
Return values obey the corresponding argument rule.

The hidden result pointer consumes `a0` and moves ordinary arguments along.
Do not assume that a callee returns that pointer in `a0`: callers retain their
own copy when needed. Classify both caller and callee from one descriptor.

Anonymous arguments use the integer convention, including `double` and
`long double`. A 16-byte-aligned two-word anonymous argument needs an even
integer-register pair. If that pair is unavailable, place it on the aligned
stack; later anonymous arguments stay on the stack. An 8-byte-aligned two-word
record can instead split across `a7` and the first stack slot. Both cases are
measured by `variable_quad_after_seven` and `variable_pair_after_seven`.

A pointer `va_list` walks a contiguous integer-save/stack sequence. Save the
remaining integer argument registers directly below the entry stack pointer,
in register order. Do not put a frame record between this save area and the
incoming stack. `abi-frame.s` shows a variadic frame with `s0 = entry_sp - 16`,
`a7` at `s0 + 8` and incoming stack arguments at `s0 + 16`. Thus `s0` is not
always the entry stack pointer. Compute these offsets as frame facts.

### 3.4 `long double`

Keep binary128 constants, objects and arithmetic. Never alias it to `double`
and never assign a 128-bit value to one FLEN=64 register. The external ABI
uses the integer convention: a pair of general registers or stack words.

Legalize `f128` operations before allocation. Represent transports as two
64-bit pieces or private 16-byte frame storage with exact bit copies. A helper
call must be an ordinary machine call with uses, definitions and clobbers;
the emitter must not insert an invisible call after allocation.

`quad_add`, `quad_mul`, `quad_to_double`, `double_to_quad` and `uint_to_quad`
in `abi_shapes.c` call `__addtf3`, `__multf3`, `__trunctfdf2`, `__extenddftf2`
and `__floatunditf`. `quad-symbols.txt` lists the installed libgcc symbols,
including subtraction, division, comparisons and signed/unsigned conversions
to and from 32-, 64- and 128-bit integers. Symbol presence alone is not an
acceptance test. The arithmetic/fenv probes call only the measured helpers.

`floating.c` directly calls add, multiply and divide helpers under four modes;
`tiny_boundary.c` measures their underflow boundary. Remaining operations,
conversion status, `sqrtl`, `fmal` and NaN-sensitive comparison contracts need
R07d tests. Until audited, a corresponding builtin lowering must refuse rather
than guess a helper. Ordinary C libm calls remain library calls.

### 3.5 Predefined macros and feature queries

`macros.txt` records all 365 definitions from GCC 14 with the chosen ISA/ABI.
It includes `__riscv_xlen=64`, `__riscv_flen=64`,
`__riscv_float_abi_double`, `__riscv_cmodel_medany`, `__CHAR_UNSIGNED__`,
signed `__WCHAR_TYPE__ int`, binary128 long-double facts, and `__PIC__=2` /
`__PIE__=2`. The architecture string includes I, M, A, F, D, C, Zicsr and
Zifencei; it does not grant Zbb or RVV instructions.

Build a reviewed per-target profile, not a copy of every GCC feature promise.
Advertise only implemented language/builtin behavior. `has_builtin.c` tests
329 named queries, including the current EvoCC profile names and six added
queries. It is not an enumeration of all GCC builtins. GCC reports
`__builtin_clz`, `__builtin_bswap64`, `__builtin_thread_pointer`,
`__builtin_riscv_frflags` and `__builtin_riscv_fsflags` as available. It reports
`__builtin_riscv_rdcycle` and `__builtin_riscv_clmul` as unavailable.
`__builtin_riscv_pause=1` does not authorize EvoCC to claim an implementation.

### 3.6 Stack, frame pointer and red zone

Keep 16-byte stack alignment and no red-zone allocation. Reserve `s0` as a
frame pointer in the initial backend. Keep explicit return-address and prior
frame links, with offsets described by the target adapter rather than the
AArch64 `fp + 16` rule. `abi-frame.s` records GCC's normal and variadic
examples and their CFI. Dynamic allocation must preserve outgoing stack
arguments and restore the prior stack pointer at the correct lifetime end.

The initial stack protector uses a GOT-loaded `__stack_chk_guard` and a call
to `__stack_chk_fail`, as `mechanics-pic.s` measures. Do not copy AArch64 TLS
or x86 segment offsets. Full unwinding through every frame form is not
measured; emitted CFI and unwinding are required R07c tests.

### 3.7 PIC, PIE and relocations

Use PC-relative `auipc` pairs for local addresses, GOT loads for interposable
data, and direct/PLT call sequences according to visibility. Keep the
`%pcrel_lo` label anchored to the matching `auipc`, not to the final target
symbol. `mechanics-{pic,local}.s` records GCC's address and call spellings;
`branch_indirect.s` links a matching `auipc/jalr` pair across a 2 MiB gap.

Use a declared medany-style PC-relative range and reject an out-of-range
relocation. Do not claim arbitrary 64-bit code distances. Conditional branches
have a signed 12-bit displacement in halfword units; JAL has a signed 20-bit
displacement in halfword units. Emit inverse-condition plus a long transfer
when a conditional edge is not provably in range. An `auipc/jalr` sequence
must use reserved scratch and preserve `ra` for a non-call jump.

`branch_conditional.s` shows GNU as expanding an 8 KiB conditional branch to
an inverted branch plus JAL. `branch_direct.s` deliberately links a JAL over
2 MiB: it fails with `R_RISCV_JAL` truncation (status 1). Thus assembler
relaxation of a conditional branch is not a general large-function solution.

### 3.8 TLS

Support local-exec, initial-exec and general-dynamic through target templates.
`mechanics-local.s` records `%tprel_hi`, `%tprel_add`, `%tprel_lo` with `tp`;
`mechanics-pic.s` records `la.tls.gd` and `__tls_get_addr@plt`.
Initial-exec runtime behavior is not measured in this design. R07c must add
PIE/shared-library and multithreaded tests for each admitted model.
Calls needed by TLS legalization must be visible before allocation.

### 3.9 Atomics (RVWMO)

Use the A extension for admitted naturally aligned 32-/64-bit operations.
Preserve order and volatile independently. `mechanics.c` records GCC's
`fence`, AMO, LR/SC and subword-helper choices in its generated assembly;
this probe does not establish their concurrent behavior.

Start with conservative fences and explicit acquire/release bits. Do not map
all orders to a plain load/store merely because a single-thread test passes.
Byte/halfword RMW requires a reviewed masked containing-word LR/SC loop or an
audited helper. GC has no byte/halfword AMO instruction. Any LR/SC retry loop
must respect the architecture's progress constraints, not just compute the
right answer on a successful iteration. Refuse unaligned and unsupported
widths rather than silently weaken atomicity.

R07c needs old/new return-value tests, failed compare-exchange tests, ordering
tests and multithreaded stress. QEMU user execution is not an RVWMO proof;
hardware concurrency evidence remains not measured by this design.

### 3.10 Floating point and the environment

Use F/D scalar operations and explicit FMA when EvoIR requests it. Keep source
formats and the no-implicit-contraction contract. Normalize scalar `f32` values
using valid NaN boxing in FLEN=64 registers. `nanbox.c` measures
`fmv.x.d` of boxed 1.0f as `ffffffff3f800000` and `fmv.x.w` of negative zero
as `ffffffff80000000`. A bits<32> transport must clear the latter high bits.

`floating.c` / `floating.txt` records explicit native instructions for f32/f64
and direct libgcc calls for f128. In this environment:

| Fact | Result |
|---|---|
| Invalid arithmetic default NaN | f32 `7fc00000`, f64 `7ff8000000000000`, f128 `7fff8000000000000000000000000000`. |
| Arithmetic quiet NaN payloads | Canonicalized, either operand position; no invalid flag. |
| Signaling NaN arithmetic | Canonicalized; invalid flag set. |
| Rounding modes in glibc | nearest 0, toward-zero 1, downward 2, upward 3. |
| `fcsr` | `frm` in bits 7:5; NV/DZ/OF/UF/NX in bits 4/3/2/1/0. |
| Inexact / underflow / overflow | `01` / `03` / `05` for the tested cases. |
| Divide by zero | `08`. |
| Quiet NaN comparison | `feq` returns false without NV; `flt` returns false with NV (`nanbox.txt`). |

The four half-ULP rounding cases agree for all three formats. These probes
do not prove every operation's payload, signed-zero or exception behavior.
Preserve exact-bit constants and transports; arithmetic canonicalization does
not authorize canonicalizing a load, store, bitcast, `copysign` or negate.

Tininess needs a real boundary test. The first probe's `tiny-rounds-normal`
case has normal final bits but UF|NX. That fact alone does not establish
before-rounding detection: full-precision rounding with an unbounded exponent
can still leave that exact case tiny. `tiny_boundary.c` instead multiplies
`(1 + 2^(1-p)) * min_normal` by `(1 - 2^(1-p))`, for p=24/53/113. The exact
product is below `min_normal`, but rounding at full precision already reaches
`min_normal`. All three formats report NX alone (`tiny_boundary.txt`). This
distinguishes after-rounding detection from before-rounding detection here.
Keep both probes; do not infer a policy from final encoded bits alone.

Add a target FP model in the numerical adapter: arithmetic NaN policy,
tininess, environment encoding and helper contracts. Do not reuse the Arm
boundary correction without correspondence vectors. Strict operations still
read/write the existing world/fenv state. Map status bits explicitly.

Fixed-environment entry checks require RNE. Dynamic C rounding admits its four
modes; unsupported `frm` values fail closed. Reads and writes of `fcsr` must
preserve the unrelated status/control fields. A call or opaque assembly
invalidates cached environment knowledge. No trap-enabled C fenv profile is
added. R07d must test fenv save/restore and native/interpreter agreement.

### 3.11 Inline assembly

Keep target templates and constraint interpretation separate from AArch64.
Start with operand-free barriers and then the bounded integer/memory subset
that the pinned applications use. Support declared uses, outputs, ties,
early clobbers and named register clobbers. Refuse unknown modifiers,
constraints and `asm goto` shapes until their control-flow and allocation
handling have tests. Do not interpret an Arm `Q` or `w` constraint as a
RISC-V promise merely because it is present in shared text today.

### 3.12 Traps

`trap.c` executes `li t0,3004; ebreak`. QEMU reports SIGTRAP and exit 133
(`trap-status.txt`); it does not print diagnostic 3004. `ebreak` has no reason
immediate. The backend must retain `MTerm::Brk.imm` in a reserved register and
documented metadata or a separately decoded trap site. Do not claim a signal
handler reads that number until tested. Native RISC-V kernel/host behavior is
not measured here. Checked UB tests must distinguish exit 133 from normal
main returning that low byte.

### 3.13 Assembly syntax facts (measured)

`syntax.sh` records each command, status and stderr in `syntax-status.tsv`
and `syntax-{gnu,llvm}-*.stderr`. GNU as 2.44 and Clang 19.1.7 report:

| Source spelling | GNU as | LLVM integrated assembler |
|---|---|---|
| `#` comments, leading and trailing | Accept | Accept |
| `//` comments | Reject | Accept |
| `.type name,@function` or `%function` | Accept | Accept |
| `.dword`, `.quad` | Accept | Accept |
| `.xword` | Reject | Reject |
| `.p2align`, `.ident` | Accept | Accept |
| `clz` under `-march=rv64gc` | Reject | Reject |
| `vsetvli` under `-march=rv64gc` | Reject | Reject |

Emit the common subset, including `#` comments and `.quad` data. This matrix
does not approve every directive in the AArch64 emitter.

## 4. Machine IR design

### 4.1 Options compared

| Option | Decision |
|---|---|
| A second RISC-V-only machine IR | Reject: duplicates text, interpretation, effects and allocation validation. |
| A string emitter that inserts hidden helper calls | Reject: hides uses, results and clobbers from allocation. |
| Shared EvoMIR with target legalization and fixed expansion templates | Select. Preserve existing semantic contracts. |

Backend legalization plans may group forms and split ABI pieces. They are not
an alternate public IR. Keep original identities available for validation.

### 4.2 Target modeling

Use the R04a ISA/OS/object split. Dispatch emission, ABI classification,
allocation facts, predefined profiles and toolchains on the full target.
Keep AArch64 defaults unchanged. Select RISC-V explicitly on this host.
Do not execute a RISC-V ELF file directly on macOS.

### 4.3 Register file

The proposed GPR table reserves `zero`, `ra`, `sp`, `gp`, `tp`, `s0` and four
scratch registers `t0` to `t3`. It offers 22 allocatable registers: `a0..a7`,
`s1..s11`, `t4..t6`. Reserve `t2` for software flags across an adjacent chain;
spill/address templates must not clobber it. Use `t3` for copy cycles when no
instruction expansion is active. Per-form scratch requirements remain
explicit and must pass all aliasing tests before this table is accepted.

Use a separate FP bank with f32/f64 views. Reserve distinct FP work and
copy-cycle registers; exclude them from allocation. Describe argument/result
registers as ordered lists. `fs0..fs11` preserve scalar values no wider than
ABI FLEN=64. No 128-bit scalar/vector view fits this bank.

`gp`/`tp`, callee saves and call clobbers are ABI inputs, not optional
allocator heuristics. The reference sequences expose only the registers their
examples use; exhaustive preservation tests remain R07b/R07d requirements.

### 4.4 Three-address forms and word normalization

Most base instructions have independent destination and source registers.
An expansion still must handle destination/source aliasing and pinned call
locations. Test every admitted alias pattern, not only distinct registers.

EvoMIR masks W results to 32 bits (`interp.rs`, `binary`). RV64 `addw`,
`subw`, `mulw`, `divw`, `divuw`, `sllw`, `srlw` and `sraw` sign-extend their
32-bit result. Clear the high half after an internal W result, for example
with `slli rd,rd,32; srli rd,rd,32`, unless an independently checked invariant
allows omission. Do not use this zero extension for a uint32 ABI boundary:
Section 3.3 requires sign extension there.

`integer.c` / `integer.txt` measures word sign extension, shift-count masking,
division by zero and MIN/-1. RV64 division by zero returns all ones, whereas
the shared EvoMIR division returns zero. Each SDIV/UDIV template must correct
the zero-divisor case, including direct machine inputs not preceded by a C
UB guard. MIN/-1 wraps to MIN in both models. Machine shifts mask their count
to 5 or 6 bits; do not confuse that with EvoIR's total source operation.

### 4.5 Fixed registers: per-form clobbers

There are no x86-style dividend/result fixed registers for ordinary multiply
and divide. Calls, TLS, atomics, inline assembly, fenv and scratch expansions
still have clobbers. Declare them through target instruction facts. A libgcc
call must use the ABI call model, never the scratch-only model.

Keep the software flags register live across spill insertion between a
producer and consumer. Test long offsets and spills on both sides of carry
chains. A template that cannot fit its reserved resources must be legalized
or refused before allocation; it must not silently borrow a live register.

### 4.6 Flags and conditions

`validate.rs` requires one adjacent consumer for every flags definition,
forbids flags block arguments and permits carry-producing chains. This is
the basis for local fusion; no flags value crosses a call or CFG edge.

Preserve the full N/Z/C/V meaning, not only a convenient signed comparison.
The reference conditions are: EQ=Z, NE=!Z, CS=C, CC=!C, MI=N, PL=!N,
VS=V, VC=!V, HI=C&&!Z, LS=!C||Z, GE=N==V, LT=N!=V,
GT=!Z&&(N==V), LE=Z||(N!=V). Arithmetic width is part of the computation.
An unknown condition/producer pairing fails closed.

For subtraction C means no borrow. For addition C means carry-out. Logical
tests compute N/Z and clear C/V as the shared machine contract requires.
Compute overflow from the original operands and the truncated result; do not
overwrite an operand first when the destination aliases it. A W signed test
must sign-extend its low 32 bits before a signed XLEN comparison.

Fuse `Cmp/CmpImm` plus a supported branch into `beq/bne/blt/bge/bltu/bgeu`.
Otherwise encode exact NZCV in the reserved flags register and read it with
integer tests. `Cset` produces 0/1. `Csel` uses a branch or mask sequence, not
Zicond. Carry chains use `sltu` and explicit no-borrow/carry bits, including
two overflow/borrow stages for ADC/SBC. `AddCarryFlags` and `SubCarryFlags`
must produce every flag needed by their next consumer, not only C.

### 4.7 Calls and argument placement

Extend the shared cursor with explicit piece placement: bank/index, stack
slot, alignment, split-pair behavior and the anonymous stack lockout. A mixed
record plan must test all required banks before consuming any register. On
failure, roll back that tentative plan and apply the integer convention.

Use a target ABI descriptor derived from source types and field offsets.
It needs flattened FP/integer pieces, integer extension, indirect parameters,
hidden results, anonymous alignment and original object size. `hfa=` alone
cannot describe mixed records, unions or packed-field offsets. Define a
versioned EvoIR mark with grammar/reader/checker tests before adding it; never
reinterpret an old AArch64 mark silently. The descriptor is classification
data, not permission to remove memory/provenance/initializedness obligations.

The generic `IndirectResult` marker must select RISC-V's argument-consuming
hidden pointer instead of AArch64's dedicated x8 convention. Generic frame
facts need a per-function incoming-stack base/bias for variadic save areas.
Caller and callee, allocation pins and the verifier must use the same plan.

### 4.8 Expansion table

This table covers every `MInst` and `MTerm` variant at the source base.
R07b/c/d/e are implementation exits, not current support. An unimplemented
cell is an explicit refusal, never a dropped operation.

| Form | Planned expansion or refusal | Exit |
|---|---|---|
| `Mov` | `mv` for integer; exact FP move for admitted scalar views; integer-pair transport for f128. | b/d |
| `MovImm` | Deterministic `lui/addi/slli` chunks, including sign-carry correction; test every 12-bit boundary and bit 63. | b |
| `Adrp`, `AddPageOff` | A paired local PC-relative or GOT address plan; keep one matching anchor. | c |
| `Adr` | PC-relative local block address, kept through critical-edge splitting. | c |
| `Load`, `Store` | Signed/unsigned byte/half/word and XLEN accesses; scalar FP loads/stores in d; exact bit masks/transport metadata retained. | c/d |
| `Binary` | `add/sub/mul/div/divu/and/or/xor/sll/srl/sra`, with zero-divisor correction and W normalization. | b |
| `BinaryImm` | Use a valid immediate or materialize it in scratch; never truncate an out-of-range immediate. | b |
| `FlagBinary` | Add/sub result and exact software NZCV; respect operand aliases. | b |
| `WideMultiply` | `Smull`: sign-extend both low words then `mul`; `Smulh`: `mulh`; `Umulh`: `mulhu`. | b |
| `Msub`, `Madd` | Multiply then subtract/add modulo width; reserve intermediate storage before writing an aliased addend. | b |
| `CarryBinary` | ADC/SBC with explicit carry/no-borrow and truncation. | b |
| `AddCarryFlags`, `SubCarryFlags` | Two-stage carry/borrow and exact N/Z/C/V. | b |
| `TstImm` | Materialize mask, AND, derive N/Z; C=V=0. | b |
| `Unary` | `Clz`, `Rbit`, `Rev16`, `Rev`: bounded shifts/masks/tests, no Zbb instruction or hidden call. | b |
| `Extend` | Shift/sign-shift or masks for Sxtb/Sxth/Sxtw/Uxtb/Uxth; preserve the declared destination class. | b |
| `Bfx` | Shift/mask/sign-extension with explicit width and boundary tests. | b |
| `Cmp`, `CmpImm` | Compare fusion or software NZCV of subtraction. | b |
| `Cset`, `Csel` | Exact condition evaluation and 0/1, branch or mask selection. | b |
| `Bl`, `Blr` | Direct `call`/PLT or `jalr ra`; explicit argument/results, call clobbers and returns-twice spills. | b/c |
| `Ldp`, `Stp` | Two scalar accesses and explicit base update; no native pair instruction. Volatile pairs require a separately specified transaction contract or refusal. | c |
| `SpillStore`, `SpillReload` | Width-correct private frame access; normalize W; scalar FP saves in d. | b/d |
| `IncomingArg`, `OutgoingArg` | Explicit ABI stack slot with per-function incoming bias. | b/c |
| `MemCopy` | Exact-byte bounded expansion; no out-of-range read and no hidden libc call; volatile access rules remain explicit. | c |
| `FrameSlot` | Aligned private object address in the computed frame. | b/c |
| `StackAlloc`, `StackRelease` | Aligned runtime allocation, saved prior-SP header and top-allocation lifetime check. | c |
| `FrameAddress` | Incoming stack, integer-save top, saved return address, current/ancestor frame through RISC-V frame facts. | c |
| `IndirectResult` | Pin the argument-consuming hidden result pointer; emit no instruction for the marker. | c |
| `FpEnvCheck` | Read/validate `fcsr`; fixed RNE or admitted dynamic modes; fail closed. | d |
| `VaSaveArea` | Save the remaining integer arguments contiguously below entry SP; no AArch64 vector-save area. | d |
| `Floating` | F/D arithmetic, sqrt, comparisons, explicit FMA, conversions and bit transports; f128 helpers legalized before allocation; strict status mapping. | d |
| `Atomic` | Aligned scalar access plus the required ordering sequence. Unsupported width/alignment refuses. | c |
| `AtomicRmw` | AMO or reviewed LR/SC/helper plan; subword preservation and compare-exchange failures tested. | c |
| `Asm` | Operand-free barriers in c; reviewed application subset in e; otherwise refuse. | c/e |
| `Aes`, `Vector` | Refuse. No RVV, vector register class or Arm AES emulation in this slice. | outside |
| `MTerm::Indirect` | `jalr zero` to a validated local target; preserve the complete edge bindings. | c |
| `MTerm::B` | Range-safe direct or `auipc/jalr zero` transfer. | b |
| `MTerm::BCond` | Fused comparison or software condition; inverse local branch plus long transfer. | b |
| `MTerm::Cb`, `MTerm::Tb` | Zero or bit test, then the same range-safe transfer. | b |
| `MTerm::Ret` | Place the declared ABI results, restore frame and `ret`; reject excess unclassified results. | b/c/d |
| `MTerm::Brk` | Reason metadata/register followed by `ebreak`; signal status 133 in the measured lane. | b |
| `MTerm::AsmGoto` | Refuse until a separately tested target constraint/edge implementation exists. | outside |

`LoadKind` and `StoreKind` scalar subtypes use the same alignment and extension
rules. Half/bfloat/vector forms without an audited scalar legalization refuse;
RV64GC does not grant Zfh, BF16 or RVV. Do not accept `Simd128` loads/stores
and then truncate them to FLEN=64.

Carry alignment through to emission. The GCC packed-load probe uses byte
accesses; GC alone does not promise fast or available misaligned XLEN loads.
Expand ordinary unaligned accesses without reading outside the object.
Never split a volatile access into a different number of observable accesses
without a target contract. Otherwise refuse that form. Apply the same rule to
volatile 128-bit pairs; two ordinary scalar accesses do not prove equivalence.

### 4.9 What the allocation verifier needs

The verifier needs ordered ABI banks, implicit-result placement, stack slots,
call clobbers, reserved scratch, FP view aliases and exact spill width. It must
not assume AArch64 numbering, partially preserved 128-bit FP views or one
fixed `frame_record_bytes` for every function. Test rejected overlap and
clobber cases as well as accepted allocations. Existing Lean allocation
theorems are not automatically RISC-V backend theorems.

### 4.10 Target tables and the generator

Add `targets/riscv64/` facts for registers, ABI, instruction effects and
expansions. Use R04a's target-directory generator. Keep the generated modules
target-local and keep the generic boundary test. Explicitly enumerate the
admitted ISA, operands, register views and per-form clobbers. Run
`generate-target.py --check` for every target, not only the new directory.

### 4.11 Lowering changes

Add target dispatch for records, variadic access, helpers, TLS, atomics and
inline assembly. Keep semantic width and target alignment available until
legalization. Preserve initializedness, transport and world/fenv effects.
Known C guards do not justify changing the total behavior of direct EvoMIR
inputs. Test such inputs separately from C-generated forms.

### 4.12 Front end and driver changes

Add the target layout constructor, signed wide-character facts, predefined
profiles and truthful feature queries. Select the assembler/linker explicitly
when cross-compiling; do not silently use the macOS AArch64 tools. Add a
RISC-V sysroot contract and export only after a backend milestone authorizes
those changes. This design exports no new sysroot and changes no driver flag.

## 5. Testing

### 5.1 Environment measured on this host (2026-10-28)

The host runs Apple `container` 1.2.2 (commit `0190097`). The disposable guest
is Linux ARM64. It runs a cross compiler and `qemu-riscv64`, not a RISC-V VM.

| Tool | Measured version / evidence |
|---|---|
| Cross GCC | `riscv64-linux-gnu-gcc-14`, 14.2.0-19; `gcc-version.txt`. |
| GNU assembler | binutils 2.44; `assembler-version.txt`. |
| LLVM assembler | Clang 19.1.7; `clang-version.txt`. No LLVM C-ABI cross-call result is claimed here. |
| QEMU user | 10.0.11, Debian `1:10.0.11+ds-0+deb13u1`; `qemu-version.txt`. |
| RISC-V libc headers | Debian `libc6-dev-riscv64-cross`; exact package version in `packages.tsv`. |
| All installed packages | Full sorted name/version/architecture list in `packages.tsv`. |

### 5.2 Images

The approved probe image is `evo-riscv64-probe:20261028`:

- Index SHA-256: `c9e7d70fb97264c21dd1a3b24657d5fceab0aca88e2a59f92fe876d3f8c09a79`.
- ARM64 manifest SHA-256: `1b4c35bde5c052cd02cff1130b28523ecc1b2a7ec798cf2aaeabde21dca39fa7`.
- Base `evo-nexlab:sealed` index:
  `e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`.

`OUT/riscv-design/image.json`, `image-context/Dockerfile` and `image-build.log`
retain the build identity. The base was resolved locally. Only the approved
Debian packages and their dependencies were downloaded from its inherited
mirror. Every probe ran with `--network none`, two CPUs and 2 GiB memory.
No downloaded program ran on macOS.

### 5.3 Lanes and oracles

| Lane | Oracle / limit |
|---|---|
| Expansion | Shared EvoMIR interpreter plus exact bit results; GCC-generated native functions for cross-checks. |
| ABI | Separate GCC/EvoCC caller and callee in both directions; then the same with Clang where its target runtime is available. |
| FP | Target-parameterized APFloat/interpreter, explicit instructions and audited libgcc/libm calls; compare value and all status bits. |
| ISA | Actual object instructions and ELF attributes; baseline GC only. |
| Applications | Original pinned upstream suites, original inputs and flags. No target-object substitution to claim a direct build. |
| AArch64 regression | Existing 792-object full-byte baseline/twin check and full workspace gate. |

An unavailable oracle, timeout, unsupported input or skipped run is not PASS.
QEMU is an execution tool, not an independent formal proof of its own ISA.

### 5.4 Unit tests per expansion

Use widths 32 and 64, all operand-alias cases, immediates at encoding limits,
carry and signed-overflow boundaries, zero divisors, MIN/-1, shift counts
0/N-1/N/2N and bit extraction at both ends. Test `clz(0)` and complete
reverse patterns. Force spills and large frames around every software flags
chain. Use branch distances beyond 4 KiB and 1 MiB; check actual relocations.

For loads/stores test each width, extension, alignment, volatility and exact
object bounds. For constants include zero, all ones, alternating bits,
12-/32-bit sign boundaries and bit 63. Assemble accepted expansions with GNU
and LLVM. Run the resulting objects under QEMU and check returned bits.

### 5.5 ABI cross-call tests

Cover the shapes in Section 3.3, both bank exhaustion orders, mixed returns,
nested/packed records, arrays, unions, narrow signed/unsigned values,
indirect arguments/results, all integer-register boundaries and stack slots.
Force callee-saved values across direct, indirect and returns-twice calls.

For variadic calls test every named-register count from zero through eight,
promoted integers/doubles, f128 and `__int128`, aligned anonymous pairs,
split ordinary pairs, stack lockout, `va_copy`, forwarding and mixed caller
languages only where explicitly supported. Check the integer-save area next
to incoming stack bytes. For FP test NaN boxing and preservation of both
f32/f64 views across calls. No result may rely on padding bytes being zero.

### 5.6 Static ISA audit

Decode emitted instructions and check attributes against RV64GC. Do not
search only mnemonic text: assembler pseudo-instructions expand. Reject Zbb,
RVV, optional atomics or half-format instructions without an admitted
legalization. Keep instruction bytes, alignment padding and literal pools
separate in size reports. The audit is not an assembler-correctness theorem.

### 5.7 Corpora

Run c-testsuite, cJSON, zlib, Lua and SQLite in that order with `CC=evocc`.
Reuse their pinned sources and upstream suites. Add a RISC-V target parameter
to the existing ladder rather than a new evaluation framework. The host
compiler remains an ARM64 program in the guest; target executables run via
QEMU or an explicit test runner. Configure probes must use that runner or a
declared cross configuration; never pass a foreign executable to the shell.

R07e requires the original test suites to pass, not merely named refusals.
Preserve any failure, reduce it and repair it; no smaller substitute corpus
closes R07e. No RISC-V corpus has run with EvoCC in this design.

### 5.8 Commands

The measured probe form is:

```sh
container run --rm --platform linux/arm64 --network none --cpus 2 --memory 2G \
  -v "$PROBES:/work" evo-riscv64-probe:20261028 sh -c '
  cd /work && riscv64-linux-gnu-gcc-14 -O2 -static -march=rv64gc \
    -mabi=lp64d layout.c -o layout.exe && qemu-riscv64 ./layout.exe'
```

`OUT/riscv-design/commands.log` records each measurement and special option;
FP probes use explicit operations, `-O0 -frounding-math -ffp-contract=off`.
Future backend gates use the existing target directory, serially and offline:

```sh
CARGO_INCREMENTAL=0 \
  tools/gate-changed.sh --full-workspace --offline
python3 tools/generate-target.py --check
python3 tools/check-spec-pins.py
```

These commands do not implement or test a RISC-V backend today. R07b must add
the named expansion/cross-call tests before any capability claim.

## 6. Milestones

Estimates below count hand-written production/tooling lines and separate test
lines. Generated tables and specifications are extra. They are planning
estimates, not measured effort or a calendar promise. Each new source file
must stay below 1,500 lines and each function below 150, after Q01 lands.

### M0: behavior-preserving preparation

R04a supplies the shared ISA/layout/generator and ABI boundaries. Q01 to Q03
must land first. Reuse its AArch64 full-object identity check. Do not duplicate
M0 in the RISC-V backend or claim it complete from this design.

### M1: integer-only functions and calls (R07b)

| Change | Files | Estimated lines |
|---|---|---:|
| Register/ABI/effect/expansion tables | `targets/riscv64/` | generated/data extra |
| Target adapters | `isa/riscv64/{mod,abi,allocation}.rs` | 300 |
| Integer templates, software flags, branches, constants and frames | `isa/riscv64/{emit,conditions,constants,frame}.rs` | 1,500 |
| Scalar/pair classification and target dispatch | `lower/`, `emit.rs`, `native.rs`, `linscan.rs`, `allocverify.rs` | 500 |
| Driver, layout, predefined profiles and proof refusal | `crates/{evocc,evocc-types,evocc-cpp,evocc-parser}/src/` | 500 |
| Toolchain/image/sysroot integration | `.toolchain/`, `contracts/`, `tools/export-sysroot.sh` | 200 |
| Tests | MIR expansion/ABI tests, driver tests and generator tests | 1,600 |

Total: about 3,000 production/tooling and 1,600 test lines. Exit: every b cell
in Section 4.8 passes GNU/LLVM assembly and QEMU execution; scalar/pair ABI
cross-calls pass in both directions; 256 integer corpus seeds agree with the
interpreter; checked UB traps report the measured signal; proof-required
RISC-V exits 6 with no export/output; AArch64 identity and full gate pass.
Records, globals and FP still refuse until their own milestones.

### M2: memory, records, globals and the full LP64D convention (R07c)

| Change | Files | Estimated lines |
|---|---|---:|
| Record descriptor, grammar and layout plumbing | `crates/{evocc-types,evocc-chir,evocc-evoir}/src/`, EvoIR specification | 450 |
| Record/stack/hidden-result classification | `lower/`, `isa/riscv64/abi.rs`, `generic/abi.rs` | 450 |
| Data, PIC, TLS, aligned/packed memory and dynamic frames | `isa/riscv64/{data,memory,frame,tls}.rs` | 950 |
| Atomics, barriers, protector and shared verifier facts | `isa/riscv64/atomics.rs`, `crates/evocc-lower/`, generic adapters | 450 |
| Tests | ABI/layout, data/PIC/TLS, atomics, stack and grammar | 1,600 |

Total: about 2,300 production/tooling and 1,600 test lines. Exit: every c cell
passes; record cross-calls cover both bank-exhaustion orders and hidden
results; existing 128-/256-case corpus populations agree without skips;
PIE/shared data and TLS tests pass; grammar readers and negative allocation
tests pass; AArch64 identity and full gate pass. FP record classification is
specified here but executable FP cross-calls finish in R07d.

### M3: floating point and varargs (R07d)

| Change | Files | Estimated lines |
|---|---|---:|
| Scalar FP, fenv and bit transports | `isa/riscv64/{floating,fenv}.rs` | 700 |
| f128 pair legalization and audited helper calls | `lower/`, target floating helper module | 550 |
| Target numerical/fenv models | `crates/evocc-types/src/floating*`, `crates/evocc-memory/src/fenv.rs`, interpreters | 450 |
| Variadic save area, pointer walk, copy and forwarding | `lower/`, `isa/riscv64/{abi,frame}.rs` | 500 |
| Tests | FP value/status, binary128, FP/mixed ABI and varargs | 1,500 |

Total: about 2,200 production/tooling and 1,500 test lines. Exit: every d cell
passes; strict values/status agree in all four modes; both tininess probes,
NaN boxing and quiet/signaling comparisons pass; f128 helpers and conversions
are audited by operation, not symbol presence; all variadic cross-calls pass;
AArch64 identity and full gate pass. Unsupported helper/builtin shapes stay
explicit refusals and cannot contribute to a PASS numerator.

### M4: corpus (R07e)

| Change | Files | Estimated lines |
|---|---|---:|
| Required application inline-assembly subset | target assembly module and lowering | 350 |
| Existing ladder target/runner support | `tools/ladder/{e0,subject,csmith}.py` and existing subject adapters | 300 |
| Baseline ISA audit | target audit tool | 150 |
| Tests and unchanged-upstream replay | driver/ladder tests and reduction fixtures | 500 |
| Defects exposed by the original corpora | affected existing modules | 500 reserved |

Total: about 800 production/tooling and 500 test lines, plus a 500-line defect
allowance. Exit: c-testsuite, cJSON, zlib, Lua and SQLite pass their pinned
original suites with `CC=evocc` under QEMU; all counted target objects come
from EvoCC and are not rebuilt by a reference compiler; ISA audit is clean;
receipts retain identities, denominators and failures; AArch64 identity and
full gate pass. An allowance is not permission to waive an exit.

### Totals and work split

About 8,300 production/tooling lines, 5,200 test lines and 500 reserved defect
lines, excluding shared R04a, generated tables and specification text. Keep
ownership separate for target descriptions, target emission, ABI/lowering
and driver/test-runner integration. Publish interfaces before dependent work.
Run heavy acceptance serially; concurrency must not create another target
tree or change frozen inputs during a gate.

### Out of scope for R07

Kernel compilation/boot; RV32, big-endian and other ABIs; Darwin RISC-V;
RVV, Zbb, Zicond, optional crypto/atomic/half formats; implicit fast math;
new optimization defaults; unbounded assembly constraints; RISC-V Lean
checkers; and hardware performance claims. Each needs separate authority.

## 7. Risks

| Risk | Required check / stop rule |
|---|---|
| Word sign extension leaks into an internal unsigned value | Direct MIR W tests and ABI round trips at bit 31. |
| Native division's zero result differs from shared MIR | Test SDIV/UDIV by zero even without C guards. |
| Scratch or spill overwrites software flags | Alias/spill/long-offset tests for every adjacent chain. |
| Named pairs are confused with anonymous aligned pairs | Boundary cross-calls and stack-lockout tests. |
| Packed records are forced to integer MEMORY incorrectly | Measured mixed-bank shapes plus nested-record expansion. |
| Pointer va_list walks into a frame record | Test every named-register count and forwarded lists. |
| Binary128 is truncated or hidden helper calls clobber values | Pair transport and live-across-call tests before corpus runs. |
| Arm NaN/tininess assumptions survive | Target correspondence vectors and both boundary probes. |
| Branches work only in small functions | Forced distances, relocations and linker-negative tests. |
| Misaligned/volatile access is silently weakened | Preserve transaction contract or refuse; no wider out-of-object access. |
| QEMU passing is presented as a memory-model/hardware proof | Keep concurrency and hardware claims explicitly separate. |
| Planning estimates miss broad dependencies | Reduce real failures; update estimates without shrinking the exit. |

## 8. The proof-required path

The existing proved path remains `core-v1` on `aarch64-unknown-linux-gnu`.
Recognition of a RISC-V target must not select the AArch64 export/checkers.
After target parsing, refuse RISC-V proof-required requests with exit 6 before
stage export, certificate emission or final output. Add a driver test that
checks all three absences, not only the exit status.

This is a future R07b contract. The current driver does not recognize the
RISC-V target; no claim is made that its present unknown-target error is exit
6. No Lean source, checker, theorem or receipt changes in this design step.
Passing ordinary RISC-V execution tests does not extend the proved subset.

## 9. Observation outside R07

The same R04a boundary serves x86-64 and RISC-V. A pointer `va_list`, ABI
piece placement, independent target FP behavior and software conditions are
separate design facts; none is a reason to weaken typed obligations or the
proof-required refusal. Do not reopen R03 or add an FP proof domain here.

## Appendix A. Probe sources and results

### A.1 Data layout probe

`layout.c` includes the target standard headers, prints `sizeof`/alignment,
signedness, endianness and mantissa sizes, and runs as a static RISC-V ELF.
`layout.txt`, `macros.txt`, `elf-{header,attributes}.txt` and package versions
are retained with it. No AArch64 compiler result is used as a RISC-V result.

### A.2 Reference sequences (GCC 14.2.0)

`abi_shapes.c` is compiled at `-O1 -fno-inline` into `abi_shapes.s` and linked
with the separate `abi_main.c` for runtime output. `abi-frame.s` adds
`-fno-omit-frame-pointer`. `mechanics.c` supplies PIC/local TLS, unaligned
memory, atomics and protected-frame examples. `narrow.c` and `integer.c`
use explicit instruction observations instead of C undefined operations.

### A.3 NaN and ISA probes

`floating.c`, `tiny_boundary.c` and `nanbox.c` separate arithmetic/status,
tininess and register boxing. `branch_{conditional,direct,indirect}.s`
separate conditional relaxation, JAL failure and a successful long pair.
`syntax.sh` retains accepted and rejected spellings. `trap.c` retains its
signal status. `executable-hashes.tsv` keeps product hashes; owned generated
objects/executables are removed after measurement, not committed.
