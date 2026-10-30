# Changelog

All notable changes to EvoCC are listed in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0] - Unreleased

### Added

- An architecture page and a README for each of the 18 compiler crates. They describe the
  current interfaces, dependencies, execution checks and proof-required trust boundaries.
- A RISC-V64 Linux backend design with measured RV64GC/LP64D layout, ABI, floating-point
  environment and assembler behavior. It shares its section structure with the x86-64 design;
  both designs are in `docs/design/`. Neither backend is implemented by this change.
- Experiment 8 gives the bugs of experiment 4 to chat models, with the frozen record of GCC 14
  and UBSan or of EvoCC. Three models completed it. With EvoCC, each one used fewer output tokens
  on the silent bugs (p < 0.01), and Qwen3.8 27B repaired 54 of 54 against 30 of 54. The
  protocol and the harness are in `tools/experiment/`; the receipts, the wire logs and the
  analysis are in `tests/contracts/agent-repair-8-*`.
- A figure in the README and the book shows the effort and the repairs of each agent with a
  registered result. `tools/render-readme.py` draws it from the receipts and refuses a stopped
  run or a failed prediction.

### Changed

- The source-size check limits new Rust and Python files to 1,500 lines and functions to 150
  lines. Older exceptions have a baseline that can only shrink. The local gate and CI check it.

- The task tracker keeps active work and one-line references to closed work. The full closed
  history is in `docs/archive/todo-0.1.0.md`.
- The book chapter on the agent experiments is now "Agent Experiments", and it reports
  experiment 8.
- The README section "Built for agents" shows the record that an agent gets from GCC and from
  EvoCC for the same bug, what EvoCC gives an agent, and the measured results.

### Fixed

## [0.1.0] - 2026-10-28

The first public version.

### Added

- Calls without a prototype of a function with an old-style definition follow ISO C: the
  arguments take the default argument promotions, EvoCC checks no argument type against a
  parameter type, and the call compiles as with GCC 14. Both interpreters and `--check` report
  the new E3068 (`old-style-call-mismatch`) when the argument count differs or a promoted
  argument type does not agree with the parameter, outside the two exceptions of C11 6.5.2.2p6.
  Before, EvoCC checked such a call against the promoted prototype: an incompatible pointer was
  W0012 or E2202, a count difference was E3051, and another argument was converted.
- The lint W0012 (`incompatible-pointer-types`) reports a conversion between pointers to
  incompatible types and a conditional operator with such operands, as GCC 14 does. It is an
  error by default in C99 and later modes and a warning in C89 and GNU89.
  `-fpermissive` and `-Wno-error=incompatible-pointer-types` make it a warning, and
  `-Wno-incompatible-pointer-types` removes it; a warned conversion is an explicit cast. Before,
  these conversions were E2202 or E2102, and three forms were accepted without a diagnostic:
  such a conditional when its result was discarded or its operands pointed to character types,
  `_Atomic T *` from `T *`, and a cast of zero to another pointer type.
- The x86 calling-convention attributes `stdcall`, `fastcall`, `thiscall`, `vectorcall`,
  `regcall` and `sseregparm`, and `regparm` on Linux, are ignored with the new warning W0011
  (`attributes`), as in GCC 14 and Apple clang; before, they were E2952. `-Wno-attributes` and
  `-w` silence it, and `-Werror` and `-Werror=attributes` make it an error. A form that GCC 14 or
  Apple clang refuses stays E2952, and so does `regparm` on Darwin.
- Both drivers accept `-fexpensive-optimizations`, `-ftree-vectorize`, `-fno-vect-cost-model`,
  `-ftrapping-math` and `-fno-trapping-math` with no change of code: they only permit transforms
  that EvoCC does not make. On Linux, a final `-fno-trapping-math` defines `__NO_TRAPPING_MATH__`,
  as in GCC 14. The fast-math options and `-Ofast` stay refused.
- Both drivers accept `-fno-common` and `-finput-charset=utf-8` (also `utf8`, in any letter
  case), which name existing contracts. `-fno-builtin-NAME` disables the recognition of one
  library name, and `-fno-inline` substitutes only `always_inline` functions, as in GCC 14.
  `-fcommon`, other charsets and `-finline` stay unsupported.
- The `optimize` attribute accepts what EvoCC already satisfies, with no effect on code: the
  levels 0 to 3, `O`, `O0` to `O3`, `Os` and `Og`, and twelve disabling options such as
  `no-strict-aliasing` and `no-tree-vectorize`. Every other option keeps E2952. Both drivers
  accept `-fno-tree-vectorize`, `-fno-tree-loop-vectorize` and `-fno-tree-slp-vectorize` on
  both targets, as GCC 14 does; Apple clang refuses `-fno-tree-loop-vectorize`.
