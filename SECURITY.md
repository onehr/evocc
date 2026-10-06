# Security policy

EvoCC is a compiler. A security problem is a defect that makes a program behave in a way that its
source does not permit:

- wrong code: the compiled program computes a different result from the C program;
- a checker that accepts a wrong result: `evocc-checker`, a Lean stage checker or the pipeline
  check accepts an output that does not refine its input;
- a certificate or digest bypass: a pinned checker, a certificate or a receipt that can be forged.

## How to report

Report it in the open, as a pull request first:

1. Open a pull request that adds a failing test (the RED test) which shows the defect. Put the C
   input, the command and the expected and actual result in the test. Do not add the fix in the
   same commit.
2. If you cannot write the test, open an issue with the "Wrong code" or "Proof-required"
   template.
3. Mark the title with `[security]`.

There is no private email channel. Do not send reports by email.

## What happens next

The maintainers reply on the pull request or issue within seven days. A confirmed wrong-code or
checker defect gets a fix, a receipt that shows the fix on the original input, and an entry in
`CHANGELOG.md`.
