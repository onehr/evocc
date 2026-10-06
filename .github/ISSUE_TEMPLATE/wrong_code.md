---
name: Wrong code
about: A program built by EvoCC does something that its source does not allow
title: ""
labels: wrong-code
---



### Version

Output of `evocc --version`:

```text

```

### Input

The smallest program that shows the problem. It should print or return the value that differs.

```c

```

### Commands

How you built and ran the program with EvoCC:

```sh

```

### Undefined behavior

Output of `evocc --check FILE.c`. A program with undefined behavior has no single right answer.

```text

```

### Oracle

Which oracle gives the expected result, and its result. For example GCC 14 or Clang 19 at `-O0`
and `-O2`, `evocc --interpret-c-hir`, `evocc --interpret-evoir`, or a value that you computed by
hand.

```text

```

### Native result

The output and the exit status of the program that EvoCC built:

```text

```
