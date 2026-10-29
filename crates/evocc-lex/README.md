# evocc-lex

C preprocessing tokens and literal spelling.

## Interface

Main types and entry points:

- `Lexer`.
- `Token`.
- `TokenKind`.
- `Lexer::from_bytes`.
- `Lexer::next`.

Source text becomes tokens with source spans.

## Invariants

Token spelling and source spans remain available to later stages.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 10.5. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
