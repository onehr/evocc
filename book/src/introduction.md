# Introduction

EvoCC is an experimental C compiler for 64-bit Arm (AArch64), written from scratch by AI agents in
Rust and Lean 4. It compiles a Linux kernel that boots. It also builds SQLite, Lua, zlib, Mbed TLS
and Doom, and those programs pass their own tests.

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

- The interpreters stop at undefined behavior and name it with a numbered diagnostic. In the
  compiled program, each condition that C requires stays as an explicit check
  ([chapter 4](undefined-behavior.md)).
- The optimizer `-Oeq-scalar` writes a certificate for each change. A separate, pinned checker
  replays the certificate before EvoCC writes any output. Lean proves each of its 62 rewrite rules
  ([chapter 6](optimization.md)).
- For a small subset of C, `--proof-required` checks the actual compilation, stage by stage, with
  checkers proved in Lean. One theorem covers the whole chain. If a check refuses, EvoCC writes no
  output ([chapter 5](proof.md)).

You need to know C and the command line, not Lean. Start with [chapter 2](install.md), which
installs EvoCC, and [chapter 3](hello.md), which builds a program.

## Limits

On the library code that we measured, EvoCC's code is 1.8 to 2.3 times the size of the code that
GCC 14 makes at `-O2`. The proofs cover small integer functions only. A program outside that subset
is checked by tests, not by the theorem. [Chapter 8](evaluate.md) gives the numbers.

Each output in this book is the output that we saw when we ran the command on macOS on Apple
silicon, or on Linux on AArch64 where the text says so. A newer version of EvoCC can print slightly
different output.
