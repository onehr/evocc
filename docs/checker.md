# Check an executed C path

`evocc --check FILE.c` preprocesses one translation unit, lowers it to EvoIR and runs `main` in the
EvoIR interpreter. It invokes no native compiler and writes no executable. `evo check FILE.c` runs
this command.

```console
$ target/release/evocc --check tests/ui/check/division.c
error[E3002]: integer-division-by-zero
  --> tests/ui/check/division.c:1:39
   |
 1 | int main(void) { int zero = 0; return 1 / zero; }
   |                                       ^^^^^^^^ integer-division-by-zero
   |
   = note: this operation failed on the executed path
   = help: check the divisor before the division or remainder operation
```

- `main` has no parameters, or `int argc, char **argv` with `argc` 1 and `argv[0]` the input path.
- Constructors, `main` and destructors share one memory state and one budget of 100,000
  interpreter steps.
- Preprocessing and language options such as `-I`, `-D` and `-std=gnu11` keep their meanings.
  `--check` refuses a second action mode and the native-only options `--ub-checks`, `--frame-opt`,
  `--isel-opt` and `--regalloc-opt`.
- Exit status 0: the executed path completed with no finding. A nonzero return value of `main` is
  not a failure. Exit status 1: a diagnostic stopped compilation or execution.
- The step limit (`E3008`) and a call that the interpreter cannot run (`E3039`) end an incomplete
  check. An incomplete check is not a pass and not a finding of undefined behavior.

[`spec/diagnostics.md`](../spec/diagnostics.md) defines the text and JSON forms of a diagnostic and
the completion record. [Undefined behavior](../book/src/undefined-behavior.md) runs the command on
example programs and states what a completed check does not show. A completed check also does not
replace native or multithreaded tests.

## Explanations

`evocc --explain CODE` prints `crates/evocc/explain/CODE.md`. The build takes the codes from
[EvoIR section 6.3](../spec/evoir.md#63-diagnostic-authority) and embeds one page for each code, so
the binary needs no source checkout. A code without a row there is refused with exit status 1.
`crates/evocc/explain/hints.txt` holds the one-line repair hints.
`crates/evocc/tests/explain_pages.rs` runs the examples of each page, and
`python3 tools/gen-error-index.py` copies the pages into the
[error index](../book/src/error-index.md).

## UI fixtures

Each `tests/ui/check/NAME.c` has two blessed outputs: `NAME.stderr` for `--error-format=text` and
`NAME.json` for `--error-format=json`. A `// flags:` line in the source replaces the default flag
`--check`, and a `// exit:` line replaces the expected exit status 1.

```sh
cargo test --offline -p evocc --test check_product
UPDATE_EXPECT=1 cargo test --offline -p evocc --test check_product
```

The first command compares both outputs byte for byte and checks each JSON record with
`tests/ui/check/validate.py`. The second rewrites the outputs. Review each changed output and the
rule behind it before you commit it.
