<p align="center">
  <img src="docs/images/mascot.png" alt="EvoCC mascot: a chameleon holding a wrench" width="180">
</p>

<h1 align="center">EvoCC</h1>

<p align="center">
  <b>A C compiler that shows its work.</b><br>
  Built by agents. Built for agents.
</p>

<p align="center">
  <a href="https://github.com/onehr/evocc/actions/workflows/ci.yml"><img src="https://github.com/onehr/evocc/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <a href="https://github.com/onehr/evocc/releases"><img src="https://img.shields.io/github/v/release/onehr/evocc?include_prereleases&label=release" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="Apache-2.0 license"></a>
</p>

EvoCC is a C compiler for AArch64 Linux and macOS, written from scratch in Rust. Use it in place of
`cc`. When your program breaks a rule of C on a path that it runs, `evo check` shows you the line and
the reason. EvoCC can also prove, with Lean 4, that a compilation is correct.

<p align="center">
  <img src="docs/images/readme/highlights.svg" alt="Four terminal sessions. evocc builds zlib, and zlib passes its tests. evo check finds a signed overflow in a binary search. GCC 14 with UBSan gives no report and the program returns a wrong value, but evo check finds the uninitialized read. evo prove checks the compilation of abs, and the checked assembly traps on overflow.">
</p>

## Highlights

- **A drop-in `cc`.** Existing Makefiles build with `CC=evocc`. CMake projects use
  [`cmake/evocc.cmake`](cmake/evocc.cmake).
- **Real software.** EvoCC builds zlib, SQLite, Lua, jq, Mbed TLS, Doom, three games and a Linux
  kernel that boots.
- **Undefined behavior, with evidence.** `evo check` runs your program on the rules of C. It stops
  at the first operation that breaks one and shows the line.
- **Catches what UBSan misses.** Uninitialized reads, dangling pointers and out-of-bounds accesses
  that GCC's warnings and UBSan do not report.
- **Every error explained.** Each diagnostic has a stable code, a tested explanation
  (`evo explain E3001`) and a JSON form.
- **Proof-carrying compilation.** `evo prove` writes assembly only when Lean 4 checks every stage of
  the compilation.
- **Made for agents.** JSON output, safe automatic repairs and an MCP server. On bugs that GCC and
  UBSan miss, a coding agent needs about a third fewer steps with EvoCC.

## Install

On macOS with Apple silicon:

```sh
curl -LO https://github.com/onehr/evocc/releases/download/v0.1.0/evocc-0.1.0-aarch64-apple-darwin.tar.gz
tar xzf evocc-0.1.0-aarch64-apple-darwin.tar.gz
export PATH="$PWD/evocc-0.1.0-aarch64-apple-darwin/bin:$PATH"
```

On Linux (AArch64), use `aarch64-unknown-linux-gnu` in the file name. To build from source, see
[Installation](book/src/install.md).

## Quick start

The example programs are in [`examples/`](examples/).

### Compile, as with `cc`

```console
$ evocc hello.c -o hello && ./hello
Hello, world!
$ make CC=evocc
```

### Find a famous bug

[`first_bad.c`](examples/first_bad.c) has the binary-search bug that broke Java's
`Arrays.binarySearch` in 2006:

```c
int mid = (lo + hi) / 2;
```

GCC 14 and Apple Clang compile it with `-O2 -Wall -Wextra` and give no warning. The GCC program
does not finish. The Clang program stops with a trap. `evo check` shows the cause:

```console
$ evo check first_bad.c
error[E3004]: signed-integer-overflow
  --> first_bad.c:9:19
   |
 9 |         int mid = (lo + hi) / 2;
   |                   ^^^^^^^^^ signed-integer-overflow
   |
   = note: this operation failed on the executed path
```

Change the line to `int mid = lo + (hi - lo) / 2;`, and the check passes.

### See what UBSan misses

In [`mean.c`](examples/mean.c), the declaration `struct stats s;` gives `count` and `total` no
value. GCC with `-Wall -Wextra -fsanitize=undefined` gives no warning and no report. The program
returns 0 at `-O0`. At `-O2`, it returns a wrong value that changes from run to run. The right
answer is 4.

