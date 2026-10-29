# evocc-mir

Machine lowering, allocation, frame layout and assembly emission.

## Interface

Main types and entry points:

- `MModule`.
- `Target`.
- `emit::CodegenOptions`.
- `lower::lower_module_for`.
- `compile_module_with_entries`.

EvoIR becomes AArch64 assembly text through machine IR.

## Invariants

The generic layer must not depend on instruction-set names.

The [target boundary test](tests/target_boundary.rs) scans
[generic code](src/generic/) for instruction-set names.

## Contracts and tests

Relevant contract: [evomir.md](../../spec/evomir.md),
section 7. Read the contract's scope before extending the crate.

Source: [src/lib.rs](src/lib.rs), [src/native.rs](src/native.rs), [src/emit.rs](src/emit.rs), [src/emit/instructions.rs](src/emit/instructions.rs).
Unit tests live in the source modules; integration tests are in [tests/](tests/).
See the [architecture](../../ARCHITECTURE.md) for the pipeline and trust boundaries.
