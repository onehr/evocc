# evocc-passes

Development pass orchestration and validation records.

## Interface

Main types and entry points:

- `ModulePass`.
- `PipelineReport`.
- `run_pipeline`.

Passes and execution inputs produce a module and validation report.

## Invariants

Development differential checks do not label arbitrary output as proved.

## Contracts and tests

Relevant contract: [evoir.md](../../spec/evoir.md),
section 10.4. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs).
Integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
