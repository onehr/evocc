# evocc-randgate

Generated-program differential checks and shrinking.

## Interface

Main types and entry points:

- `GeneratorConfig`.
- `CampaignReport`.
- `run_campaign`.

Seeds and budgets produce comparisons, counters and reduced failures.

## Invariants

A test campaign reports coverage; it is not a universal proof.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 10.4. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/report.rs](src/report.rs).
Integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