- GNU range initializers on Linux share each nonconstant scalar or record value across its
  surviving destinations. Fully overwritten values do not run. Values execute in destination
  order, as in GCC 14. E2969 names a range value that must be constant: each nonconstant value
  on Darwin, and a nonconstant array value on Linux, which GCC 14 also refuses. E2917 describes
  an index or bound.
- `evo check FILE.c --json --form=concise-1` prints one short record: the check result, each
  diagnostic with its location, a one-line explanation and a repair hint, and the plan's repairs.
  `full-1` stays the default. The MCP `check` tool takes the same optional `form`.
- `__builtin_strlen` of an immutable file-scope character array with static storage, or of a
  `const` file-scope pointer into one, is an integer constant expression on both targets, as in
  GCC. Every other operand stays an ordinary call with its effects.
- `typeof` evaluates its operand exactly when the operand's type is variably modified, as C23
  requires.
- `__builtin_return_address(n)` and `__builtin_frame_address(n)` with `n` from 1 to 65535 return
  a null pointer on both targets and in both interpreters, as GCC does. An invalid depth is E2913.
- Integer-complex division and compound division use GCC's wide algorithm in the component
  type on both targets. Each step keeps its division-by-zero and overflow checks, and only the
  selected branch runs. E2948 is withdrawn. Apple clang uses another formula, so Darwin results
  can differ from Apple clang's.
- Integer vectors of 1, 2 and 4 bytes can be arguments and results. Linux follows GCC 14 in
  general registers; Clang 19 returns them in SIMD registers, which stays a recorded
  incompatibility. Darwin follows Apple clang and returns them in `v0` with widened lanes; that
  result is E4002 under `-mgeneral-regs-only`.
- The GNU `__atomic` fetch-op, op-fetch, exchange, compare-exchange, test-and-set, clear, fence
  and lock-free builtins on 1-, 2-, 4- and 8-byte scalars, inline on both targets. E2947 reports
  exactly the constant orders that GCC 14 warns about and every constant outside 0 to 5. The
  interpreters run one thread and report E3021 at a non-relaxed fence, as at
  `__sync_synchronize`. Crossed native thread tests pass against GCC 14, Clang 19 and Apple
  clang; they are not a memory-model proof.
- `_Atomic` objects use GCC's layout on Linux and Apple clang's on Darwin, and `_Atomic(T)` is
  parsed. Reads and stores of 1-, 2-, 4- and 8-byte scalars are seq_cst atomic accesses, and
  `+=`, `++` and `--` are one seq_cst compare-exchange loop with the usual overflow checks. E2955
  and E2956 are new. Floating updates, whole records and 16-byte objects stay E2702.
- The generic `__atomic_load`, `__atomic_store`, `__atomic_exchange` and
  `__atomic_compare_exchange` forms work on objects of 1, 2, 4 and 8 bytes whose alignment is at
  least their size. They move bytes, and a compare-exchange compares every byte. Other shapes stay
  E2913. Crossed thread tests pass against GCC 14, Clang 19 and Apple clang; they are not a
  memory-model proof.
- `__has_include_next`. On Darwin, `<limits.h>` defines `PATH_MAX` and the other SDK limits, as
  with Apple clang.
- Enumerations with a fixed underlying type (`enum E : T`), in every language mode on both
  targets. E1306, E1307 and E2958 are new.
- GNU attributes after the parenthesis of a declarator, as libxml writes them. An attribute with
  an effect is E2903 there.
- `__builtin_fpclassify` and `__builtin_tgmath`, as GCC 14 documents them, so glibc's
  `fpclassify` and `<tgmath.h>` work for real and integer arguments. E2959 is new.
- Floating complex values (`float`, `double` and `long double _Complex`) as AAPCS64 HFA pairs.
  Multiplication and division call libgcc's helpers on Linux and compiler-rt's on Darwin, and
  the interpreters and static initializers use one bit-exact model of the Linux helpers. Plain
  `_Complex` is `double _Complex`. `__builtin_complex`, `creal`, `cimag`, `conj`, imaginary
  floating constants, `_FloatN _Complex` declarations and complex `<tgmath.h>` selections work.
  E2960 and E3067 are new.
- GNU `float` and `double` vectors of 8 and 16 bytes execute on both targets with per-lane scalar
  rules, integer masks, per-lane C casts (E3063) and GCC's `++`, `--` and `&v[i]`. A record of one
  to four short vectors of one size passes as an HVA whatever its lane types. Floating vectors
  are E4002 under `-mgeneral-regs-only`.
- `_Float16`, `__fp16` and `__bf16` values execute on both targets. Each operation evaluates in
  binary32 and rounds once, as in GCC 14, and halves and binary16 HFAs pass in SIMD registers.
  The `f16` suffix works on both targets, and the `bf16` suffix on Linux, as in GCC 14. A
  `__bf16` HFA is E3049, and `_Float16` with `__bf16` in one operation is the new E2961.
