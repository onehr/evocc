# evocc-memory

Shared interpreter memory and world state.

## Interface

Main types and entry points:

- `MemoryState`.
- `WorldState`.
- `PointerValue`.
- `MemoryState::load_integer`.
- `MemoryState::store_integer`.

Memory operations read or update explicit state and return values or faults.

## Invariants

Pointer provenance and initializedness are checked by memory operations.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 5. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
