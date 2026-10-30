# Evaluating EvoCC

EvoCC builds real programs, runs their own tests and records each result with the exact compiler
and inputs that produced it. To run the tests again, you need a source checkout.

## The ladder

`tools/bench.py` runs the evaluation. Its tests form a ladder, from small programs to a whole
operating system. Each rung is a *lane*:

| Group | Lanes |
|---|---|
| Conformance | `ctest` (c-testsuite), `torture` (the GCC torture tests), `csmith` (random programs), `llvm-test-suite` |
| Programs | `zlib`, `zstd`, `cjson`, `mbedtls`, `tinycc`, `toybox`, `musl`, `polybench`, `jq`, `lua`, `sqlite`, `doom` |
| Linux | `kernel`, `kernel-full`, `kernel-selftest`, `kernel-selftest-all`, `kernel-kunit` |

A program lane downloads a pinned release of the program and checks its digest. It builds the
program with EvoCC and runs the program's own tests. A lane does not change the tests of the
program to make them pass.

The lanes run the tests in a sealed Linux image, `evo-nexlab:sealed`, with no network. GCC 14.2
and Clang 19.1 in the same image are the reference compilers. On macOS, the image runs in the Apple
container runtime. `contracts/build-environment.json` pins the image, and `.toolchain/` holds the
files that describe and check it.

To run one lane, give its name:

```console
$ python3 tools/bench.py zlib --jobs 2
```

`--list` prints all lanes with what each one measures. `--dry-run` prints the commands that a run
would start and each input of a kernel lane that does not exist yet. `--evocc PATH` measures a
compiler that you built elsewhere. A full run of all lanes takes many hours. Start with one program.

## Retained results

For a local Make project, `evo build` runs its configure script, if present,
then Make with EvoCC. Copy the cJSON 1.7.19 source tree of the `cjson` lane to
`cjson`, then run:

```console
$ make -C cjson clean > /dev/null
$ evo build cjson -- all test -j2
build exit 0; log: $PWD/cjson/evo-build.log
```

