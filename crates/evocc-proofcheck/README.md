# evocc-proofcheck

Independent scalar certificate replay.

## Interface

Main types and entry points:

- `main`.

Before text, after text and a certificate produce an exit status.

## Invariants

Malformed inputs and invalid claims have separate exit statuses.

This is the Rust rewrite checker, not the Lean pipeline executable.
Its normal dependency closure must exclude the e-graph and producer.
The [dependency test](tests/normal_dependencies.rs) enforces that boundary.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 11.4. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/replay.rs](src/replay.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
