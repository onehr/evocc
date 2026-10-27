# Introduction

Welcome to *The EvoCC Book*. EvoCC is a C compiler for 64-bit Arm (AArch64) that checks its own
work. It is written from scratch in Rust, and its checkers are proved in Lean 4. EvoCC compiles a
Linux kernel that boots. It also builds real programs such as SQLite, Lua, zlib, Mbed TLS and Doom,
and those programs pass their own tests.

## Who this book is for

This book is for you if you write C and want a compiler that tells you about undefined behavior.
It is also for you if you build software and want evidence that the compiler did its job, or if
you study compilers and verification. You should know C and the command line. You do not need to
know Lean.

## What is different

Most C compilers ask you to trust them. EvoCC follows one rule instead: untrusted parts propose,
and small checked parts decide. You meet this rule in three places.

- **Undefined behavior is explicit.** EvoCC has two interpreters that stop at undefined behavior
  and name it with a numbered diagnostic. In the compiled program, each condition that C requires
  stays as an explicit check. [Chapter 4](undefined-behavior.md) shows how.
- **Optimization is checked.** The optimizer `-Oeq-scalar` writes a certificate for each change.
  A separate, pinned checker replays the certificate before EvoCC writes any output. Each of its
  62 rewrite rules is proved in Lean. [Chapter 6](optimization.md) shows how.
- **A compilation can carry a proof.** For a small subset of C, `--proof-required` checks the
  actual compilation, stage by stage, with checkers proved in Lean. One theorem covers the whole
  chain. If a check refuses, EvoCC writes no output. [Chapter 5](proof.md) shows how.

Here is a first taste. This program divides by zero:

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

## What EvoCC does not do yet

EvoCC is young, and we want you to know its limits. On the library code that we measured, its
code is 1.8 to 2.3 times the size of the code that GCC 14 makes at `-O2`. The proofs cover small
integer functions only. A program outside that subset is checked by tests, not by the theorem.
[Chapter 8](evaluate.md) gives the numbers.

## How to use this book

Read [chapter 2](install.md) and [chapter 3](hello.md) first. They install EvoCC and build a
program. Chapters 4 to 6 show the three differences above. [Chapter 7](how-it-works.md) explains
how the compiler works inside. [Chapter 8](evaluate.md) shows how the project measures EvoCC, and
[chapter 9](contributing.md) shows how you can help.

Each output in this book is the output that we saw when we ran the command on macOS on Apple
silicon, or on Linux on AArch64 where the text says so. A newer version of EvoCC can print slightly
different output.