The log path follows your directory; `$PWD` stands for its absolute path.
`cjson/evo-build-receipt.json` records the build
([fields](agents.md#command-records)). It is not a proof or a frozen ladder
result.

Each lane keeps its last result. `--status` prints it without running anything:

```console
$ python3 tools/bench.py --status zlib cjson lua sqlite doom
zlib: retained PASS 19/19; run=20261027T114544Z; artifacts not revalidated
cjson: retained PASS 23/23; run=20261027T114729Z; artifacts not revalidated
lua: retained PASS 34/34; run=20261027T121740Z; artifacts not revalidated
sqlite: retained PASS 86/86; run=20261027T121832Z; artifacts not revalidated
doom: retained PASS 81/81; run=20261027T122636Z; artifacts not revalidated
```

`19/19` means that EvoCC compiled all 19 translation units of zlib, and the tests passed. Note the
words "artifacts not revalidated". A retained result is a record of a past run, not a new run.

## Receipts

Each result has a *receipt*: a JSON file in `tests/contracts/` that binds the result to its
compiler, its inputs and its limits. Here is the receipt of the zlib lane:

```console
$ jq '{subject, summaryLine, url, tarballSha256, target, driverSha256, run}' tests/contracts/ladder-zlib-receipt.json
{
  "subject": "zlib",
  "summaryLine": "zlib: 19 of 19 units by EvoCC in a passing build; refused by code {}",
  "url": "https://zlib.net/zlib-1.3.2.tar.gz",
  "tarballSha256": "bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16",
  "target": "aarch64-unknown-linux-gnu",
  "driverSha256": "ab0c90437192d6824ce884c71d80a028e1d785ae89a11fe58ecb0145d76de525",
  "run": "20261027T114544Z"
}
$ jq '.test | {command, passed}' tests/contracts/ladder-zlib-receipt.json
{
  "command": "make test CFLAGS='-O3 -D_LARGEFILE64_SOURCE=1 -DHAVE_HIDDEN -no-pie'",
  "passed": true
}
```

With these fields, anyone can download the same archive, build it with the same compiler and run
the same command.

## Kernel acceptance inputs

The full kernel lanes read trees and payloads that you build first. Three tools in `tools/k1` make
them in the sealed image, from the Linux source on the case-sensitive volume:

```console
$ python3 tools/k1/configure.py --output CAPTURE/config
$ python3 tools/k1/reference.py --config CAPTURE/config/c6.config --output .build/case/final-s/pristine
$ python3 tools/k1/kunit_payload.py --reference-boot BOOT/boot.json --install-deps
```

`configure.py` solves the recorded configurations and compares each one with its recorded SHA-256.
`c2.config` is the KUnit configuration. `c6.config` is the selftest and BPF configuration.
`reference.py` builds the GCC reference kernel of one configuration. It stops if `olddefconfig`
changes that configuration. `kunit_payload.py` builds the KUnit runtime payload for each kernel of
a complete composition. `--dry-run` of the first two tools and `--check` of the third start nothing.

A replay with `--selftest-select` writes `tests/contracts/kernel-selftest-selected-receipt.json`.
It does not replace the receipt of the complete run.

## The dashboard

`docs/status.md` is the dashboard. A script generates it from the receipts, so nobody edits it by
hand. `python3 tools/render-status.py` writes it again, and `--ascii` prints a terminal view. The
check mode compares the file with the receipts and exits with 0 if they match:

```console
$ python3 tools/render-status.py --check > /dev/null; echo $?
0
```

## Read the results honestly

Each word in a result has a narrow meaning:

- `PASS` holds for the configuration, the objects and the test selection that the lane records.
  It does not mean that every configuration of the program works.
- `INCOMPLETE` means that some part did not finish or could not be judged. It is not a pass.
- `PROFILE_CLEAN` means that every case passed or has a recorded, evidenced reason to be left
  out. It is not a pass of every program.
- A skip, a timeout and a missing result are never a pass.

With these words, here is the state of EvoCC on 2026-10-27. The release compiler of commit
`8177cff2` measured each item that gives no other date:

- Linux 6.18.44 boots with all 2,799 target C files compiled by EvoCC. A second configuration
  booted with 7,008 files and 848 modules on 2026-09-28 (EvoCC `ab2eeb7b…`).
- Ten of fifteen application lanes pass, and the other five have no compiler failure. Two Doom
  demos give the same frame, by hash, for each of 350 frames under EvoCC, GCC 14 and Clang 19.
- On 2026-10-06 (commit `e85f5904`), the proof-required sweep checked 107 of 107 small integer
  functions to the final assembly text. Two more functions, optimized with `-Oeq-scalar`, were
  checked too. A native run of the checked code found no difference from the Lean model of the
  source in 14,194 comparisons.
- The code is 1.8 to 2.3 times the size of GCC 14 `-O2` code on five library files. On
  2026-10-01 (commit `5a21d523`), the checked rewrites saved 0.8 to 2.4 percent. The project makes
  no claim about speed.

The [evidence document](https://github.com/onehr/evocc/blob/main/docs/evidence.md) keeps every
qualifier of these numbers, and the
[dashboard](https://github.com/onehr/evocc/blob/main/docs/status.md) shows the current receipts.

## The showcase

The [gallery of the README](https://github.com/onehr/evocc/blob/main/README.md#what-it-compiles-correctly)
shows images that programs built by EvoCC produced, from a Doom frame to a Mandelbrot set computed
by Lua. Each image links to its receipt or to its capture notes. The
[capture notes](https://github.com/onehr/evocc/blob/main/docs/images/showcase/README.md#reproduce)
give the commands that reproduce the images. For example, the zlib image shows six sprites that the
EvoCC-built zlib compressed and restored without a changed pixel.
