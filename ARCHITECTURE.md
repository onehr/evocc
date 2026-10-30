# Architecture

```text
C source -> lexer + preprocessor -> parser + types -> typed C-HIR
         -> EvoIR lowering -> optional scalar EqSat -> EvoMIR lowering
         -> checked out-of-SSA -> register allocation + allocation check
         -> frame layout + AArch64 assembly -> platform assembler -> object
```

`evocc` also invokes the platform linker. `evo` delegates user commands to `evocc`.

C-HIR and EvoIR have separate Rust interpreters that share memory and floating-point
models. `evo run` executes C-HIR, and `evo check` executes checked EvoIR. EvoMIR has an
interpreter for backend tests.

With `-Oeq-scalar`, the independent Rust checker `evocc-checker` replays each rewrite certificate
before the driver writes output. In proof-required mode, the driver exports typed Core and stage
snapshots to the Lean checker `evoir-sem`. It checks source lowering, rewrites, machine lowering,
register allocation and frame emission before the driver publishes assembly text. The
[verification contract](spec/verification.md) gives the theorem, the trusted components and the
source, target and execution-check limits.

## Crates

| Crate | Job | Entry points | `[dependencies]`, without the prefix `evocc-` |
| --- | --- | --- | --- |
| [evocc](crates/evocc/) | The compiler and user-facing command drivers. | binaries `evocc`, `evo`, `evocc-checker`, `evo-guard-size` | `chir`, `common`, `cpp`, `eqopt`, `evoir`, `lex`, `lower`, `mir`, `parser`, `passes`, `proof`, `proofcheck`, `types` |
| [evocc-chir](crates/evocc-chir/) | Typed source trees, source checks and source execution. | `CHirUnit`, `CHirExpr`, `CHirStmt` | `common`, `memory`, `types` |
| [evocc-common](crates/evocc-common/) | Source positions, diagnostics and arena allocation. | `SourceSpan`, `DiagnosticCollector`, `Arena` | none |
| [evocc-cpp](crates/evocc-cpp/) | Preprocessing, macro expansion and header lookup. | `Preprocessor`, `TokenStream`, `MacroMap` | `common`, `lex` |
| [evocc-egraph](crates/evocc-egraph/) | Equality classes, hash-consing and congruence rebuilding. | `Egraph`, `EClassId`, `ENode` | `proof` |
| [evocc-eqopt](crates/evocc-eqopt/) | Scalar rewrite production and persistent equality state. | `EqualityState`, `ExtractionPolicy`, `run_scalar_chain_with_checker` | `egraph`, `evoir`, `proof`, `types` |
| [evocc-evoir](crates/evocc-evoir/) | Typed block-argument SSA, validation, text and execution. | `Module`, `validate_module`, `parse_target_module` | `memory`, `types` |
| [evocc-lex](crates/evocc-lex/) | C preprocessing tokens and literal spelling. | `Lexer`, `Token`, `TokenKind` | `common` |
| [evocc-lower](crates/evocc-lower/) | Typed C-HIR lowering into EvoIR. | `SourceLocations`, `lower_unit`, `lower_unit_with_locations` | `chir`, `common`, `evoir`, `types` |
| [evocc-memory](crates/evocc-memory/) | Shared interpreter memory and world state. | `MemoryState`, `WorldState`, `PointerValue` | `types` |
| [evocc-mir](crates/evocc-mir/) | Machine lowering, allocation, frame layout and assembly emission. | `MModule`, `Target`, `compile_module_with_entries`; binaries `evo-ir-asm`, `evo-asm-ast`, `evo-alloc-export` | `evoir`, `memory`, `proof`, `types` |
| [evocc-parser](crates/evocc-parser/) | C syntax analysis and typed C-HIR construction. | `Parser`, `ParseDiagnostic` | `chir`, `common`, `cpp`, `lex`, `types` |
| [evocc-passes](crates/evocc-passes/) | Development pass orchestration and validation records. | `ModulePass`, `PipelineReport`, `run_pipeline` | `evoir`, `proof`, `randgate`, `types` |
| [evocc-policy](crates/evocc-policy/) | Deterministic policy evaluation and bounded search. | `Corpus`, `PolicyReport`, `evaluate`; binaries `evo-policy-eval`, `evo-cost-search`, `evo-cost-axes` | `eqopt`, `evoir`, `proof`, `randgate` |
| [evocc-proof](crates/evocc-proof/) | Shared proof records, text and checker process control. | `EvpCertificate`, `StandaloneProofChecker`, `parse_evp1` | none |
| [evocc-proofcheck](crates/evocc-proofcheck/) | Independent scalar certificate replay. | binaries `evo-proofcheck`, `evo-facts-vectors`; `main`, which `evocc-checker` also runs | `evoir`, `proof` |
| [evocc-randgate](crates/evocc-randgate/) | Generated-program differential checks and shrinking. | `GeneratorConfig`, `CampaignReport`, `run_campaign`; binary `evo-randgate` | `eqopt`, `evoir`, `memory`, `mir`, `proof`, `types` |
| [evocc-types](crates/evocc-types/) | C type identity, target layout and bit-exact floating values. | `CType`, `CanonicalType`, `TypeInterner` | `rustc_apfloat` |
