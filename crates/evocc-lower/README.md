# evocc-lower

Typed C-HIR lowering into EvoIR.

## Interface

Main types and entry points:

- `SourceLocations`.
- `lower_unit`.
- `lower_unit_with_locations`.

A C-HIR unit and target layout produce an EvoIR module or a lowering error.

## Invariants

Source execution obligations become explicit EvoIR operations.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 10.1. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