- On Darwin, `<stdatomic.h>` compiles and runs through Clang's `__c11_atomic_*` builtins.
- Darwin framework headers, `-F`, `-iframework` and `-framework`, and `#import` on both targets.
- Apple clang availability for the macOS deployment target (`-mmacosx-version-min`,
  `MACOSX_DEPLOYMENT_TARGET`, default 11.0). E2962 and E0114 are new.
- Named refusals on Darwin for Blocks, `overloadable`, the os_log builtins and the ACLE builtins
  (E2963 to E2966).
- Multi-character constants such as `'TEXT'`, with the values of GCC 14 and Apple clang.
- The EvoMIR interpreter runs records passed in general registers, padding and unstored members
  included, and the randomized gate covers them.
- Three more checked string builtins of the macOS 26 SDK (`__builtin___memccpy_chk`,
  `__builtin___strlcpy_chk`, `__builtin___strlcat_chk`), four checked `printf`-family builtins
  and `__builtin___clear_cache`, which calls the target's runtime helper.
- A registered repair-effort replication with 96 small-program sessions and 48
  larger-program sessions. EvoCC feedback used fewer steps and tokens on the
  selected small silent defects and fewer steps on the larger items. The three
  exact effort tests held, but relative step savings did not grow with size.
  The report retains the control result, functional failures, overload notices,
  quota use and transcripts. No general repair or causal size claim is made.
- A registered 24-program coding-agent study of silent memory defects and integer-UB
  controls. All 48 sessions completed under three hidden scoring builds. Default GCC
  warnings plus UBSan and EvoCC each repaired 18/18 silent items and 5/6 controls;
  the predicted repair-count advantage did not hold. The report retains the detection
  matrix, pilot, transcripts, quota use and the two incorrect functional repairs.
- A preregistered 30-program agent repair experiment with one model, three feedback
  conditions and three repetitions. All 270 outcomes and full transcripts are retained,
  including response failures. EvoCC feedback passed 87/90 attempts, GCC plus UBSan
  88/90, and Clang plus UBSan 82/90; all successful repairs took one round. The report
  makes no general repair advantage claim.
- A second preregistered repair experiment with 30 two-defect programs, five feedback
  conditions (C0 to C4), two models and new cost, diff-size and fix-span measures. The
  provider key limit stopped it after 444 of 900 repetitions; all outcomes, refused attempts
  and transcripts are retained. None of the five predictions held for either model, and the
  report states the reason for each. The harness gained a total HTTP deadline and `--parallel`.
- A C compiler for AArch64, written in Rust: its own preprocessor, parser, three intermediate
  representations (C-HIR, EvoIR and EvoMIR), optimizer, register allocator and assembly emitter.
  The platform assembler and linker build the final program.
- A drop-in `cc` driver. `evocc` takes several inputs and one action (`-E`, `-S`, `-c`, or a
  link), `-o`, `-x`, the usual preprocessor options and the `-M` family of dependency options. It
  runs the platform assembler and linker and passes `-l`, `-L`, `-Wl,`, `-shared` and `-static` to
  the linker. An unknown `-f` or `-m` option prints one warning; `-Werror` makes it an error
  so compiler probes do not report unsupported features as available. With
  `CC=evocc`, cJSON 1.7.19 and zlib 1.3.2 build and pass their own tests on macOS.
- The target `aarch64-apple-darwin`, the default on macOS. It emits Mach-O assembly and uses the
  Apple variadic convention: every anonymous argument goes on the stack, and `va_list` is one
  pointer. `aarch64-unknown-linux-gnu` is the default on Linux. On macOS, the sealed container
  can assemble Linux objects. Cross-linking requires an explicit `--linker=COMMAND`.
- `--version`, which prints the version, the commit and the date of the build, and the target.
  `-dumpmachine`, `-dumpversion` and `--help` are also available.
- Kbuild resource queries, assembly on standard input, and assembly preprocessing with
  dependency files. The basic stack-protector option enables the existing strong policy.
- Constant-result assembly operands retain their original calls, volatile reads and UB
  obligations. Inlined constant pointers remain visible through later local assignments.
- Application replays reject reference recompilation during tests, including tests that
  copy their build tree before running make.
- Two interpreters, one for C-HIR and one for EvoIR, that stop at undefined behavior with a
  numbered diagnostic (`--interpret-c-hir`, `--interpret-evoir`).
- Source diagnostics with primary and secondary labels, notes and help. Text, short and stable
  JSON formats share the same locations. Color can be automatic, always on or always off.
  The tests cover 62 source/text/JSON snapshot triples.
- `--check`, which runs the executed path and reports undefined behavior. `--explain` prints
  one of 122 tested diagnostic pages. The book includes the generated error index. Twenty
  pages explicitly state that their diagnostic has no C-source trigger.
- Five front-end warnings: unused variables, unused parameters, implicit function declarations,
  implicit int and missing return values. Named warning levels, `-Wall`, `-Wextra`, `-Werror`,
  `-w` and `--cap-lints` control them. This is not a full GCC warning implementation.
