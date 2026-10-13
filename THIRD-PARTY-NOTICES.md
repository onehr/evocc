# Third-party notices

EvoCC is licensed under the Apache License 2.0. Copyright 2026 Haoran Wang, NexRidge. See
[LICENSE](LICENSE) and [NOTICE](NOTICE).

Some files in this repository, some code in the release binaries and the Linux headers in the
release archives come from other projects. This file lists them. For each, it gives the path, the
origin and the license. The license of the other project applies to that material, not the
license of EvoCC.

The release archives hold four binaries: `evo`, `evocc`, `evocc-checker` and `evoir-sem`. An
archive can also hold Linux headers. Section 1 lists the third-party code inside the binaries.
Section 2 lists the Linux headers. Sections 3 to 6 list material that stays in the repository and
is not part of any binary or release archive.

## 1. Code in the release binaries

`evo`, `evocc` and `evocc-checker` are Rust programs. All three are binaries of the `evocc`
package in [`crates/evocc/`](crates/evocc/). Three crates are vendored in `vendor/`, and Cargo
builds them offline from there (`.cargo/config.toml`). Cargo links all three crates into `evocc`
and `evocc-checker`, through the `evocc-types` crate.

`evo` is the command program: `evo run`, `evo check`, `evo prove` and the other commands. It starts
`evocc` and `evoir-sem` to do the work. Its source is
[`crates/evocc/src/bin/evo.rs`](crates/evocc/src/bin/evo.rs) and the modules in
[`crates/evocc/src/bin/evo/`](crates/evocc/src/bin/evo/). It uses one other crate, `evocc-proof`,
which is EvoCC code. It contains none of the vendored crates.

| Component | Path | Origin | License |
|---|---|---|---|
| `bitflags` 2.13.2 | [`vendor/bitflags/`](vendor/bitflags/) | crates.io; The Rust Project Developers | MIT OR Apache-2.0 ([MIT](vendor/bitflags/LICENSE-MIT), [Apache](vendor/bitflags/LICENSE-APACHE)) |
| `smallvec` 1.16.1 | [`vendor/smallvec/`](vendor/smallvec/) | crates.io; The Servo Project Developers | MIT OR Apache-2.0 ([MIT](vendor/smallvec/LICENSE-MIT), [Apache](vendor/smallvec/LICENSE-APACHE)) |
| `rustc_apfloat` 0.2.3+llvm-462a31f5a5ab | [`vendor/rustc_apfloat/`](vendor/rustc_apfloat/) | crates.io; <https://github.com/rust-lang/rustc_apfloat>, a port of the LLVM APFloat code | Apache-2.0 WITH LLVM-exception ([text](vendor/rustc_apfloat/LICENSE.txt), [history of the license](vendor/rustc_apfloat/LICENSE-DETAILS.md)) |

The build also links code that is not in this repository:

| Component | Where it comes from | In which binaries | License |
|---|---|---|---|
| Rust standard library | The pinned Rust toolchain (`rust-toolchain.toml`) | `evo`, `evocc`, `evocc-checker` | MIT OR Apache-2.0 |
| musl libc | The Rust target `aarch64-unknown-linux-musl`, when a Linux release is built for it | `evo`, `evocc`, `evocc-checker` on Linux | MIT |
| Lean 4 runtime and core library | The pinned Lean toolchain (`lean/lean-toolchain`, v4.32.2) | `evoir-sem` | Apache-2.0 |
| GNU MP (GMP) | The Lean toolchain, which links it into compiled Lean programs | `evoir-sem` | GNU LGPL version 3, as the toolchain's `LICENSES` file states |
| libuv | The Lean toolchain, which links it into compiled Lean programs | `evoir-sem` | MIT |

The Lean toolchain installs the full texts of these licenses in its `LICENSE` and `LICENSES`
files. The source of `evoir-sem` is [`lean/`](lean/). `(cd lean && lake build)` builds it again
and links it with the libraries of the toolchain, so you can link it with a changed GMP.

## 2. Linux headers in the release archives

`evocc --target=aarch64-unknown-linux-gnu` reads the C headers of a Linux system.
`tools/export-sysroot.sh` exports them from the sealed build image, and `tools/release.sh` adds
them to an archive when the export exists. The binaries do not contain them.

The headers are in `sysroot/aarch64-unknown-linux-gnu/` of the archive: 3,966 files from 13
packages of Debian 13. They come from the image `evo-nexlab:sealed-20260810`, digest
`sha256:2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2`.
[`contracts/sysroot-aarch64-unknown-linux-gnu.json`](contracts/sysroot-aarch64-unknown-linux-gnu.json)
records the SHA-256 of each header and each license text, and the version of each package. The
archive also includes that file as `sysroot/aarch64-unknown-linux-gnu.json`.

