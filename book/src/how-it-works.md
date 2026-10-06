# How EvoCC Works

This chapter opens the compiler. It follows a C file from its bytes to the assembly, and it shows
where each check happens. It also tells how the project decided what counts as a correct answer,
before the compiler could read a single token.

## The pipeline

EvoCC has three intermediate representations, or IRs. Each IR answers one question. C-HIR says
what the program means. EvoIR says which programs are equal. EvoMIR says how the machine computes
one of them.

```text
C source
  │  preprocessor · lexer · parser
  ▼
C-HIR    checked front-end IR ·················· C-HIR interpreter ─┐
  │  lowering                                                       ├─ agree
  ▼                                                                 │
EvoIR    typed SSA, ub.assume, mem and world ··· EvoIR interpreter ─┘
  │  -Oeq-scalar: e-graph search ──► certificate ──► evocc-checker replays it
  ▼
EvoMIR   registers, ABI, frames ················ allocation verifier
  ▼
AArch64 assembly ──► system assembler and linker ──► native program

Lean 4, offline: rule proofs · stage checkers · the pipeline theorem
```

The rest of this chapter walks down this picture.

## From bytes to tokens

Before the parser sees anything, the preprocessor turns bytes into *preprocessing tokens*, reads
directives and expands macros. This sounds simple. It is not, because each step removes
information, and the order of the steps changes the program.

### Spaces can matter

A C lexer takes the longest token that it can. This rule is called *maximal munch*. Because of it,
`+ +` and `++` are different programs, and so are `/ *` and `/*`. The preprocessor must keep these
boundaries when it writes its output. Look at what `evocc -E` writes for this file, `spacing.c`:

```console
$ cat spacing.c
int f(int x) {
    return x + +1;
}
int g(int x, int *y) {
    return x / *y;
}
$ evocc -E spacing.c
int f(int x){return x+ +1;}int g(int x,int*y){return x/ *y;}
```

The output drops every space that it can, but it keeps the space in `x+ +1` and in `x/ *y`. Without
it, `x++1` would be an increment, and `x/*y` would start a comment that eats the rest of the
program. An early version of EvoCC made exactly these two mistakes. The fix was a general rule, not
a list of special cases: before the writer drops a space, it lexes the result again and checks that
it gets the same tokens.

### Macro expansion is an evaluator

A macro is not plain text substitution. The expander must find the arguments, substitute them,
scan the result again for more macros, and stop a macro from expanding inside itself. Here is a
file `expand.c` that tests these rules:

```console
$ cat expand.c
#define A B
#define B A
int x = A;
#define STR(x) #x
#define JOIN(a, b) a ## b
const char *s = STR(1 + 2);
int JOIN(var, 7) = 7;
$ evocc -E expand.c
int x=A;const char*s="1 + 2";int var7=7;
```

`A` expands to `B`, and `B` expands back to `A`. Then the expansion stops, because `A` is already
being expanded. EvoCC keeps a set of the macros that it is expanding. A limit on the depth would
also stop, but it would not follow the rule of the language. `#` turns the argument into a string,
and `##` pastes two tokens into one.

### Tokens remember where they came from

Each token has a kind, a spelling and a source span: a file, a byte range, a line and a column.
A token that comes from a header points into the header, not into your file. An early version of
the parser read the spelling of a header token from the main file. A long name in a header then
pointed past the end of the main file, and the parser crashed. Now a token from another file owns
its spelling, and its span still points at the header for diagnostics.

A missing header is a numbered diagnostic, not a crash. Make a file `missing.c` that includes a
header that does not exist:

```c
#include "config.h"

int main(void) {
    return 0;
}
```

```console
$ evocc -E missing.c; echo "exit status $?"
missing.c:1:1: error[E0105]: include-not-found: config.h
exit status 1
```

### Compare tokens, not text

How do you know that the preprocessor is right? You compare it with another one. But GCC and EvoCC
place spaces differently, so their text differs even when their tokens agree. And if you remove all
spaces, you hide the very mistakes of the last section. So the project compares *tokens*. A small,
separate program lexes the output of both compilers into a list of tokens. A fixed corpus of 34
cases must give the same tokens under EvoCC and GCC 14. The number 34 is part of the test: a run
with 33 cases is a different test, and it fails.

