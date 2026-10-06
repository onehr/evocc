# Evaluating EvoCC

How do we know that EvoCC works? Not from a feeling, and not from one green test. The project
builds real programs with EvoCC, runs their own tests, and writes down each result with the exact
compiler and inputs that produced it. This chapter shows how to read those results and how to run
them again. You need a source checkout for it.

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

The lanes run the tests in a sealed Linux image, `evo-flight-lab:sealed`, with no network. GCC 14.2
and Clang 19.1 in the same image are the reference compilers. On macOS, the image runs in the Apple
container runtime. `contracts/build-environment.json` pins the image, and `.toolchain/` holds the
files that describe and check it.

To run one lane, give its name:

<!-- verify after the driver change -->
```console
$ python3 tools/bench.py zlib --jobs 2
```

`--list` prints all lanes with what each one measures. `--dry-run` prints the commands that a run
would start, and `--evocc PATH` measures a compiler that you built elsewhere. A full run of all
lanes takes many hours. Start with one program.

## Retained results

Each lane keeps its last result. `--status` prints it without running anything:

```console
$ python3 tools/bench.py --status zlib cjson lua sqlite doom
zlib: retained PASS 19/19; run=20260919T231524Z; artifacts not revalidated
cjson: retained PASS 23/23; run=20260927T092953Z; artifacts not revalidated
lua: retained PASS 34/34; run=20260919T231634Z; artifacts not revalidated
sqlite: retained PASS 86/86; run=20260919T231713Z; artifacts not revalidated
doom: retained PASS 81/81; run=20260928T122910Z; artifacts not revalidated
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
  "driverSha256": "ff5f51e540abf40d9f660ade9193a5930ce2f9a274e18fe43d77b4f74c691882",
  "run": "20260919T231524Z"
}
$ jq '.test | {command, passed}' tests/contracts/ladder-zlib-receipt.json
{
  "command": "make test CFLAGS='-O3 -D_LARGEFILE64_SOURCE=1 -DHAVE_HIDDEN -no-pie'",
  "passed": true
}
```

The receipt names the source archive and its digest, the digest of the `evocc` that built it, and
the exact test command. Anyone can download the same archive, build it with the same compiler and
run the same command.

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

- **PASS** holds for the configuration, the objects and the test selection that the lane records.
  It does not mean that every configuration of the program works.
- **INCOMPLETE** means that some part did not finish or could not be judged. It is not a pass.
- **PROFILE_CLEAN** means that every case passed or has a recorded, evidenced reason to be left
  out. It is not a pass of every program.
- A skip, a timeout and a missing result are never a pass.

With these words, here is the state of EvoCC on 2026-10-06:

- Linux 6.18.44 boots with all 2,799 target C files compiled by EvoCC. A second configuration
  boots with 7,008 files and 848 modules.
- Ten of fifteen application lanes pass. Two Doom demos give the same frame, by hash, for each of
  350 frames under EvoCC, GCC 14 and Clang 19.
- The proof-required sweep checked 107 of 107 small integer functions to the final assembly text.
  Two more functions, optimized with `-Oeq-scalar`, were checked too. A native run of the checked
  code found no difference from the Lean model of the source in 14,194 comparisons.
- The code is 2.0 to 2.7 times the size of GCC 14 `-O2` code on five library files. The checked
  rewrites save 0.8 to 2.4 percent. The project makes no claim about speed.

The [evidence document](https://github.com/onehr/evocc/blob/main/docs/evidence.md) keeps every
qualifier of these numbers, and the
[dashboard](https://github.com/onehr/evocc/blob/main/docs/status.md) shows the current receipts.

## The showcase

The README of the project opens with a gallery: images that programs built by EvoCC produced, from
a Doom frame to a Mandelbrot set computed by Lua. Each image links to its receipt, and the README
gives the command that reproduces it. For example, the zlib image shows six sprites that the
EvoCC-built zlib compressed and restored without a changed pixel.
