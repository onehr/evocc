# evocc-proof

Shared proof records, text and checker process control.

## Interface

Main types and entry points:

- `EvpCertificate`.
- `StandaloneProofChecker`.
- `parse_evp1`.
- `render_evp1`.

Proof data become serialized records or checked process outcomes.

## Invariants

The module shares formats, not optimizer rule implementations.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 11.3. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/evp1.rs](src/evp1.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