## C-HIR

The parser builds C-HIR, the checked front-end IR. It keeps everything that C needs to say what a
program means: types, qualifiers, lvalues, the order of evaluation, and the spans of the source.
Make a file `max.c`:

```c
int max(int a, int b) {
    return a > b ? a : b;
}
```

`--emit=c-hir` prints a short outline of its C-HIR:

```console
$ evocc --emit=c-hir max.c
function max
  parameter a
  parameter b
```

C-HIR has its own interpreter, `--interpret-c-hir`. The interpreter tracks two extra facts. Each
byte of memory knows whether it was written, and each pointer knows the object that it points into,
its *provenance*. That is how the interpreter can report a read of an uninitialized variable or a
read past the end of an array, as in [chapter 4](undefined-behavior.md).

## EvoIR

Lowering turns C-HIR into EvoIR, the IR of the optimizer. Here is the EvoIR of `max.c`:

```console
$ evocc --emit=evoir max.c
func @max(%arg0: bits<32>, %arg1: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>):
    %v2 = icmp.slt %v1, %v0
    cond_br %v2, ^b1(%v0, %v1), ^b2(%v0, %v1)

^b1(%v3: bits<32>, %v4: bits<32>):
    br ^b3(%v3, %v4, %v3)

^b2(%v5: bits<32>, %v6: bits<32>):
    br ^b3(%v5, %v6, %v6)

^b3(%v7: bits<32>, %v8: bits<32>, %v9: bits<32>):
    ret %v9
}
```

EvoIR is in *static single assignment* form: each value has one definition. A value that crosses
from one block to another is an explicit argument of the block. Here `^b3` receives the result of
the conditional as `%v9`. EvoIR has four more properties:

- **Values are bit vectors.** `bits<32>` has no sign. An operation such as `icmp.slt` says how it
  reads the bits.
- **Undefined behavior is an instruction.** Each condition that C requires is a `ub.assume`
  obligation, as you saw in [chapter 4](undefined-behavior.md).
- **Effects are values.** Memory and the outside world travel as the tokens `mem` and `world`.
  A function that touches neither, like `max`, has no tokens, and its `effects` line says so.
- **Pointers have provenance.** Each pointer carries the thing that it points into: an object, a
  function or null.

Two EvoIR terms are *equal* only if they have the same value and the same effects. Two terms with
the same value but different trap conditions are not equal. The optimizer of
[chapter 6](optimization.md) uses exactly this equality.

EvoIR has its own interpreter, `--interpret-evoir`. It is a second, separate implementation of the
meaning of your program. Each test program must give the same result in both interpreters, and the
same result as GCC 14 and Clang 19. When the two interpreters reach the same mistake, they report
the same code. Here is `uninit.c` from chapter 4:

```console
$ evocc --interpret-c-hir uninit.c
uninit.c:4:12: error[E3001]: read-of-uninitialized-object: score
$ evocc --interpret-evoir uninit.c
uninit.c:1:1: error[E3001]: read-of-uninitialized-object
```

## EvoMIR

The backend lowers EvoIR to EvoMIR, the machine IR. EvoMIR has virtual registers in classes, such
as `gpr32` and `gpr64`, and views of them, such as `w` and `x`. It knows the calling convention,
the registers that a call destroys and the slots for spilled values. A verifier checks the result of
register allocation. Then EvoCC prints the assembly, and the system assembler and linker make the
program.

The facts about the machine are data, not code. The directory `targets/aarch64/` holds versioned
tables of registers, calling conventions, instructions, encodings, selection patterns and costs.
A generator turns them into the tables of the backend, and the gate checks that the generated
tables are current.

## The Lean checkers

The last part of the picture runs offline. In Lean 4, the project proves three kinds of facts:

- each rewrite rule of the optimizer is correct for every bit width;
- each stage checker is *sound*: if it accepts an input and an output, the output refines the
  input;
- the pipeline check is sound: if all stages accept, the published assembly refines the C source.