The archive holds the Debian copyright file of each package in
`sysroot/licenses/aarch64-unknown-linux-gnu/<package>/copyright`. The copyright file gives the
full terms. Some files of a package have a license other than the main one. The license texts
that the copyright files cite from `/usr/share/common-licenses/` are in
`sysroot/licenses/aarch64-unknown-linux-gnu/common-licenses/`.

| Package | Version | Headers | Directories in `sysroot/aarch64-unknown-linux-gnu/` | License of the headers | Copyright file in `sysroot/licenses/aarch64-unknown-linux-gnu/` |
|---|---|---|---|---|---|
| `libc6-dev` (GNU C Library) | 2.41-12+deb13u3 | 471 | 107 files in `usr/include/`; `usr/include/aarch64-linux-gnu/` except `asm/`, `c++/` and `openssl/`; `arpa/`, `finclude/`, `net/`, `netinet/` and the other `net*/` directories, `nfs/`, `protocols/`, `rpc/`, three files in `scsi/` | LGPL-2.1-or-later. Some files have other terms. | `libc6-dev/copyright` |
| `libclang-common-19-dev` (Clang 19) | 1:19.1.7-3+b1 | 482 | `usr/include/clang/19/` and `usr/include/clang/19.1.7/`: the same 241 files in each | Apache-2.0 WITH LLVM-exception. 15 files in each directory carry an MIT license text. | `libclang-common-19-dev/copyright` |
| `libcrypt-dev` (libxcrypt) | 1:4.4.38-1 | 1 | `usr/include/crypt.h` | LGPL-2.1-or-later | `libcrypt-dev/copyright` |
| `libelf-dev` (elfutils) | 0.192-4 | 5 | `usr/include/libelf.h`, `gelf.h`, `nlist.h` and `elfutils/` | LGPL-3.0-or-later OR GPL-2.0-or-later | `libelf-dev/copyright` |
| `libgcc-14-dev` (GCC 14) | 14.2.0-19 | 34 | `usr/lib/gcc/aarch64-linux-gnu/14/include/` except `objc/` | GPL-3.0-or-later WITH GCC-exception-3.1. The five files in `sanitizer/` are Apache-2.0 WITH LLVM-exception. | `libgcc-14-dev/copyright` |
| `libobjc-14-dev` (GCC 14) | 14.2.0-19 | 10 | `usr/lib/gcc/aarch64-linux-gnu/14/include/objc/` | GPL-3.0-or-later WITH GCC-exception-3.1 | `libobjc-14-dev/copyright` |
| `libssl-dev` (OpenSSL 3.5) | 3.5.6-1~deb13u2 | 142 | `usr/include/openssl/` and `usr/include/aarch64-linux-gnu/openssl/` | Apache-2.0 | `libssl-dev/copyright` |
| `libstdc++-14-dev` (GCC 14) | 14.2.0-19 | 825 | `usr/include/c++/14/` and `usr/include/aarch64-linux-gnu/c++/` | GPL-3.0-or-later WITH GCC-exception-3.1. The 23 files in `pstl/` are Apache-2.0 WITH LLVM-exception. | `libstdc++-14-dev/copyright` |
| `libzstd-dev` (Zstandard) | 1.5.7+dfsg-1 | 3 | `usr/include/zstd.h`, `zdict.h` and `zstd_errors.h` | BSD-3-Clause OR GPL-2.0-only | `libzstd-dev/copyright` |
| `linux-libc-dev` (Linux 6.12 UAPI) | 6.12.101-1 | 1,966 | `usr/include/linux/`, `asm-generic/`, `drm/`, `misc/`, `mtd/`, `rdma/`, `regulator/`, `sound/`, `video/`, `xen/`, ten files in `scsi/`, and the `asm/` directory of 22 multiarch triples, `aarch64-linux-gnu` among them | GPL-2.0 WITH Linux-syscall-note, as most SPDX lines state. Some files have other or dual terms. | `linux-libc-dev/copyright` |
| `make` (GNU Make) | 4.4.1-2 | 1 | `usr/include/gnumake.h` | GPL-3.0-or-later | `make/copyright` |
| `rpcsvc-proto` | 1.4.3-1+b1 | 24 | `usr/include/rpcsvc/` | BSD-3-Clause | `rpcsvc-proto/copyright` |
| `zlib1g-dev` (zlib) | 1:1.3.dfsg+really1.3.1-1+b1 | 2 | `usr/include/zlib.h` and `zconf.h` | Zlib | `zlib1g-dev/copyright` |