```console
$ evo check mean.c
error[E3001]: read-of-uninitialized-object
  --> mean.c:7:9
   |
 7 |         s->count += 1;
   |         ^^^^^^^^^^^^^ read-of-uninitialized-object
   |
   = note: this operation failed on the executed path
$ evo explain E3001 | head -n 3
# E3001: read-of-uninitialized-object

The executed path reads object bytes that have not been initialized.
```

### Prove a compilation

```console
$ evo prove abs.c -o abs.s
checked from typed Core source to final assembly text: …/checked/record.json
```

Lean 4 checks each stage of this compilation, from the typed C source to the assembly text. If a
check fails, `evo prove` writes no output. `abs(INT_MIN)` has no valid result in C, so the checked
assembly traps instead of returning a wrong value:

```asm
    subs w0, w2, w0
    b.vs .Labs_block4
    ...
.Labs_block4:
    brk #3004
```

`subs` computes `0 - a` and sets the overflow flag. On overflow, `b.vs` jumps to the trap.

### Give it to your agent

```console
$ claude mcp add evo -- evo mcp
```

The `evo` commands also write JSON with `--json`. In a
[registered experiment](book/src/evaluation-agents.md#experiment-5-repair-effort-and-program-size),
a coding agent fixed bugs that GCC's warnings and UBSan miss with about a third fewer steps and
tokens when it used EvoCC. [Agents](book/src/agents.md) shows the setup for Claude Code and Codex.

## What it compiles correctly

EvoCC compiled each program below. Each caption names the check, and each results link opens its
record.

<table>
  <tr>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/doom.png"><img src="docs/images/showcase/doom.png" alt="Real Freedoom demo frame rendered by EvoCC-built Doom" width="320"></a><br><b>Doom / Freedoom</b><br>Headless replay<br><a href="tests/contracts/ladder-doom-receipt.json">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/tinycc.svg"><img src="docs/images/showcase/tinycc.svg" alt="Reflective spheres ray-traced by a C program compiled with EvoCC-built TinyCC" width="320"></a><br><b>TinyCC paints reflections</b><br>Compiles a ray tracer<br><a href="tests/contracts/ladder-tinycc-receipt.json">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/lua.svg"><img src="docs/images/showcase/lua.svg" alt="Color Mandelbrot image computed by EvoCC-built Lua" width="320"></a><br><b>Lua explores a fractal</b><br>Floating-point pixels<br><a href="tests/contracts/ladder-lua-receipt.json">results</a></td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/sqlite.svg"><img src="docs/images/showcase/sqlite.svg" alt="End of the SQLite veryquick test log: 0 errors out of 329803 tests" width="320"></a><br><b>SQLite</b><br>0 errors in 329,803 tests<br><a href="tests/contracts/ladder-sqlite-receipt.json">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/jq.svg"><img src="docs/images/showcase/jq.svg" alt="jq make check log: all nine test programs pass" width="320"></a><br><b>jq</b><br>libjq passes make check<br><a href="tests/contracts/ladder-jq-receipt.json">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/cjson.svg"><img src="docs/images/showcase/cjson.svg" alt="cJSON test program output: the example JSON, parsed and printed" width="320"></a><br><b>cJSON</b><br>216 unit tests, 0 failures<br><a href="tests/contracts/ladder-cjson-receipt.json">results</a></td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/linux.svg"><img src="docs/images/showcase/linux.svg" alt="Serial console of the EvoCC-built Linux kernel: it starts 4 CPUs, runs init and powers down" width="320"></a><br><b>Linux 6.18</b><br>7,008 C files; boots on 4 CPUs<br><a href="tests/contracts/kernel-bpf-full-receipt.json">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/toybox.svg"><img src="docs/images/showcase/toybox.svg" alt="End of the Toybox test log: PASS lines for xxd and zcat" width="320"></a><br><b>Toybox</b><br>1,669 tests, same results as GCC<br><a href="tests/contracts/ladder-toybox-receipt.json">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/mbedtls.svg"><img src="docs/images/showcase/mbedtls.svg" alt="Actual AES-GCM plaintext and ciphertext bytes with verified recovery and tamper rejection" width="320"></a><br><b>Mbed TLS seals pixels</b><br>Encrypt, restore, reject<br><a href="tests/contracts/ladder-mbedtls-receipt.json">results</a></td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/zstd.svg"><img src="docs/images/showcase/zstd.svg" alt="Real Lua fractal compressed and restored with Zstandard, with measured byte sizes" width="320"></a><br><b>Zstandard packs a fractal</b><br>Identical pixels<br><a href="tests/contracts/ladder-zstd-receipt.json">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/zlib.svg"><img src="docs/images/showcase/zlib.svg" alt="Six colorful arcade sprites compressed by zlib and restored pixel-perfect, with measured byte sizes" width="320"></a><br><b>zlib ships a tiny arcade</b><br>Sprites restored intact<br><a href="tests/contracts/ladder-zlib-receipt.json">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/polybench.svg"><img src="docs/images/showcase/polybench.svg" alt="Heatmap of the actual PolyBench FDTD-2D magnetic field after twenty steps" width="320"></a><br><b>PolyBench models a field</b><br>FDTD-2D simulation<br><a href="tests/contracts/ladder-polybench-receipt.json">results</a></td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/opentyrian.png"><img src="docs/images/showcase/opentyrian.png" alt="Actual scrolling-shooter replay frame from EvoCC-built OpenTyrian" width="320"></a><br><b>OpenTyrian</b><br>Three frames match GCC<br><a href="docs/images/showcase/README.md#three-native-c-games">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/neverball.png"><img src="docs/images/showcase/neverball.png" alt="Actual Neverball level rendered by the EvoCC-built game with a disclosed material-macro fix" width="320"></a><br><b>Neverball</b><br>One disclosed source fix<br><a href="docs/images/showcase/README.md#three-native-c-games">results</a></td>
    <td width="33%" align="center" valign="top"><a href="docs/images/showcase/quakespasm.png"><img src="docs/images/showcase/quakespasm.png" alt="Real LibreQuake demo replay rendered by EvoCC-built Quakespasm" width="320"></a><br><b>Quakespasm / LibreQuake</b><br>3D demo replay<br><a href="docs/images/showcase/README.md#three-native-c-games">results</a></td>
  </tr>
</table>

Each image is real program output, a plot of it, or an excerpt of a recorded test log. See the
[capture notes and credits](docs/images/showcase/README.md) and the
[compatibility table](book/src/compatibility.md).

## How it works

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/readme/pipeline-dark.svg">
    <img src="docs/images/readme/pipeline-light.svg" alt="C source, C-HIR, EvoIR, EvoMIR and AArch64 assembly; evo check runs on C-HIR; Lean 4 checks every stage for evo prove" width="860">
  </picture>
</p>

- **C-HIR** keeps the meaning of C: types, evaluation order, which bytes are initialized and what
  each pointer may point to. `evo check` runs at this level.
- **EvoIR** turns the undefined-behavior rules of C into explicit checks. An optimization can
  remove work, but never a check.
- **EvoMIR** treats the machine as data. Registers, calling conventions and instruction encodings
  are tables in [`targets/aarch64/`](targets/aarch64/).
- **Lean 4** proves the 62 rewrite rules of the optimizer. For a subset of C, five Lean checkers
  compare the input and the output of each stage, and one theorem covers the whole chain.

AI agents wrote EvoCC in about two months, under the direction of one engineer.
[The paper](docs/paper.md) tells that story and describes the design and the evaluation.

## Status

EvoCC 0.1.0 is experimental.

- It targets AArch64 only, on Linux and macOS.
- Its code is 2 to 2.7 times the size of GCC `-O2` output, and it is not tuned for speed.
- `evo check` runs programs that do not call the C library.
- Proofs cover a subset of C: integer functions without pointers, arrays or recursion.
- GNU C support is broad but not complete.

## Learn more

- [The EvoCC Book](book/src/SUMMARY.md): installation, a first program, undefined behavior, proofs
  and agents.
- [The paper](docs/paper.md): how agents built EvoCC, its design and its evaluation.
- [Specifications](spec/README.md): EvoIR, EvoMIR, the target tables and the verification contract.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). To report wrong code, follow [SECURITY.md](SECURITY.md).

## License

Apache License 2.0. Copyright 2026 Haoran Wang, NexRidge. See [LICENSE](LICENSE), [NOTICE](NOTICE)
and [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
