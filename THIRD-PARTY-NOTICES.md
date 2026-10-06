# Third-party notices

EvoCC is licensed under the Apache License 2.0. Copyright 2026 Haoran Wang, NexRidge. See
[LICENSE](LICENSE) and [NOTICE](NOTICE).

Some files in this repository, and some code in the release binaries, come from other projects.
This file lists them. For each, it gives the path, the origin and the license. The license of the
other project applies to that material, not the license of EvoCC.

The release archives hold three binaries: `evocc`, `evocc-checker` and `evoir-sem`. Section 1
lists the third-party code inside them. Sections 2 to 5 list material that stays in the
repository and is not part of any binary.

## 1. Code in the release binaries

`evocc` and `evocc-checker` are Rust programs. Three crates are vendored in `vendor/`, and Cargo
builds them offline from there (`.cargo/config.toml`). Both binaries contain all three crates.

| Component | Path | Origin | License |
|---|---|---|---|
| `bitflags` 2.13.2 | [`vendor/bitflags/`](vendor/bitflags/) | crates.io; The Rust Project Developers | MIT OR Apache-2.0 ([MIT](vendor/bitflags/LICENSE-MIT), [Apache](vendor/bitflags/LICENSE-APACHE)) |
| `smallvec` 1.16.1 | [`vendor/smallvec/`](vendor/smallvec/) | crates.io; The Servo Project Developers | MIT OR Apache-2.0 ([MIT](vendor/smallvec/LICENSE-MIT), [Apache](vendor/smallvec/LICENSE-APACHE)) |
| `rustc_apfloat` 0.2.3+llvm-462a31f5a5ab | [`vendor/rustc_apfloat/`](vendor/rustc_apfloat/) | crates.io; <https://github.com/rust-lang/rustc_apfloat>, a port of the LLVM APFloat code | Apache-2.0 WITH LLVM-exception ([text](vendor/rustc_apfloat/LICENSE.txt), [history of the license](vendor/rustc_apfloat/LICENSE-DETAILS.md)) |

The build also links code that is not in this repository:

| Component | Where it comes from | In which binaries | License |
|---|---|---|---|
| Rust standard library | The pinned Rust toolchain (`rust-toolchain.toml`) | `evocc`, `evocc-checker` | MIT OR Apache-2.0 |
| musl libc | The Rust target `aarch64-unknown-linux-musl`, when a Linux release is built for it | `evocc`, `evocc-checker` on Linux | MIT |
| Lean 4 runtime and core library | The pinned Lean toolchain (`lean/lean-toolchain`, v4.32.2) | `evoir-sem` | Apache-2.0 |
| GNU MP (GMP) | The Lean toolchain, which links it into compiled Lean programs | `evoir-sem` | GNU LGPL version 3, as the toolchain's `LICENSES` file states |
| libuv | The Lean toolchain, which links it into compiled Lean programs | `evoir-sem` | MIT |

The Lean toolchain installs the full texts of these licenses in its `LICENSE` and `LICENSES`
files. The source of `evoir-sem` is [`lean/`](lean/). `(cd lean && lake build)` builds it again
and links it with the libraries of the toolchain, so you can link it with a changed GMP.

## 2. Arm instruction model

| Path | Origin | License |
|---|---|---|
| [`targets/aarch64/isa/prelude.sail`](targets/aarch64/isa/prelude.sail), [`builtins.sail`](targets/aarch64/isa/builtins.sail), [`integer.sail`](targets/aarch64/isa/integer.sail) | The Arm ASL-to-Sail model, <https://github.com/rems-project/sail-arm>, commit `1bf2e5574ba9d704639a28401b6a387dcb113cae`, directory `arm-v9.4-a/src`. `integer.sail` holds verbatim spans of the model. [`provenance.json`](targets/aarch64/isa/provenance.json) records each span and its SHA-256. | BSD-3-Clause-Clear. Copyright 2023 Arm Limited (or its affiliates), Thomas Bauereiss, Brian Campbell, Alasdair Armstrong, Alastair Reid, Peter Sewell. Text: [`targets/aarch64/isa/LICENSE`](targets/aarch64/isa/LICENSE). |

`boundary.sail` and `runtime.sail` in the same directory are EvoCC code. `tools/isa_model_slice.py`
and its tests read these files. No binary contains them.

## 3. Showcase images

