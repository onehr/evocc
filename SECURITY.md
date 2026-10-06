# Security policy

EvoCC is a compiler. A defect in a compiler can put a defect into every program that it builds.
Some of those defects are security problems. Please report them privately first.

## How to report

Send an email to **security@…** (the project owner fills in this address before the first
release). Do not open a public issue for a security problem.

Please include:

- the output of `evocc --version`;
- the command line and the input file, as small as possible;
- what you expected and what happened;
- for a checker or a proof problem, the certificate directory or the `record.json` and the
  `checks/` logs.

## What counts

These problems are in scope:

- **Wrong code.** EvoCC accepts a program and the output does something that the source does not
  allow. This includes a removed undefined-behavior check that the program needed.
- **A checker that accepts a wrong result.** `evocc-checker` replays a certificate for a rewrite
  that changes the meaning of the program, or a Lean stage checker (`evoir-sem`) accepts an output
  that does not refine its source.
- **A certificate or proof bypass.** `-Oeq-scalar` publishes output without a successful replay, or
  `--proof-required` writes `output.s` or `record.json` after a refusal, or writes a record that does
  not match the published bytes.
- **A pin bypass.** EvoCC runs a checker whose SHA-256 is not the pinned digest.

A crash, a refusal of a valid program or poor code size is a bug. Please report it with a normal
issue.

## What happens next

- We aim to acknowledge a report within 7 days.
- We aim to confirm or reject it, and to agree on a disclosure date with you, within 30 days.
- We credit you in the release notes unless you ask us not to.
