# evocc-policy

Deterministic policy evaluation and bounded search.

## Interface

Main types and entry points:

- `Corpus`.
- `PolicyReport`.
- `evaluate`.

Corpora and policies produce measurements and search reports.

## Invariants

A lost or rejected certificate cannot supply a successful fitness result.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 8.11. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
