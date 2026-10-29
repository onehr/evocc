# evocc-common

Source positions, diagnostics and arena allocation.

## Interface

Main types and entry points:

- `SourceSpan`.
- `DiagnosticCollector`.
- `Arena`.

Source bytes and locations become shared records.

## Invariants

Positions retain their file identity.

## Contracts and tests

Relevant contract: [diagnostics.md](../../spec/diagnostics.md),
section 2. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
