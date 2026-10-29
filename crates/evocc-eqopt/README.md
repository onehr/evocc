# evocc-eqopt

Scalar rewrite production and persistent equality state.

## Interface

Main types and entry points:

- `EqualityState`.
- `ExtractionPolicy`.
- `run_scalar_chain_with_checker`.

EvoIR and a policy produce a proposed chain and certificates.

## Invariants

The chain commits caller state only after all required replays succeed.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 8.11. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/chain.rs](src/chain.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
