# Changelog

All notable changes to EvoCC are listed in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] - Unreleased

The first public version.

### Added

- A C compiler for AArch64, written in Rust: its own preprocessor, parser, three intermediate
  representations (C-HIR, EvoIR and EvoMIR), optimizer, register allocator and assembly emitter.
  The platform assembler and linker build the final program.
- Two interpreters, one for C-HIR and one for EvoIR, that stop at undefined behavior with a
  numbered diagnostic (`--interpret-c-hir`, `--interpret-evoir`).
- `--check`, which runs the executed path and reports undefined behavior; `--explain` for each
  diagnostic code; `--error-format=json` for editors and tools.
- Undefined behavior as data in EvoIR: each definedness condition is an explicit
  `ub.assume.<reason>` obligation, and EvoMIR lowers it to a checked trap.
- Verified equality saturation (`-Oeq-scalar`): 62 rewrite rules (55 in `p0.scalar.v4`, 7 in
  `g0.ub.v1`), each proved in Lean 4 for every bit width. A separate, pinned checker,
  `evocc-checker`, replays each certificate before EvoCC writes output.
- Compilation with proof (`--proof-required`) for the integer subset `core-v1`: five compiled Lean
  checkers in `evoir-sem` check each stage of the actual compilation, and the theorem
  `checkPipeline_sound` covers the chain from typed Core source to the final assembly text. A
  refusal writes no output.
- Four pinned specifications in `spec/`: EvoIR 2.0.0, EvoMIR 1.0.0, the target description 1.0.0
  and the verification contract 1.0.0.
- The evaluation ladder, `tools/bench.py`: conformance suites, fifteen application lanes and Linux
  build, boot, kselftest and KUnit lanes. Each lane writes a receipt.
- The EvoCC Book in `book/`.
- The Apache License 2.0 (copyright 2026 Haoran Wang, NexRidge), a `NOTICE` file and third-party
  notices.

### Changed

- The project license changed from the placeholder GPL-2.0-only to Apache-2.0.
- The book replaces the implementation notebook in `docs/notebook/` and its web site in `web/`.
  The dated lab notes stay in `docs/archive/notebook/`.

### Results at this version

These results were recorded between 2026-09-13 and 2026-10-06. [docs/evidence.md](docs/evidence.md)
keeps their limits.

- Linux 6.18.44 ARM64 boots with 2,799 of 2,799 target C objects compiled by EvoCC. A second
  configuration boots with 7,008 objects and 848 modules.
- Ten of fifteen application lanes pass their recorded selection, among them zlib, zstd, cJSON,
  Mbed TLS, TinyCC, jq, Lua, SQLite and PolyBench/C. Two Doom demos give the same framebuffer
  hash for each of 350 frames under EvoCC, GCC 14 and Clang 19.
- The proof-required sweep checked 107 of 107 pilot entries to the final assembly text. A native
  run compared 14,194 vectors with the Lean source interpreter and found no difference.
- On five library translation units, the code is 2.0 to 2.7 times the size of GCC 14 `-O2`
  output.