## 3. Arm instruction model

| Path | Origin | License |
|---|---|---|
| [`targets/aarch64/isa/prelude.sail`](targets/aarch64/isa/prelude.sail), [`builtins.sail`](targets/aarch64/isa/builtins.sail), [`integer.sail`](targets/aarch64/isa/integer.sail) | The Arm ASL-to-Sail model, <https://github.com/rems-project/sail-arm>, commit `1bf2e5574ba9d704639a28401b6a387dcb113cae`, directory `arm-v9.4-a/src`. `integer.sail` holds verbatim spans of the model. [`provenance.json`](targets/aarch64/isa/provenance.json) records each span and its SHA-256. | BSD-3-Clause-Clear. Copyright 2023 Arm Limited (or its affiliates), Thomas Bauereiss, Brian Campbell, Alasdair Armstrong, Alastair Reid, Peter Sewell. Text: [`targets/aarch64/isa/LICENSE`](targets/aarch64/isa/LICENSE). |

`boundary.sail` and `runtime.sail` in the same directory are EvoCC code. `tools/isa_model_slice.py`
and its tests read these files. No binary contains them.

## 4. Showcase images

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

## 5. Test inputs

Files in `tests/fixtures/` and `tests/contracts/` are test inputs and records. They are not part of
the compiler binaries, and the release archives do not contain them.

### 5.1 Functions quoted with their license notice

| Path | Origin | License |
|---|---|---|
| [`tests/fixtures/verification/doom-fixedmul.c`](tests/fixtures/verification/doom-fixedmul.c) | `FixedMul` from doomgeneric `doomgeneric/m_fixed.c`, commit `dcb7a8dbc7a16ce3dda29382ac9aae9d77d21284`. Copyright 1993-1996 Id Software, Inc. and 2005-2014 Simon Howard. | GPL-2.0-or-later |
| [`tests/fixtures/verification/musl-abs.c`](tests/fixtures/verification/musl-abs.c) | `abs` from musl 1.2.6 `src/stdlib/abs.c`. Copyright 2005-2020 Rich Felker, et al. | MIT |

[`tests/contracts/core-v1-pilot.json`](tests/contracts/core-v1-pilot.json) records the source
digests and the licenses of both functions. The compiler's exports of these functions (typed
source, EvoIR, EvoMIR and assembly) are in `tests/fixtures/verification/pipeline/`,
`tests/fixtures/verification/asm/` and `tests/fixtures/verification/rewrite/`, and some Lean test
fixtures in `lean/EvoIR/` contain them too. They derive from the same two functions.

### 5.2 Linux kernel test code

| Path | Origin | License |
|---|---|---|
| `tests/fixtures/hw-breakpoint-kernel-task.c`, `kernel-neon-module-values.c`, `pm-runtime-barrier-completion.c`, `sched-static-key-lifetime.c`, `sched-static-key-phases.c`, `static-key-immediate.c` | Linux kernel modules written for EvoCC to diagnose kernel test results. They build only against Linux. | GPL-2.0-only, as the SPDX line of each file states |
| `tests/fixtures/bpf-runtime-contract.c` | A Linux kernel module of the same kind | Declares `MODULE_LICENSE("GPL")` |
| `tests/fixtures/ublk-background-exit.sh` | A reduction of the Linux kselftest `tools/testing/selftests/ublk/test_stress_04.sh` | GPL-2.0, as its SPDX line states |
| `tests/fixtures/day5-kernel-reductions/overlay/candidates/` | Line-level reductions of Linux 6.18.44 sources (`include/linux/compiler-version.h`, `compiler_types.h`, `kconfig.h`, `lib/bitrev.c` and preprocessed parser inputs). `overlay.json` records the source line of each kept line. | GPL-2.0, the license of the Linux kernel |

### 5.3 Other reductions

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

### 5.4 Toolchain records

`.toolchain/records/frozen-kernel-gcc.config` and `frozen-kernel-clang.config` are Linux kernel
configurations that the Linux 6.18.44 build generated. `sealed-package-manifest.tsv` and
`qemu-11.0.3-dylib-closure.txt` list package and library names, versions and digests. The other
files in `.toolchain/` are EvoCC code.

## 6. Programs that the evaluation downloads

`tools/bench.py` and `tools/ladder/` download pinned releases of zlib, zstd, cJSON, Mbed TLS,
TinyCC, toybox, musl, PolyBench/C, the LLVM test-suite, jq, Lua, SQLite, doomgeneric, Freedoom,
Linux and others into ignored build directories. This repository does not track their sources. Each
keeps its own license.