- The `evo` command with help and JSON results for user commands. It uses the existing compiler,
  interpreters and checkers. `evo build` runs configure and make and writes a build receipt;
  it is not a new build system. Help exits 0; no arguments exits 64.
- Five diagnostic repair forms, exposed by `evo fix --plan` and `--apply`. Safe repairs are
  checked after editing. Review repairs require human approval; macro and volatile boundaries
  are not silently crossed. A syntax recheck is not a proof of equivalent behavior.
- `evo mcp`, a five-tool JSON-RPC server on standard input and output. Each tool uses the
  matching JSON command. Root `AGENTS.md` and `llms.txt` give the same agent instructions.
- A recorded repair loop, from an executed-path overflow finding through two safe repairs
  and an owner-selected guard to checked assembly. The book includes one real MCP model
  session and three refusals: changed certificate, deleted obligation and wrong checker hash.
  The scripts retain actual output; they do not extend the proved C subset.
- Paired-run tests for byte-identical agent JSON and proof records with fixed inputs and
  compiler context. Only temporary directory paths are normalized. The book gives tested
  Claude Code and Codex connection settings, and `skills/evocc/` gives bounded repair rules.
- `evo lsp`, with full-text synchronization, diagnostics from the compiler, compile database
  flags and safe repair code actions. The book describes Zed, Neovim and the source-only
  VS Code extension. The editor protocol tests pass; the owner waived the Zed screenshot.
- A CMake toolchain file for both targets, compiler identification macros, and tests for
  pkg-config linking and Make dependency rebuilds. Coverage options warn and have no effect;
  EvoCC does not produce gcov instrumentation or data files.
- A generated compatibility matrix from every retained ladder receipt, a single rolling
  nightly prerelease workflow, and an exact-version changelog check before packaging.
  The nightly workflow reuses the release build and smoke tests; it has not run on GitHub.
- A bundled AArch64 Linux header sysroot. Releases find it beside `bin/` without `--sysroot`.
  The file-hash contract records 1,657 headers and eight unsupported netfilter headers that
  collide by case. The bundle supports the default macOS volume. It contains no linker, libc
  or C runtime. The driver does not change include lookup for excluded names.
- Undefined behavior as data in EvoIR: each definedness condition is an explicit
  `ub.assume.<reason>` obligation, and EvoMIR lowers it to a checked trap.
- Verified equality saturation (`-Oeq-scalar`): 62 rewrite rules (55 in `p0.scalar.v4`, 7 in
  `g0.ub.v1`), each proved in Lean 4 for every bit width. A separate, pinned checker,
  `evocc-checker`, replays each certificate before EvoCC writes output.
- Compilation with proof (`--proof-required`) for the integer subset `core-v1`: five compiled Lean
  checkers in `evoir-sem` check each stage of the actual compilation, and the theorem
  `checkPipeline_sound` covers the chain from typed Core source to the final assembly text. A
  refusal writes no output.
- Five pinned specifications in `spec/`: EvoIR 2.39.0, EvoMIR 1.16.0, the target description
  1.1.0, the verification contract 1.1.3 and the diagnostic format 1.4.1.
- The evaluation ladder, `tools/bench.py`: conformance suites, fifteen application lanes and Linux
  build, boot, kselftest and KUnit lanes. Each lane writes a receipt.
- The EvoCC Book in `book/`: installation, a first program, undefined behavior, compiling with
  proof, checked optimization, how EvoCC works and the evaluation. A workflow builds it with
  mdBook and publishes it to GitHub Pages.
- The Apache License 2.0 (copyright 2026 Haoran Wang, NexRidge), a `NOTICE` file and third-party
  notices.
- `++` and `--` on integer complex values change the real part, as in GCC 14 and Apple clang.
  GNU imaginary integer constants such as `4il`, `4li` and `4uil` have the references' types.
- Four warnings follow GCC 14: W0006 `pointer-sign`, W0007 `pointer-arith`, W0008
  `discarded-qualifiers` and W0009 `discarded-array-qualifiers`, with GCC 14's default levels.
  `-Werror=<name>` now refuses the programs that GCC 14 refuses; before, EvoCC accepted the
  option and the program.
- `#warning` is the warning W0010, which `-Werror` and `-Werror=cpp` turn into an error, as in
  GCC 14. `-pedantic-errors`, `--pedantic-errors` and `-Werror=pedantic` are refused with E2968,
  because EvoCC has no ISO pedantic diagnostics; before, EvoCC accepted them without effect.

### Changed

- Application status rows name reference failures separately from EvoCC failures, and show
  refusals, deferrals, inapplicable programs and unusable oracles. Their statuses and counts
  are unchanged; raw refusals are not an extra count of compiler failures. A legend on the
  status page explains raw refusals and reference failures, and its table gives each count once.
