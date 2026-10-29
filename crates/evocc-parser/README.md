# evocc-parser

C syntax analysis and typed C-HIR construction.

## Interface

Main types and entry points:

- `Parser`.
- `ParseDiagnostic`.
- `Parser::from_tokens`.
- `Parser::parse_translation_unit_in`.
- `Parser::had_error`.

Preprocessed tokens produce a C-HIR unit and stored diagnostics.

## Invariants

A returned tree is not an admission result. Callers check Parser::had_error before lowering.

A parser instance parses one unit. The arena must outlive that unit.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 10.1. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/configuration.rs](src/configuration.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
