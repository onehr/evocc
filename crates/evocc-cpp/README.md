# evocc-cpp

Preprocessing, macro expansion and header lookup.

## Interface

Main types and entry points:

- `Preprocessor`.
- `TokenStream`.
- `MacroMap`.
- `Preprocessor::for_target`.
- `Preprocessor::process`.
- `Preprocessor::has_errors`.

Tokens and include paths become the preprocessed token stream.

## Invariants

Callers check Preprocessor::has_errors before using the expanded stream.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 10.5. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
