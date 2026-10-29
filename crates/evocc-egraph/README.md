# evocc-egraph

Equality classes, hash-consing and congruence rebuilding.

## Interface

Main types and entry points:

- `Egraph`.
- `EClassId`.
- `ENode`.
- `Egraph::new`.
- `Egraph::insert`.
- `Egraph::rebuild`.

Nodes and union requests produce an e-graph and replay events.

## Invariants

A producer's merge is not an independent proof of equality.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 8. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
