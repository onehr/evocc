# Introduction

EvoCC is an experimental C compiler for 64-bit Arm (AArch64), written from scratch by AI agents in
Rust and Lean 4. It compiles a Linux kernel that boots. It also builds SQLite, Lua, zlib and
Mbed TLS, which pass their own tests, and Doom, which replays its demos.

This program divides by zero:

```c
int average(int total, int count) {
    return total / count;
}

int main(void) {
    return average(10, 0);
}
```

EvoCC finds the problem before you run a compiled program:

```console
$ evocc --check divide.c
divide.c: error[E3002]: integer-division-by-zero
```

One rule runs through EvoCC: an untrusted part proposes, and a small checker decides. You meet it
in three places:

- The interpreters stop at undefined behavior and name it with a numbered diagnostic. The
  compiled program keeps each condition that C requires as an explicit check
  ([Undefined Behavior](undefined-behavior.md)).
- The optimizer `-Oeq-scalar` writes a certificate for each change, and a separate, pinned checker
  replays it before EvoCC writes output. Lean proves each rewrite rule
  ([Checked Optimization](optimization.md)).
- For a small subset of C, `--proof-required` checks each stage of the compilation with checkers
  that Lean proves. If a check refuses, EvoCC writes no output
  ([Compiling with Proof](proof.md)).

You need to know C and the command line, not Lean. [Installation](install.md) installs EvoCC, and
[Hello, World!](hello.md) builds a first program.

## Limits

EvoCC's code is larger than the code of GCC 14 at `-O2`. The proofs cover small integer functions
only. Tests, not the theorem, check a program outside that subset.
[Evaluating EvoCC](evaluate.md#read-the-results-honestly) gives the numbers.

Each output in this book is the output that we saw when we ran the command on macOS on Apple
silicon, or on Linux on AArch64 where the text says so. A newer version of EvoCC can print slightly
different output.
