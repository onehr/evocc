# evocc-evoir

Typed block-argument SSA, validation, text and execution.

## Interface

Main types and entry points:

- `Module`.
- `validate_module`.
- `parse_target_module`.
- `render_target_module`.
- `interpret_module_with_target`.

EvoIR modules are validated, rendered or executed.

## Invariants

Rust data construction alone does not establish SSA validity. Callers validate imported modules.

Its production dependency closure must exclude the e-graph.
The [checker dependency test](../evocc-proofcheck/tests/normal_dependencies.rs)
checks this crate too.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 2. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/validation.rs](src/validation.rs), [src/module_execution.rs](src/module_execution.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