- `E3007` has one meaning, `unsupported-interpreter-input`: an interpreter cannot execute the
  input. It is a refusal, not a finding. Before, its page described a withdrawn diagnostic.
- A shift by a constant count in range has no run-time count check any more; the K16
  `mm/memory.c` loses 70 of its 90 shift-count traps. A constant count out of range and a
  variable count keep the check.
- `tools/release.sh` builds the archive programs with a `dist` profile that keeps symbols but
  removes the debug map and the paths of the build machine. Two checkouts at two paths give
  byte-identical macOS programs and archives, and the script refuses a program that still holds
  a build path.
- A clean check now states that it did not check pointer relations, in both `full-1` and
  `concise-1`. Records of earlier versions keep their old scope sentence, so a replay of a
  frozen experiment needs its recorded compiler.
- The backend checks each SSA destruction against its input with an independent validator, and
  the allocation verifier relates the input program to the allocated registers and slots.
  Functions whose `asm goto` binds no fixed register now forward unchanged block arguments, and
  live ranges have holes, so fewer values pass through the stack at joins.
- One-word private records and small arrays indexed only by constants live in registers. A
  function whose protected objects all became registers needs no stack canary.
- `-mbranch-protection=pac-ret` signs every function that stores its return address and leaves
  a leaf that keeps it in `x30` unsigned, as GCC 14 does; `pac-ret+leaf` signs every function.
- Integer machine selection removes masks and extensions whose upper bits
  are already zero on every incoming path. It preserves W-register clearing,
  execution checks, memory effects and control-flow edges.
- Integer zero tests can use CBZ/CBNZ through a private width-preserving copy.
  The machine proof checker accepts the selection and rejects changed branch
  operands or polarity. Execution obligations and branch arguments stay intact.
- `evo prove` can find the Lean checker from a source checkout without an override.
  Its JSON record names the chosen checker; demo scripts use the same discovery path.
- The project license changed from the placeholder GPL-2.0-only to Apache-2.0.
- The book replaces the hand-written implementation notebook and its web site. The dated lab notes
  stay in `docs/archive/notebook/`.
- Darwin stack-protector lowering uses Mach-O symbols and relocations. Angle includes honor
  supplied include paths; quoted includes still search the source file's directory first.
- The sealed build environment is now named nexlab. Its image tags are `evo-nexlab:sealed`
  and `evo-nexlab:sealed-20260810`, with unchanged digests, and new kernel builds use the
  build host `nexlab`. The old tags stay, and historical receipts keep the old names.

### Fixed

- A clean Lean build no longer depends on a fast or large host. Each `bv_decide` proof of the
  transfer table has a 60 s SAT timeout, and its rows moved out of `Proofs/Transfer.lean` into
  three modules that `evoir-sem` does not import. A clean `lake build` now peaks at 8.3 GiB
  instead of 17.2 GiB, and a clean `lake build evoir-sem` at 3.9 GiB.
- The Rust job of `ci.yml` builds what its tests need on a clean runner. A new headers job
  exports the Linux headers of the sealed image, as `release.yml` does. The Rust job unpacks
  them and builds `evoir-sem` before `cargo test`. It builds the tests with line tables only,
  because full debug information needs more than 21 GB of disk. The 37 tests that need the macOS
  SDK or Apple clang run on the macOS job only. No hosted run has passed yet (see Known limits).
- `--interpret-c-hir` and `--interpret-evoir` run a `main(int argc, char *argv[])`, as `evo run`
  does: with no program arguments, `argc` is 1 and `argv[0]` is the source path. Before, both
  refused it with E3007.
- Of the 1,496 Darwin SDK and framework headers that Apple clang compiles alone, EvoCC
  compiles 1,443; each of the other 53 needs Blocks, `overloadable`, the Clang vector types,
  or a construct that GCC 14 refuses too.
- A GNU vector comparison has the mask type of the target's reference: `long long` lanes for
  64-bit lanes and `char` lanes for 8-bit lanes on Darwin, as in Apple clang, and `long` and
  `signed char` on Linux, as in GCC 14. Before, Darwin gave `long` masks for integer 64-bit
  lanes and `signed char` masks for 8-bit lanes.
- On Darwin, Clang attributes that change a type, a value, the layout, the linkage or the ABI,
  such as `ext_vector_type`, `neon_vector_type`, `ms_struct` and `counted_by`, are E2967. Before,
  EvoCC ignored them and compiled another type or layout. The nullability keywords `_Nonnull`,
  `_Nullable` and `_Null_unspecified` are accepted without effect on Darwin.
- Integer constant suffixes follow GCC's grammar: `4lL`, `1i0` and `4lil` are E0112, as both
  references refuse them. An imaginary integer constant in `#if` is E0111; before, `#if 4i` was
  false with no diagnostic.
