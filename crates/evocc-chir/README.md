# evocc-chir

Typed source trees, source checks and source execution.

## Interface

Main types and entry points:

- `CHirUnit`.
- `CHirExpr`.
- `CHirStmt`.
- `core_profile::inspect_core`.
- `interp::interpret_scalar_function_with_target`.

Typed declarations and expressions support lowering and interpretation.

## Invariants

The typed Core admission walk inspects dead branches too.

## Contracts and tests

Relevant contract: [verification.md](../../spec/verification.md),
section 2. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/interp.rs](src/interp.rs), [src/core_profile.rs](src/core_profile.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
