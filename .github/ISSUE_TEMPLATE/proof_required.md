---
name: Proof-required report
about: --proof-required refuses a program in core-v1, or accepts a wrong result
title: ""
labels: proof
---

<!-- If a checker accepted a wrong result, mark the title with [security] and read SECURITY.md. -->

### Versions

Output of `evocc --version`, and the SHA-256 of the checker (`shasum -a 256 evoir-sem`):

```text

```

### Command

```sh

```

### Input

The C file, and each `--proof-proposal` file if you used one.

```c

```

### Result

- Exit status:
- The refusing stage. Standard error names it, as in `stage lowering for entry abs`:
- The standard error of `evocc`:

```text

```

### Record and checker logs

If `NEW_DIR/record.json` exists, attach it. Attach the files in `NEW_DIR/checks/` for the
refusing stage (`STAGE-ENTRY.out` and `STAGE-ENTRY.err`).

### Why you expected a different result

For a refusal: why the program is in `core-v1` (`spec/verification.md`, section 2). For an
acceptance: what the checked output does wrong.