- A `defined` that macro expansion produces in `#if` is evaluated, as GCC 14 and Apple clang
  do, and one whose operand is not an identifier is E0111. A plain character constant with UTF-8
  bytes has GCC 14's value on Linux and is refused on Darwin, as Apple clang refuses it.
- Wide character constants have the value and signedness of the target `wchar_t`, also in
  `#if`: on Linux `#if L'\0' - 1 < 0` is false, as with GCC 14, and `L'\xffffffff'` is
  accepted. Before, `#if` treated them as signed on every target, and `L'\xffffffff'` was E1006.
- A GNU statement expression whose value is a record, a union or a complex value copies the
  value out before its own objects end. Before, the EvoIR read a dead object: the EvoIR
  interpreter reported E3010, and native code relied on storage that was no longer live.
- A record, union, vector or void controlling expression, for example `if (v)` for a vector, is
  the new front-end error E2103 at the operand. Before, it reached a later stage, and a vector
  `if` ended in an internal error (E3101).
- Under `-mstrict-align`, a by-value record with alignment 1, 2 or 4 moves through registers
  with aligned accesses. Before, it could move as one doubleword from an address that only 4
  divides, which faults with alignment checking or with the MMU off. A new check, E4022, stops
  compilation when it cannot prove an access aligned. Without the option, output does not change.
- A store to a volatile bit-field no longer reports E3001 for an uninitialized neighbor, and
  the EvoMIR interpreter follows the same per-bit rule.
- On Darwin, `wchar_t` and `wint_t` are `int`, as Apple clang defines them. Before, they were
  the Linux `unsigned int`, so `(wchar_t)-1 < 0` and `WCHAR_MAX` differed from Apple clang.
- A store to a bit-field no longer reads the other members of its storage unit, so `evo check`
  no longer reports E3001 at the first store to a bit-field of an uninitialized object. A read
  of a member that was never stored is still E3001.
- Linux programs that include `<pthread.h>` or `<math.h>` compile again. The execution and
  calling-convention attributes of EvoIR 10.1.9 are accepted on a function prototype and
  recorded. A definition and each use stay E2952, except for functions that return twice and
  calls of `aarch64_vector_pcs`, `preserve_all` and `preserve_most` functions, which are
  ordinary base-PCS calls.
- A value that is live across a call of `setjmp`, `sigsetjmp`, `vfork`, `getcontext`, `savectx` or
  a `returns_twice` function keeps its value after the second return, as with GCC 14. Before, a
  shared stack slot could change it. Such a function is never inlined, and an `always_inline`
  one is E2957. The glibc `pthread_cleanup_push` and `pthread_cleanup_pop` macros compile.
- A compound update of a real integer by an integer-complex value, such as `x *= 2 + 1i`, now
  stores the converted complex result. Native code stored a wrong value before. A floating or
  pointer target is E2102.
- `evocc-checker --version` and `evoir-sem --version` report the release
  version without reading files from a source checkout.
- Repeated inline-assembly operands keep their own constraints when one operand
  has a fixed register. Only a numeric tie propagates that binding. This fixes
  the allocation refusal in Linux's 128-bit compare-exchange code without
  weakening the independent allocation check.
- Function `optimize` attributes can disable automatic tree, loop or SLP
  vectorization, which EvoCC does not perform. Other optimization settings,
  including a supported restriction mixed with an unknown setting, still fail
  with E2952. The unchanged Linux zstd source now compiles.
- An `asm` statement that clobbers `x19` to `x28` no longer loses the caller's value: the
  prologue saves the register and every epilogue restores it. A clobber of `x29` is refused
  (E4009).
- Calls between EvoCC code and Apple clang code on macOS follow the Apple arm64 convention.
  Narrow integer arguments and results are extended by their signedness, named stack arguments
  take their natural size and alignment, and a 128-bit argument does not start at an even
  register. Before, these shapes passed wrong values.
- On macOS, `long double` is binary64 with size and alignment 8, as with Apple clang. Darwin
  EvoIR text names its target, and the rewrite checker reads it, so `-Oeq-scalar` works for
  `aarch64-apple-darwin`. A certificate whose texts name different targets is refused.
- `__attribute__((naked))` and the other function attributes that change the code or the
  calling convention are refused with E2952. Before, a naked function got a frame record, so its
  `asm` `ret` returned with the stack pointer moved.
- On macOS, both interpreters use the eight-byte `va_list`, and named stack arguments fill only
  the argument registers that the call uses.
- A proof-required entry that is also called with constant arguments keeps its full parameter
  domain, so the checked stages accept it.
- A cast to a struct type from another type is E2953. A later declaration that changes an object
  used by a constant string query is E2954.
- More shapes fail closed instead of compiling wrongly: a read of an `_Atomic` object (E2702), a
  nonconstant range-designator initializer (E2917), a record of floating vectors at a function
  boundary (E3049), and ABI-changing options such as `-fpack-struct` and `-mabi=ilp32`, also
  under `-w`.
