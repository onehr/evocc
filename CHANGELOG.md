# Changelog

All notable changes to EvoCC are listed in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] - Unreleased

The first public version.

### Added

- A preregistered 30-program agent repair experiment with one model, three feedback
  conditions and three repetitions. All 270 outcomes and full transcripts are retained,
  including response failures. EvoCC feedback passed 87/90 attempts, GCC plus UBSan
  88/90, and Clang plus UBSan 82/90; all successful repairs took one round. The report
  makes no general repair advantage claim.
- A C compiler for AArch64, written in Rust: its own preprocessor, parser, three intermediate
  representations (C-HIR, EvoIR and EvoMIR), optimizer, register allocator and assembly emitter.
  The platform assembler and linker build the final program.
- A drop-in `cc` driver. `evocc` takes several inputs and one action (`-E`, `-S`, `-c`, or a
  link), `-o`, `-x`, the usual preprocessor options and the `-M` family of dependency options. It
  runs the platform assembler and linker and passes `-l`, `-L`, `-Wl,`, `-shared` and `-static` to
  the linker. An unknown `-f` or `-m` option prints one warning, and the build continues. With
  `CC=evocc`, cJSON 1.7.19 and zlib 1.3.2 build and pass their own tests on macOS.
- The target `aarch64-apple-darwin`, the default on macOS. It emits Mach-O assembly and uses the
  Apple variadic convention: every anonymous argument goes on the stack, and `va_list` is one
  pointer. `aarch64-unknown-linux-gnu` is the default on Linux. On macOS, the sealed container
  can assemble Linux objects. Cross-linking requires an explicit `--linker=COMMAND`.
- `--version`, which prints the version, the commit and the date of the build, and the target.
  `-dumpmachine`, `-dumpversion` and `--help` are also available.
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
  The file-hash contract records 3,966 headers and eight unsupported netfilter headers that
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
- Five pinned specifications in `spec/`: EvoIR 2.0.1, EvoMIR 1.0.1, the target description 1.0.0,
  the verification contract 1.0.0 and the diagnostic format 1.0.0.
- The evaluation ladder, `tools/bench.py`: conformance suites, fifteen application lanes and Linux
  build, boot, kselftest and KUnit lanes. Each lane writes a receipt.
- The EvoCC Book in `book/`: installation, a first program, undefined behavior, compiling with
  proof, checked optimization, how EvoCC works and the evaluation. A workflow builds it with
  mdBook and publishes it to GitHub Pages.
- The Apache License 2.0 (copyright 2026 Haoran Wang, NexRidge), a `NOTICE` file and third-party
  notices.

### Changed

- `evo prove` can find the Lean checker from a source checkout without an override.
  Its JSON record names the chosen checker; demo scripts use the same discovery path.
- The project license changed from the placeholder GPL-2.0-only to Apache-2.0.
- The book replaces the hand-written implementation notebook and its web site. The dated lab notes
  stay in `docs/archive/notebook/`.
- Darwin stack-protector lowering uses Mach-O symbols and relocations. Angle includes honor
  supplied include paths; quoted includes still search the source file's directory first.

### Results at this version

These results were recorded between 2026-09-13 and 2026-10-07. [docs/evidence.md](docs/evidence.md)
keeps their limits.

- The recorded agent repair loop reaches checked assembly in 12.41 seconds in the final
  workspace gate. Its refusal demo returns the three required failures without publishing
  assembly. Paired JSON/proof-record tests pass three consecutive runs and the final gate;
  both the full offline workspace and Lean gates pass on clean commit `200b4e73`.
- Linux 6.18.44 ARM64 boots with 2,799 of 2,799 target C objects compiled by EvoCC. A second
  configuration boots with 7,008 objects and 848 modules.
- Ten of fifteen application lanes pass their recorded selection, among them zlib, zstd, cJSON,
  Mbed TLS, TinyCC, jq, Lua, SQLite and PolyBench/C. Two Doom demos give the same framebuffer
  hash for each of 350 frames under EvoCC, GCC 14 and Clang 19.
- The proof-required sweep checked 107 of 107 pilot entries to the final assembly text. A native
  run compared 14,194 vectors with the Lean source interpreter and found no difference.
- On five library translation units, the code is 2.0 to 2.7 times the size of GCC 14 `-O2`
  output.
- A release unpacked on the default macOS volume compiles a Linux hello object and the original
  zlib 1.3.2 archive without a sysroot flag or manual include paths. Hello, the zlib example and
  a minigzip round trip pass in the sealed Linux environment. The cross-host release workflow
  is configured; these local results do not claim a hosted GitHub Actions run.
- Original CMake builds on macOS pass cJSON 1.7.19's 22 tests and zlib 1.3.2's 16 default
  CTest commands. The zlib coverage-summary command reports a missing .gcno file, but Apple's
  gcov returns 0. Its passing exit does not establish coverage support. No test was excluded.
- The final product gate on clean `95466ea0` passes 3,225 Rust tests with zero failures and
  five existing ignored campaigns. Clippy and formatting pass. The existing Lean gate passes
  all six steps against EvoIR 2.0.1; this check does not expand the verified language subset.
