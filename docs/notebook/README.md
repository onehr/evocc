# Building EvoCC

This notebook tells the story of building EvoCC as a sequence of working,
inspectable systems. It is not a changelog and it is not a second roadmap. Each
chapter introduces one part of the compiler, explains why it has its particular
shape, points to the real repository artifacts, and ends with a command that
lets the reader observe the result for themselves.

The teaching rhythm is inspired by [*Crafting Interpreters*][ci]: begin with a
map, add one coherent piece at a time, keep the implementation visible, and
pause for design notes and exercises. The subject is different. EvoCC is a
GNU C11 compiler, an equality-saturation research system, and a proof-producing
systems project whose primary launch target is an ARM64 Linux kernel.

## Reading contract

Every chapter follows four rules:

1. **The narrative must name the artifact.** A concept is joined to source,
   contract, test, command, or evidence in this repository.
2. **The happy path is not enough.** We show what failure means and how a gate
   distinguishes a real failure from missing or stale evidence.
3. **Semantics and infrastructure stay distinct.** A boot proves integration
   liveness; it does not prove that an optimization is valid.
4. **History is allowed to be wrong.** When an early design or test was weak,
   the chapter explains the error and the repair instead of polishing the
   journey into a fictional straight line.

## Contents

### Part I — Ground Truth

1. [Before the First Token](01-before-the-first-token.md) — What EvoCC is, why
   Day 1 begins with contracts and independent oracles, and how a source tree
   becomes a reproducible ARM64 Linux boot.

### Part II — The GNU C Frontend

2. [From Bytes to Preprocessing Tokens](02-from-bytes-to-preprocessing-tokens.md)
   — How logical lines, maximal munch, macro rescanning, `#`/`##`, conditional
   stacks, include provenance, and a 34-case differential oracle establish the
   parser's input boundary.

Later chapters will continue through arena-owned C-HIR, declarators, types,
statements, functions, and GNU C constructs.

### Part III — Equality With Reasons

Future chapters will introduce EvoIR, e-classes, congruence closure, rewrite
guards, extraction, and independently checkable proof artifacts.

### Part IV — Back to the Machine

Future chapters will lower EvoIR to EvoMIR, cross the external-tool boundary,
implement the Apple and Linux ARM64 ABIs, and return to the kernel flight lab.

[ci]: https://craftinginterpreters.com/contents.html
