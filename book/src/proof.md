# Compiling with Proof

Tests can show that a compiler works on the programs that you tried. They cannot show that it works
on the next one. For a small subset of C, EvoCC can do more. It checks the actual compilation, stage
by stage, with checkers that are proved in Lean. One theorem covers the whole chain, from the C
function to the final assembly text. If any check refuses, EvoCC writes no output.

This chapter compiles a function with proof, reads the record of the proof, and shows what a
refusal looks like.

## What you need

You need the Lean checker, `evoir-sem`. It is in the release archive. If you built EvoCC from
source, build it with Lake, as in [chapter 2](install.md):

```console
$ (cd lean && lake build 2>&1 | tail -n 1)
Build completed successfully (728 jobs).
```

You also need a function in the checked subset. Its name is `core-v1`. In short, it covers
integer functions:

- the types `int`, `long` and `long long`, signed and unsigned;
- arithmetic, bitwise, comparison, conditional and logical operators, and casts between these
  types;
- local variables, `if`, `while`, `do`, `for`, `break`, `continue` and `return`;
- direct calls to other functions in the subset, with no recursion.

Pointers, arrays, structures, global variables, floating point, `switch`, `goto` and calls to
outside functions are not in the subset. The
[verification contract, section 2](https://github.com/onehr/evocc/blob/main/spec/verification.md#2-the-checked-subset-core-v1)
gives the full list.

## Compile `abs` with proof

Make a file `abs.c`. It holds the same function as `abs` in musl, a C library:

```c
int abs(int a) {
    return a > 0 ? a : -a;
}
```

Compile it with proof:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm --proof-required=proof-abs \
    --proof-checker="$(command -v evoir-sem)" --core-entry=abs -O0 -fno-builtin abs.c
evocc: proof-required result: CHECKED_TO_FINAL_ASSEMBLY: checked from typed Core source to final assembly text; record proof-abs/record.json
// evocc-native-ub-checks=checked
.text

.globl abs
.p2align 2
.type abs, %function
abs:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
.Labs_block0:
    cmp w0, #0
    b.le .Labs_block2
.Labs_block1:
    mov w1, w0
    b .Labs_block3
.Labs_block2:
    movz w2, #0
    subs w0, w2, w0
    b.vs .Labs_block4
.Labs_block5:
    mov w1, w0
.Labs_block3:
    mov w0, w1
    ldp x29, x30, [sp], #16
    ret
.Labs_block4:
    brk #3004
.size abs, .-abs

.ident "EvoCC 0.0.0 aarch64-unknown-linux-gnu"
```

The options say what to check:

| Option | Meaning |
|---|---|
| `--proof-required=proof-abs` | Compile with proof, and write the proof files into the new directory `proof-abs`. |
| `--proof-checker=PATH` | Use this Lean checker. |
| `--core-entry=abs` | Check the function `abs`. |
| `--emit=asm`, `-O0`, `-fno-builtin` | The subset requires these options. `-fno-builtin` makes the call of `abs` mean this definition, not a built-in function. |
| `--target=aarch64-unknown-linux-gnu` | The subset is defined for this target. |

The first line of the output comes from standard error. It gives the result,
`CHECKED_TO_FINAL_ASSEMBLY`. The rest is the checked assembly.

Look at `subs` and `b.vs` again. For `a` equal to `INT_MIN`, `-a` overflows, so `abs(INT_MIN)`
jumps to `brk #3004` and stops. It cannot return a wrong number. The proof covers this: the checked
program behaves as the C source, and the C source says that this case is undefined behavior.

## Read the record

The directory `proof-abs` holds the inputs of each check, the log of each check, the published
assembly and the record:

```console
$ ls proof-abs proof-abs/inputs proof-abs/checks
proof-abs:
checks
inputs
output.s
record.json

proof-abs/checks:
allocation-abs.err
allocation-abs.out
frame-emission-abs.err
frame-emission-abs.out
lowering-abs.err
lowering-abs.out
pipeline-abs.err
pipeline-abs.out
source-to-evoir-abs.err
source-to-evoir-abs.out

proof-abs/inputs:
allocation.json
asm-ast.json
assembly.s
evoir.evo
post-ssa-evomir.json
source-abs.json
ssa-evomir.json
```

`record.json` is JSON. These commands use `jq` to read it. The top of the record gives the result:

```console
$ jq '{schema, result, scope, all_stages_checked, profile, target}' proof-abs/record.json
{
  "schema": "evocc-proof-required-record-v1",
  "result": "CHECKED_TO_FINAL_ASSEMBLY",
  "scope": "checked from typed Core source to final assembly text",
  "all_stages_checked": true,
  "profile": "core-v1",
  "target": "aarch64-unknown-linux-gnu"
}
```

The record has one row for each stage:

```console
$ jq -c '.stages[] | {id, status}' proof-abs/record.json
{"id":"source-to-evoir","status":"CHECKED"}
{"id":"rewrite","status":"NOT_CHECKED"}
{"id":"lowering","status":"CHECKED"}
{"id":"allocation","status":"CHECKED"}
{"id":"frame-emission","status":"CHECKED"}
{"id":"pipeline","status":"CHECKED"}
```

Each stage compares the input and the output of one part of the compiler:

| Stage | What the checker compares |
|---|---|
| `source-to-evoir` | The typed C source and the EvoIR |
| `rewrite` | Each step of the optimizer. It is `NOT_CHECKED` here, because we did not use `-Oeq-scalar`. |
| `lowering` | The EvoIR and the machine IR, EvoMIR |
| `allocation` | The EvoMIR before and after register allocation |
| `frame-emission` | The allocated EvoMIR and the assembly, byte for byte |
| `pipeline` | All stages together, for the one theorem |

Each accepted check wrote one line:

```console
$ jq -r '.stages[] | .results[]?.report' proof-abs/record.json
STAGE_ACCEPTED stage=source-to-evoir entry=abs route=leaf scope=source-to-evoir
STAGE_ACCEPTED stage=lowering entry=abs scope=evoir-to-post-ssa-mir
STAGE_ACCEPTED stage=allocation entry=abs functions=1 scope=post-ssa-mir-to-allocated-mir
STAGE_ACCEPTED stage=frame-emission entry=abs functions=1 bytes=493 scope=allocated-mir-to-assembly-text
PIPELINE_ACCEPTED entry=abs rewrites=0 functions=1 scope=source-to-assembly-text
```

The record binds the result to exact bytes. Each input has a SHA-256 digest, and the output too:

```console
$ jq '.output' proof-abs/record.json
{
  "path": "output.s",
  "sha256": "32635a8b34075e66034e065c731fb31fbb3556d3ba8c451bb9aef453fc1ee368",
  "checked": true,
  "note": "frame-emission checked these bytes: the printed assembly AST is equal to them."
}
$ shasum -a 256 proof-abs/output.s
32635a8b34075e66034e065c731fb31fbb3556d3ba8c451bb9aef453fc1ee368  proof-abs/output.s
```

Your digest can be different, because the version of EvoCC is part of the assembly text. The two
digests on your machine must be equal.

## What you still trust

A proof always rests on something. The record lists it:

```console
$ jq -r '.trusted[]' proof-abs/record.json
preprocessing, parsing, elaboration and typed source export
the Lean kernel, which checks the theorems, and the compiled Lean checker with the Lean compiler, its runtime and the host toolchain
the readers of the input files
the printer of the assembly AST (EvoIR.Asm.File.print)
the Lean semantics of typed Core C and of the AArch64 subset
the ABI entry assumption (EvoIR.Asm.Proof.AbiSourceEntry): the caller obeys the AArch64 procedure call standard and gives sufficient stack; for an argument of width w, only the low w bits of its register or 8-byte stack slot are equal to the argument, and the other bits are arbitrary
the assembler, the linker, the runtime libraries and the hardware
```

The optimizer, the instruction selector, the register allocator and the Rust code that writes the
assembly are not on this list. They propose a result, and the checkers decide.

## Pin the checker

A check is only as good as the checker that ran it. `--proof-checker-sha256` pins the checker: EvoCC
refuses to run a checker with another digest. Take the digest from a source that you trust, such as
the `SHA256SUMS` file of the release. Here we take it from the file itself:

```console
$ CHECKER_SHA256=$(shasum -a 256 "$(command -v evoir-sem)" | cut -d' ' -f1)
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm --proof-required=proof-pinned \
    --proof-checker="$(command -v evoir-sem)" --proof-checker-sha256="$CHECKER_SHA256" \
    --core-entry=abs -O0 -fno-builtin abs.c > abs.s
evocc: proof-required result: CHECKED_TO_FINAL_ASSEMBLY: checked from typed Core source to final assembly text; record proof-pinned/record.json
$ jq '.checker_digest_pinned' proof-pinned/record.json
true
```

## See a refusal

A refusal is a normal result. It means that a check did not succeed, so EvoCC publishes nothing.
Here are four refusals.

A pointer is outside the subset. Make a file `first.c`:

```c
int first(const int *values) {
    return values[0];
}
```

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm --proof-required=proof-first \
    --proof-checker="$(command -v evoir-sem)" --core-entry=first -O0 -fno-builtin first.c; echo "exit status $?"
evocc: proof-required compilation refused: entry `first` is outside core-v1: Type; no output was written
exit status 6
$ ls proof-first
ls: proof-first: No such file or directory
```

A loop needs help. Make a file `loop.c`:

```c
int sum_to(int n) {
    int total = 0;
    for (int i = 1; i <= n; i++)
        total += i;
    return total;
}
```

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm --proof-required=proof-loop \
    --proof-checker="$(command -v evoir-sem)" --core-entry=sum_to -O0 -fno-builtin loop.c; echo "exit status $?"
evocc: proof-required compilation refused: stage source-to-evoir for entry `sum_to` rejected: STAGE_REJECTED stage=source-to-evoir entry=sum_to no checked source-to-EvoIR correspondence; no output was written
exit status 3
$ ls proof-loop proof-loop/checks
proof-loop:
checks
inputs

proof-loop/checks:
source-to-evoir-sum_to.err
source-to-evoir-sum_to.out
```

`sum_to` is in the subset, but the Lean checker refused the first stage. A loop needs *cutpoints*:
a statement of which values correspond at the head of each loop. You can give them in a proposal
file with `--proof-proposal=sum_to=PATH`. The proposal is not trusted. The checker checks it, so a
wrong proposal can make a check fail, but it cannot make a wrong result pass. Section 2.3 of the
[verification contract](https://github.com/onehr/evocc/blob/main/spec/verification.md#2-the-checked-subset-core-v1)
describes the format. After a refusal, `inputs/` and `checks/` stay for diagnosis, but there is no
`output.s` and no `record.json`.

A checker with the wrong digest does not run:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm --proof-required=proof-wrong \
    --proof-checker="$(command -v evoir-sem)" --proof-checker-sha256=0000000000000000000000000000000000000000000000000000000000000000 \
    --core-entry=abs -O0 -fno-builtin abs.c; echo "exit status $?"
evocc: proof-required compilation refused: checker digest 7cd984fd9c304c4efc4989adb17eedcd632bc15a4487edb474ade70924e74240 is not the pinned digest 0000000000000000000000000000000000000000000000000000000000000000; no output was written
exit status 5
```

EvoCC never writes over an old proof directory:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm --proof-required=proof-abs \
    --proof-checker="$(command -v evoir-sem)" --core-entry=abs -O0 -fno-builtin abs.c; echo "exit status $?"
evocc: proof-required compilation refused: cannot create `proof-abs`: File exists (os error 17); no output was written
exit status 4
```

The exit status tells you the kind of refusal:

| Exit status | Meaning |
|---|---|
| 0 | Each check accepted. `output.s` and `record.json` exist. |
| 1 | The command line is wrong, or the compilation failed before the proof inputs existed. |
| 3 | A stage checker rejected an entry. Standard error names the stage. |
| 4 | A proof input is missing or changed, the checker does not support an input, or the directory exists. |
| 5 | The checker is missing, has the wrong digest, failed or timed out. |
| 6 | The program is outside the subset. |

There is no fallback. A refusal never turns into an ordinary compilation, and an ordinary
compilation never gets a proof label.

## What the theorem says

The theorem is `checkPipeline_sound`. In words: if the pipeline check accepts the files of a
compilation, then the printed assembly is the published text, and the assembly *refines* the C
source. For every list of arguments of the right types, and for every caller that follows the
AArch64 calling convention:

- if the assembly stops, the source stops with the same observation: the same returned bits, or
  the same diagnostic, such as `E3004`;
- if the assembly runs forever, the source runs forever too;
- the assembly never reaches a state where it cannot continue.

The Lean kernel checks the theorem once, for all inputs. Each compilation then runs only the
compiled checker. The theorem uses no axiom beyond the three standard axioms of Lean.
[Section 5 of the verification contract](https://github.com/onehr/evocc/blob/main/spec/verification.md#5-the-pipeline-theorem)
states it in full, and
[section 7](https://github.com/onehr/evocc/blob/main/spec/verification.md#7-proof-required-mode)
describes proof-required mode.