The showcase in the README shows output of programs that EvoCC built. Three images show game data
from other projects. [`docs/images/showcase/README.md`](docs/images/showcase/README.md#artwork)
describes each capture.

| Path | Origin | License |
|---|---|---|
| [`docs/images/showcase/doom.png`](docs/images/showcase/doom.png) | A frame of Freedoom 0.13.0 Phase 1, rendered by doomgeneric | BSD-3-Clause ([copying](docs/images/showcase/FREEDOOM-COPYING.txt), [credits](docs/images/showcase/FREEDOOM-CREDITS.txt)) |
| [`docs/images/showcase/quakespasm.png`](docs/images/showcase/quakespasm.png) | A frame of a LibreQuake demo, rendered by Quakespasm | BSD-3-Clause ([copying](docs/images/showcase/LIBREQUAKE-COPYING.txt), [credits](docs/images/showcase/LIBREQUAKE-CREDITS.txt)) |
| [`docs/images/showcase/neverball.png`](docs/images/showcase/neverball.png) | A Neverball level | GPL-2.0-or-later, with the exceptions in [`NEVERBALL-LICENSE.md`](docs/images/showcase/NEVERBALL-LICENSE.md) ([GPL text](docs/images/showcase/NEVERBALL-COPYING.txt)) |
| [`docs/images/showcase/opentyrian.png`](docs/images/showcase/opentyrian.png) | A frame of the Tyrian 2.1 freeware data, rendered by OpenTyrian | Freeware game data. EvoCC does not relicense it. |

The other images in `docs/images/` and the text captures beside them are EvoCC material. The names
of the games and projects belong to their owners. The captures do not imply an endorsement.

## 4. Test inputs

Files in `tests/fixtures/` and `tests/contracts/` are test inputs and records. They are not part of
the compiler binaries, and the release archives do not contain them.

### 4.1 Functions quoted with their license notice

| Path | Origin | License |
|---|---|---|
| [`tests/fixtures/verification/doom-fixedmul.c`](tests/fixtures/verification/doom-fixedmul.c) | `FixedMul` from doomgeneric `doomgeneric/m_fixed.c`, commit `dcb7a8dbc7a16ce3dda29382ac9aae9d77d21284`. Copyright 1993-1996 Id Software, Inc. and 2005-2014 Simon Howard. | GPL-2.0-or-later |
| [`tests/fixtures/verification/musl-abs.c`](tests/fixtures/verification/musl-abs.c) | `abs` from musl 1.2.6 `src/stdlib/abs.c`. Copyright 2005-2020 Rich Felker, et al. | MIT |

[`tests/contracts/core-v1-pilot.json`](tests/contracts/core-v1-pilot.json) records the source
digests and the licenses of both functions. The compiler's exports of these functions (typed
source, EvoIR, EvoMIR and assembly) are in `tests/fixtures/verification/pipeline/`,
`tests/fixtures/verification/asm/` and `tests/fixtures/verification/rewrite/`, and some Lean test
fixtures in `lean/EvoIR/` contain them too. They derive from the same two functions.

### 4.2 Linux kernel test code

| Path | Origin | License |
|---|---|---|
| `tests/fixtures/hw-breakpoint-kernel-task.c`, `kernel-neon-module-values.c`, `pm-runtime-barrier-completion.c`, `sched-static-key-lifetime.c`, `sched-static-key-phases.c`, `static-key-immediate.c` | Linux kernel modules written for EvoCC to diagnose kernel test results. They build only against Linux. | GPL-2.0-only, as the SPDX line of each file states |
| `tests/fixtures/bpf-runtime-contract.c` | A Linux kernel module of the same kind | Declares `MODULE_LICENSE("GPL")` |
| `tests/fixtures/ublk-background-exit.sh` | A reduction of the Linux kselftest `tools/testing/selftests/ublk/test_stress_04.sh` | GPL-2.0, as its SPDX line states |
| `tests/fixtures/day5-kernel-reductions/overlay/candidates/` | Line-level reductions of Linux 6.18.44 sources (`include/linux/compiler-version.h`, `compiler_types.h`, `kconfig.h`, `lib/bitrev.c` and preprocessed parser inputs). `overlay.json` records the source line of each kept line. | GPL-2.0, the license of the Linux kernel |

### 4.3 Other reductions

Many other fixtures are short reductions of an upstream test or program. The header comment of
each file names its source. Where such a file keeps upstream text, that text stays under the
license of the upstream project:

| Upstream | License of the upstream project | Example fixtures |
|---|---|---|
| Linux kernel and its kselftests | GPL-2.0 | `bitmap-constant-mutation.c`, `cgroup-percpu-accounting.c`, `assembly-static-key-toggle.c` |
| musl and libc-test | MIT | `musl-hex-tail-reduction.c`, `assembly-exclusive-window.c`, `descriptor-exit-filesystem.c` |
| doomgeneric | GPL-2.0-or-later | `doom-angle-wrap.c` |
| Neverball | GPL-2.0-or-later | `neverball-material-conversion.c` |
| SQLite | Public domain | `switch-default-pointer-environment.c` |
| TinyCC | LGPL-2.1-or-later | `static-narrow-member-offset.c` |
| LLVM test-suite, GCC torture tests, PolyBench/C | See each upstream project | `aggregate-parameter-edges.c`, `llvm-vector-overflow-reduction.c`, `floating-hfa-varargs-alignment.c`, `repeated-pointer-index.c` |
| Programs generated by Csmith | Csmith output, reduced | `csmith-reductions/` |

`tests/fixtures/musl-powf-rounding-reduction.c` and `tests/fixtures/doom-headless.c` include
upstream headers or sources from a pinned upstream tree at build time. They do not copy them.

### 4.4 Toolchain records

`.toolchain/records/frozen-kernel-gcc.config` and `frozen-kernel-clang.config` are Linux kernel
configurations that the Linux 6.18.44 build generated. `sealed-package-manifest.tsv` and
`qemu-11.0.3-dylib-closure.txt` list package and library names, versions and digests. The other
files in `.toolchain/` are EvoCC code.

## 5. Programs that the evaluation downloads

`tools/bench.py` and `tools/ladder/` download pinned releases of zlib, zstd, cJSON, Mbed TLS,
TinyCC, toybox, musl, PolyBench/C, the LLVM test-suite, jq, Lua, SQLite, doomgeneric, Freedoom,
Linux and others into ignored build directories. This repository does not track their sources. Each
keeps its own license.
