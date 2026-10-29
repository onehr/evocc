# evocc

The compiler and user-facing command drivers.

## Interface

Main types and entry points:

- `evocc`.
- `evo`.
- `evocc-checker`.

C files and command options produce diagnostics, IR, assembly, objects or execution results.

## Invariants

Proof-required output is published only after all required checks pass.

Ordinary compilation is not proof-required compilation.
The proof-required path covers typed Core through final assembly text only.
Preprocessing and the source export remain outside its theorem.

## Contracts and tests

Relevant contract: [verification.md](../../spec/verification.md),
section 7. Read the contract's scope before extending the crate.

Source: [src/main.rs](src/main.rs), [src/bin/evo.rs](src/bin/evo.rs), [src/proof_required.rs](src/proof_required.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
