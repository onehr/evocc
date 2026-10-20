# Check an executed C path

`evocc --check` preprocesses and lowers one C translation unit, then runs the
existing EvoIR program interpreter. It does not invoke a native compiler or
produce an executable.

```sh
target/frontend/debug/evocc --check tests/ui/check/division.c
# tests/ui/check/division.c: error[E3002]: integer-division-by-zero

target/frontend/debug/evocc --explain E3002
target/frontend/debug/evocc --check --error-format=json tests/ui/check/division.c
```

The entry is the existing zero-source-argument `main` interface. Constructors,
`main`, and destructors share the interpreter's existing memory state and
100,000-step budget. Normal preprocessing and language options (`-I`, `-D`,
`-std=gnu11`, and so on) retain their existing meanings. `--check` cannot be
combined with another action mode or with native-only `--ub-checks`.

Exit 0 means the admitted, executed path completed without a finding. A
nonzero integer returned by `main` is reported but is not itself a checker
failure. Exit 1 means a diagnostic stopped compilation or execution. Resource
exhaustion and unmodeled operations are **incomplete checks**, not successful
checks and not automatically undefined behavior. The checker does not invent
results for unresolved external calls.

This is not a static proof about other inputs, a whole-program absence-of-UB
claim, or a replacement for native or multithreaded testing. Unsupported
library calls, `main` arguments, system interactions and interpreter limits
retain the existing refusal boundary. The new interface changes no interpreter
semantics. Runtime EvoIR currently lacks source-span mappings: a runtime
diagnostic therefore shows the input filename without an invented `1:1`
location. Front-end diagnostics retain their actual line and column. The
legacy interpreter command's text output remains unchanged.

## Editor output

`--error-format=json` emits one JSON diagnostic per stderr line. The stable
`evocc.diagnostic.v1` record has these fields:

| Field | Meaning |
| --- | --- |
| `schema`, `kind` | `evocc.diagnostic.v1`, `diagnostic` |
| `severity` | `error` |
| `code` | Existing E-code, or `null` for a driver usage error |
| `message` | Existing diagnostic text; JSON escaping preserves its contents |
| `phase` | Input, lexing, preprocessing, parsing, lowering, codegen, execution or driver |
| `category` | Compilation, driver, undefined-behavior, resource-limit, unsupported-execution or execution-contract |
| `source` | Source filename, or `null` if none applies |
| `location` | Actual `{ "line": N, "column": N }`, or `null` when unavailable |
| `explanation` | `available`, `todo` or `unavailable` |

All enumerated strings are lowercase, hyphenated where shown. The category
describes the existing diagnostic rather than redefining its semantic meaning;
unclassified execution errors conservatively use `execution-contract`.
`--survey` retains its parser recovery and emits independent JSON records.
Successful `--check --error-format=json` writes a separate stdout record:

```json
{"schema":"evocc.check.v1","kind":"check","status":"completed","main_return":"7","claim":"executed-path-only","scope":"No finding from the implemented checks on the executed path. Pointer subtraction and ordering obligations are not checked. This does not check that the results are correct."}
```

Failure writes no completion record. Other action modes keep their existing
stdout protocol; JSON affects their diagnostics only.

## Explanation authority and UI fixtures

The build derives `--explain` entries from
[EvoIR section 6.3](../spec/evoir.md#63-diagnostic-authority), preserving the
authoritative message and retired/withdrawn annotations. Named codes found
elsewhere in the spec or compiler sources receive an explicit explanation
TODO when the authority table has no row; an unknown code is rejected. The
catalogue is embedded in the binary and does not need a source checkout at
runtime. These are short authority pages, not a claim that every diagnostic
already has a tutorial.

The reviewed `.stderr` files in `tests/ui/check/` are blessed expected output.
`cargo test --offline -p evocc --test check_product` compares them exactly and
validates the public JSON channel with Python's standard JSON parser. There is
no automatic blessing: a diagnostic change requires an explicit fixture edit
and review of the underlying semantics. Existing legacy diagnostic suites also
remain part of the full-workspace gate.
