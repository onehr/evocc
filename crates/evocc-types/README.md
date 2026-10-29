# evocc-types

C type identity, target layout and bit-exact floating values.

## Interface

Main types and entry points:

- `CType`.
- `CanonicalType`.
- `TypeInterner`.
- `TargetDataLayout`.
- `TypeInterner::intern`.
- `TargetDataLayout::layout_of`.

C type descriptions become interned types and layout results.

## Invariants

Layout is an explicit target input where the API accepts it.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 1. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/floating.rs](src/floating.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