- The proof-required path checks the guarded division of D06 to final assembly text.
- The EvoIR, EvoMIR, target and verification specifications state what the code does where the
  2026-10-13 audit found them wrong: EqSat bounds and failures, the trusted base, GOT addresses,
  guard lowering and reader leniency. `tools/render-status.py --check` fails when the Lean receipt
  binds another EvoIR revision than the pinned one.
- `evo prove` accepts the input file before, between or after its output and
  entry options. All file-bearing command JSON tests cover both option orders.
- `evo fix` accepts its file before or after options. Removing an unused declaration also
  removes its otherwise empty line, without removing adjacent comments or code.
- Linux assembly and checked assembly records use the same release version as `evocc --version`.
- A source-checkout compiler finds its Linux sysroot from the executable's repository root,
  regardless of the current directory. Explicit and installed roots keep their priority.
- `-mbranch-protection=pac-ret+leaf` now signs return addresses and sets
  `__ARM_FEATURE_PAC_DEFAULT` to 5, as GCC 14 does. Linux probes this option. Before, the
  probe failed, the kernel asked for signing through `-msign-return-address`, and the
  kernel was built without it. As in GCC 14, `-msign-return-address` has no effect.
- `used` and `retain` written after a pointer `*`, as in Linux's `static void * __used
  __section(...) name`, now mark the declared object, as in GCC 14. Before, `-O2` could
  omit such an object when it had no `section`, and `-Wall` reported it as an unused
  variable, 13,601 times in one Linux build. The rebuilt kernel reports none. `unused` on
  the pointer next to a variable's name also prevents that warning. EvoIR revision 2.1.0
  states the rule.
- A warning for an ignored option labels that option, not the whole command line. In the
  text form, a command line longer than 80 bytes shows only the labelled argument. In one
  Linux build log, these warnings fell from 74.5 MiB to 4.5 MiB. `-ffixed-x18` no longer
  warns: x18 is never assigned. Diagnostic format revision 1.3.0 states the rule.
- Fortified C library calls compile. The macOS 15 SDK and glibc at `_FORTIFY_SOURCE` levels 1 to
  3 turn calls such as `memcpy` and `snprintf` into `__builtin___X_chk` calls. EvoCC implements
  the 14 checked builtins of GCC 14 and keeps each check unless the object size is unknown or a
  constant length fits. An overflow stops the program, as with Apple clang and gcc-14.
  `__has_builtin` reports exactly the implemented builtins.
- A call to an unknown `__builtin_`, `__atomic_` or `__sync_` name is E2949 at that name, with or
  without `-w`. Before, `-w` turned it into an implicit call.
- `builtin-argument-type` has its own code, E2951; E2911 named two diagnostics.
- The EVP certificate reader refuses the epoch marker 0.
- The EvoMIR text parser reads every form that the writer prints.

### Results at this version

The gates, the kernel replay, the 15 application lanes, Doom, the size measurement and the CMake
builds below ran on 2026-10-27 with the release compiler of commit `08bf09bd`. The other results
give their dates and were not run again on the final commit. [docs/evidence.md](docs/evidence.md)
keeps their limits.

- On 2026-10-27 the final health gate (T05) passes on the clean tree of `fecd612f`, whose
  compiler source is that of `08bf09bd`: the workspace gate, 1,209 Python tests, Ruff, the
  textual grammar, both frozen layout corpora (128 and 22 cases), 13 isolated offline replays,
  the Lean gate, the generated pages, the relative links and the book build.

- On 2026-10-27 both release archives were built from the clean commit `9e83b76a`, whose
  compiler source is that of `08bf09bd`: `evocc-0.1.0-aarch64-apple-darwin.tar.gz` on macOS 26.6,
  and `evocc-0.1.0-aarch64-unknown-linux-gnu.tar.gz` in an AArch64 Debian 13 container with no
  network. Both pass the smoke steps of `release.yml`. On Linux, `evocc` and `evocc-checker` are
  static, and `tools/tests/test_sysroot.py` passes on the archive. A second macOS build from a
  checkout at another path gave the same bytes. The release assets are built again from the
  dated commit, and their `SHA256SUMS` is published with them.

- On the clean tree of `08bf09bd` the full offline workspace gate passes 522 suites and 3,928
  tests with zero failures; 62 ignored tests run only on request, such as the sealed Linux lanes.
  Clippy and formatting pass. The Lean gate passes all its steps: 62 rule theorems against EvoIR
  2.39.0. Neither gate expands the verified language subset.
- On 2026-10-07 the recorded agent repair loop reaches checked assembly in 12.41 seconds in the
  workspace gate. Its refusal demo returns the three required failures without publishing
  assembly. Paired JSON/proof-record tests pass three consecutive runs and the final gate;
  both the full offline workspace and Lean gates pass on clean commit `200b4e73`.
- Linux 6.18.44 ARM64 boots with 2,799 of 2,799 target C objects compiled by EvoCC. The release
  compiler built every target C unit of this configuration in a fresh tree, in one uninterrupted
  Kbuild run. That kernel boots under HVF with pointer authentication and under TCG, as does the
  GCC reference, and nine paired upstream self-test programs pass with 125 of 125 subtests on
  each. A second configuration booted with 7,008 objects and 848 modules on 2026-09-28. This is
  not full Linux acceptance.
- One release compiler ran all 15 application lanes and Doom. c-testsuite 220/220, zlib, zstd,
  cJSON, TinyCC, Mbed TLS 382/382, PolyBench/C, jq, Lua, SQLite and Doom pass every program or
  unit. GCC torture (1,543 of 1,692) and the LLVM test suite (1,066 of 1,735) have no compiler
  failure; their other programs are not applicable, have an unusable reference, or are
  floating-point deferrals. Csmith agrees on 932 of 1,000 programs, and the other 68 have no usable
  reference; it has no refusal and no mismatch. Toybox and musl give the same test results as
  their GCC builds. Two Doom demos give the same framebuffer hash for each of 350 frames under
  EvoCC, GCC 14 and Clang 19.
- On 2026-10-06 (commit `e657709b`) the proof-required sweep checked 107 of 107 pilot entries to
  the final assembly text. A native run compared 14,194 vectors with the Lean source interpreter
  and found no difference.
- On five library translation units, the code is 1.8 to 2.3 times the size of GCC 14 `-O2`
  output.
- On 2026-10-14 a release unpacked on the default macOS volume compiles a Linux hello object and
  the original zlib 1.3.2 archive without a sysroot flag or manual include paths. Hello, the zlib
  example and a minigzip round trip pass in the sealed Linux environment. The cross-host release
  workflow is configured; these local results do not claim a hosted GitHub Actions run.
- Original CMake builds on macOS with the release compiler pass cJSON 1.7.19's 22 tests and
  zlib 1.3.2's 16 default CTest commands. The zlib coverage-summary command reports a missing
  .gcno file, but Apple's gcov returns 0. Its passing exit does not establish coverage support. No
  test was excluded.

### Known limits

These items are open in this version. Each line names its entry in [TODO.md](TODO.md).

- Kernel runtime acceptance is open: the complete paired kselftest run (K13), the complete
  expanded KUnit run (K14), the diagnoses K01 to K04 and K10 to K12, KUnit behind the common bench
  entry point (T02) and the binding of the final kernel claims to the final composition (T04).
- The EvoIR execution model judges the alignment of an access by the declared alignment of its
  object, not by its concrete address. Two GCC torture programs therefore report E3012 although
  their native accesses are aligned (D33).
- Values that cross a block join move through stack slots. Two transport reductions still run
  more than twice the instructions of GCC 14, and the original K02 workload times out (N08).
- The leaf-helper reduction still runs more than twice the instructions of GCC 14 (32 against 9
  per call), and the code size of the whole ladder has not been measured (N09).
- Return signing for `-mbranch-protection=pac-ret` has no kernel tracing acceptance: ftrace and
  kprobes have not run (N11).
- Floating `_Atomic` updates, accesses of a whole atomic record and 16-byte atomic objects stay
  refused with E2702 (L11a).
- In agent experiment P17, three failures spent the response budget after a correct overflow
  repair. A separate response limit for the overflow feedback is not tested (P17a).
- The check feedback of the agent interface is long. A shorter form has not been measured (P22).
- No hosted run of `ci.yml`, `nightly.yml` or `release.yml` has passed. GitHub refuses the jobs
  because of the billing state of the account (R05m).
- A clean hosted runner fails seven tests (R05m). Four need inputs that Git does not track:
  `evo_cli.rs`, `returns_twice_calls.rs` and `interpreter_main_arguments.rs` (`.build/ladder/` and
  `.evidence/`), and `n03f_frame_native.rs` on macOS (`.evidence/codegen/n03f/`); `ci.yml` skips
  these four. Three need the macOS 26 SDK: `clang_extension_refusals.rs`, `darwin_availability.rs`
  and `fixed_enum_underlying_types.rs`; the macOS jobs of `ci.yml` and `release.yml` now run on
  `macos-26`, which has that SDK.
- The daily nightly build is off until a hosted CI run passes (R05m).
- Only a direct call of an old-style definition takes the default argument promotions. A call
  through a function pointer object keeps the signature check E3019, and `(*f)(...)` and
  `(&f)(...)` keep the prototype check (N09, open item K4 of the old-style call repair).
- A call without a prototype of a function whose definition has a prototype is undefined
  behavior, and EvoCC does not report it (N09, open item K6 of the old-style call repair).

- LLVM `aarch64-acle-fmv-features` passed before 2026-10-25 and does not pass now. Its reference
  build finds compiler-rt and enables FMV, and the unchanged Clang 19 rejects its CSSC check, so
  the reference has no usable result (N09).
