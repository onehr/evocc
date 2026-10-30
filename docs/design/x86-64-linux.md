# R04 design: x86-64 Linux userspace backend

This note is the design of R04 in `TODO.md` ("x86-64 userspace backend/ABI
milestone"). The owner asked on 2026-10-13 to finish every open item, R04
included. The roadmap names the task in `docs/archive/roadmap.md` (lines
1263-1277 list the x86 backend facts; lines 1637-1638 make x86 userspace a
separate, gated step after ARM64). This note changes no code, specification,
table or task file. An implementation agent works through the milestones of
Section 6 in order; each milestone names its tests from Section 5.

## Base, scope and evidence

- Base: commit `0f598cce`. Every `crates/evocc-mir` line number cites that
  commit; those files had no uncommitted change when they were read.
  `crates/evocc-parser/src/lib.rs` had uncommitted changes, so its line
  numbers can drift a few lines.
- Frozen compiler: `.build/frozen/366fd796/evocc-darwin`
  (`evocc 0.1.0 (366fd796613e 2026-10-13)`). It refuses
  `--target=x86_64-unknown-linux-gnu` with exit 1 today.
- The design phase ran no Cargo, no Lake and no build of the compiler.
- Measured on this host (Section 5.1 has the commands): Docker and Apple
  `container` both run linux/amd64 containers through Rosetta; GCC 14.2.0,
  binutils 2.44 and glibc 2.41 work in a Debian 13 amd64 container; QEMU
  7.2.22 user mode runs x86-64 binaries in an arm64 container; GNU as and LLVM
  accept or reject the assembly spellings that Section 3.13 lists.
- Read, not run: the compiler sources, the specifications, the Lean checker
  sources and the ladder tools. Three parallel read-only surveys covered the
  front end and driver, the test infrastructure, and the proof-required path.
  The author spot-checked their load-bearing claims.

**Claim ceiling.** This is a design with measured environment facts. It is not
an implementation, not an ABI oracle and not a performance plan.

## 1. Decisions

| # | Decision | Section |
|---|---|---|
| D1 | x86-64 is a third EvoMIR target over the **shared EvoMIR instruction set**. There is no second machine IR. The x86-64 emitter implements each admitted EvoMIR form with a fixed AT&T template. | 4.1 |
| D2 | Two-address forms are an emission rule with one reserved scratch register (`%r10`). The allocator gets no tied operands in the first slice. | 4.4 |
| D3 | Fixed-register needs (`%cl` shifts, `%rax`/`%rdx` division and widening multiply, `%rax` for `cmpxchg` loops, `%rcx`/`%rsi`/`%rdi` for `rep movsb`) are **per-form clobbers** in a new x86 table. The generic allocator and verifier read them through the mechanism they already use for `asm` clobbers. | 4.5 |
| D4 | A flags consumer maps its EvoMIR condition by the **family of its producer**: subtraction, addition or logical. Unknown pairs fail closed. | 4.6 |
| D5 | SysV classification lives in lowering, next to the AAPCS64 classifier. The generic layer gets two extensions: ABI register banks become ordered lists, and an argument-placement marker sends a piece or a record to the stack while registers remain. | 4.7 |
| D6 | The C front end computes the SysV eightbyte classes of a record and writes them into the `byval`/`sret` mark, as it writes `hfa=` today. This is an EvoIR grammar change. | 4.12 |
| D7 | `long double` is the x87 80-bit format in 16 bytes. The first slice admits it as **storage only** and refuses every value operation with a new named front-end diagnostic. | 3.4 |
| D8 | PIE by default. Defined symbols use RIP-relative `lea`; EvoIR `extern` and `declare` symbols use `@GOTPCREL`; calls to declarations use `@PLT`. No red-zone use, except one transient MXCSR read. | 3.6, 3.7 |
| D9 | Operand-carrying inline assembly is refused until M4 adds the subset that SQLite needs. TLS, `_Float128`, vectors, AES and every optional selection, allocation and frame variant are refused on x86-64 in R04. | 3.8, 3.11 |
| D10 | A trap is `ud1l CODE(%eax), %eax`. It keeps the diagnostic number in the instruction, as `brk #imm16` does, and raises SIGILL (exit 132). | 3.12 |
| D11 | The proof-required path refuses x86-64 explicitly with exit 6, before any export. The Lean checkers stay AArch64-only. | 8 |
| D12 | Tests run under Apple `container` with a sealed amd64 twin of `evo-nexlab:sealed`. FP-bit-exact and baseline-ISA lanes run under QEMU, because Rosetta is not a faithful x86 oracle for NaN signs and accepts AVX2. | 5 |

## 2. The current target abstraction

### 2.1 What is shared

| Layer | Where | Target dependence today |
|---|---|---|
| C front end, C-HIR | `crates/evocc-parser`, `crates/evocc-chir`, `crates/evocc-types` | Through `TargetDataLayout` (three flags, `crates/evocc-types/src/lib.rs:1752-1801`) and many hard-coded AArch64 calls (Section 2.4) |
| EvoIR and its interpreter | `crates/evocc-evoir` | Target-neutral IR. The record marks carry AAPCS64 HFA/HVA facts (`spec/evoir.md:3554-3556`) |
| Optimization (EqSat) | `crates/evocc-eqopt`, `crates/evocc-egraph` | Neutral; the persistent cache keys on a frozen layout identity (`crates/evocc-eqopt/src/persistent.rs:25-28`) |
| EvoMIR data model, text, validation | `crates/evocc-mir/src/{lib,text,text_parse,validate}.rs` | The forms are AArch64-shaped (Section 4 of `spec/evomir.md`), but each has interpreter semantics |
| EvoMIR interpreter | `crates/evocc-mir/src/interp.rs` | Hard-codes the AArch64 Linux memory layout (`interp.rs:252`); refuses physical frame places (`interp.rs:1997-2001`) |
| SSA destruction | `crates/evocc-mir/src/generic/ssa.rs` | Neutral; needs `SsaInst`/`SsaTerm` (`ssa.rs:11-23`) |
| Linear scan, spills, slot compaction | `crates/evocc-mir/src/generic/allocation.rs` | Neutral over `AllocInst`, `AllocTerm` and `AllocationTarget` (`allocation.rs:70-107`) |
| Allocation verifier | `crates/evocc-mir/src/generic/allocverify.rs` | Neutral, but assumes contiguous argument banks (`allocverify.rs:227-275`) |
| Argument cursor and frame layout | `crates/evocc-mir/src/generic/{abi,frame}.rs` | Neutral, but `RegisterBank` is a contiguous range (`abi.rs:13-38`, `abi.rs:93-153`, `frame.rs:79-180`) |

The boundary test `crates/evocc-mir/tests/target_boundary.rs:43-60` keeps ISA
names, including `x86`, `x86_64` and `amd64`, out of `generic/`.

### 2.2 What is AArch64-specific

| Part | Where |
|---|---|
| Target selection | `evocc_mir::Target { Darwin, Linux }` (`emit.rs:495-610`) conflates OS ABI, object format and ISA. 14 `==`/`!=` comparisons: 11 in `emit.rs`, 2 in `lower.rs` (509, 648), 1 in `bin/evo-ir-asm.rs:64` |
| Register file | Generated `isa/aarch64/regfile.rs` from `targets/aarch64/regfile` |
| Allocation adapter | `isa/aarch64/allocation.rs:41-87` (`AllocationTarget` impl); `linscan.rs:43-58` (`allocate_for`), `linscan.rs:94-104` (`physical`) |
| ABI adapter and tables | `isa/aarch64/abi.rs:16-35` (`VaListLayout`, `Aarch64Abi`), `abi.rs:123-131` (`Target::abi`); generated `abi_tables.rs` |
| Instruction facts | Generated `isa/aarch64/insts.rs` from `targets/aarch64/insts` (the `MachInst`/`AllocInst` impls of `MInst`) |
| Encodings, patterns, costs, selection variants | `isa/aarch64/{encodings,patterns,costs,constants,local,extensions,controls}.rs` |
| ABI classifier in lowering | `lower.rs`: `Passing` 5942-5960, `RecordPlan` 5973-5985, `record_plan` 6016-6207, `call_arguments` 6318-6416, `call_record_result` 6418-6450, `named_argument_extent` 4941-4990 |
| Variadic lowering | `lower_va_start` 5032-5121, `lower_stack_va_arg` 5126, the `va_arg` block procedure in `lower_block` 2123-2472 |
| TLS | `lower_inst` lowers TLS addresses to `MInst::Asm` with AArch64 templates (`lower.rs:3165-3190`) |
| Emission | `emit.rs` (3,453 lines): module 772-1084, function 1325-1464, blocks 1496-1617, prologue 1619-1722, epilogue 1822-1848, frame access 2046-2091, inline assembly 2093-2255, instructions 2326-3255, terminators 3257-3453; FP in `floating.rs:331-442` |
| Native pipeline and host toolchain | `native.rs:138-329` (pipeline), `native.rs:369-470` (`Toolchain`, macOS arm64 `clang` only, `native.rs:386`) |
| Exact-input adapter | `bin/evo-ir-asm.rs:47-50` accepts `darwin-arm64` and `linux-aarch64` |
| Generator | `tools/generate-target.py` hard-codes `target aarch64` in each table envelope (lines 84, 416, 871) and the output paths (1561-1580) |
| Proof-required path and Lean | Section 8 |

### 2.3 What already generalizes

- `MachineFunction<I, T, C>` and the four traits make SSA destruction,
  allocation and verification generic over the instruction type
  (`generic/machine.rs`, `generic/ssa.rs:11-23`, `generic/allocation.rs:70-107`).
- The allocator already supports every constraint x86 needs, **for `asm`
  bodies only**: pins to physical registers (`pin_boundaries` 302-446,
  `pin_call` 562-636), fixed operands with boundary copies (`pin_assembly`
  448-560), ties (`tied_pairs` 1390-1425), early clobbers (`intervals`
  1208-1329) and fixed clobber intervals (`asm_interferences` 1343-1388). The
  verifier checks the same facts (`allocverify.rs:30-180`, `transfer`/`apply`
  424-477). D3 generalizes the clobber intervals from `asm` to every form.
- Frame layout is pure arithmetic over `AbiFacts` (`frame.rs:79-180`). Its
  areas (outgoing, variadic saves per bank, spills, objects, saves) fit the
  SysV frame without change.
- The tiny code model already maps `adrp` to a GOT literal load and `add
  @PAGEOFF` to a register move (`emit.rs:2545` onward, the `Adrp` and
  `AddPageOff` arms). x86-64 uses the same split.

### 2.4 AArch64 assumptions outside the backend

| Fact | Where | x86-64 needs |
|---|---|---|
| 33 production calls of `TargetDataLayout::aarch64_unknown_linux_gnu()` that ignore `--target` | 23 in `crates/evocc-parser/src/lib.rs`; 6 in `crates/evocc-chir/src` (`lib.rs` 3, `core_profile.rs`, `place.rs`, `interp.rs`); 3 in `crates/evocc-evoir/src` (`lib.rs` 2, `lifecycle.rs`); 1 in `crates/evocc-mir/src/interp.rs:252` | The driver's layout, passed down (M0). The call in `core_profile.rs:76-86` is the proof gate and stays |
| `long double` is always `F128` | parser `lib.rs:15779-15787` and the `L`-suffix sites; `float_layout` `evocc-types/src/lib.rs:2393-2400` | A target long-double format |
| `wchar_t` is `UInt` or `UShort` only | `wide_character_rank` `evocc-types/src/lib.rs:1803-1811`; macros at `crates/evocc/src/main.rs:1410-1431` (`__WCHAR_MIN__` is `0U`) | Signed `int` |
| `va_list` is 32/8 or 8/8 | `va_list_layout` `evocc-types/src/lib.rs:1813-1822` | 24/8 array of one `__va_list_tag` |
| Plain `char` default | `crates/evocc/src/main.rs:1185-1186` | Signed |
| FP model | `crates/evocc-types/src/floating.rs:1-4` (AArch64 NaN priority), `floating/environment.rs:127-140` (positive default NaN), `floating/environment.rs:159-190` (Arm tininess before rounding), `crates/evocc-memory/src/fenv.rs:6-10` (`Aarch64Glibc`, `Aarch64Musl`) | x86 SSE rules (Section 3.10) |
| Predefined macros | `crates/evocc-cpp/src/lib.rs:750-789` accepts only the AArch64 triple; one 421-line GCC dump | An x86-64 profile |
| `__has_builtin` | one global list with 140 `__builtin_aarch64_*` names | Per-target lists |
| `__sync_synchronize` | the front end emits the EvoIR `asm` text `dmb ish` (`crates/evocc-lower/src/lib.rs:13002-13033`) | `mfence` |
| Global register variables | `crates/evocc-lower/src/lib.rs:14673-14738` | Refused on x86-64 |
| Driver target | `DriverTarget` `main.rs:72-98`; `parse_target` 1203-1211; `host()` 85-93 picks the OS only, so an x86-64 Linux host becomes AArch64 | A third variant; host by `target_arch` |
| Toolchain | `toolchain.rs:12` (`evo-nexlab:sealed`), `toolchain.rs:364` (`linux/arm64`), multiarch `aarch64-linux-gnu` | The amd64 image, platform and multiarch name |
| Ladder | `tools/ladder/e0.py:50` (`TARGET`), `tools/ladder/subject.py:194` (`linux/arm64`) | A target parameter |
| Trap status | `tools/ladder/csmith.py:406` (`TRAP_EXIT = 133`), `crates/evocc-mir/tests/ub_profiles.rs:138` | 132 on x86-64 |

## 3. The x86-64 Linux userspace slice

### 3.1 Identity

- Triple: `x86_64-unknown-linux-gnu`. Object format: ELF64. Code model: small.
- Baseline ISA: x86-64 v1 (SSE2, CMOV, CX8). No SSE3 and later, no POPCNT,
  LZCNT, BMI, AVX or CX16. Section 5.6 enforces this.
- Assembly: AT&T syntax for GNU as 2.44; the host LLVM assembler accepts the
  same text (Section 3.13).
- Producer note: `.ident "EvoCC VERSION x86_64-unknown-linux-gnu"`.

### 3.2 Data layout

Measured with GCC 14.2.0 in Debian 13 amd64 (Appendix A.1):

| Type | Size | Align | Note |
|---|---|---|---|
| `_Bool`, `char` | 1 | 1 | `char` is **signed** |
| `short` | 2 | 2 | |
| `int` | 4 | 4 | |
| `long`, `long long`, pointers, `size_t`, `ptrdiff_t` | 8 | 8 | LP64 |
| `float`, `double` | 4, 8 | 4, 8 | IEEE; `__FLT_EVAL_METHOD__` 0 |
| `long double` | 16 | 16 | x87 extended, `__LDBL_MANT_DIG__` 64 |
| `__int128`, `_Float128` | 16 | 16 | |
| `wchar_t` | 4 | 4 | **signed** `int` |
| `va_list` | 24 | 8 | `__va_list_tag[1]`, an array type |
| `max_align_t` | 32 | 16 | from the sysroot's `stddef.h` |
| Functions | | 1 | GCC `__alignof__` of a function |

`TargetDataLayout` gets an `x86_64_unknown_linux_gnu()` constructor. Its
`pointer_va_list` flag becomes `VaListKind { Aapcs64Record, Pointer,
SysvArray }`. It gains a wide-character signedness and a long-double format
(`Binary128` today; `X87` for x86-64). `FloatKind` gains `X87` with layout
16/16 for storage. GNU vectors are refused on x86-64 (E4002), so the vector
alignment rule (`lib.rs:1988`) does not change.

### 3.3 Calling convention (SysV AMD64)

The psABI (`https://gitlab.com/x86-psABIs/x86-64-ABI`) is the authority. The
rules below are restricted to what EvoIR passes. What is not listed is refused.

**Registers.**

| Use | Registers, in order |
|---|---|
| Integer and pointer arguments | `%rdi %rsi %rdx %rcx %r8 %r9` |
| SSE arguments | `%xmm0` to `%xmm7` |
| Integer results | `%rax %rdx` |
| SSE results | `%xmm0 %xmm1` |
| Callee-saved | `%rbx %rbp %r12 %r13 %r14 %r15`; MXCSR control bits; x87 control word |
| Call-clobbered | `%rax %rcx %rdx %rsi %rdi %r8-%r11`, all of `%xmm0-%xmm15`, flags |
| Variadic call | `%al` holds an upper bound of the SSE registers that the call uses |

Each bank has its own counter, as in the shared cursor (`abi.rs:93-153`). A
piece goes to the stack when its bank is empty.

**Classification.** A scalar integer or pointer is INTEGER. `float` and
`double` are SSE. `__int128` is two INTEGER eightbytes. A record of more than 16
bytes, or with an unaligned field, is MEMORY. Otherwise each eightbyte of the
record gets the merge of its fields' classes: INTEGER wins over SSE, and a
`long double` or `_Complex long double` member makes the record MEMORY. Arrays
classify element by element; unions overlay their members; bit-fields use their
declared type; GNU empty records are ignored.

**All or nothing.** When the registers that an argument needs are not all free,
the whole argument goes to the stack, and **later arguments still use the
remaining registers**. This differs from AAPCS64, where the bank is exhausted.
It applies to `__int128` (two GPRs) and to records of one or two eightbytes.

**Records.** An INTEGER eightbyte travels in the next GPR, an SSE eightbyte in
the next XMM register. The last eightbyte of a record smaller than 16 bytes
carries only the remaining bytes (for example 4 bytes for a trailing lone
`float`), so no load reads past the object, as `record_chunks` already ensures
for AArch64. A MEMORY record is copied **by value** into the stack argument
area; the callee receives no pointer. This differs from AAPCS64, which passes a
pointer to a copy above 16 bytes.

**Results.** Pieces of a record result use the two result banks with
independent counters: `struct {double; long}` returns in `%xmm0` and `%rax`. A
MEMORY result uses a hidden pointer passed as the **first** integer argument
(`%rdi`); the callee returns that pointer in `%rax`. The frozen compiler keeps
the `sret` parameter at its source position (`@ret_big(%arg0: bits<64>,
%arg1: ptr<generic> sret(24, 8), ...)`), so the x86-64 lowering moves it first.

**Stack.** Stack arguments occupy 8-byte slots in argument order at increasing
addresses from `%rsp` at the call. A 16-byte-aligned argument starts at a
16-byte boundary. `%rsp` is 16-byte aligned at every call. The callee reads
slot `i` at address `%rbp + 16 + 8 * i`, exactly as AArch64 reads
`x29 + 16 + 8 * i`.

**Narrow values.** The psABI leaves the upper bits of a `char` or `short`
argument undefined (a `_Bool` has bits 1 to 7 clear). GCC extends such an
argument to 32 bits at the caller, and Clang relies on that extension. The
EvoMIR rule already extends at cross-boundary calls: zero extension, plus the
front end's `sext` for signed values (Section 6.2 of `spec/evomir.md`). For
results, GCC does not extend in the callee, and Clang marks narrow results
`signext`/`zeroext`. EvoIR has no result signedness, and EvoMIR zero-extends
every narrow result. A Clang caller of an EvoCC `signed char` function can
therefore read 255 for -1. **M1 measures this against GCC-14 and Clang-19 in
both directions before the rule is fixed.** The candidate fix is a front-end
extension to `bits<32>` by signedness before `ret`.

**Variadic calls and callees.** Anonymous arguments classify as named ones; the
front end already promotes `float` to `double`. Before every call the emitter
writes `movl $k, %eax`, where `k` counts the call's arguments in XMM
registers. This is exact for variadic callees and harmless for others. A
variadic callee saves `%rdi`..`%r9` from position `k_gpr` and all of
`%xmm0`..`%xmm7` into a 176-byte register save area. The shared frame layout
already places the general area (6 x 8 = 48 bytes) and the vector area (8 x 16
= 128 bytes) adjacent and 16-byte aligned, which is the SysV `reg_save_area`.
The callee does not test `%al`; it stores all eight vector registers, as the
AArch64 prologue stores `q0`..`q7`.

**`va_list`.**

| Field | Offset | Initial value at `va_start` |
|---|---|---|
| `gp_offset` (unsigned 32) | 0 | `8 * k_gpr` |
| `fp_offset` (unsigned 32) | 4 | `48 + 16 * k_sse` |
| `overflow_arg_area` | 8 | `%rbp + 16 + named_stack_bytes` (the existing `frame.address [incoming]` plus the extent) |
| `reg_save_area` | 16 | the save area base: `frame.address [gr_top]` minus 48 |

**`va_arg`** (psABI 3.5.7). INTEGER scalar: if `gp_offset <= 40`, read at
`reg_save_area + gp_offset` and add 8; else read the overflow area and add 8.
SSE scalar: if `fp_offset <= 160`, read at `reg_save_area + fp_offset` and add
16; else the overflow area. `__int128`: needs `gp_offset <= 32`; else the
overflow area aligned to 16. A record of at most 16 bytes with `n_int` INTEGER
and `n_sse` SSE eightbytes: if both banks have room, gather its eightbytes into
a temporary (always for a mixed record or for two SSE eightbytes, whose slots
are 16 bytes apart); else read it from the overflow area. A MEMORY record:
the overflow area aligned to its alignment, then advance by its size rounded
to 8. Every procedure uses existing EvoMIR forms (`ldr.w`, `cmp #imm`,
`b.cond`, `add`, `str`).

### 3.4 `long double`

Decision: **storage only in the first slice; refuse every value operation.**

- Admitted: declarations, `sizeof`/`_Alignof`, pointers, record and union
  members, zero initialization, and byte copies (`memcpy`, record assignment).
- Refused with a new front-end code (the next free `E29xx` in the diagnostic
  authority table of `spec/evoir.md` Section 6.3, named
  `x87-long-double-value-unsupported`): arithmetic, comparison, conversion, a
  nonzero constant or static initializer, a call argument, a return value and
  `va_arg` of `long double` or `_Complex long double`.
- Why not `F128`: a constant folded as binary128 would store wrong bytes in a
  global, and glibc would read them as x87 data. That is silent wrong code.
- Why not now: x87 values need a new EvoIR scalar format with interpreter
  semantics (`rustc_apfloat` has `X87DoubleExtended`), x87 stack code in
  emission, MEMORY passing and `%st(0)` returns. That is a separate milestone
  (Section 6, "Later").
- Corpus impact, checked on the pinned sources: SQLite 3.53.4 has no `long
  double` and no `LONGDOUBLE_TYPE` in `src/`; in its default configuration
  Lua 5.5.1 uses `long double` only as an alignment member (`luaconf.h:734`,
  `LUAI_MAXALIGN`); zlib 1.3.2 and cJSON 1.7.19 use none. The c-testsuite
  count is measured at M4 start.

### 3.5 Predefined macros and feature queries

- Source: the GCC 14.2.0-19 dump of the sealed amd64 image (`gcc -dM -E -x c
  /dev/null`, 400 macros on Debian 13). Generate the profile file from the image;
  do not hand-write it.
- The AArch64 profile has 421 macros. 41 names exist only there: 32
  `__ARM_*`, `__AARCH64*` and `__arm_*` macros, `__aarch64__`,
  `__CHAR_UNSIGNED__`, `__FLT_EVAL_METHOD_C99__`, five `__FP_FAST_FMA*`
  macros and `__GCC_HAVE_SYNC_COMPARE_AND_SWAP_16`. Shared names can have
  other values (for example `__LDBL_MANT_DIG__`, `__WCHAR_TYPE__`).
- 20 names exist only on x86-64. Keep 11: `__x86_64__`, `__x86_64`,
  `__amd64__`, `__amd64`, `__k8__`, `__k8`, `__code_model_small__`,
  `__SSE_MATH__`, `__SSE2_MATH__`, `__SIZEOF_FLOAT80__` and
  `__SIZEOF_FLOAT128__`. `__PIC__` and `__PIE__` are 2.
- **Omit** the other 9, which promise intrinsics or extensions that the
  compiler does not have: `__SSE__`, `__SSE2__`, `__MMX__`, `__FXSR__`,
  `__MMX_WITH_SSE__`, `__SEG_FS`, `__SEG_GS`, `__ATOMIC_HLE_ACQUIRE` and
  `__ATOMIC_HLE_RELEASE`. Also omit `__GCC_ASM_FLAG_OUTPUTS__`, which both
  dumps define, because flag-output operands are outside the M4 subset. A
  guarded program then takes its portable path. This follows the AArch64
  rule: `__ARM_NEON` is defined because NEON intrinsics exist.
- `__has_builtin` answers per target: no `__builtin_aarch64_*` and no
  `__builtin_ia32_*` on x86-64.

### 3.6 Stack, frame pointer and red zone

- Prologue: `pushq %rbp; movq %rsp, %rbp; subq $SIZE, %rsp`. After the push,
  `%rsp` is 16-byte aligned, and `SIZE` is a multiple of 16, so every call
  sees an aligned stack.
- The frame record is the return address and the saved `%rbp`: 16 bytes, the
  same `frame_record_bytes` as AArch64. Callee-saved registers go to their
  generic save slots with `movq`.
- Over-aligned frames add `andq $-A, %rsp`. Dynamic frames address slots from
  `%rbp`; with both, the base is recomputed as `(%rbp - SIZE) & -A`, as
  `emit_frame_address` does on AArch64 (`emit.rs:2046-2066`).
- Epilogue: restore callee-saved registers, then `leave; ret`.
- The red zone is never used for values. The one exception is the transient
  MXCSR read of Section 3.10, which no call or signal can disturb.
- Functions carry no alignment directive unless EvoIR requests `align=N`. GCC
  emits none at -O0 and -O1 (measured), and the Section 6.14 claim about
  `-fno-align-functions` then stays true.

### 3.7 PIC, PIE and relocations

- Default: PIE, as Debian GCC (`__PIE__ 2`, measured).
- Address of a defined symbol: `leaq sym(%rip), %reg` (`R_X86_64_PC32`).
- Address of an EvoIR `extern` object or `declare` function, or a
  default-visible symbol under PIC: `movq sym@GOTPCREL(%rip), %reg`
  (`R_X86_64_REX_GOTPCRELX`, relaxable by the linker). This keeps EvoMIR
  Section 8.4: a PIE needs no direct relocation against external data. GCC uses
  copy relocations here instead (measured); both are valid.
- Call of a declaration: `call sym@PLT` (`R_X86_64_PLT32`). Call of a local
  definition: `call sym`.
- Data relocations: `.quad sym+addend` (`R_X86_64_64`). `.xword` does not exist
  on x86 (measured: both assemblers reject it).
- `-no-pie`/`-fno-pic`: the same code. RIP-relative addressing works in both.

### 3.8 TLS

Refused in the first slice through the existing profile gate
(`ProfileFeature::Tls`, `native.rs:162-166`) with E4009. The later sequences are
local-exec `%fs:sym@tpoff`, initial-exec `sym@gottpoff(%rip)` and
general-dynamic `__tls_get_addr`. No corpus program needs TLS: glibc reaches
`errno` through `__errno_location`.

### 3.9 Atomics (x86-TSO)

| EvoMIR form | x86-64 | Note |
|---|---|---|
| `atomic.load`, any order | `movzbl`/`movzwl`/`movl`/`movq` | Aligned loads are atomic and acquire |
| `atomic.store` relaxed, release | `mov` | |
| `atomic.store` seq_cst | `movW %v, %r10; xchgW %r10, (%b)` | `xchg` is a full barrier |
| `atomic.rmw.add` / `.sub` | `movW %v, %d; [negW %d;] lock xaddW %d, (%b)` | Old value in `%d` |
| `atomic.rmw.exchange` | `movW %v, %d; xchgW %d, (%b)` | |
| `atomic.rmw.and/or/xor/nand` | `movW (%b), %rax; 1: movW %rax, %r10; opW %v, %r10; [notW %r10;] lock cmpxchgW %r10, (%b); jne 1b; movW %rax, %d` | Clobbers `%rax` |
| `atomic.rmw.cmpxchg` | `movW %e, %rax; lock cmpxchgW %v, (%b); movW %rax, %d` | Clobbers `%rax` |
| `__sync_synchronize` | EvoIR `asm` text `mfence` | Front-end change |

A `lock` prefix is a full barrier, so every order is met. 16-byte atomics need
CX16, which is not baseline; EvoIR atomics are 1, 2, 4 or 8 bytes. The table
writes `%rax` for the accumulator; the expansion uses its view of width `W`
(`%al`, `%ax`, `%eax`, `%rax`).

### 3.10 Floating point and the environment

- Scalar `f32`/`f64` use SSE2. `f128` and every vector form are refused.
- **FMA**: baseline x86-64 has none. Lowering legalizes an explicit FMA to a call
  of `fma`/`fmaf` through the existing FP-helper calls (`bl.fixed_fp`,
  `bl.strict_fp`). Emission never makes a call.
- **NaN and tininess.** SSE produces the negative default NaN
  (`0xfff8000000000000`, printed `-nan`; measured under QEMU) and detects
  tininess after rounding. AArch64 produces a positive default NaN and detects
  tininess before rounding; the EvoIR FP model adds an Arm-specific underflow
  adjustment on top of APFloat's after-rounding rule
  (`floating/environment.rs:159-190`). M3 parameterizes the model by target,
  and x86-64 skips that adjustment. The emitter keeps the left operand as the
  first source, so the x86 NaN priority (first source operand) is
  deterministic.
- **Environment check.** The fixed check requires MXCSR & `0xFFC0` ==
  `0x1F80`: every exception masked, round to nearest, no FZ, no DAZ. The strict
  check masks with `0x9FC0` (it admits any rounding mode). On failure the code
  executes `ud1l 3064(%eax), %eax`. The read is `stmxcsr -4(%rsp); movl
  -4(%rsp), %r10d`.
- The fenv runtime gets an `X86_64Glibc` variant: x86 glibc spells rounding
  modes and exception bits differently from AArch64 (`FE_DOWNWARD` is `0x400`,
  not `0x800000`).

### 3.11 Inline assembly

- M1 to M3: refuse operand-carrying x86 `asm` with E4009. Admit an operand-free
  `asm` whose clobbers are only `memory` and `cc`; its template is copied
  verbatim. Module-level `asm` stays verbatim. This admits `mfence`.
- M4 adds the subset that the corpus needs: SQLite's test build defines
  `SQLITE_ENABLE_STMT_SCANSTATUS` (`main.mk:968`), so `vdbe.c` includes
  `hwtime.h`, whose x86-64 branch is `__asm__ __volatile__ ("rdtsc" : "=a"
  (lo), "=d" (hi))` (`hwtime.h:39-46`). The subset is constraints `r`, `q`,
  `a`, `b`, `c`, `d`, `S`, `D`, `i`, `n`, `m`, digit ties, `&`, `+`;
  modifiers `b`, `h`, `w`, `k`, `q`, `c`; clobbers `memory`, `cc` and named
  registers. Fixed letters reuse the existing fixed-binding path
  (`pin_assembly`, `allocation.rs:448-560`).
- glibc 2.41 x86-64 headers contain no inline-assembly statements that common
  programs include; their `__asm` uses are symbol labels (`__REDIRECT`), which
  the front end already handles (measured with `grep` in the image).

### 3.12 Traps

`brk #N` becomes `ud1l N(%eax), %eax`. GNU as encodes `ud1l 3024(%eax), %eax`
as `67 0f b9 80 d0 0b 00 00`; the displacement holds the diagnostic number, as
`brk` holds it (measured). LLVM uses the same encoding for its x86 sanitizer
traps. The signal is SIGILL, so the exit status is 132, not 133. The trap
status becomes a target fact that harnesses read.

### 3.13 Assembly syntax facts (measured)

| Spelling | GNU as 2.44 (x86-64) | Apple clang 21 (`-target x86_64-unknown-linux-gnu`) |
|---|---|---|
| `// comment` at the start of a line | accepted | accepted |
| `insn // comment` after an instruction | **rejected** ("junk after register") | accepted |
| `# comment` | accepted | accepted |
| `.type f, %function`, `%progbits`, `%init_array` | accepted | accepted |
| `.xword` | **rejected** | **rejected** |
| `.quad`, `.p2align`, `.ident`, `.L` labels | accepted | accepted |

Rule: keep the header lines `// evocc-native-...` at column 0, so receipts and
tools read them unchanged. Never emit a trailing `//` comment; use `#`.

## 4. Machine IR design

### 4.1 Options compared

| Option | Reuses | Costs | Verdict |
|---|---|---|---|
| **A. Shared EvoMIR; x86-64 is a target whose emitter expands each form** | Lowering (6,450 lines, with UB predicates, 128-bit pairs, atomics, strict FP, `mem.copy`, frame slots), the EvoMIR interpreter lane, validation, text, SSA destruction, allocator, verifier, frame layout, cursor | Long expansions; extra moves; 12 allocatable GPRs | **Chosen** |
| B. Separate x86 instruction set | The generic algorithms only | A new lowering (5,000 lines or more), a new interpreter, text form and validator; loses the shared MIR interpreter lane | Rejected |
| C. Lower x86 fixed-register operations to `MInst::Asm` bodies | Allocator constraint support as is | `asm` leaves the interpreter lane (E3021) and invalidates FP admission | Rejected |

Under option A, the EvoMIR forms keep the semantics that `interp.rs` gives
them. The names are AArch64 mnemonics, but for x86-64 they name operations, not
instructions. `spec/target.md` Section 6 anticipates a new ISA as a new
`targets/<isa>/` directory; under A the shared `insts` table stays the operand
facts of the shared instruction set.

### 4.2 Target modeling

- Add `Isa { Aarch64, X86_64 }` and a third `Target` variant `LinuxX86_64`
  (mnemonic `linux-x86_64`). Add `Target::isa()`, `is_elf()` and
  `is_darwin()`.
- M0 replaces the 14 non-exhaustive comparisons with these predicates **before**
  the variant exists. Otherwise `target == Target::Linux` would silently treat
  x86-64 as Darwin (for example `.private_extern` in place of `.hidden`, and the
  binary128 refusal at `lower.rs:509`).
- The driver has the same hazard: `cc.rs` compares with
  `DriverTarget::Aarch64UnknownLinuxGnu` at lines 752, 760, 989 and 1178 to mean
  "a Linux target". These become an `is_linux()` predicate in M0.
- `Target::abi()` returns `&Aarch64Abi`. Split it into `abi_facts()` (generic)
  and per-ISA accessors, and pass the AArch64 classifier its `&Aarch64Abi` from
  a `match` on the ISA. Production code gets no `expect` on the ISA.
- `DriverTarget` gains `X86_64UnknownLinuxGnu`; `host()` decides by
  `target_arch`, not only by `target_os`.

### 4.3 Register file

Numbers are the hardware encodings: GPR bank `0 16`, SIMD bank `64 16`.

| Set | Registers |
|---|---|
| GPR views | `gpr8`: `al cl dl bl spl bpl sil dil r8b`..`r15b`; `gpr16`: `ax`..`r15w`; `gpr32`: `eax`..`r15d`; `gpr64`: `rax`..`r15` (explicit name lists, not prefix plus number) |
| SIMD views | `simd32`, `simd64`, `simd128`: `xmm0`..`xmm15` |
| Flags | `virtual flags eflags 0` |
| Allocatable GPRs, in order | `rsi rdi r8 r9 rax rdx rcx rbx r12 r13 r14 r15` (12) |
| GPRs for a value live across a call | `rbx r12 r13 r14 r15` |
| Allocatable SIMD | `xmm0`..`xmm13` |
| SIMD live across a call | none: such a value is spilled, as on AArch64 |
| Reserved, never assigned | `rsp`, `rbp` (frame pointer), `r10` (emission scratch), `r11` (SSA cycle scratch), `xmm14` (SIMD emission scratch), `xmm15` (SIMD cycle scratch) |
| Roles | `SCRATCH` r11, `SCRATCH_UNIT` r10, `SIMD_SCRATCH` xmm15, `SIMD_SCRATCH_UNIT` xmm14, `FRAME_POINTER` rbp; no link register, platform register or zero register |

The cycle scratch and the emission scratch are separate registers. A parallel
copy holds a live value in `r11`, and no expansion uses `r11`. AArch64 shares
`x16` between both roles. The order of the allocatable set is policy, not
correctness; `rax`, `rdx` and `rcx` come late because Section 4.5 clobbers
them most often.

### 4.4 Two-address forms

For `op.W rd, rn, rm` (AT&T: `opW src, dst`):

| Case | Commutative (`add`, `mul`, `and`, `orr`, `eor`) | Non-commutative (`sub`, flag-setting forms) |
|---|---|---|
| `rd == rn` | `opW m, d` | `opW m, d` |
| `rd == rm`, `rd != rn` | `opW n, d` | `movW n, %r10; opW m, %r10; movW %r10, d` |
| otherwise | `movW n, d; opW m, d` | `movW n, d; opW m, d` |

Every multi-instruction expansion reads all sources before it writes `rd`, or
it computes into `%r10` first. A W-form writes a 32-bit register, which zeroes
the upper half, as an AArch64 W form does.

### 4.5 Fixed registers: per-form clobbers

A new table `targets/x86_64/expansions` lists, for each EvoMIR form, the
physical registers that its expansion writes besides its destinations. The
scratch registers `r10` and `xmm14` are reserved and are not listed.

| EvoMIR form | Clobbers |
|---|---|
| `lsl`, `lsr`, `asr` by register | `rcx` |
| `sdiv`, `udiv` | `rax`, `rdx` |
| `smulh`, `umulh` | `rax`, `rdx` |
| `atomic.rmw` `and`/`or`/`xor`/`nand`/`cmpxchg` | `rax` |
| `mem.copy` above 64 bytes, not volatile (`rep movsb`) | `rcx`, `rsi`, `rdi` |
| `rbit.x`, `rev16.x` (64-bit mask constants) | `rax` |
| `fp.from_unsigned` from 64 bits (M3) | `rax` |
| `arg.outgoing.copy` (Section 4.7) above 64 bytes | `rcx`, `rsi`, `rdi` |

Calls need no entry. The `%al` write happens before `call`, and `rax` is not an
argument register; `blr` first moves its target into `%r10` (Section 4.7).

**Generic change G3.** `AllocationTarget` gains `fn instruction_clobbers(&self,
inst: &I) -> &'static [u8]`. AArch64 returns an empty slice, so its output
stays byte-identical.

- The allocator adds a fixed interval `[read, read + 1]` for each listed
  register, beside the `asm` clobbers in `asm_interferences`
  (`allocation.rs:1343-1388`). An input is read at `read` and the result is
  written at `read + 1`, so no operand of the form and no value live across it
  can get a listed register.
- The verifier removes the listed registers from its state after the form, as it
  does for an `asm` clobber (`apply`, `allocverify.rs:453-477`). It refuses
  (E4020) when an operand of the form is in a listed register.
- This makes x86 scratch use verified, per form. AArch64 has the known gap that
  emitter scratch use rests on `RESERVED` alone (`spec/target.md`, Appendix A).

Pinned values (ABI boundaries, `asm` bindings) can sit in `rax`, `rcx`, `rdx`,
`rsi` or `rdi`. Pins only span boundary copies and the anchoring call, return
or `asm` (`pin_boundaries`, `pin_call`, `pin_assembly`). With the ordering rule
of Section 4.7, no clobbering form lies inside a pinned range. The verifier
check catches any violation.

### 4.6 Flags and conditions

EvoMIR keeps a flags value between two adjacent instructions (Section 2.4 of
`spec/evomir.md`); the validator and the verifier check this. The emitter
therefore knows the producer when it emits the consumer. Every x86 expansion
can write EFLAGS. The producer's last instruction sets the consumed flags, and
the consumer reads them before it writes any flag. A `mov`, `lea`, `cmov` or
`set` between them does not change EFLAGS.

AArch64 `C` after a subtraction means "no borrow"; x86 `CF` after a subtraction
means "borrow". After an addition both mean "carry". Hence:

| EvoMIR condition | Family S: `cmp`, `cmp #imm`, `subs`, `sbcs` | Family A: `adds`, `adcs` | Family L: `tst` |
|---|---|---|---|
| `eq`, `ne` | `e`, `ne` | `e`, `ne` | `e`, `ne` |
| `lt`, `ge`, `gt`, `le` | `l`, `ge`, `g`, `le` | `l`, `ge`, `g`, `le` | `l`, `ge`, `g`, `le` |
| `lo`/`cc` (C clear) | `b` | `ae` | `ae` (always true) |
| `hs`/`cs` (C set) | `ae` | `b` | `b` (never true) |
| `hi` | `a` | refuse | refuse |
| `ls` | `be` | refuse | refuse |
| `vs`, `vc` | `o`, `no` | `o`, `no` | `o`, `no` |

- Evidence that the families matter: `limb_add` (`lower.rs:5575-5592`) emits
  `adds` then `cset hs` for a carry. Mapping `hs` to `ae` would be wrong code.
- Carry consumers: `adc` after family A and `sbc` after family S need no
  adjustment (`sbb` subtracts the borrow, as `sbc` subtracts `1 - C`). The
  cross pairs need `cmc` first. The emitter refuses any pair that no test
  covers.
- `cset` is `setCC d8; movzbl d8, d32`. `csel` is `cmovCC` with the
  two-address rule: inverse condition when `rd == rn`. A 32-bit `cmov` zeroes
  the upper half even when the condition is false, which matches a W `csel`.

### 4.7 Calls and argument placement

**G1: ordered register lists.** `AbiFacts.argument_banks` and `result_banks`
become ordered lists (`RegisterList(&'static [u8])` with `count`, `nth` and
`position`). No x86 numbering makes both `rdi rsi rdx rcx r8 r9` and `rax rdx`
contiguous, because `rdx` is third in one list and second in the other.
`ArgumentCursor::place` (`abi.rs:112-128`) uses `nth`; the verifier's entry
check (`allocverify.rs:254-262`) uses `position`. AArch64 generates the same
registers as lists. `indirect_result` and `result_overflow` become `Option`.

**G2: argument placement.** SysV sends a piece or a whole record to the stack
while registers remain (Section 3.3). The cursor cannot express this. Add:

- A marker form, `arg.placement [...]`, placed directly before a call or first
  in the entry block, as `indirect.result` is today (`lib.rs:1047-1062`). Each
  entry names an argument position and a placement:
  - `stack(A)`: this piece goes to the next stack offset aligned to `A` bytes
    and takes no register. Used for `__int128` that does not fit.
  - `memory(S, A)`: this pointer argument stands for `S` bytes copied by value
    into the stack argument area at an `A`-aligned offset. Used for MEMORY
    records and for records that do not fit.
- Two allocator-only pseudo-forms, beside `arg.incoming` and `arg.outgoing`:
  `arg.outgoing.copy [slot #S], [ptr], #size` (the caller copies the record)
  and `rd = arg.incoming.address [slot #S]` (the callee materializes the
  pointer as the address `%rbp + 16 + 8 * S`).
- `ArgumentCursor::stack(words, align_words)` places a stack piece without
  touching the bank counters.
- `pin_call` and `pin_boundaries` read the marker. The verifier treats the
  placed parameters as defined by the pseudo-forms, not as live-in registers.
- `pin_call` emits every `arg.outgoing.copy` and stack store **before** the
  first copy into a pinned argument register. A large copy uses `rep movsb`,
  which clobbers `%rdi`, `%rsi` and `%rcx`, the first and fourth argument
  registers. The existing "saturated" path of `pin_call`
  (`allocation.rs:607-621`) already orders stack stores first.

Before allocation, a MEMORY record is an ordinary pointer argument, exactly as
an AAPCS64 record above 16 bytes is. The EvoMIR interpreter therefore keeps its
semantics ("the callee receives a pointer to a copy"), and the marker changes
only the physical placement.

**`sret`.** The x86 classifier puts the destination pointer first, as an
ordinary pointer argument, and makes the callee return it. No
`indirect.result` marker appears on x86-64.

**Variadic `%al`.** The emitter counts the call arguments assigned to
`xmm0`..`xmm7` and writes `movl $k, %eax` before `call`. An indirect call first
moves its target into `%r10`, so a target that the allocator placed in `rax`
survives the `%al` write. `%r10` is caller-saved and reserved, so the move needs
no clobber entry.

### 4.8 Expansion table

`W` is the width suffix (`l` or `q`); `d`, `n`, `m`, `a`, `t` are the
allocated registers of `rd`, `rn`, `rm`, `ra` and `rt` in that width; `T` is
`%r10` in the same width. Brackets mark a step that is skipped when its
registers are equal.

| EvoMIR form | x86-64 expansion |
|---|---|
| `mov rd, rn` | GPR: `[movW n, d]`; GPR across classes: `movl n32, d32`; SIMD: `movaps n, d` |
| `mov rd, #imm` | `movl $imm, d32` below 2^32; `movq $imm, d` for a sign-extended imm32; else `movabsq $imm, d`; SIMD zero: `xorps d, d` |
| `adrp rd, @g` / `add rd, rn, @g@PAGEOFF` | `leaq g(%rip), d` or `movq g@GOTPCREL(%rip), d` / `[movq n, d]` |
| `adr rd, ^bN` | `leaq .L<f>.<N>(%rip), d` |
| loads | `movzbl`, `movzwl`, `movl`, `movq`, `movsbl`/`movsbq`, `movswl`/`movswq`, `movslq`, `movss`, `movsd`, `movups` from `off(n)`; a displacement outside imm32 goes through `T` |
| stores | `movb`, `movw`, `movl`, `movq`, `movss`, `movsd`, `movups` to `off(n)` |
| `ldp` / `stp` | two `movq` at `off` and `off + 8`; a load that overwrites the base goes last |
| `add`/`sub`/`and`/`orr`/`eor`/`mul` register | Section 4.4 (`imulW` for `mul`) |
| immediates | `[movW n, d]; opW $imm, d`; a logical mask outside imm32: `movabsq $mask, T; [movq n, d]; andq T, d` |
| `sdiv`/`udiv` | `movW n, %eax/%rax; cltd`/`cqto` or `xorl %edx, %edx; idivW`/`divW m; movW %eax/%rax, d` |
| shifts by register / immediate | `movl m32, %ecx; [movW n, d]; shlW`/`shrW`/`sarW %cl, d` / `[movW n, d]; shlW $imm, d` |
| `adds`/`subs` | Section 4.4; the flags come from the last ALU instruction |
| `adc`/`sbc` | `movW n, T; adcW`/`sbbW m, T; movW T, d` (`cmc` first for a cross-family carry) |
| `adcs`/`sbcs` (flags only) | `movW n, T; adcW`/`sbbW m, T` |
| `smull` | `movslq m32, T; movslq n32, d; imulq T, d` |
| `smulh`/`umulh` | `movq n, %rax; imulq`/`mulq m; movq %rdx, d` |
| `madd` / `msub` | `movW n, T; imulW m, T; [negW T;] addW a, T; movW T, d` |
| `sxtb`/`sxth`/`sxtw`/`uxtb`/`uxth` | `movsbl`/`movsbq`, `movswl`/`movswq`, `movslq`, `movzbl`, `movzwl`; the destination class picks `l` or `q` |
| `sbfx`/`ubfx` | `[movW n, d]; shlW $(N - lsb - width), d; sarW`/`shrW $(N - width), d` |
| `clz` | `bsrW n, d; movl $(2N - 1), T32; cmovzW T, d; xorW $(N - 1), d` (gives `N` for zero) |
| `rbit` | `[movW n, d]; bswapW d`, then three mask-and-shift swaps (4, 2, 1 bits); 64-bit masks through `%rax` |
| `rev` / `rev16` | `[movW n, d]; bswapW d` / W: `bswapl d; roll $16, d`; X: masked byte swap through `T` and `%rax` |
| `cmp` / `cmp #imm` / `tst #mask` | `cmpW m, n` / `cmpW $imm, n` / `testW $mask, n` (or `movabsq` into `T` and `testq T, n`) |
| `cset` / `csel` | Section 4.6 |
| `bl` / `blr` | `movl $k, %eax; call sym` or `call sym@PLT` / `movq n, %r10; movl $k, %eax; call *%r10` |
| `spill.store` / `spill.reload` | `movq`, `movsd`, `movss` or `movups` to or from the slot address |
| `arg.incoming` / `arg.outgoing` | `movq D(%rbp), d` with `D = 16 + 8i` / `movq t, D(%rsp)` with `D = 8i` |
| `arg.outgoing.copy` / `arg.incoming.address` (M2) | as `mem.copy` into `D(%rsp)` / `leaq D(%rbp), d` with `D = 16 + 8i` |
| `mem.copy` | up to 64 bytes: unrolled moves of 8, 4, 2 or 1 bytes through `T`; above: `movq s, %rsi; movq d, %rdi; movq $size, %rcx; rep movsb`; volatile: one load and one store through `T` |
| `frame.slot` | `leaq off(%rsp), d`, or the dynamic-frame base of Section 3.6 |
| `stack.alloc` / `stack.release` | the Section 6.11 procedure with `%rsp` arithmetic and `T` |
| `frame.address` | `incoming`: `leaq 16(%rbp), d`; `gr_top`: `leaq SAVE+48(%rsp), d`; `saved_lr`: `movq 8(%rbp), d`; `fp`: `movq %rbp, d`; `fp_N`: N loads of `(r)` |
| atomics | Section 3.9 |
| `fp.*` (M3) | `addsd`/`subsd`/`mulsd`/`divsd` (`ss` for `f32`) with the left operand as the first source; compares with `ucomisd` (quiet) or `comisd` (signaling) and `set`, adding `setnp`/`setp` for `oeq`/`une`; `olt`/`ole` swap the operands and use `a`/`ae`; conversions `cvtsi2sd`, `cvttsd2si`, `cvtss2sd`, `cvtsd2ss`, and unsigned 64-bit sequences; negation flips the sign bit through `T`; constants through `movabsq` and `movq` |
| `fp.env.check[.strict]` | Section 3.10 |
| `vec.*`, `crypto.*` | refused (E4002) |
| `asm` | Section 3.11 |
| `indirect.result` | never produced on x86-64; emission refuses it |
| terminators | `b`: `jmp` or fall through; `b.cond`: `jCC` plus `jmp`; `cbz`/`cbnz`: `testW n, n; je`/`jne`; `tbz`/`tbnz`: `btW $bit, n; jnc`/`jc`; `br`: `jmp *n`; `ret`: epilogue; `brk #N`: `ud1l N(%eax), %eax` |

x86 needs no branch relaxation: `jcc rel32` reaches every target, and GNU as
picks the short form when it fits.

**Division by zero.** EvoMIR defines `sdiv` by zero as 0 (Section 4.3), and
`idiv` raises SIGFPE. The difference is admissible: `divisor_nonzero` and
`signed_div_rem_no_overflow` are asserted before the division (Section 1.1), and
under the checked profile the trap fires first. A test must show that a
checked build traps with `ud1` before any `idiv` sees a zero divisor.

### 4.9 What the allocation verifier needs

1. G1: the entry check reads list positions, not `first + n`.
2. G2: placed parameters are defined by `arg.incoming[.address]`, not live-in.
3. G3: listed clobbers kill state, and an operand in a listed register is
   refused.
4. Optional hardening: refuse an assignment to a reserved register except a pin
   to the cycle scratch.

Everything else is unchanged: flags adjacency, bank checks, call clobbers,
`asm` constraints and `exclusive_result` (which keeps an atomic result apart
from its inputs, as `lock xadd` and `xchg` require).

### 4.10 Target tables and the generator

| Table | Contents |
|---|---|
| `targets/x86_64/regfile` | Banks, named views, sets and roles of Section 4.3 |
| `targets/x86_64/abi-linux` | Ordered argument and result lists, `indirect-result first-argument`, stack facts, callee-saved and clobber sets, `frame-record-bytes 16`, save areas (`gpr 8`, `simd 16`), the SysV `va-list` offsets 0/4/8/16 and size 24, `variadic-vector-count al`, `record-register-bytes 16`, trap status 132 |
| `targets/x86_64/expansions` | The per-form clobbers of Section 4.5 |
| `targets/x86_64/encodings` | imm32 and disp32 ranges, `nop-bytes 1` (for patchable entries) |

The shared operand facts stay in `targets/aarch64/insts`; a later move to a
neutral path is a separate, behavior-preserving change. Generator changes: a
`target` parameter in every envelope; explicit name lists for views; optional
roles; list-valued ABI facts; output under `crates/evocc-mir/src/isa/x86_64/`;
the `--check` mode covers both targets. x86 has no `patterns` or `costs`
table in the first slice.

### 4.11 Lowering changes

- `Lowerer` holds `LoweringAbi::Aarch64(&Aarch64Abi)` or
  `LoweringAbi::X86_64(&X86_64Abi)`. The call, entry, record and `va_*`
  paths dispatch on it. Everything else stays shared.
- New module `crates/evocc-mir/src/lower/sysv.rs`: the placement plan of
  Section 3.3. It produces the flat argument list, the `arg.placement`
  marker, the stack extent of named arguments, and the result pieces. It
  never applies AAPCS64 even-register rounding.
- SysV `va_start`/`va_arg` lowering (Section 3.3), built from existing forms.
- Legalization: explicit FMA becomes an `fma`/`fmaf` helper call.
- Refusals on x86-64: `f128`, vectors, AES, TLS, operand-carrying `asm`
  (until M4), global register variables, `asm goto`, more than two integer or
  two SSE results (E4013), and a record of at most 16 bytes without a `sysv`
  mark.

### 4.12 Front end and driver changes

- **EvoIR record mark.** `RecordShape` (`crates/evocc-types/src/lib.rs:1353-1415`)
  gains `sysv: Option<SysvClasses>`; the text gains `, sysv=integer.sse`. The
  front end computes it beside the HFA (`crates/evocc-chir/src/lib.rs:127-241`),
  only for x86-64, and writes no `hfa=`/`hva=` mark there. This changes the
  grammar of `spec/evoir.md` Section 9.3 (`record_mark`, lines 3554-3556): bump
  the revision, update the pins, and run the grammar readers.
- `va_list` as `__va_list_tag[1]`: array decay, parameter adjustment, `sizeof`
  24, `va_copy` as a 24-byte copy. `is_aggregate_type`
  (`crates/evocc-lower/src/lib.rs:3771-3778`) and `record_signature_marks`
  (3736-3769) must not mark a decayed `va_list` parameter `byval`.
- `long double` as `X87` storage, with the refusal of Section 3.4.
- `__sync_synchronize` emits `mfence` on x86-64.
- The driver refuses AArch64-only options on x86-64: `--isel-opt`,
  `--regalloc-opt`, `--frame-opt`, `-mgeneral-regs-only`,
  `-mbranch-protection`, `-mstrict-align`, the system-register stack guard,
  `-ffixed-x18` and `-mcmodel=tiny`. It admits `-mcmodel=small`.
- The stack protector reads the glibc canary at `%fs:40` (a new `StackGuard`
  variant); glibc x86-64 exports no `__stack_chk_guard`.
- The DWARF type emitter (`crates/evocc/src/dwarf.rs`) emits no register
  locations; it needs only the `.quad` data directive.

## 5. Testing

### 5.1 Environment measured on this host (2026-10-13)

| Check | Result |
|---|---|
| Docker Desktop 29.6.2 engine | linux/arm64 VM, kernel 6.12.76-linuxkit; binfmt handlers for `x86_64` (QEMU) and Rosetta |
| `docker run --rm --platform linux/amd64 evocc-r04/debian-amd64:trixie-slim uname -m` | `x86_64`, 0.46 s |
| Apple `container` 1.2.2: `container run --rm --platform linux/amd64 --network none docker.io/library/debian@sha256:918311b7b6c4c6f68b232ba516584925f6c78ad82b6fd534b98979df6438e483 uname -m` | `x86_64`, 4.8 s; `--rosetta` is optional |
| Active emulator | Rosetta (`/run/rosetta/rosetta` mapped; CPU `VirtualApple`; glibc reports x86-64-v3) |
| Toolchain in Debian 13 amd64 | `gcc (Debian 14.2.0-19) 14.2.0`, GNU as 2.44, glibc 2.41; installing and probing took 22 s |
| QEMU user mode | `qemu-x86_64` 7.2.22 (Debian 12 `qemu-user 1:7.2+dfsg-7+deb12u18+b3`) in an arm64 container |
| Baseline ISA enforcement | `qemu-x86_64 -cpu qemu64` stops an AVX2 instruction with SIGILL (exit 132); `-cpu max` and Rosetta execute it |
| NaN fidelity | runtime `0.0/0.0`: Rosetta `0x7ff8000000000000` (`nan`); QEMU `0xfff8000000000000` (`-nan`, the SSE default NaN). Rosetta is **not** an oracle for NaN signs |

Notes. Docker's daemon could not pull from Docker Hub: its proxy
`http.docker.internal:3128` returned EOF. The image was fetched through the
registry API from the host, every blob digest was checked, and it was loaded
with `docker load` as `evocc-r04/debian-amd64:trixie-slim` (amd64 manifest
`sha256:918311b7…`, config `sha256:567e3c17…`). Apple `container` pulled the
same manifest by digest without trouble. That runtime is the repository's
runtime (`contracts/build-environment.json`).

### 5.2 Images

- **`evo-nexlab-amd64:sealed`.** Build `.toolchain/Dockerfile.nexlab` for
  linux/amd64. Change only `FROM` to the amd64 manifest of the same pinned index
  `sha256:3a39a0592364683e6bab97937b72cad5a8fa6dcbbee90edb3bb48c7f8e94f258`,
  which is
  `docker.io/library/debian@sha256:38a76d01668772e381ad2826d876627c89e7133e2f8a0f5d567306798b0f2a16`
  (resolved from the registry and verified). Keep the apt snapshot
  `20260810T022135Z` and the package list, so GCC 14.2.0-19 and Clang 19.1.7
  match the arm64 lane. Build with `container build --platform linux/amd64`.
  Record a second build-image entry, its package manifest and its image ID in
  `contracts/build-environment.json`.
- **`evo-qemu-x86:sealed`** (arm64): Debian 13 from the same snapshot plus
  `qemu-user`, for the FP-bit-exact and baseline-ISA lanes.
- **Sysroot.** Extend `tools/export-sysroot.sh` with a target argument and add
  `contracts/sysroot-x86_64-unknown-linux-gnu.json`.

### 5.3 Lanes and oracles

| Lane | Oracle | Runs where |
|---|---|---|
| Expansion units | EvoMIR interpreter on the same function | amd64 image (Rosetta) |
| ABI cross-calls | GCC-14 and Clang-19 objects, both directions, at -O0 and -O2 | amd64 image |
| Randomized `native-main` | EvoIR interpreter (exit status mod 256) | amd64 image |
| day7 and day5 C corpora | `gcc-14 -std=c11 -O0` build of the same source | amd64 image |
| c-testsuite, ladder | upstream expected output and test suites | amd64 image |
| FP bit-exact | the target FP model; GCC under the same emulator | `evo-qemu-x86` |
| Baseline ISA | static mnemonic allowlist; QEMU `-cpu qemu64` | amd64 image; `evo-qemu-x86` |
| Determinism | two compilations, byte-identical (`crates/evocc/tests/determinism.rs`) | host |

A lane that finds no runtime or image prints `skipped=` and passes, as the
Linux lanes do (`crates/evocc-mir/tests/section8_linux_profile.rs:46-65`).
Receipts record each skip.

### 5.4 Unit tests per expansion

`crates/evocc-mir/tests/x86_64_expansions.rs`:

1. For each admitted form, width and operand-aliasing class (`rd` distinct,
   `rd == rn`, `rd == rm`, `rn == rm`), build the function from EvoMIR text and
   an allocated form with a **chosen** assignment. The verifier must accept the
   assignment before emission.
2. Emit, assemble with GNU as 2.44 in the image and with the host LLVM
   assembler, and check the `objdump -d` mnemonic sequence (obligation B5).
3. Link with a C driver built by `gcc-14` and run it on boundary inputs (0, 1,
   -1, signed and unsigned extremes, every shift count) plus seeded random
   inputs. Compare each result with the EvoMIR interpreter on the
   pre-allocation function.
4. Each (producer family, condition) pair, including the refused ones.
5. Verifier refusals: a forged assignment that keeps a live value in `rax`
   across `sdiv`, a shift amount in `rcx`, an `atomic.rmw` result in `rax`.

### 5.5 ABI cross-call tests

`crates/evocc-mir/tests/x86_64_abi_calls.rs` with a shape generator in
`tools/x86_64/abi_shapes.py`. Model: `section6_records.rs:855-883` (records in
both directions with `gcc-14 -O2`) and `section9_calling_convention_probe.rs`
(shapes, both directions, a `checks=N` line).

- Classes: every integer type (`_Bool`, signed and unsigned `char`, `short`,
  `int`, `long`, `long long`), pointers, function pointers, `__int128`,
  `float`, `double`; records of 1 to 16 bytes that are INTEGER, SSE, SSE+INTEGER
  or INTEGER+SSE, two floats in one eightbyte, a 12-byte float triple,
  16-byte-aligned records, unions, bit-fields, nested records, arrays in
  records, GNU empty records; MEMORY records (above 16 bytes, packed fields);
  `long double` shapes (expect the refusal).
- Positions: exhaustion at 5, 6 and 7 GPRs and at 7, 8 and 9 SSE registers;
  the all-or-nothing cases (a two-GPR record or `__int128` with one GPR left, a
  mixed record with one bank full, followed by an argument that must still take
  the free register); many stack arguments with 16-byte alignment.
- Results: every class, `sret` above 16 bytes (check that `%rax` holds the
  destination), narrow signed and unsigned results.
- Variadic (M3): EvoCC calls glibc `printf` and `vsnprintf` with integers,
  doubles, pointers and a `va_list`; GCC-built variadic functions call EvoCC
  ones and the reverse; `va_copy` across compilers.
- Directions: reference caller to EvoCC callee, and EvoCC caller to reference
  callee, with GCC-14 and Clang-19 at -O0 and -O2. Pass: every shape prints its
  check line and exits 0.

### 5.6 Static ISA audit

`tools/x86_64_isa_audit.py` runs `objdump -d --no-show-raw-insn` in the amd64
image over every emitted object and refuses a mnemonic outside the allowlist of
the expansion table. Rosetta advertises AVX2, BMI2 and FMA and executes an
AVX2 instruction (measured), so only this audit and the QEMU `-cpu qemu64`
lane catch an instruction above the baseline. LZCNT and TZCNT silently decode
as BSR and BSF on an older CPU, so the audit is the primary check.

### 5.7 Corpora

- **c-testsuite** at the pinned commit `5c7275656d75` (`tools/ladder/e0.py:130-141`):
  220 programs, 220 PASS on AArch64 (run `20260927T082713Z`).
- **Ladder** (`tools/ladder/subject.py`): the reference build runs in the image;
  EvoCC on the host emits assembly per unit; the image assembles, relinks and
  runs the upstream tests. cJSON 1.7.19 (23 units), zlib 1.3.2 (19), Lua 5.5.1
  (34; `all.lua` must print `final OK`), SQLite 3.53.4 (86; `testrunner.tcl
  veryquick` with zero errors).
- The pass rules, sources and digests stay those of the AArch64 lanes. Do not
  change a configuration to avoid a refusal; record the refusal.

### 5.8 Commands

```sh
# Unit, ABI and corpus lanes (serial, offline, the shared target tree).
cargo test --offline -p evocc-mir --test x86_64_expansions
cargo test --offline -p evocc-mir --test x86_64_abi_calls
cargo test --offline -p evocc-mir --test x86_64_native_corpus
cargo test --offline -p evocc --test x86_64_driver
cargo test --offline -p evocc --test proof_required

# One program by hand.
evocc --target=x86_64-unknown-linux-gnu --emit=asm prog.c > prog.s
container run --rm --platform linux/amd64 --network none -v "$PWD:/work" -w /work \
  evo-nexlab-amd64:sealed sh -c 'gcc-14 -o prog prog.s && ./prog; echo exit=$?'

# FP bit-exact and baseline ISA lane.
container run --rm --platform linux/arm64 --network none -v "$PWD:/work" -w /work \
  evo-qemu-x86:sealed qemu-x86_64 -cpu qemu64 ./prog-static

# Corpora (new --target option).
python3 tools/ladder/e0.py --suite ctest --target x86_64-unknown-linux-gnu
python3 tools/ladder/cjson.py --target x86_64-unknown-linux-gnu
python3 tools/ladder/zlib.py --target x86_64-unknown-linux-gnu
python3 tools/ladder/lua.py --target x86_64-unknown-linux-gnu
python3 tools/ladder/sqlite.py --target x86_64-unknown-linux-gnu

# Gate.
tools/gate-changed.sh --full-workspace --offline
python3 tools/generate-target.py --check
python3 tools/check-spec-pins.py
```

## 6. Milestones

Each milestone ends with the full workspace gate and the AArch64 identity check:
the two-build identity on the six translation units of the codegen size
comparison and on both native corpora (384 Darwin and 384 Linux objects
byte-identical). Line counts are hand-written lines; generated tables and the
generated predefined profile are extra.

### M0: behavior-preserving preparation

| Change | Files | Lines |
|---|---|---|
| Target predicates; replace 14 comparisons and the 4 driver comparisons | `crates/evocc-mir/src/{emit,lower}.rs`, `bin/evo-ir-asm.rs`, `crates/evocc/src/cc.rs` | 90 |
| G1 ordered register lists; `Option` ABI facts | `generic/{abi,allocverify,allocation}.rs`, `isa/aarch64/abi.rs` | 110 |
| Generator: target parameter, named views, optional roles, list facts | `tools/generate-target.py`, regenerated `isa/aarch64/*.rs` | 250 |
| Pass the driver's layout to the 32 production call sites (parser 23, chir 5, evoir 3, the EvoMIR interpreter at `interp.rs:252`) and to the EqSat layout identity | parser, chir, evoir, `crates/evocc-mir/src/interp.rs`, `crates/evocc-eqopt/src/persistent.rs:25-28` | 320 |
| Per-target `__has_builtin` lists | `crates/evocc-cpp`, `crates/evocc-parser/src/builtins.rs` | 60 |
| Tests | `tools/tests/test_generate_target.py`, `target_abi.rs`, `target_regfile.rs`, layout plumbing tests | 300 |

Total: about 830 production and 300 test lines. **Exit:** AArch64 identity;
`generate-target.py --check`; the full gate.

### M1: integer-only functions and calls

Scope: integer and pointer scalars, `bits<128>` arithmetic, scalar loads and
stores of frame slots, spills, control flow, UB traps, direct and indirect calls
with up to six register arguments and stack arguments by exhaustion, results in
`%rax`/`%rdx`. Records, globals and floating point are refused.

| Change | Files | Lines |
|---|---|---|
| Tables | `targets/x86_64/{regfile,abi-linux,expansions,encodings}` | (tables) |
| ISA adapters | `crates/evocc-mir/src/isa/x86_64/{mod,abi,allocation}.rs` | 270 |
| Emitter for the M1 forms, prologue, epilogue, frame access, calls, terminators | `isa/x86_64/emit.rs` | 1,300 |
| Condition mapping | `isa/x86_64/conditions.rs` | 150 |
| G3 per-form clobbers | `generic/{allocation,allocverify}.rs` | 120 |
| SysV plan for scalars and pairs; target dispatch; refusals | `lower.rs`, `lower/sysv.rs` | 500 |
| Target variant and dispatch | `emit.rs`, `native.rs`, `linscan.rs`, `allocverify.rs`, `bin/evo-ir-asm.rs` | 200 |
| Driver, toolchain, options, host target | `crates/evocc/src/{main,toolchain,cc}.rs` | 300 |
| Layout constructor, `VaListKind`, wide signedness, `X87` storage and its refusal | `crates/evocc-types`, `crates/evocc-parser`, `crates/evocc-chir` | 350 |
| Predefined profile selection | `crates/evocc-cpp/src/lib.rs`, new `predefined/x86_64-unknown-linux-gnu.h` | 60 |
| Explicit proof-path refusal | `crates/evocc/src/proof_required.rs` | 20 |
| Amd64 image, sysroot export and contracts | `.toolchain/`, `contracts/`, `tools/export-sysroot.sh` | 150 |
| Specifications | `spec/evomir.md` (an x86-64 target section: registers, expansions, conditions, ABI), `spec/target.md` (the `targets/x86_64/` tables), `spec/evoir.md` (the new code), `spec/verification.md` (the refusal sentence), the explain page | 400 (text) |
| Tests | `x86_64_expansions.rs` 700, `x86_64_abi_calls.rs` (integer classes) 400, `x86_64_native_corpus.rs` 300, `crates/evocc/tests/x86_64_driver.rs` 250, proof refusal 50, generator 100 | 1,800 |

Total: about 3,400 production and tooling lines and 1,800 test lines. **Exit
tests:**

1. Every M1 expansion passes Section 5.4 under GNU as and LLVM.
2. Integer ABI cross-calls pass in both directions with GCC-14 and Clang-19, and
   the narrow-result rule is measured and fixed.
3. 256 seeds of `native-main` agree with the EvoIR interpreter on x86-64
   (the generator gets a target result limit of two GPRs).
4. `--target=x86_64-unknown-linux-gnu --proof-required=...` exits 6 and writes
   nothing.
5. A checked division by zero traps with `ud1` (exit 132).

### M2: memory, structs, globals and the full SysV convention

| Change | Files | Lines |
|---|---|---|
| Globals, data, relocations, block addresses, aliases, visibility, sections, lifecycle arrays, patchable entries; share the ELF module emitter with AArch64 Linux through ISA hooks (`.quad`, `.ident`) | `emit.rs`, `isa/x86_64/emit.rs` | 450 |
| All load and store kinds, `ldp`/`stp`, `mem.copy`, dynamic frames, frame addresses, atomics, CFI when requested, the `%fs:40` stack protector | `isa/x86_64/emit.rs` | 550 |
| G2 placement marker and two pseudo-forms | `lib.rs`, `text.rs`, `text_parse.rs`, `validate.rs`, `interp.rs`, `targets/aarch64/insts`, `generic/{abi,allocation,allocverify}.rs` | 450 |
| `sysv` record mark | `crates/evocc-types`, `crates/evocc-chir`, `crates/evocc-evoir`, `spec/evoir.md` | 410 |
| SysV plan for records, `sret`, MEMORY and `__int128` | `lower/sysv.rs` | 350 |
| `mfence`, operand-free `asm`, refusals | `crates/evocc-lower`, `crates/evocc-evoir/src/assembly.rs` | 100 |
| Tests | ABI shapes (INTEGER, MEMORY, `sret`, `__int128`, stack) 700; globals, GOT/PLT, PIE and `-no-pie` links 250; atomics, including a multithreaded stress test 200; dynamic frames, `mem.copy`, volatile 150; mark grammar 100 | 1,400 |

Total: about 2,300 production and 1,400 test lines. **Exit tests:** the integer
and MEMORY ABI shapes of Section 5.5 in both directions; the day7 C corpus (128)
against `gcc-14 -O0`; the day5 corpus (256) as an ignored lane; Section 5.4 for
every M2 form; the EvoIR grammar readers (`spec_grammar_contract.rs`,
`tools/check-textual-grammar.py`).

### M3: floating point (SSE2) and varargs

| Change | Files | Lines |
|---|---|---|
| SSE2 emission of every scalar FP form, strict variants, environment checks, SIMD moves and spills | `isa/x86_64/{emit,floating}.rs` | 700 |
| FMA legalization; helper names | `lower.rs`, `floating.rs` | 80 |
| Target FP model (default NaN, NaN priority, tininess); `FenvRuntime::X86_64Glibc` | `crates/evocc-types/src/floating*`, `crates/evocc-memory/src/fenv.rs`, interpreters | 450 |
| SSE classes in the plan | `lower/sysv.rs` | 150 |
| Variadic prologue; `va_start`, `va_arg`, `va_copy` | `isa/x86_64/emit.rs`, `lower.rs` | 550 |
| `va_list` as an array type | `crates/evocc-chir`, `crates/evocc-lower`, `crates/evocc-chir/src/fortify.rs` | 200 |
| Tests | FP units with boundary values under QEMU 500; SSE and variadic ABI shapes in both directions 500; strict FP 200 | 1,200 |

Total: about 2,100 production and 1,200 test lines. **Exit tests:** Section 5.4
for every FP form, bit-exact under QEMU; every SSE and mixed ABI shape and every
variadic test of Section 5.5; the strict-FP tests (`strict_floating.rs`)
mirrored for x86-64.

### M4: corpus

Order: c-testsuite, then cJSON, zlib, Lua, SQLite.

| Change | Files | Lines |
|---|---|---|
| x86 inline-assembly subset (needed by SQLite) | `crates/evocc-evoir/src/assembly.rs`, `crates/evocc-lower`, `isa/x86_64/emit.rs`, regfile names | 600 |
| Ladder target parameter, amd64 image, trap status 132 | `tools/ladder/{e0,subject,csmith}.py` and their tests | 250 |
| ISA audit | `tools/x86_64_isa_audit.py` | 100 |
| Defects that the corpus finds | wherever they are | budget 500 |
| Tests | ladder tests, reduced cases for each failure | 400 |

Total: about 950 production and tooling lines, 400 test lines, and the defect
budget.

**Exit:** c-testsuite 220/220 PASS, or each failure has a named refusal and a
reduced test (no wrong code); cJSON, zlib, Lua and SQLite pass their upstream
tests with the pinned sources and flags; the ISA audit is clean on every corpus
object; receipts under `tests/contracts/` and `.evidence/ladder/`.

### Totals and work split

About 9,600 production and tooling lines (M0 830, M1 3,400, M2 2,300, M3
2,100, M4 950) and 5,100 test lines, plus about 500 lines of specification text
and a defect budget of 500. Within a milestone, four slices can run
in parallel with strict file ownership: (A) tables, generator and `generic/`;
(B) `isa/x86_64/`; (C) `lower.rs`, `lower/`, `native.rs` and the target
dispatch; (D) the front end, the driver, images and contracts. A publishes its
interfaces first. Tests belong to the slice that owns the code under test.

### Out of scope for R04

- The x86-64 kernel (a separately declared stretch, as `TODO.md` states).
- x87 `long double` values (later: EvoIR format, x87 emission, `%st(0)` returns,
  about 2,000 lines).
- `_Float128` on x86-64 (later: the AArch64 Linux helper path applies).
- TLS (later: about 300 lines).
- SSE3 and later, AVX, AVX-512, BMI, POPCNT and LZCNT; x86 intrinsics and
  GNU vectors.
- Every optional selection, allocation and frame variant; a tied-operand
  allocator; LEA and memory-operand selection.
- Darwin x86-64 and 32-bit x86.
- The proof-required path and Lean checkers for x86-64 (Section 8).
- `asm goto` and constraint letters outside the M4 subset.

## 7. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| Rosetta returns a positive default NaN | FP-bit-exact results disagree with the x86 model | Bit-exact lanes run under QEMU (Section 5.1 measured the difference) |
| Rosetta executes AVX2 (measured) and advertises BMI2 and FMA | An instruction above the baseline goes unnoticed | Static audit plus QEMU `-cpu qemu64` |
| Condition mapping by family | A wrong carry sense is silent wrong code | Exhaustive pair tests; unknown pairs refused |
| 12 allocatable GPRs and clobber intervals | More spills; the allocator can hit its 64-retry limit or the "unspillable spill temporary" refusal | Count these refusals on the corpus; a tied-operand allocator is later work |
| Front-end layout plumbing (M0) touches four crates | An AArch64 behavior change | The identity check gates M0 |
| EvoIR grammar change (`sysv=`) | Readers and receipts that pin `evoir.md` change | Follow the change procedure; run the grammar readers and the ladder campaign-input checks |
| Narrow-result extension differs between GCC and Clang | Wrong values at a cross-compiler call | Measured in M1 before the rule is fixed |
| Inline assembly | SQLite needs `rdtsc` with fixed outputs | The M4 subset; refusal until then |
| Trap signal 132 | Harnesses that assume 133 misclassify | The trap status becomes a target fact |
| Interpreters hard-code AArch64 facts | False mismatches in the MIR and EvoIR lanes | M0 plumbing; M3 FP model |
| Docker daemon proxy failure | `docker pull` fails on this host | Use Apple `container` (the repository runtime), or the registry API with digest checks |
| Shared files with concurrent AArch64 work (`emit.rs`, `lower.rs`, `generic/`) | Merge conflicts | Land M0 first and keep x86 code in new files |
| Disk | The amd64 image is about the size of the arm64 one; free space moved between 22 and 36 GiB during this session | Check `df -h /` before each image build |

## 8. The proof-required path

**Decision: refuse x86-64 in the first slice, explicitly, with exit 6.**

- The claim of `spec/verification.md` names `aarch64-unknown-linux-gnu` (lines
  19-20 and 35). The Lean semantics are AArch64: `lean/EvoIR/Asm/` has 6,277
  lines (781 trusted definition, 385 checker, 4,829 proof), and `Alloc/` has
  3,357 more. The readers require the AArch64 triple.
- Today the only guard is a data-layout comparison in `inspect_core`
  (`crates/evocc-chir/src/core_profile.rs:76-86`). `TargetDataLayout` holds
  three flags. In that encoding an x86-64 layout differs only by `char`
  signedness, so with `-funsigned-char` it would compare equal. The M1 layout
  adds fields that make the layouts differ, but a target check must not rest on
  a layout comparison. The lowering in `proof_required.rs` also hard-codes
  `evocc_mir::Target::Linux` (lines 524-536).
- Change: at the start of `export` (`proof_required.rs:439`), refuse every
  target other than `aarch64-unknown-linux-gnu` with `Failure::Unsupported`
  (exit 6) and the message "target `T` is outside core-v1". Nothing is written.
  Add the test to `crates/evocc/tests/proof_required.rs`; today no test passes
  `--target` there. `evo prove` keeps its fixed AArch64 target
  (`crates/evocc/src/bin/evo.rs:209-222`).
- Add one sentence to `spec/verification.md`: other targets are refused with
  exit 6 before export.
- An x86-64 checker would need x86 semantics, templates and proofs. That is a
  separate project of thousands of Lean lines and is not part of R04.

## 9. Observation outside R04

For `aarch64-apple-darwin`, the frozen compiler folds `sizeof(long double)` to
16 while `__LDBL_MANT_DIG__` is 53. Apple arm64 uses an 8-byte binary64 `long
double`. The parser maps `long double` to `F128` for every target. The
long-double format knob of M1 can fix this; it needs its own task and ABI test.

```sh
printf 'int s(void){return sizeof(long double);}\n' > ld.c
.build/frozen/366fd796/evocc-darwin --target=aarch64-apple-darwin --emit=evoir ld.c
# %v0 = const.bits<64> 16
```

## Appendix A. Probe sources and results

### A.1 Data layout probe

Built with `gcc -std=gnu11 -O0` in Debian 13 amd64. It prints `sizeof` and
`_Alignof` of each type in Section 3.2, then `char_signed=1 wchar_signed=1`
and `ldbl_mant_dig=64 flt_eval_method=0`. `gcc -Q --help=target` reports
`-march=x86-64`, `-mfpmath=sse`, `-mlong-double-80`, `-mred-zone` and
`-msse2` enabled.

### A.2 Reference sequences (GCC 14.2.0 -O1)

```text
call_variadic:  movl $2, %edi; movl $2, %eax; call vdsum     # %al = 2 SSE registers
divide:         movq %rdi, %rax; cqto; idivq %rsi; addq %rdx, %rax
shift:          movq %rdi, %rax; movl %esi, %ecx; salq %cl, %rax
take_big:       movq 16(%rsp), %rax; addq 8(%rsp), %rax; addq 24(%rsp), %rax   # MEMORY record by value
ret_big:        movq %rdi, %rax; ...                          # sret pointer returned in %rax
take_mixed:     cvttsd2siq %xmm0, %rax; addq %rdi, %rax       # {double; long}: %xmm0 + %rdi
after_regs:     addq 8(%rsp), %rdi; leaq (%rdi,%r9), %rax     # {double; long} after five longs: %xmm0 and %r9; the last long on the stack
ld_add:         fldt 8(%rsp); fldt 24(%rsp); faddp %st, %st(1) # long double: MEMORY, result in %st(0)
read_ext (PIC): movq ext_obj@GOTPCREL(%rip), %rax; movl (%rax), %eax
```

### A.3 NaN and ISA probes

```text
gcc -O0, run under Rosetta:  runtime=7ff8000000000000 nan
gcc -O0 -static, qemu-x86_64 -cpu qemu64:  runtime=fff8000000000000 -nan
vpxor %ymm0, %ymm0, %ymm0 under qemu-x86_64 -cpu qemu64:  SIGILL (exit 132)
same under -cpu max and under Rosetta:  executed
```
