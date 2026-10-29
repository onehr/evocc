# Architecture

EvoCC compiles C for AArch64 Linux and macOS. The ordinary compiler and the
proof-required path have different guarantees. This page describes the code,
not a promise that every accepted C program has a proof.

## Compilation pipeline

The pipeline has the following responsibilities. The front end combines
preprocessing with lexing, and it elaborates types while it parses.

1. The preprocessor expands macros and includes headers.
2. The lexer supplies raw preprocessing tokens and literal spelling rules.
   The preprocessor uses those tokens; there is no second full lexing pass.
   Parser configuration checks and classifies surviving expanded tokens.
3. The parser reads syntax and constructs typed C-HIR directly.
4. The type interner supplies canonical C types and target layout results
   during parsing; it is not a later whole-program type pass.
5. C-HIR holds typed source declarations, expressions and statements.
6. Lowering builds EvoIR block-argument SSA and explicit execution obligations.
   Optional scalar EqSat proposes replacements and records certificates.
7. EvoMIR lowering selects machine operations and validates the module.
8. Checked out-of-SSA resolves block arguments before allocation.
9. Allocation places values in registers and stack slots; validation checks it.
10. The AArch64 backend lays out frames and prints platform assembly.
11. The platform assembler makes an object. The driver can invoke the linker.

The front end and each IR retain diagnostics and validation boundaries. A
refusal is not evidence that a C program has undefined behavior.

## Execution checks

C-HIR and EvoIR have separate Rust interpreters. They share the memory model
and bit-exact floating adapter; each executes the operations of its own IR.
Their tests compare results across IR levels and against native executions. Such comparisons are tests,
not proofs for every program.

The command `evo check` uses checked EvoIR execution. It reports the path that ran.
It does not examine all input values or prove that another path is safe.
The interpreter supports a bounded set of external runtime operations, not
arbitrary host library calls. EvoMIR also has an interpreter for backend tests.

## Checked compilation

The proof-required path admits the typed Core subset named `core-v1` and
the target `aarch64-unknown-linux-gnu`. The source export is the start of
the theorem. C preprocessing, parsing and elaboration are outside that theorem.

The driver exports each selected entry and its direct-call closure. The compiled
Lean checker `evoir-sem` checks the stage snapshots and the pipeline.
Acceptance connects the decoded typed Core program to final assembly text.
A checker rejection, unsupported profile or failed checker does not produce a
proof-labelled output. Loops require the supported cutpoint proposal.

Optional rewrite certificates run through the independent Rust
`evocc-checker`. In proof-required mode the Lean rewrite checker also checks
the rewrites. The pipeline theorem depends on that Lean check, not on trusting
the Rust producer or its choice of replacements.

See [the verification contract](spec/verification.md), especially sections 1,
4, 5, 7, 8 and 9, for the precise statement and assumptions.

## Drivers

`evocc` offers compiler actions and development IR actions. Its cc command
line runs the external assembler and linker; the development command line also
exposes the interpreters, exports and proof-required compilation.

`evo` offers run, check, explain, prove, build and repair commands, plus the
stdio MCP and LSP interfaces. A repair plan does not apply a repair. A repair
that needs review still needs human approval.

## Crates

The following table lists direct production dependencies. A short dependency
name has the prefix `evocc-`. Development dependencies are not included.
`rustc_apfloat` is an external dependency and has no `evocc-` prefix.