Lean compiles the checkers into one program, `evoir-sem`. In proof-required mode, EvoCC writes the
input and the output of each stage to files, and `evoir-sem` checks them. The compiler proposes; the
checker decides. So the theorems hold for each accepted compilation, and the compiler can change
without a change to a proof. The cost is that a checker can refuse a correct program, as you saw in
[chapter 5](proof.md).

## Before the first token

The project did not start with a lexer. It started with a question: when the compiler gives an
answer, who says that the answer is right? A compiler that cannot name its inputs cannot explain its
output. So the first work was to pin everything that can change a result.

### Who decides what is correct

Calling GCC the "golden compiler" is convenient, and wrong. Compilers have bugs, and GCC and Clang
share history. The project ranks its sources of truth:

| Rank | Authority | What it can establish |
|---:|---|---|
| 1 | Frozen language and ABI contracts | The meaning of a defined program |
| 2 | Reviewed test fixtures | The expected result of one reviewed case |
| 3 | Independent GCC and Clang runs | Strong evidence, not proof |
| 4 | ABI observers and derived relations | Properties that follow from the target contract |
| 5 | A kernel boot | That a large program stays alive, and nothing more |

If GCC and Clang disagree on a defined program, the result is a conflict. EvoCC does not win a vote
by siding with one of them. A kernel that boots cannot overrule the language contract.

One detail shows why this matters. On macOS, `/usr/bin/gcc` is Apple Clang. Two commands, one
compiler: treating them as two oracles gives one implementation two votes. So the reference
compilers, GCC 14.2 and Clang 19.1, run in a sealed Linux image with no network, and their versions
and digests are checked on each run.

### Pin what can change the answer

A kernel build has many hidden inputs: the kernel version, its configuration, the real compiler
behind the name `gcc`, the container image, the time stamps, the build paths, the locale and the
model of the emulated CPU. If one of them changes, a later failure may be a compiler bug, or only a
change of the environment. So each of them is pinned in a contract file in `contracts/`, or it is
named as an observation only.

"Reproducible" is a claim that needs evidence. The project builds each reference kernel twice from
a clean state and compares the bytes. This found a real defect: a bare epoch time stamp made one
build helper drop its time option, and the two kernel images differed. The fix pinned the
time stamp in the form that the build expects.

### A boot that cannot be faked

A simple boot test searches the serial log for a known line. Such a test can accept an old log from
an earlier run. So each test boot gets a fresh value, a *nonce*. The kernel command line carries it,
and the init program prints it back. The test accepts the boot only if it sees the right kernel
image, the value exactly once, the markers in order, an orderly power-off, and no panic or timeout.

### Red is a valid answer

On the first day, EvoCC compiled nothing. A dishonest skeleton could return success and produce
nothing. EvoCC instead failed each request with a named `NOT_IMPLEMENTED` result. A missing feature
is a precise failure. A crash, a timeout, missing evidence or a silent success is a failure of the
test itself. This rule stayed: each change still starts with a failing test, a RED test, and a gate
that cannot find its evidence fails.

Green can lie too. Early reviews found a test corpus that had become smaller when a hard case was
removed, and a check that tested an example instead of the real files. The repair was not more
green results. It was a stronger link between the result and the evidence: a fixed number of cases,
no result other than PASS, and a check that rejects changed, missing and extra files.

## The specifications

Four documents in `spec/` define the IRs and the checked path. Each is pinned by SHA-256, and a
change needs a new revision:

- [EvoIR](https://github.com/onehr/evocc/blob/main/spec/evoir.md): types, SSA form, instructions,
  effects, memory, undefined behavior and diagnostics, equality saturation and the rule registries;
- [EvoMIR](https://github.com/onehr/evocc/blob/main/spec/evomir.md): registers, lowering, the frame
  and calling convention, allocation and emission;
- [The target description](https://github.com/onehr/evocc/blob/main/spec/target.md): the tables in
  `targets/aarch64/`;
- [The verification contract](https://github.com/onehr/evocc/blob/main/spec/verification.md): the
  checked subset, the semantics, the stage checkers, the pipeline theorem and the trusted base.

When the code and a specification disagree, one of them has a defect. The
[index of the specifications](https://github.com/onehr/evocc/blob/main/spec/README.md) explains how
to change one.