| Crate | Job | Main entry points | Direct dependencies |
| --- | --- | --- | --- |
| [evocc](crates/evocc/) | The compiler and user-facing command drivers. | `evocc`, `evo`, `evocc-checker` | `chir`, `common`, `cpp`, `eqopt`, `evoir`, `lex`, `lower`, `mir`, `parser`, `passes`, `proof`, `proofcheck`, `types` |
| [evocc-chir](crates/evocc-chir/) | Typed source trees, source checks and source execution. | `CHirUnit`, `CHirExpr`, `CHirStmt` | `common`, `memory`, `types` |
| [evocc-common](crates/evocc-common/) | Source positions, diagnostics and arena allocation. | `SourceSpan`, `DiagnosticCollector`, `Arena` | none |
| [evocc-cpp](crates/evocc-cpp/) | Preprocessing, macro expansion and header lookup. | `Preprocessor`, `TokenStream`, `MacroMap` | `common`, `lex` |
| [evocc-egraph](crates/evocc-egraph/) | Equality classes, hash-consing and congruence rebuilding. | `Egraph`, `EClassId`, `ENode` | `proof` |
| [evocc-eqopt](crates/evocc-eqopt/) | Scalar rewrite production and persistent equality state. | `EqualityState`, `ExtractionPolicy`, `run_scalar_chain_with_checker` | `egraph`, `evoir`, `proof`, `types` |
| [evocc-evoir](crates/evocc-evoir/) | Typed block-argument SSA, validation, text and execution. | `Module`, `validate_module`, `parse_target_module` | `memory`, `types` |
| [evocc-lex](crates/evocc-lex/) | C preprocessing tokens and literal spelling. | `Lexer`, `Token`, `TokenKind` | `common` |
| [evocc-lower](crates/evocc-lower/) | Typed C-HIR lowering into EvoIR. | `SourceLocations`, `lower_unit`, `lower_unit_with_locations` | `chir`, `common`, `evoir`, `types` |
| [evocc-memory](crates/evocc-memory/) | Shared interpreter memory and world state. | `MemoryState`, `WorldState`, `PointerValue` | `types` |
| [evocc-mir](crates/evocc-mir/) | Machine lowering, allocation, frame layout and assembly emission. | `MModule`, `Target`, `compile_module_with_entries` | `evoir`, `memory`, `proof`, `types` |
| [evocc-parser](crates/evocc-parser/) | C syntax analysis and typed C-HIR construction. | `Parser`, `ParseDiagnostic` | `chir`, `common`, `cpp`, `lex`, `types` |
| [evocc-passes](crates/evocc-passes/) | Development pass orchestration and validation records. | `ModulePass`, `PipelineReport`, `run_pipeline` | `evoir`, `proof`, `randgate`, `types` |
| [evocc-policy](crates/evocc-policy/) | Deterministic policy evaluation and bounded search. | `Corpus`, `PolicyReport`, `evaluate` | `eqopt`, `evoir`, `proof`, `randgate` |
| [evocc-proof](crates/evocc-proof/) | Shared proof records, text and checker process control. | `EvpCertificate`, `StandaloneProofChecker`, `parse_evp1` | none |
| [evocc-proofcheck](crates/evocc-proofcheck/) | Independent scalar certificate replay. | `main` | `evoir`, `proof` |
| [evocc-randgate](crates/evocc-randgate/) | Generated-program differential checks and shrinking. | `GeneratorConfig`, `CampaignReport`, `run_campaign` | `eqopt`, `evoir`, `memory`, `mir`, `proof`, `types` |
| [evocc-types](crates/evocc-types/) | C type identity, target layout and bit-exact floating values. | `CType`, `CanonicalType`, `TypeInterner` | `rustc_apfloat` |

## Contracts and evidence

- [Specifications](spec/README.md) define EvoIR, EvoMIR, diagnostics and verification.
- [Target descriptions](targets/aarch64/) hold machine and ABI tables.
- Crate unit and integration tests live under each crate; shared fixtures and
  retained receipts live in [tests](tests/).
- [Tools](tools/) run gates, generators, release packaging and program replays.
- The [book](book/src/SUMMARY.md) gives command examples and installation steps.
- [The tracker](TODO.md) records active work and links to closed history.

A receipt names the inputs and scope of one run. It is not a theorem and does
not replace an independent replay.

## Trust boundaries

The scalar producer can propose a wrong rewrite. The standalone checker
recomputes eligibility, facts and replay independently. Its normal dependency
closure excludes the producer and e-graph. Tests enforce that separation.

The Lean pipeline theorem is about decoded stage values and assembly text.
The compiled checker, its readers, the typed source export, the printing
boundary and the endpoint semantics have the trusted assumptions listed in
section 8 of the verification contract. The assembler, linker, runtime
libraries and hardware remain outside the theorem.

The ordinary compilation path has validation and test evidence, but no
proof-required label. New targets, broader source constructs and faster code
are separate work; this architecture does not make those claims.
