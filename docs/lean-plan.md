# EvoCC verification plan

## First public release verification

Plan revision 1, **2026-10-04**; inspected implementation `19bb5f7d`.
The owner requires a practical compiler-correctness path before the first public
version, followed by verified rewrites on that path. This section is the current
implementation plan; the scalar-proof plan below remains historical evidence.
**Current state (2026-10-05): V00, V01, V02 and V03 are complete in the frozen
`core-v1` scope. V04 and V05 are proved and integrated in proof-required mode;
they stay open until the full release gates run. One composed theorem for the
whole chain and V06 release acceptance are in work. No release acceptance is
claimed.** The driver mode `--proof-required` runs the V01, V05 (with
`-Oeq-scalar`), V02, V03 and V04 checkers. For the `core-v1` subset its record
says "checked from typed Core source to final assembly text". Limits: Linux
AArch64 with default code generation options only; no private frame objects;
allocations with edge bridge blocks and rewrite certificate forms that V05
does not support are refused; a loop entry needs a proposal file. The trusted
base is the source export, the Lean kernel and the compiled checker, the file
readers, the printer of the assembly AST, the Lean semantics of typed Core C
and of the AArch64 subset, the `AbiEntry` assumption, and the assembler, the
linker and the hardware.
Execution is tracked only in [TODO.md](../TODO.md#first-public-release-verification).

### Release outcome and boundary

Deliver a **verified compilation core**, not a claim that the entire supported
GNU C language, Linux, or the application ladder has been formally verified.
The release must contain one composed semantic-preservation theorem and an
executable, proved result checker for the admitted `core-v1` path:

```text
typed Core C-HIR -> EvoIR -> virtual EvoMIR -> out-of-SSA EvoMIR
    -> allocated EvoMIR -> framed AArch64 assembly AST

baseline path closed first
    -> existing P0/g0 rewrites admitted through a proved replacement path
    -> the same backend theorem and final artifact
```

CompCert's relevant precedent is composition from a processed C AST to an
assembly AST, with preprocessing/elaboration and assembling/linking boundaries
stated separately; it is not a promise to prove every tool or all of GNU C.
See its [semantic-preservation contract](https://compcert.org/man/manual001.html).
An EvoIR-to-MIR theorem alone is a backend result, not this release outcome.

The first theorem boundary is the serialized, typed Core C-HIR input and a
restricted assembly AST decoded from the **actual final emitted assembly**.
The current emitter produces text; this decoding boundary does not exist yet.
V04 must connect that output to the target semantics, rather than checking an
earlier MIR snapshot and assuming emission is correct. Preprocessing, parsing,
elaboration, faithful C-HIR export and faithful assembly decoding are disclosed
trusted boundaries. The platform assembler/linker/runtime and actual hardware
are outside this first theorem. Native differential tests check those boundaries;
they do not remove them from the trusted base. Do not insert an unproved semantic
normalization into either decoder or silently reinterpret a complete function
as a smaller arithmetic expression.

### Fixed implementation envelope

Use the existing compiler pipeline and existing Lean project, not a second toy
compiler. Start with the Linux AArch64 LP64 target; Darwin and kernel execution
remain existing tested capabilities, not automatically covered proofs.

| Area | Required for `core-v1` | Explicitly outside this first proof |
| --- | --- | --- |
| Values | 32/64-bit signed and unsigned integers, conditions, integer conversions within this set; total integer IR operations and their existing source-definedness guards | FP, vectors, complex, 128-bit integers, arbitrary-width C objects |
| Control | Structured sequencing, conditionals, short-circuiting, loops, break/continue, return; corresponding CFG joins and backedges | Computed goto, exceptions, setjmp/longjmp, arbitrary inline assembly |
| Objects | Non-address-escaping scalar automatic objects with independently checked definite initialization; their private lowered storage and compiler spill slots | Pointer parameters, heap, arbitrary aliasing, aggregates, volatile/atomic/TLS/global objects |
| Calls | Direct calls within an admitted, nonrecursive closure, integer arguments/results, including register exhaustion and stack arguments | Indirect/variadic/external calls and recursion |
| Backend | Operand widths and aliases, flags, simultaneous edge copies, spill/reload, ABI entry/exit, call clobbers, frame layout and normal prologue/epilogue | Unproved optional frame/selection/allocation transforms, unwinding, stack-protector/PAC-specific profiles |
| Optimization | First close a baseline with optional optimizations OFF; then compose admitted instances of frozen `p0.scalar.v4` / `g0.ub.v1` | New rewrite registries, branch-derived facts, interval-domain expansion, R03, memory rewrites |

This is a planning envelope, not an implemented parser whitelist. V00 must turn
it into an exact constructor/operation matrix using the existing source and IR
semantics, including signed conversions and independent shift-count widths.
Signed-left-shift rules excluded by the current guard contract stay excluded.
No new C semantic choice is implicitly approved here. Constructs without a
completed row must be rejected by the proof-required path, not treated as opaque
instructions that preserve an invented state. The ordinary compiler retains its
existing feature coverage and defaults.

Before proof implementation, freeze a small population in the existing fixtures:
at least one joined/backedge function, one actual spill/reload function, one
admitted direct-call closure exercising stack arguments, one checked diagnostic,
and **two unchanged function bodies from the existing pinned application sources**.
Inputs/signatures may not be simplified to get acceptance. V00 names the exact
functions, source identities and expected admission, plus nearby out-of-profile
negative cases. If this envelope admits no useful original functions, stop at
V00 and revise it explicitly; do not spend the next stages proving a zero-use
profile. Acceptance covers every frozen in-profile function, not just survivors.

### Theorem and execution contract

The schematic release theorem is:

```text
well_formed_core(S) /\ admitted_profile(P) /\ check_pipeline(S, A, C, P) = true
  -> refines(AsmSem(P, A), CoreSem(P, S))
```

`S` and `A` are both inputs to the checker; `C` contains proposed correspondences
and annotations, not trusted assertions. State the refinement relation before
implementing the checker: related entry states, returns, effects of admitted
calls, normal termination and divergence, and the diagnostic observations promised
by the checked profile. A finite interpreter fuel budget is not a proof of loop
behavior. Administrative moves/spills require a progress argument as well as
stuttering simulation; the compiler cannot introduce silent infinite execution.

Baseline closure first uses the existing **checked** profile. Preserve the first
supported diagnostic and prior observations; do not hide an incorrect trap behind
C UB. A separate unchecked corollary may quantify only source-defined executions;
it is not an automatic consequence of deleting `ub.assume`. Resource assumptions
such as valid ABI entry and adequate private stack storage must be explicit.

Prove small deterministic checkers in Lean; leave optimization, allocation and
annotation search in Rust untrusted. Check annotations for inductiveness at the
entry and on every reachable edge, including backedges; do not prove the search
algorithm merely to accept its result. Check original/candidate structure and
memory/frame separation as well as register liveness. Target `uses/defs` tables
alone cannot establish arithmetic instruction semantics.

New semantic/checker/composition theorems must be kernel-checked with no `sorry`,
admitted lemma or new semantic axiom. Extend the existing Lean audit to these
actual theorem declarations. Reuse old scalar lemmas only with their transitive
dependencies visible: existing native-backed rows do not become kernel-only by
being called from a new theorem. The first proved optimization profile admits
only instances whose entire required proof path is closed; the ordinary scalar
optimizer and its historical receipts are unchanged.

The first executable integration uses the compiled proved Lean checker, rather
than trusting agreement with a second Rust implementation. Its acceptance relies
on Lean's compiler/runtime and the host toolchain as well as decoding; disclose
that execution trusted base. A kernel-checked general soundness theorem is not
the same as a kernel-checked proof term for every invocation. See
[Lean's elaboration/compilation boundary](https://lean-lang.org/doc/reference/latest/Elaboration-and-Compilation/).
Use one result-checking path, not another audit service or identity framework.
Keep the pinned Lean toolchain and core-only dependency policy; do not make an
Aeneas migration, Mathlib import or the blocked N07 SMT generator a prerequisite.

### Work packages and concrete exits

Work in dependency order. Every package starts with minimal positive/negative
fixtures and ends with a bounded commit, theorem names, test results and claim
ceiling in the existing notebook/receipt conventions. These are deliverables,
not calendar estimates or already implemented interfaces.

| ID | Implementation surface and work | Exit; passing tests alone never closes a proof task |
| --- | --- | --- |
| **V00** | Freeze profile, source/target state and observation relations, trusted boundaries and pilot population. Specify the input/export format in the existing specs/Lean plan. | Exact theorem statement, operation/constructor coverage matrix, named original functions, minimal unsupported cases and per-stage REDs. No unbound source/target pair or wildcard admission. |
| **V01** | Core C-HIR and EvoIR small-step semantics in `lean/`; result-check the actual `evocc-lower` output against the source AST. Cover local storage, evaluation order, calls, guards and block arguments. | Source-to-EvoIR checker soundness for all admitted executions; reject missing effects, swapped evaluation, wrong conversions/edges and changed diagnostics. The existing N02 volatile-initializer mismatch remains a regression/refusal case, not silently normalized away. |
| **V02** | EvoIR-to-EvoMIR selection and out-of-SSA checks in `evocc-mir`, backed by Lean semantics. Include baseline DCE and every always-on lowering transform. | Composable selection/copy simulation; reject wrong opcode/width, sequentialized copy cycles, missing edge transfers and deleted observable instructions. No hidden unproved pass between checked snapshots. |
| **V03** | Replace the allocation guarantee with a proved two-input check around `generic/allocation.rs` / `generic/allocverify.rs`; track registers **and spill slots**, exact entry bindings and proposed block invariants. | Allocation soundness, including inserted copies/spills, slot reuse, calls and aliases. Reject wrong reload slot, clobbered live value, argument swap, changed source operation and false loop invariant. A self-consistent allocated program alone is insufficient. |
| **V04** | Frame layout, prologue/epilogue and decoding the final emitted text into the restricted assembly AST at the `generic/frame.rs`, `emit` and `native.rs` boundary. Compose V01–V03 with this checker. | **Baseline compilation theorem** from Core C-HIR to framed assembly, plus the frozen original functions accepted. Reject overlapping frame areas, missing restores, wrong stack argument offsets, truncating width views and changes after allocation validation. Unknown semantic assembly/directives refuse; decoder/native correspondence is separately tested and remains disclosed. |
| **V05** | Reuse current P0/g0 rules; prove admitted fact/poison/seed premises, replay, extraction and whole-function replacement in context. Integrate with `evocc-proofcheck` / `evocc-eqopt` without adding rules. | **Optimized compilation theorem** composed with V04; at least a P0 replacement and a g0 guard fold on the frozen path. Reject forged facts/rules, poison bypass, deleted obligations and splices outside the proved region. A local `RuleSound` theorem alone is insufficient. |
| **V06** | Driver proof-required integration and R05 release acceptance, using the existing gate/bench paths and a compact coverage record. | Missing/rejected/timed-out proof or unsupported constructor publishes no verified output. One frozen release subject passes Lean, full workspace and selected native/original tests; reported scope matches the exact artifact and admitted call closure. Ordinary output never inherits the proof label. |

V03's gap is already observable in the implementation: `verify_allocation`
accepts only `Allocated`; its state is register-to-virtual-value and its transfer
does not validate spill-slot contents. The allocator also inserts spills, drops
copies and compacts slots. Do not translate that checker unchanged and claim the
larger theorem. CompCert's [two-program structural and value validation](https://compcert.org/doc/html/compcert.backend.Allocation.html)
is the engineering reference, not a dependency on its implementation.

### V00 constructor and snapshot contract

`core-v1` is a selected typed-function closure, not an acceptance claim for its
entire original translation unit. The pilot is frozen in
[core-v1-pilot.json](../tests/contracts/core-v1-pilot.json). The Rust
`evocc_chir::core_profile::inspect_core` inventory visits original nodes,
including dead branches; it does not normalize them or certify compilation.
Independent Lean input validation is implemented in `Core/WellFormed.lean`;
its connection to the small-step execution semantics remains a V01 proof
obligation. Successful validation is not a source-to-EvoIR result check.
The pilot fixes `-O0 -fno-builtin`: ordinary-name `abs` recognition would
otherwise bypass the very definition whose source/target relation is required.

| Source row | Exact initial constructor set and restrictions |
| --- | --- |
| Types | `Int`, `UInt`, `Long`, `ULong`, `LongLong`, `ULongLong`; retain rank as well as width/signedness. Const is retained; volatile, atomic, explicit alignment and noninteger types refuse. |
| Leaves | Integer `Lit`; `Ident` of an active parameter or scalar automatic object. No global object or implicit external memory. |
| Unary | `Plus`, `Neg`, `BitNot`, `Not`; no address/dereference or builtin absolute-value substitution. |
| Binary | Add/sub/mul/div/rem, bitwise and/or/xor, six comparisons, short-circuit and/or, unsigned-left shift and signed/unsigned-right shift, comma. Independently promoted count width is retained. Signed-left shift remains outside this first source whitelist; its ordinary GNU compiler semantics is unchanged. |
| Conversion and choice | Integer `Cast`, three-operand `Ternary`, `Comma`. Implicit argument/return/assignment conversions retain their source and destination types; they are not erased in export. |
| Objects | `Object` declaration with automatic/register/no storage specifier, optional single root `StoreScalar` initializer, scalar `Assign` and `Update` on a named local. No static, cleanup, alias, memory-place update, or implicit zero-initialization. |
| Statements | `Null`, `Expr`, valued `Return`, `Compound`, `If`, `While`, `DoWhile`, `For`, loop-local `Break` and `Continue`. Explicit value-return on every conservatively reachable exit; no fall-off poison. |
| Calls | `Call(Ident, args)` to one prototyped, nonvariadic definition in the selected acyclic closure. Check the complete declaration chain; no inline/asm-name/attribute semantic changes. |
| All other constructors | Refuse, even if unreachable or accepted by the ordinary compiler. No opaque wildcard row. |

The inventory's definite-initialization analysis intersects continuing branch
states and uses incoming loop facts for body/continue/increment checks. The
independent Lean check recomputes these properties from the exported tree;
producer claims are not inputs. It additionally checks integer rank/result
types, binding/place/conversion consistency, const writes, signatures, scoped
lifetimes and unsequenced conflicting local accesses. Non-sequenced siblings
cannot supply each other's initialization premises. Comma, short-circuit and
conditional sequencing stay explicit. The policy conservatively refuses nested
writes to an assignment target and some otherwise defined C inputs; it is not
a claim to admit every defined C program. Calls must name preceding definitions
in the acyclic closure. The front-end's prior elaboration of macros/typedefs is
a disclosed input boundary. Proving this validator sufficient for the complete
small-step semantics remains open, as does the source/target checker.

**Source interface.** V01's `core-v1` export is a tagged tree, not `Debug` output
or an expression selected from the middle of a body. It contains a format
version, ordered entry symbol IDs, the ordered complete definition closure, each
function's symbol/name/return type/ordered parameter bindings and complete body.
Every object and identifier retains its numeric symbol ID and the six-way C
integer type above. Every expression retains its resolved result type; casts,
assignment operators, conditional arms, call arguments and initializer plans
remain explicit. Literals carry source integer type and exact width-sized
hexadecimal bits, never host floating-point or unbounded JSON numbers.
Statements retain lexical scopes and original order. Locations are diagnostic
metadata, not trusted semantic facts. The exporter is
`crates/evocc/src/core_export.rs`; the independent grammar reader and source AST
are `lean/EvoIR/Core/Source.lean`. Decoding is **not** well-formedness, a source
execution semantics, or a compilation proof. The separate input validator
below must not be confused with the still-unimplemented compilation checker.

The dependency-free wire format is JSON with fixed-arity tagged arrays:

```text
["core-v1", "aarch64-unknown-linux-gnu", "checked", entry_ids, functions]
["function", symbol, name, qualified_return, internal, parameters, body]
["parameter", symbol, qualified_type, storage]
qualified_type = [int_type, is_const]
int_type = "int" | "uint" | "long" | "ulong" | "long_long" | "ulong_long"
```

Every expression starts with its constructor tag and unqualified result type;
the complete operands follow in source order. Literal bits have exactly 8 or
16 lowercase hexadecimal digits. Symbol IDs are unsigned 32-bit integers.
Assignment/update nodes additionally retain the place ID/type and the original
target expression. `argument`, `return` and `store_scalar` nodes record their
destination type rather than silently applying a conversion. A `compound`
contains the ordered declaration/statement list; absent optional syntax is
JSON `null`, not an invented expression. The reader rejects unknown tags/types,
extra/missing fields, noncanonical literal bits and excessive syntactic nesting.
Its nesting bound is an input-reader resource limit, not execution fuel.

The existing driver and Lean evaluator expose this **input-only** boundary:

```sh
target/frontend/debug/evocc --emit=core-c-hir --core-entry=abs -O0 -fno-builtin \
  tests/fixtures/verification/musl-abs.c > /tmp/musl-abs.core.json
(cd lean && lake exe evoir-sem --read-core /tmp/musl-abs.core.json)
```

The reader prints `DECODED_NOT_VERIFIED`. Export refuses an unsupported selected
closure before publishing any tree, checks every relevant prototype declaration,
and does not modify ordinary compilation defaults or emit a verified artifact.
Passing this command is not the V01 whole-function checker gate.

Replacing `--read-core` by `--check-core-source` independently validates the
source and prints `WELL_FORMED_NOT_COMPILATION_VERIFIED`. Malformed syntax exits
2; a decoded but refused source exits 1; an accepted source exits 0 without any
target program or proof-output label. All five frozen entry exports pass this
input check, including the two-function pressure closure. Forty-four focused
input tests cover both admission and refusal, but neither those tests nor the
five existing primitive theorems prove this validator's execution safety.

**Source execution.** `Core/Scalar.lean` and `Core/Execution.lean` now execute
the complete exported source tree with typed integer values, initialized local
slots, explicit expression/statement continuations, scope exit and isolated
call frames. The transition relation is unbounded; `Core/Divergence.lean`
constructs an actual infinite trace for `while (1) {}`. Kernel-only lemmas also
establish deterministic steps, the bounded runner's finite-prefix property,
terminal diagnostic preservation and the typed arithmetic/primitive bridge.
These do not yet establish input-validator soundness or source/IR simulation.

The machine preserves the existing executed source-literal wrapping rule for
add/sub/mul/neg; object reads and comma expressions are not constant operands,
and literal classification cannot erase a reached MIN/-1 failure. Ordinary
variable overflow still reports E3004. Shifts decode the independently typed
count before checking its bounds. Malformed states (including bad scope,
uninitialized local or signature mismatch) are distinct from diagnostics; V01
must prove they unreachable on the admitted path rather than label them safe
termination. Source execution remains the existing deterministic left-to-right
policy, not a new license for unsequenced conflicting accesses.

`evoir-sem --run-core SOURCE.json ENTRY STEP_LIMIT [0xARG ...]` is a bounded
test observer only. Returns/diagnostics print `EXECUTED_NOT_VERIFIED`; reaching
the step bound prints `STEP_LIMIT_NOT_VERIFICATION`, never success or divergence.
Twenty-two focused cases, the five frozen entry exports, and the ten-function
`source-execution.c` closure exercise this path. The unchanged original bodies
remain in that closure; its result matches both existing Rust interpreters.
The general EvoIR result checker and its soundness theorem remain OPEN.

**Target execution.** `Core/IRReader.lean` independently reads the existing
canonical EvoIR text into a distinct SSA/CFG tree. It retains all admitted
types, integer widths, block arguments, effect fields, obligation reasons,
private allocation layouts, call signatures and memory/world operands.
Unknown operations/metadata, duplicate or nonlocal bindings, wrong arities,
widths and edges refuse. `IRWellFormed.lean` additionally recomputes acyclic
call effects and checks current-token forwarding and matching access guards.
Its execution-safety theorem, like the source validator's, remains open.

`Core/IRExecution.lean` executes one instruction/edge/call transition at a time,
with immutable byte/initializedness snapshots and frame-owned fresh objects.
Edges simultaneously bind captured arguments; calls isolate SSA bindings and
return ends their private object lifetimes. It preserves independent shift
widths, poison transport versus consumption and the first reached diagnostic.
`--read-core-ir TARGET.evo` reports `DECODED_NOT_VERIFIED`;
`--run-core-ir TARGET.evo ENTRY LIMIT [0xARG ...]` also checks the target input
and reports only `EXECUTED_NOT_VERIFIED` or a non-verifying step limit.
Neither command compares source and target or publishes a verified artifact.
The five frozen entries and the whole original-body closure execute through
this independent target path; 51 focused target cases pass.

**First full-body correspondence case.** `Core/AbsCorrespondence.lean` now
proves the complete original `abs` source body against its actual four-block
EvoIR output for every 32-bit argument, including E3004 at INT_MIN. The theorem
`pilot_pair_refines` takes both complete programs as inputs and accepts only
the exact proved pair, using structurally recursive, proof-producing equality
over the full source AST and target CFG. It does not extract an expression,
ignore unused block arguments/effects, or compare a hash/pretty printer.

The shared `LoweringRefines` proposition covers terminal observations,
divergence and progress. The universally proved finite witnesses for this
terminating function, together with deterministic-transition lemmas, exclude
any different terminal outcome or added infinite execution. No execution-fuel
bound appears in the final refinement proposition. This exact original-function
case is a bootstrap for the general result checker, not a narrowed definition
of `core-v1`: the other frozen functions, loops, calls and storage obligations
remain required. It does not prove a backend pass or native artifact.

`evoir-sem --check-core-pilot-pair SOURCE.json TARGET.evo` runs this proved
decision procedure on independently decoded complete inputs, with no sample
arguments or execution limit. It prints `PROVED_PILOT_PAIR` with explicit
`scope=source-to-evoir backend=UNPROVED core-v1=INCOMPLETE`; a decoded unproved
pair exits 1, malformed input exits 2. The retained original exporter output
passes, as do 36 positive/negative input cases. After the owner reopened bounded
verification, the clean Lean gate passed with 14 kernel-only core targets; the
full workspace passed 3,080 tests, zero failures and five ignored, plus Clippy
and formatting. An observed audit expected-type printing mismatch was corrected
without relaxing exact proposition matching. This closes that case's integration,
not V01's generic checker or any release admission.

**Reusable simulation and local storage.** `Core/Simulation.lean` proves that
checked local diagrams imply the same terminal/divergence/progress contract.
A target step either matches a positive source path or strictly decreases a
natural administrative rank while retaining the source state. Consequently a
target-only loop cannot masquerade as compiler bookkeeping. Entry membership,
terminal observations and progress remain separate obligations; proposing a
relation or rank is not proof. `Refines.trans` composes checked stages without
assuming equal step counts. The concrete `ranked_lowering_refines` adapter uses
the actual source and IR machines, not replacement semantics.
`Core/LocalStorage.lean` proves read-after-write, noninterference for other local
identifiers and scope-depth preservation for the existing execution functions.
These are generic proof components, not an executable generic pair checker.
Instantiating and independently checking the diagrams for actual CFG edges,
calls and private-leaf substitution remains V01's next implementation obligation.
The clean Lean gate at `dbd98afb` records 19 kernel-only core targets.

**Executable target fragments.** `Core/SymbolicScalar.lean`,
`SymbolicBindings.lean` and `SymbolicInstruction.lean` now check initialized
integer instruction segments against typed symbolic expectations. Their
soundness quantifies over every valuation, checks SSA/result widths and preserves
the ordered assumption trace, including the first diagnostic. Unknown parameter
values may be carried unchanged but cannot be consumed as integers. The
single-instruction theorem is linked to the actual `IR.execute` arm. Calls,
objects and CFG traversal are refused by this local component, not asserted to
preserve state. The source expectation and the entry/edge relationships still
need independent justification; a successful fragment check is not V01 admission.
The clean gate at `88e7f458` records 22 kernel-only core targets.
`SymbolicSource.lean` additionally proves the six source types' conversions and
source-directed add/sub/mul checks, including usual arithmetic conversions and
their signed/unsigned diagnostic distinction. `segment_execution_sound` connects
the complete finite instruction segment to actual `IR.Steps`, not just its local
evaluator. These do not yet prove source expression, edge, loop or call diagrams.
The `6321f218` checkpoint passes the clean Lean gate (25 kernel-only core
targets) and full workspace (3,080 passed, zero failures, five ignored;
Clippy/format pass). The pilot receipt records the scope, logs and 3.86 GiB
cache/capture increase, mostly the reused frontend Cargo target. This remains
V01 component evidence, not a stage-completion or verified-release claim.

**Source-derived diagrams and edges.** `Core/SourceExpression.lean` now derives
the target obligation from the original expression tree (initialized reads,
literals, six-type casts, add/sub/mul and admitted shifts), rather than accepting
an unproved expected term. Its theorem connects both actual small-step machines,
for every valuation and source continuation, including ordered diagnostics and
the literal-only wrapping exception. The original `FixedMul` expression, fifteen
source/IR mutations and a consistently changed pair exercise this component;
they do not constitute a second complete
function proof. `Core/SymbolicEdge.lean` checks exact successor parameters and
simultaneous integer/token transport against the supplied target CFG, with
proved actual unconditional/conditional steps and backedge copy regressions.
The enclosing program checker still must establish entry-local/SSA relations,
source control/loop invariants, calls/storage and private-leaf substitution.
Neither a successful expression nor an edge check is whole-program admission.

**Whole-leaf checking.** `Core/SymbolicEntry.lean` checks both machines' actual
parameter binding and proves coverage of every argument representation of the
original signature. `Core/LeafCorrespondence.lean` assembles source-derived
expression diagrams, complete return bodies (including compound scopes), return
conversion and the real target terminator. `LeafPair.refines` proves terminal
observations, divergence and progress from actual initial states for all typed
arguments, without an assumed local/SSA relation or execution fuel. The
executable `evoir-sem --check-core-leaf-pair SOURCE.json TARGET.evo` admits the
original complete `FixedMul` pair and consistent changed pairs, not a named
function or exact-tree whitelist. Sixty-five runtime cases include the original
pair, source/target mutations, parameter order and all 36 return-conversion type
pairs; fifteen kernel examples include a universal two-argument refinement.
This is the straight-line leaf case of the required checker. The branching
extension below reuses its entry and return contracts. Loops, local writes and
storage, direct calls and private-leaf substitution remain required.
It grants no backend or first-release verification label and does not close V01.

The source-derived expression path also covers bitwise AND/OR/XOR with C usual
arithmetic conversions and retained source ranks. The parametric
`SourceOperators.bitwise_sound` bridge (namespace `Core.Symbolic`) connects the
actual source operation to the target term. Whole-leaf tests cover all 108
operator/type pairs and refuse 108 mismatched target operators. These tests
extend the same checker; they do not change the remaining control/state/call
obligations or turn a passing local build into release acceptance.

**Branching return checking.** `BranchExpression.lean` constructs a source
decision tree with ordered guard failures and distinct conditional arms.
`BranchTarget.lean` independently checks actual instruction lists, conditional
terminators, joins and simultaneous edge transport. `ReturnPair.refines` in
`ReturnCorrespondence.lean` connects both complete machines from their actual
initial states for every original-signature argument, including first
diagnostics, terminal observations and divergence/progress. The existing
`--check-core-leaf-pair` entry now uses this structural checker for both original
`FixedMul` and musl `abs`; neither name nor exact source/target tree is special.
Changing both function names or the comparison threshold consistently still
passes. Changing a branch, used join argument, guard or diagnostic refuses.
An unused join argument may change because its value is not observed.

Six comparisons retain C usual arithmetic conversions; the proved truth bridge
connects C's int-valued comparison to the target's one-bit branch condition.
All four unary operators preserve the source wrapping exception and ordinary
negation guard. Local tests cover 216 comparison/type pairs, 648 mutations,
24 unary/type pairs and 26 branching whole-return cases. Eight kernel examples
include universal original-abs admission and refinement. Its CFG exploration
bound rejects cycles; it is not a bounded-execution proof of loops.

**Stateful expression extension (October 5).** `SymbolicStorage.lean` checks
exact parameter/local storage, first-binding lookup, uninitialized masking,
const initialization and assignment, declarations and scope exit. `StateExpression.lean`
composes the supported numeric operators with casts, conditional expressions,
comma, assignment and short-circuit AND/OR. Each diagnostic leaf retains
the actual local store after preceding effects; an unselected arm is not run.
`state_expression_sound` connects successful construction to the original
source small-step machine for every valuation and continuation. `StateEntry.lean`
checks exact parameter storage, successful scope depth and erasure of private
locals from the return observation. The existing `ReturnPair.refines` path now
uses these stateful diagrams; no separate program-pair checker or changed source
semantics was introduced (`Source.place` was made public only for its proof).

The actual exporter pair for `tests/fixtures/verification/stateful-return.c`
passes, and 11 source/target mutations refuse. The previous original `FixedMul`
and `abs` cases remain accepted. The first stateful checkpoint `74b33c80` passes
the clean Lean gate (42 kernel-only core targets) and full workspace (401 suites,
3,080 passed, zero failed, five existing ignored; Clippy/format pass).
The extension now uses one assignment continuation for simple and supported
compound operations, preserving the RHS evaluation, usual arithmetic conversion,
destination conversion and write in their actual order. All four pre/post
increment/decrement forms preserve the old/new result distinction and first
diagnostic. Actual `compound-return.c` and `update-return.c` exporter pairs pass;
dropping the second update's guard is refused even though its stored value is
dead at return. There are 21 whole-return cases, 37 stateful expression
outcome/refusal examples, 18 storage boundaries and 136 comparisons against the
actual source machine. The assignment-extension clean Lean gate passes at
`1746ceed` (42 kernel-only core targets; 1,246,159 vector answers).
This is **not V01 completion**: div/rem and their compound forms, statement
sequencing/declaration integration, loops, calls/private-leaf substitution and
validator execution safety remain required. Storage adapter lemmas alone do
not admit a complete function containing local declarations or loops.

**Finite-statement integration (October 5).** The same whole-function checker
now consumes `FlowDiagram` proofs for declarations (including const
initialization), statement sequencing, conditional statements and scope exits.
Normal, returned, break and continue exits stay distinct; only normal flow
executes the rest of a block. Faults retain the preceding writes and the active
scope rather than performing cleanup or later expressions. Successful scope
exits are proved against `Source.resumeFlow`, replacing the older return-only
same-depth shortcut. A function must return on every admitted successful path;
fallthrough and escaping loop control refuse.

The actual `statement-return.c` exports pass, and the unchanged frozen
`short_circuit` source export is checked against the complete original
`control.evo` module. Selection of a named target function does not confer a
proof on unrelated module functions. One observed representation mismatch
(C integer truth versus a one-bit IR constant) is handled by a proved literal
branch normalization; runtime guards are retained and no P0 rule/allowlist
changes. Local tests add 54 exact-store/source-machine comparisons, 23 kernel
statement examples, five constant-branch examples and 28 complete-pair tests
(four positive and 24 mutations). The clean Lean gate passes at `6c532776`
(44 kernel-only core targets; 1,246,159 vector answers); seven actual export
pairs replay successfully. Full workspace at that checkpoint passes 401 suites,
3,080 tests, zero failures and five ignored, plus Clippy/format. Div/rem, cyclic
control, calls/private-leaf substitution and input
validator safety still block V01; no frozen constructor or pilot was removed.

One concrete V01 correspondence obligation is now visible: the ten-function
source-execution fixture emits nine EvoIR functions at the frozen `-O0`
options. Existing C-HIR private-leaf substitution removes `frame` and embeds
its body in `caller` (`inline.rs`'s `private_leaf`). The actual source-to-target
checker must cover that transformation, not silently substitute the post-inline
tree for its declared original input or omit the callee from the source proof.

**Cyclic span construction (October 5, not loop admission).** Source and target
builders now construct proof-carrying finite spans between proposed cutpoints.
Every root executes a positive number of actual machine steps; a cutpoint
cannot justify itself by a zero-step edge. Source continuations preserve loop
tests, increments, break/continue, scope exits and the first diagnostic. Target
spans read actual terminators and perform simultaneous block-argument binding.
The bound limits syntax exploration between cutpoints, not loop iterations.
`checked_cutpoint_refines` proves unbounded refinement, including divergence,
conditional on every outgoing span and its successor-invariant closure being
checked. This is not an assumption that the loop terminates.

The frozen `loop_join` exports have four matching symbolic spans (entry, for,
while and do-while), six refused zero-step roots and 16 refused mutations. The
test's fixed point associations are test data, not a name-specific admission
path. An executable, independently checked successor-invariant instantiation
and connection to the actual function entry are still required before the
whole-function checker may admit `loop_join`. Existing seven finite-function
admissions are unchanged. At `8cf85cc6`, the clean Lean gate passes (47
kernel-only core targets), and full workspace passes 3,080 tests with zero
failures and five ignored, plus Clippy/format. The earlier sandboxed attempt
had two container-permission failures and remains a failed attempt.

**Whole-loop correspondence (October 5 continuation).** `CutpointStorage`,
`CutpointInvariant` and `LoopReturn` now close those two obligations. Fresh
valuations cover the complete typed store, including qualifiers and
initializedness; checked projections relate it to target block arguments.
Every proposed point and successor is checked, and `LoopPair.refines` connects
the table to both actual function entries for all typed arguments. The original
`loop_join.json` and complete `control.evo` pass the public interface; the seven
finite-function admissions remain green. A divergent loop is also admitted;
six zero-step roots and 30 mutations refuse. These are source-to-EvoIR results,
not a V01-completion or backend claim. The clean Lean gate at `493aa9ac`
passes with 49 kernel-only core targets and 1,246,159 vector answers; the
workspace gate passes 401 suites / 3,080 tests / zero failures / five ignored,
plus Clippy/format. Div/rem, calls/private-leaf substitution and validator safety
remain. The six cases in `division-return.c` are the next observed RED, not
admitted functions or a new P0 rewrite domain.

```sh
lean/.lake/build/bin/evoir-sem --check-core-loop-pair SOURCE.json TARGET.evo CUTPOINTS.json
```

`CUTPOINTS.json` is an untrusted proposal with grammar
`["core-cutpoints-v1", span_budget, [[site_index, block_label, store_shape, links], ...]]`.
`site_index` addresses `functionSites`' deterministic preorder over the actual
source tree (statement sites and loop test sites, retaining continuations).
A shape row is `[symbol, integer_type, is_const, initialized]`; a link is
`[target_binding, source_symbol, integer_type]`, in complete target-scope order.
Integer types use the source wire spellings. The budget is in `1..4096` and
limits span construction, not execution or loop iterations. Wrong or incomplete
proposals refuse; syntactic enumeration is not a trusted fact source. The
interface takes one source function and may select its target from an unchanged
multi-function IR module. Other target functions receive no proof label.
`tests/fixtures/verification/loop-join.cutpoints.json` is proposal data, not
hard-coded admission logic. Malformed wire input exits 2; failed checking exits
1; successful checking exits 0 and explicitly retains `backend=UNPROVED` and
`core-v1=INCOMPLETE`. V01 and V02–V06 remain open.

**Division correspondence (October 5 continuation).** A separate compilation
term language adds div/rem without changing the frozen P0 language or registry;
`Term.of_p0_sound` proves its existing-operation embedding. `division_sound`
and `division_trace_sound` connect usual arithmetic conversions and exact
truncation/remainder bits to the existing source and IR machines, including
E3002/E3003/E3004. Finite and cyclic proof diagrams normalize repeated identical
guards only after the first successful check; neither input IR is rewritten.
Baseline target admission separately requires matching explicit div/rem
obligations in the current block, so intrinsic traps cannot mask their deletion.
All six original `division-return.c` pairs now pass the public interface, as do
the nested, conditional and looping cases in `division-control.c`. The tests
cover 144 mixed-type/compound pairs, 1,060 refused mutations and 11 original
control cases. At `b718a856`, the clean Lean gate passes with 53 kernel-only
core targets and 1,246,159 vector answers. Full workspace passes 401 suites /
3,080 tests / zero failures / five ignored, plus Clippy/format. Calls,
private-leaf substitution, validator safety and the complete frozen population
still block V01; backend stages and release acceptance are unchanged.

Disk checkpoint including the developing call boundary: measured `.lake` plus
capture growth since the whole-loop checkpoint is 5,128 KiB; `target/frontend`
is unchanged. Host free space is 51 GiB; capture is 10,404 KiB against its
500 MiB cap. No extra build tree or runtime image was created.

**Finite direct-call correspondence (October 5 continuation).**
`CallPair.refines` connects original source closures to actual IR call/return
frames, for every typed input. Program-relative source diagrams preserve argument
order, destination conversions, caller-local restoration and the first fault,
including faults halted inside a callee. Target diagrams check actual callees,
arguments/results, signatures and forwarded memory/world tokens; they admit no
allocation and preserve the core entry's empty private memory, rather than
stripping state parameters. The original `converted_caller`, `checked_caller`,
`nested_caller` and 32-argument `pressure` pairs pass the existing
`--check-core-leaf-pair` interface, which prints `PROVED_CALL_PAIR` for this path.
Nine execution/diagram controls, six source-boundary refusals and 18 target
mutations pass. At `1b859487`, the clean Lean gate passes with 61 kernel-only core
targets and 1,246,159 vector answers; full workspace passes 401 suites / 3,080
tests / zero failures / five ignored, plus Clippy/format. All five original
named pilot entries now replay successfully (`FixedMul`, `abs`, `short_circuit`,
`loop_join`, `pressure`); this does not close the ten-function combined fixture.
Calling statement forms beyond the implemented finite-return
composition, calls combined with loops/private storage, private-leaf substitution,
validator safety and complete constructor/closure coverage still block V01.
This is neither backend nor verified-release acceptance.
The actual `caller` export still contains `frame`, while the unchanged target
has inlined it; the pair refuses with exit 1 (`private-leaf-red.log`).
Slice artifact net: **+4,196 KiB** across `.lake` and the capture;
`target/frontend` unchanged, capture 10,780 KiB, host free 51 GiB. Three temporary
proposition dumps (8,512 bytes) were removed after a no-open-handle check; their
statements/dependencies remain in the committed audit/receipt and are regenerable.

**Private-leaf source/target correspondence (October 5 continuation).**
The unchanged `source-execution.c` caller export retains its `frame` definition
and call; the actual O0 EvoIR substitutes the private leaf. This complete pair
now passes `--check-core-leaf-pair` for all signature arguments. `CallFlow.lean`
extends the existing symbolic statement trees with program-relative execution:
callee faults retain their real activation; successful calls restore the caller,
then declarations, sequence and scope exits follow the original source machine.
The target diagram now checks unconditional CFG edges and the actual final
return block, including simultaneous argument transport. Uninitialized scalar
placeholders retain poison and cannot be read as numeric terms or returned.
No source rewriting, IR cleanup or changed compiler default is used for admission.
Focused selftests pass 17 call execution/diagram controls, five complete pairs,
six call-boundary refusals, three source mutations and 33 target mutations.
At implementation `b7a0a70e`, the clean Lean gate passes with 64 kernel-only
core targets and 1,246,159 vector answers; full workspace passes 401 suites /
3,080 tests / zero failures / five ignored, plus Clippy/format. All five frozen
named entries and the original private-leaf pair replay successfully.
Calling expressions beyond the
implemented forms, cyclic/private-storage call composition, complete constructor
coverage and validator execution safety still block V01; V02–V06 remain open.
The complete `main` closure still refuses. Five isolated calling-expression
REDs in `tests/fixtures/verification/call-control.c` independently validate as
source but refuse pair checking: choice, short-circuit and/or, assignment and
comma. They are next-work inputs, not passing proof evidence.
After the receipt synopsis correction, the clean gate at `0724fcc9` passes
again and reproduces the committed receipt byte-for-byte. Final slice artifact
growth is **2,440 KiB** across `.lake` and the capture; `target/frontend` is
unchanged. The capture is 11,948 KiB against its
500 MiB cap; host free space is 51 GiB. One regenerable proposition dump
(2,520 bytes) was removed after checking for open handles.

**Calling-control correspondence (October 5 continuation).** Implementation
`0eec2a26` closes all five original `call-control.c` REDs without changing their
source or target. Source diagrams compose unary, conditional, short-circuit,
assignment and comma evaluation with actual callee execution. Target result
trees retain conditional edges and bind the caller continuation only at a
successful selected callee return. Proved diagram normalization handles literal
width casts and the observed retest of an int-valued boolean; it does not change
either input, interpreter, guard order or P0/g0 rules. Four additional original
`call-branch-return.c` pairs cover branching callees, nested arguments, calls on
both short-circuit sides and caller-local assignment. The new selftest passes
nine whole pairs, 31 source/target execution controls and 21 target mutations;
the prior call suite remains green. Clean Lean passes with 69 kernel-only core
targets, zero native dependencies and 1,246,159 vector answers. Full workspace
passes 401 suites / 3,080 tests / zero failures / five ignored, plus Clippy/format.
All five frozen primary entries and the original private-leaf pair replay.
The next minimal REDs are `loop_call` and `call_loop` in `call-loop.c`: both
closures are well-formed and execute consistently, but correspondence refuses.
The current loop interfaces must compose program-relative source spans and
actual target call/token continuations. This, private storage, full constructor
and closure coverage, and validator execution safety still block V01; V02–V06
remain OPEN. No release or compiled-backend theorem is claimed.
This slice's build/capture disk delta is **+4,748 KiB** (`.lake` plus capture;
`target/frontend` unchanged); the capture is 13,132 KiB against its 500 MiB cap.
The temporary diagram-inspection source was removed after an open-handle check.
Receipts, fixtures and original inputs remain; host free space is 52 GiB.

**Calls inside cyclic spans (October 5 continuation).** `loop_call` now passes
against its unchanged original export. Source spans are relative to the actual
program; target spans execute the actual callee, restore the caller stack and
owner, and carry the advanced frame counter to the next invariant. A frame
counter is not reset or assumed constant across a loop iteration. The shared
whole-loop checker now supports scalar-only and memory/world signatures and
checks the complete original closure. Empty-memory/world transport remains the
admitted call profile; this is not private-allocation support.

`core-cutpoints-v2` retains the v1 budget and source-site meanings and adds a
fifth row field: token links `[[target_id,"mem"],[target_id,"world"],...]`.
These links precede the numeric projection; exact entry and successor scope
comparison must justify them. Unknown token kinds refuse. V1's four-field rows
continue to work unchanged. Both formats are untrusted proposals, not new IR
semantics, source facts or optimizer assumptions.

Four complete original pairs pass: the initial RED, a branching callee,
callee-side signed overflow and an infinite calling loop. Ten execution/frame
controls and 15 target, 11 proposal and three source mutations pass. The five
frozen primary entries and original private-leaf pair replay. The first replay
attempt accidentally paired `caller.json` with `pressure.evo` and correctly
refused; the corrected original-target replay is retained separately. The
development axiom/proposition audit passes. Implementation `10f0c558` passes the
clean Lean gate (69 kernel-only core targets, zero native dependencies,
1,246,159 vector answers) and full workspace (401 suites, 3,080 passed, zero
failed, five ignored; Clippy/format pass). Receipts are in the pilot manifest.
Build/capture disk delta is **+4,612 KiB**, with `target/frontend` unchanged;
the capture is 14,260 KiB of its 500 MiB cap and free space is 52 GiB. No new
target, clone, kernel tree or VM disk was created; protected artifacts remain.

`call_loop` still refuses with a valid entry proposal: calling a cyclic callee
needs a continuation-aware simulation, not a larger finite call budget.
That composition, private storage, full constructor/closure coverage and
validator execution safety still block V01. V02–V06 remain OPEN.

**Cyclic callee contexts (October 5 continuation).** Implementation `854f6b9e`
strengthens the same checked spans and successor associations to quantify over
the actual suspended source caller and target return stack. A source return
restores the caller's locals, continuation and function identity. An IR return
uses the actual result bindings, owner and accumulated frame counter, retaining
binding failures rather than fabricating a successful return. A returned source
cursor with a residual callee continuation is refused.

`LoopDiagram.call_refines` connects both actual call-entry transitions to the
cyclic invariant and a separately proved caller continuation. Normal return
rejoins that continuation; a callee diagnostic remains terminal; divergent
callee execution is covered without assuming termination or unfolding a fixed
number of iterations. The theorem is kernel-only and its complete conditional
statement is recorded by the existing audit. The clean Lean gate passes with
70 core targets and 1,246,159 vector answers. Six suspended-context execution
controls and the residual-return negative pass, as do the existing pair and
mutation tests. Acceptance details are in `cyclic_callee_context` in the pilot
manifest. Full workspace completes with 401 suites / 3,080 passed / zero failed /
five ignored, Clippy/format pass. Ten original pairs replay; `call_loop` remains
an expected refusal. Build/capture disk delta is +3,108 KiB, target/frontend is
unchanged, capture is 15,400 KiB of 500 MiB, and 51 GiB remains free. No disposable
payload is retained and no protected artifacts were deleted.

This is a **composition theorem, not automatic caller admission**. Its caller
continuation premise must still be constructed from the actual source and IR.
The unchanged `call_loop` remains refused until argument/frontier handling and
post-return expression checking use this theorem. Private-storage composition,
full frozen coverage and validator execution safety also remain required.
V01 is OPEN; V02–V06 are not implemented. No scope or interpreter was changed.

**Checked return construction (October 5 continuation).** Implementation
`51b27bd2` constructs the post-call source/IR suffix instead of leaving its
simulation as an external premise. `resumeExpression` continues a symbolic
returned value through binary operations and conversions; the IR diagram starts
at the actual post-call instruction position. `checkedReturn` reserves an
independent return variable by proved renaming of all saved caller terms,
locals and IR bindings. `LoopDiagram.call_checked_return` composes this
constructed suffix with the cyclic callee's checked diagram.

Six source continuation REDs now pass, including overflow, division by zero,
right-operand effects/calls and first-fault order. The unchanged `call_loop`
return suffix is accepted; eight suffix mutations refuse and 18 fresh-return
controls pass. Clean Lean passes with 72 kernel-only core targets and 1,246,159
vector answers. Full workspace passes 401 suites / 3,080 tests / zero failures /
five ignored, plus Clippy/format; ten original whole pairs replay. The full
`call_loop` entry still refuses: source argument/frontier and IR call-site
prefix construction, and checked selection of the callee cutpoint proposal,
remain required. A caller entry proposal is not a callee invariant. Private
storage, full frozen coverage and validator safety still keep V01 OPEN;
V02–V06 are unchanged. Receipt: `checked_return_continuation` in the pilot
manifest. Artifact disk delta +3,252 KiB, target/frontend unchanged, 51 GiB free.
Thirteen temporary intermediate logs/receipts (820 KiB) were retired; RED,
final gates/replay and protected artifacts remain.

**Whole cyclic caller entry (October 5 continuation).** `60adb247` constructs
the actual source argument/frontier and IR call-site prefixes, selects and checks
the callee's own cutpoint proposal, and composes its proved return continuation.
`CallFrontierPair.refines` covers all typed arguments, first diagnostics,
progress and unbounded execution. The proof-producing executable is also in the
existing dependency audit. The unchanged `call_loop` now passes whole-entry
checking; the old caller-only proposal still refuses, as it must.

The same `--check-core-loop-pair` command accepts function-scoped proposals:
`["core-call-cutpoints-v1", caller_budget, [[source_function_id, cutpoint_plan], ...]]`.
Each embedded plan uses the existing cutpoint grammar for that source function;
duplicate/unknown function IDs refuse. This is untrusted proposal data, not a
name-specific admission rule. The frozen source and IR are unchanged.

Six source-frontier controls, three whole-pair controls, 21 mutation refusals,
three execution/frame controls and 11 original pair replays pass. Clean Lean:
74 kernel-only targets, 1,246,159 vector answers. Full workspace: 401 suites /
3,080 passed / zero failed / five ignored, Clippy/format pass. The first-fault
test uses runtime arithmetic, not the profile's separately handled wrapping
constant expressions. Receipt: `whole_cyclic_caller` in the pilot manifest.
Four valid-source REDs in `call-loop-context.c` retain the remaining gaps:
right-operand, assignment and nested-call source frontiers, and post-call
conditional resumption. Their target frontiers and callee diagrams already
construct. They are not PASS. These contexts, private-storage composition,
full coverage and validator safety keep V01 OPEN; V02–V06 remain unimplemented.
Measured build/capture delta +4,292 KiB; target/frontend unchanged, 51 GiB free.
Two intermediate logs and the temporary phase probe were retired; final
evidence and protected artifacts remain. No backend/release label is granted.

**Saved calls and statement contexts (October 5 continuation).** `a1338248`
and `014bfa62` close the four preceding context REDs, including nested cyclic
calls and fresh renaming of saved operands. `e5db8d5e` additionally closes
initializer calls, post-call `&&`/`||`, and branches with a return-only arm.
The same proof constructor checks both frontier arms; a terminal source suffix
may take zero steps, but cyclic spans still require positive progress. Return
matching reuses the proved constant-cast normalization, not numeric samples.
Source/IR execution semantics, P0/g0 and the frozen profile are unchanged.

Nineteen original pairs replay, including all eight newly closed contexts.
The new statement-context tests cover eight original/renamed pairs, 17
mutations and 20 execution controls; 19 source-resumption and nine frontier
controls pass. Clean Lean at `e5db8d5e`: 77 kernel-only audited core targets,
no native axioms, 1,246,159 vector answers. The completed full Rust workspace
gate at `014bfa62` is 401 suites / 3,080 passed / zero failed / five ignored,
Clippy/format pass. Rust sources and that gate are unchanged; this is retained
Rust-baseline evidence, not a new full-workspace run at the later Lean commit.

`call-loop-short-right.c` retains two valid-source REDs: a cyclic call as the
right operand of `&&` or `||`. Its source frontier still needs the selected
path premise preserved through the suspended logical frame. These, private
storage, full closure coverage and validator safety keep V01 OPEN; V02–V06
remain unimplemented. Receipts: `saved_call_contexts` and
`call_statement_contexts` in the existing pilot manifest. Combined artifact
delta +6,396 KiB; target/frontend unchanged, capture 16,284 KiB of 500 MiB,
53 GiB free. Temporary probe source removed; protected evidence retained.

**Logical RHS call closure (October 5 continuation).** `9ab35d27` closes both
`call-loop-short-right.c` REDs without changing either execution semantics.
`cutMatchUnder` establishes premises only at matched branches; root admission
starts with none. `PathConditions.for_return_sound` transports those premises
with saved caller values, so a fresh return cannot capture an old condition.
This is scoped source/target correspondence, not an EqSat fact-domain extension.
Twenty-one original pairs and the retained caller-only refusal replay. The
new tests cover four original/renamed pairs, 18 mutations, 14 execution controls,
15 premise-isolation controls and 24 six-type logical-resumption diagrams.
Clean Lean at this commit passes all steps with 79 kernel-only core audit
targets and 1,246,159 vector answers. The Rust baseline remains `014bfa62`, not
a newly run workspace gate. Private storage, complete call/constructor closure
and validator execution safety still keep V01 OPEN; V02–V06 are unimplemented.
Artifact delta +3,660 KiB (.lake +2,796; capture +864); target/frontend unchanged,
capture 17,148 KiB of 500 MiB, 53 GiB free. No temporary source remains.

**Mixed finite/cyclic calls (October 5 continuation).** `59e0f8cc` closes
`call-loop-mixed.c`'s three REDs: a finite call before a cyclic call, inside its
argument, and after its return. Source frontiers now stop at the first actual
call, rather than eagerly executing finite argument calls. An unannotated
finite callee receives an entry-only suggestion; `loopDiagram` still checks
its body, bindings and successors. An explicit bad plan is never replaced.
Twenty-four original pairs and the historical refusal replay; six new
original/renamed pairs, 23 rejection controls and 27 execution/frame controls
pass. Clean Lean at this commit passes all steps: 79 kernel-only core targets,
1,246,159 vector answers. Rust-baseline scope remains `014bfa62`.
`call-loop-nested.c` is the next RED: an unbounded caller loop invokes the
cyclic callee. Both source and target execute the three recorded controls
identically, but whole-pair admission refuses even with both functions' proposed
cutpoints. Cyclic caller spans currently only compose finite callees; their
call/return and backedge relation must be closed together. This is not PASS.
V01 and V02–V06 remain open. Artifact delta +1,216 KiB (.lake +1,124;
capture +92); target/frontend unchanged, capture 17,240 KiB of 500 MiB,
53 GiB free. Temporary inspection source removed; all final evidence retained.

**Cyclic caller/callee composition (October 5 continuation).** `d14f4d95`
closes the original `call-loop-nested.c` pair. `OpenCheckpoint.bind` links
callee returns to checked caller fragments; `OpenCheckpoint.close` closes all
caller cutpoints with positive actual source/target paths. No assumed caller
refinement, iteration bound or termination premise is used. The old whole-call
entry theorem now reuses this open call proof. Twenty-five original pairs and
the historical refusal replay. Two original/renamed pairs, 24 mutations,
27 execution/frame controls and two matched nonterminating variants pass.
Clean Lean at this commit: all steps PASS, 82 kernel-only core targets, no native
axioms in those targets, 1,246,159 vector answers. Rust evidence remains the
unchanged `014bfa62` full-workspace baseline, not a new run.
The saved next RED, `call-loop-deep.c`, adds a finite caller around the now
accepted nested loop. Its four-function source closure validates and three
execution controls agree, but pair admission still refuses: the callee builder
must itself reuse open cyclic caller tables under an outer return context.
Private storage, full constructor/closure coverage and validator safety still
keep V01 OPEN; V02–V06 are unimplemented. Receipt: `cyclic_caller_contexts` in
the existing pilot manifest. Artifact delta +3,268 KiB (.lake +3,220; capture
+48), target/frontend unchanged, capture 17,288 KiB of 500 MiB, 52 GiB free.
No temporary source remains; protected evidence is retained.

**General call closure and execution safety (October 5 continuation).**
`8b7d16d0` replaces the two-level construction with one nonrecursive call-graph
builder over arbitrary suspended caller contexts. `8afff094` adds proved
transport of unused poison placeholders and closed-term diagram normalization;
false/unknown guards and the first diagnostic remain intact. The unchanged
`source-execution.c` / `main` closure now passes with all signature bits and
unbounded execution. Thirty-one positive original pairs and one retained
negative replay; no source/IR input, EqSat registry or runtime bound was changed.
`05fb62d4` connects actual identifier admission to a realized local store and
proves typed scalar continuation safety. `4a8ac98e` derives
`LoweringRefines.source_no_malformed`: an accepted whole-pair proof excludes
malformed source terminals, including when target divergence is considered.
This conditional result does **not** prove standalone input-validator safety.
Clean Lean at `4a8ac98e`: PASS, 90 kernel-only core targets, no native axioms in
those targets. Full workspace at `8afff094`: 401 suites, 3,080 PASS, 0 FAIL,
5 ignored; Clippy/format PASS. Private lowered-storage composition, complete
constructor coverage, and preservation of validator facts through full
expressions/statements/loops/calls remain OPEN; V02–V06 remain unimplemented.
Details and exact subjects are in the existing pilot receipt. Artifact delta
+6,412 KiB; target/frontend unchanged; capture 17,832 KiB of 500 MiB; 52 GiB
free. Two temporary inspection inputs removed, protected evidence retained.

**Source write preservation (October 5 continuation).** `6c28845a` connects the
actual assignment/update admission checks to runtime binding permissions and
proves preservation of the local-store invariant for all eleven assignment
forms, all four updates, declarations, initialization and caller restoration.
`4db6012e` adds scope-exit preservation under an explicit frame-shape invariant:
the dropped prefix cannot shadow retained bindings, retained types remain exact,
and the post-scope facts use the actual validator's initialization filter.
Failures retain the old store and the first checked diagnostic; successful
writes retain source ranks, qualifiers and other first bindings. Whole-expression
and continuation preservation are still required; these component results do
not establish standalone `checkWellFormed` safety.
The proof exposed a model defect: `int x = (x = 1, 2);` passed validation but
execution reported duplicate initialization. The retained RED demonstrates that
failure. Mutable initialization now follows its sequenced subexpression;
const assignment and duplicate const initialization still refuse. This follows
[C11 draft N1570](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf)
6.5.17 and 6.7.9, rather than restricting the source profile to hide the failure.
The original C pair is accepted by the source/IR checker, both existing Rust
interpreters return zero, and two original/renamed pairs, three mutations and
eleven kernel storage controls pass. Clean Lean at `4db6012e`: 98 kernel-only
core targets, no native axioms in those targets. The 31 previous positive pairs,
one previous refusal and the new original pair replay as expected. Exact gate
subjects, logs and disk accounting are in `validator_storage_preservation` in
the existing pilot receipt. Full workspace on the unchanged Rust tree of
`6c28845a`: 401 suites, 3,081 PASS / 0 FAIL / 5 ignored, Clippy/format PASS.
The sandbox-denied attempt is retained as FAIL, separate from the authorized
complete rerun. Scoped artifact delta +19,260 KiB; 52 GiB free, no new target
tree or temporary inspection inputs retained. Private lowered-storage composition, full constructor
coverage and global validator invariants still keep V01 OPEN; V02–V06 remain
unimplemented. No backend or release theorem is claimed.

**Stage inputs.** Use existing canonical EvoIR text and lossless MIR/allocated
representations; certificates propose node/block/value correspondences and
invariants between *two supplied programs*. Assembly input is final emitted
text, not pre-frame MIR. Every always-on transformation lies between an explicit
input/output pair. Phase readers must refuse unsupported constructors and
ill-typed/dangling/duplicate bindings, rather than silently omit them.

**States and observations.** Source state contains the current function, typed
local bindings with lifetime/initialization, expression/statement continuation
and a call stack. EvoIR state additionally contains block/PC, local SSA bindings
and explicit private memory/world snapshots. Machine state contains PC,
register views/flags, private stack bytes/initialization and call/frame state.
Related entries have identical typed argument bits and valid ABI entry/adequate
private storage. Private stores, edge copies and spills are silent; there is no
external I/O in this profile. Observable completion is either returned typed
bits or the **first** `E3002`/`E3003`/`E3004`/`E3005` diagnostic. Fuel exhaustion
is not a program observation or proof. Unsupported memory/initialization faults
must be excluded by well-formedness and the invariant, not mapped to a return.

The refinement quantifies all admitted argument values and every execution:
every finite target termination/diagnostic has the same source observation;
every infinite target execution corresponds to an infinite source execution;
and a nonterminal reachable target state is not stuck. Silent matching requires
a decreasing natural-number rank, preventing administrative infinite loops.
For this closed deterministic profile these obligations also rule out replacing
source divergence by a return/trap. Internal calls are operational transitions,
not new external trace events; their argument/result/private-frame relation is
checked. Source locations and resource exhaustion remain outside the theorem.

Per-stage REDs are the mutation rows in V01--V06 above. The existing executable
pilot additionally requires a genuine allocated `SpillReload`, incoming stack
arguments, unchanged original function excerpts, loops/joins and the `abs`
minimum-value diagnostic. These observations freeze a useful population; they
are not yet a source/target simulation theorem or native execution receipt.

### Release gate, budget and stop rules

- **V04 before V05**: close baseline correctness before counting proved optimizer
  integration. Existing scalar proofs remain reusable assets throughout.
- **V00–V06 block the first public release under this plan.** A missed proof does
  not become a negative-result closure or an ordinary test PASS. Report the exact
  missing lemma/unsupported path; do not lower the release promise silently.
- A proof-required invocation must bind the actual source, all intermediate
  correspondences and final emitted assembly. No silent fallback, mixed unproved
  callee, or unsupported pass. Gate this in the existing driver/bundle boundary;
  do not create another storage or attestation platform.
- Run focused proof/tests during implementation; at completed integration slices
  run `tools/gate-changed.sh --full-workspace --offline` and `tools/gate-lean.sh`.
  V06 runs final acceptance once on a frozen subject. Reuse existing native and
  application harnesses; do not schedule kernel rebuilds for each lemma. Existing
  unresolved runtime/release tasks keep their own requirements and are not waived.
- Follow TODO's disk discipline: inspect before any build, do not launch below
  25 GiB, stop/clean below 15 GiB, only `target/frontend`, two build jobs and
  serial Cargo. Captures are capped at 500 MiB by default; preserve Lean caches
  and sealed evidence. Record each slice's disk net change. This planning edit
  does not authorize deleting retained artifacts.
- Do not reopen timing, AI discovery, new guard facts, memory EqSat, or the
  1.4x size campaign as prerequisites. Defaults stay unchanged. Reuse existing
  models and tools; N07's zero-proof SMT outcome is not a machine-semantics proof.
- First implementation deliverable is **V00's frozen interface and real-function
  admission**, followed by V01's first source/lowering checker theorem. Do not
  postpone a missing source/target bridge until the final release gate, or spend
  weeks adding theorem infrastructure without a checkable compiler output.

Public wording after acceptance: *a proved, fail-closed compilation path for the
listed Core C-HIR subset and target, with an explicit trusted boundary; existing
scalar rewrites compose with that path.* Do not say "CompCert-equivalent GNU C",
"all UB is caught", "the entire compiler is verified", or "Linux is proved".
Current status is **implementation in progress**, not release acceptance.
The initial documentation-only commit `409b84e3` produced approximately 22 KiB
of tracked text and no build/capture artifacts. Subsequent slice measurements
belong to their own records; filesystem-wide free-space changes are not assumed
to be compiler artifact growth. The October 1 numbers below remain unchanged.

## Historical scalar proof plan

**October 1 receipt refresh.** All six correspondence groups were rerun:
1,325,045 P0 comparisons plus 162,644 guard comparisons, **1,487,689 total with
zero differences**. Selected runs now replace only their own groups; retained
groups keep their run/commit/toolchain metadata and totals are recomputed.
`tools/gate-lean.sh` ran from a clean tree and records actual answered counts for
all six groups, with `lean_worktree_clean: true` and Lean source commit `595272a2`.
The current audit covers 55 P0 rules plus seven g0 rules; all seven new g0 A2
transfer rows are parametric and kernel-only. The older five-group counts below
remain the P0 subset, rather than the combined receipt total.

**September 29 integration note, revision 5 (design only).** The existing L0–L4 plan and its
historical evidence below remain scoped to the reviewed scalar semantics,
registry and analysis modules. The maintained
[Q2/Q3 implementation architecture](framing-draft.md#4-rule-soundness-and-admission)
adds a proposed consumer of this proof boundary: offline AI rule candidates.
V1 accepts typed candidate data only, never AI tactic scripts or imports.
Trusted fixed templates construct the exact `RuleCovered rule_<id>` statement,
which currently quantifies all natural width environments, not a JSON-selected
1–64 domain. Finite-width admission needs R03g's explicit predicate/theorem/audit
extension first; it cannot be inferred from existing enumerated analysis proofs.
Universal templates and derived existing-rule steps can proceed without it.
The trusted runner selects a fixed-order template ladder; candidates cannot
select a template. Admission records actual attempted/successful templates.
Rule/shortcut campaigns require their own residual-opportunity preflight, not
a policy-track pass; bounded enumeration/fuzz is not a proof of the universal
semantic or `simp`-provable rule space.
The preflight freezes grammar cells and resource/termination rules. A completed
witness-containing cell supports finite enumeration/efficiency, not a
beyond-enumeration discovery claim. Template UNKNOWN remains unresolved even
when syntax enumeration is complete. Discovery-qualifying enumeration budgets
must meet the planned total AI track's evaluation budget (or a factor >= 1),
not a deliberately smaller exploratory quota. EVP checker outages remain unknown;
malformed-input exit 2 is a bundle defect, distinct from invalid-claim exit 3.
Audit actual transitive dependencies: a kernel-only rule can still depend on
native-backed fact transfer in the complete admission chain. New dependency
classes need review; no automatic kernel-only end-to-end claim follows.
R03f provides bounded manual integration and independent matching mutations;
arbitrary proof scripts/export/rechecking are a separate V2 project, not a V1
prerequisite. Optional shadow utility screening cannot emit trusted artifacts.
No new proof, registry, toolchain upgrade or acceptance result is asserted by
this note. Execution is tracked under R03b/R03c/R03f–R03h in [TODO.md](../TODO.md).

Date: 2026-09-09. Status: the implementation version after two rounds of external review. The repository implements L0–L4 according to this document; L5 waits for the kernel milestone and is then decided by measurement.
Basis: the team brief (`docs/notebook/lean-semantics-brief.md`), the two rounds of replies from the review panel on 2026-09-08, the local measurements of Section 0, and the independent exhaustive check by the review panel in Section 0b. Anything not marked "measured" is a plan, not a fact.

This document is self-contained. Appendix A is the current registry, unabridged. The probe source files are `lean/probes/probe2.lean` and `lean/probes/probe3.lean`.

## Changes in the second version relative to the first

1. `RuleSound` becomes a multi-width interface: each variable has its own bit width, and the rules, terms, fact environment and transfer functions share one set of type information. Different widths no longer go into one `BitVec n` (Section 4).
2. The result-class fact R stays a separate premise. The plan states the complete chain of A2, B, A3 and A7 on which it depends, and explicitly forbids using the fact of the merged class to prove the same merge (Section 4).
3. The quantifier of `MeetSound` is corrected to an existential (Section 4).
4. Coverage has two kinds: segment A covers rule instances (`S(r) ⊆ P(r)`), and segment B covers the analysis instances used to produce facts. The plan states who enforces "an uncovered instance is not enabled", and an old digest implies no new enabling condition (Section 2).
5. The shift-amount width M does not change the registry semantics. The proof splits on "whether the fact is fully known / how `k.toNat` relates to N", and does the bitwise transfer only on `0 ≤ s < N` (Section 4). 128-bit shift amounts are in scope, and L3 adds the corresponding vectors (Section 6).
6. The audit no longer passes a theorem on an axiom-name pattern. Instead, it fixes the modules and the allowed imports, collects the actual transitive dependencies and records the declaring module and the type, rejects unknown declarations, and checks the complete proposition (Section 2, R3).
7. The promise of the generator becomes "a fixed `RuleSound` prevents the prover from replacing the definition of correctness; the correspondence from text to data still depends on the trusted decoder, the round-trip check and independent review" (Section 3).
8. The three inductive conditions of L3b state an explicit direction and coverage requirements, with a negative test. Its claim is lowered to "independent checking evidence" (Section 6).
9. The acceptance of L1/L2 is rewritten: each receipt row must correspond to an existing theorem, the correct parameter space, a fixed proposition and the actual dependencies. "The only risk is time" becomes "not yet verified". "Primitives only, abbrev required" is lowered to a preprocessing choice of the proof script (Section 6).
10. The formulas for `or`, `lshr`, `ashr` and `icmp.ne/sle/ule`, which the first version gave only as "dual", are written out in full for checking (Section 4).

---

## 0. Facts measured on this machine (2026-09-08)

Toolchain and axioms (`lean +leanprover/lean4:v4.32.2`, available offline; the machine default toolchain is 4.25.0, and the project pins the version with `lean-toolchain`):

| Theorem shape | Proof method | `#print axioms` report |
|---|---|---|
| `x + 0 = x`, `x * 1 = x` (bits<32>/<64>) | `bv_decide`, closed in the preprocessing stage, SAT not called | `[propext]` |
| `x * 2 = x + x` (bits<64>) | Same as above | `[propext, Quot.sound]` |
| `x.slt x = false`(bits<64>) | `bv_decide` | `[propext, Classical.choice, Quot.sound]` |
| known-bits transfer rows (see below) | `bv_decide`, SAT + LRAT | The three above plus one native axiom named after the theorem: `<theorem name>._native.bv_decide.ax_1_5` |

On 4.25.0, the same file reports `Lean.ofReduceBool, Lean.trustCompiler` on the SAT path. Axiom names change between versions, so the audit cannot depend on a single name (Section 2, R3).

Measured and passing (the two probes take 15.3 seconds in total; this is the measured subset, not all rows and all widths):

- Tier A rules at 32/64 bits; `mul(x,2) = x+x` at 64 bits.
- Transfer rows, all at 64 bits: `and`, `add`, `mul`, `icmp.ult`, `icmp.slt` (both sign bits known), `shl` (shift amount fully known and below the width, M = 64); `zext`/`sext`/`trunc` at 32↔64.
- The shift amount is decoded at its own width: `decide` decides `(1#8) <<< (256#16) = 0#8`. Lean defines the shift of a `BitVec` by a `BitVec` as `x <<< k.toNat`, which agrees with EvoIR 3.4.
- Preprocessing in the proof script: when a transfer function is a plain `def`, `bv_decide` treats it as an opaque variable and gives a spurious counterexample. The proof passes if `simp only [defs]` unfolds it first, or with `abbrev`. This is a preprocessing choice of the tactic; the semantic definitions can stay plain `def`.

Current state of the repository:

- Current registry `p0.scalar.v4`: 55 rules, digest `1a5391…9d74` (Appendix A). Six kinds of condition: `true`, `all_const`, `fully_known`, `covers`, `known_zero`, `width_min(?x,2)`. Of these rules, 18 `known_const` rules can instantiate the same lemma with the correct result type, and 12 `fold` rules are definitional equalities under the Lean definitions. The receipt still lists each rule.
- Normalization of known-bits facts exists already: spec section 11.3 states that a premise is malformed if its value has a one where the mask is zero, and the checker's `Fact::from_premise` (`crates/evocc-proofcheck/src/facts.rs`) rejects it. The internal representation of the checker is `(zeros, ones)`, which is in bijection with `(mask, value)`.
- The repository has no `.evp` file. Tests generate the triples in a temporary directory (`write_case` in `crates/evocc-proofcheck/tests/day8_corpus_replay.rs`). The driver has only `-Oeq-scalar`/`-Oeq`.
- The seed-facts fixed point is in `crates/evocc-proofcheck/src/seed_facts.rs`: `entry_arguments` (entry top), `by_current` (worklist), `carry` (propagation along edges), `by_whole_passes` and `agreement` (oracles for tests only).
- Spec section 8.5: the root and the witness are `bits<N>`, `1 ≤ N ≤ 64`; the two operands of a comparison have the same width; a shift amount is "any `bits<M>`"; both ends of a cast are in bounds. Instance parameter space: one N for a unary/binary rule; one N for a comparison; (N, M) for a shift; (W, N) for a cast. The original spec sentence gives the shift-amount width M no upper bound, but lane B measured that the two independent eligibility implementations, in the producer and in the checker, both limit it to 1..64, and the fact lattice has only a 64-bit mask. On this basis, spec revision 1.9.87 sets M of the P0 profile to 1..64 (a wider shift amount makes the region ineligible; the interpreter still executes it as 3.4 states). 128-bit roots, casts and shift amounts are all outside the P0 profile. 128-bit shift amounts are still part of the language semantics, and the L3 named vectors stay for the interpreter.

## 0b. Independent exhaustive check by the review panel (2026-09-08, external evidence)

The review panel used an independent Python program to check the formulas of Section 4 of the first version exhaustively at bit widths 1–4: 120 normalized facts and 7,380 pairs of same-width facts; the relations of `meet`, conflict, `join`, the order and concretization; the and/or conclusions of `covers`; 69,904 concrete inputs each for `and`, `xor`, `add`, `sub`, `mul`, `icmp.eq`, `icmp.ult` and `icmp.slt`; 115,600 inputs for `shl`, with N and M each taken independently from 1–4; 448, 448 and 1,252 inputs for `zext`/`sext`/`trunc`. It found no counterexample, and all output facts were normalized. Not checked: `or`, `lshr` and `ashr`, which the first version gave only as "dual", and the unexpanded `icmp.ne/sle/ule`; Section 4 of this version writes them out in full. An exhaustive check at small bit widths is not a proof; it verifies neither the registry decoding nor the Rust/Lean correspondence.

---

## 1. Decision

Lean 4 is only an offline proof layer for the P0 scalar semantics, the registry rules and the known-bits analysis. It does not change the IR, the host language, the e-graph or the independence of `evo-proofcheck`, and it is not a prerequisite of the kernel milestone. The goal of Stage 1 is to reduce manual trust in the existing rules and analysis formulas. It is not a verified C compiler, and it is not a new formalization framework.

## 2. Coverage, audit, equality: implementing the four corrections

**R1 Coverage has two kinds, and both live in the registry and the receipt.**

Let `S(r)` be the set of instances that a digest-bound registry allows, and `P(r)` the set of instances that the proof covers. To mark the whole registry as proved, `S(r) ⊆ P(r)` must hold for each rule r in it. This is segment A.

Segment B: the analysis instances that produce the fact premises of rules must also be covered. Counterexample shape: `shift<N=8, M=128>` gives a `Fact 8` through its transfer, and this fact is a premise of a later `add`/`xor`/`covers`/`known_const`. The later rules are all in the proved range, but the uncovered shift analysis still took part in the premise. Narrowing only the shift rewrite rules does not close this gap. The acceptance object of segment B is therefore "each transfer that the current P0 actually uses is proved over the required parameter range". The first choice is to make the M dimension of shifts parametric (Section 4); if that is not possible, keep the side-path, partial-coverage claim. After spec revision 1.9.87 limits M to 1..64, the `shift<N=8, M=128>` of the counterexample shape cannot enter the e-graph at all in either of today's two implementations, and complete segment B coverage for P0 is N and M each in 1..64. If partial analysis coverage is to be used at run time in the future, an unsupported analysis branch must conservatively return top, or an explicit additional trust assumption must stay. This plan builds no fact provenance tracking.

Enforcer: the old registry stays in use for the existing flow, and the receipt marks its coverage as incomplete. For a Lean-gated version, either complete the whole instance space, or publish a new registry with narrowed conditions (the certificates bind the new digest, and the producer and the checker must interpret the new side condition in the same way). The separate Lean gate can fail, but it does not make the kernel build fail. An old digest implies no enabling condition that the registry does not state.

**R2 The three guarantees stay separate.** Three theorem families, three receipt segments: A `RuleSound`; B `TransferSound`/`MeetSound`/`JoinSound`/predicate lemmas; C `Correspondence` (differential test evidence only). Full coverage of segment A does not mean full coverage of segment B; the status of segment C does not stand for A or B.

**R3 Audit: fixed proof code, a checkable dependency report.** An axiom-name pattern cannot prove a legitimate origin: a custom axiom can take the same name, and when a proof reuses a native-backed helper lemma, the final theorem inherits the axioms named after the helper lemma. The audit procedure is therefore:

1. The audit covers the fixed modules `lean/EvoIR/**` and the proof scripts in the repository. The only allowed import is Lean core. No external proof input is accepted.
2. After `lake build`, enumerate all `axiom` declarations in the environment. For each declaration other than the three core axioms (`propext`, `Classical.choice`, `Quot.sound`), record the name, the declaring module and the type. Reject a declaration if its declaring module is not one of our proof modules, or if its type does not have the shape of a `bv_decide` reflection axiom.
3. For each target theorem, collect the transitive dependencies from `#print axioms`, write them to the receipt, and mark the theorem `kernel-only` (the three core axioms only) or `native-backed`.
4. Check the complete proposition: the type of the target theorem must match, character for character, the `RuleSound (rule "p0_001")`/`TransferSound op T` (with the parameter space) that the audit script generates itself from the registry data. A check of only the name and the axioms does not pass.
5. The receipt binds: the registry digest, the spec revision number and the `lean/` source commit, the Lean toolchain version, and the name and proposition text of each target theorem.
6. State the trust assumptions: the audit script, the fixed Lean modules, the Lean kernel and the compiler (on the native path) are trusted. The procedure does not apply to arbitrary untrusted proof programs; to accept them in the future, design an isolation separately.

**R4 P0 is conditional equality.** The conclusion of `RuleSound` is an equality. The plan introduces no refinement relation and does not change the symmetric e-class. A future rule with a `ub.assume` premise is first treated as a conditional equality within its scope, and a one-way transformation takes a separate, directed verification step. Neither is in this plan.

## 3. Structure

Directory `lean/`, Lake package `EvoIR`. It depends only on Lean core (`Std.Tactic.BVDecide`) and does not import Mathlib. `lean-toolchain` pins `leanprover/lean4:v4.32.2`. The build is offline.

```text
lean/EvoIR/Sem.lean        P0 opcode semantics: one denoteOp, a BitVec function parameterized by type
lean/EvoIR/Term.lean       small term syntax (typed): Var | Const | App; denoteTerm only recurses and then calls denoteOp
lean/EvoIR/Fact.lean       Fact n, γ, order, meet/join, six side-condition predicates, transfer functions (types follow op)
lean/EvoIR/Rule.lean       rule data type, WellTyped, the fixed RuleSound definition
lean/EvoIR/Registry.lean   generated: the registry text decoded into rule data (not propositions)
lean/EvoIR/Proofs/*.lean   theorems: rules, transfer rows, meet/join, predicate lemmas
lean/EvoIR/Eval.lean       executable evaluator (lake exe evoir-sem), for differential tests
lean/probes/*.lean         the original probe sources of Section 0
tools/lean/registry_to_lean.py   extracts the registry block from the spec, checks that its SHA-256 equals the digest in the spec, decodes it into Registry.lean, and renders the data back to text for a byte-for-byte comparison with the original
tools/lean/audit.py              the six steps of R3; writes tests/contracts/lean-p0-receipt.json
tools/lean/vectors.py            generates differential vectors (the L3 format of Section 6)
tools/lean/diff.py               compares the outputs of the two sides
tools/gate-lean.sh               separate gate, not on the kernel path
```

One `denoteOp` plus a small term syntax; the first step does not represent the CFG, memory or C. The generator produces only rule data, and the propositions come from the fixed `RuleSound`. A fixed `RuleSound` prevents the prover from replacing the definition of rule correctness. It does not verify that the decoding is faithful: if the decoder decodes a rule as `lhs = lhs`, the fixed `RuleSound` correctly proves this wrong rule. The correspondence from text to rule data still depends on the trusted decoder (A6), the round-trip check and independent review. The round-trip check shows only that the two conversions agree with each other.

## 4. Theorem statements

**Typed interface.** Each variable has its own bit width, and both sides of a rule have a result bit width. Data of different widths do not share one `BitVec n`. Below is the structure, not compiled Lean:

```text
Γ : Var → Nat                          -- bit width of each variable
Term Γ n                               -- term of result width n: Var v (Γ v = n) | Const n c | App op args (the signature of op gives the argument types)
r : Rule Γ out                         -- lhs : Term Γ out; rhs : Term Γ out or known_const; cond
σ : (v : Var) → BitVec (Γ v)
F : (v : Var) → Fact (Γ v)
R : Fact out

RuleSound r :=
  ∀ σ F R,
    (∀ v, γ (F v) (σ v)) →               -- the fact of each variable holds
    γ R (denoteTerm r.lhs σ) →           -- the fact of the result class holds (pre-merge state)
    CondHolds r.cond σ F R →             -- the side condition evaluates to true on F and R
    denoteTerm r.lhs σ = denoteRhs r.rhs σ R
```

The implementation can use an intrinsically typed inductive type, or a plain AST plus a `WellTyped` predicate. `WellTyped` checks the operation signatures (the two operands of an arithmetic/bitwise operation have the same width; the two operands of a comparison have the same width and the result has width 1; a shift amount has any M; `zext/sext` requires W ≤ N; `trunc` requires N ≤ W; a same-width cast is the identity). The same σ handles a nonlinear pattern (`add(?x,?x)`).

**Semantics.** `denoteOp` follows spec sections 3.3–3.6: `add/sub/mul` are `+ - *`; `and/or/xor`; `shl x k = x <<< k.toNat`, `lshr x k = x >>> k.toNat`, `ashr x k = x.sshiftRight k.toNat` (`k : BitVec M`; when `k.toNat ≥ N`, the results are 0, 0 and the sign fill, measured to agree with 3.4); `icmp.*` returns `BitVec 1`; `zext/sext/trunc` are `zeroExtend/signExtend/truncate`.

**Facts.** `Fact n := { mask value : BitVec n // value &&& ~~~mask = 0 }`; `γ K x := x &&& K.mask = K.value`. `fullyKnown K := K.mask = allOnes`; `knownZero K := K.mask = allOnes ∧ K.value = 0`; `covers M X := (~~~(X.mask &&& ~~~X.value)) &&& ~~~M.value = 0`.

**Order and merge.** `K ⊑ K' := K'.mask &&& ~~~K.mask = 0 ∧ K.value &&& K'.mask = K'.value`. `meet K K'`: if `(K.value ^^^ K'.value) &&& K.mask &&& K'.mask ≠ 0`, then `conflict`; otherwise `some {mask ||| mask', value ||| value'}`. `join K K' := {mask &&& mask' &&& ~~~(value ^^^ value'), value &&& that mask}`.

```text
MeetSound     : γ K x → γ K' x → ∃ K'', meet K K' = some K'' ∧ γ K'' x
MeetConflict  : γ K x → γ K' x → meet K K' ≠ conflict
JoinSound     : γ K x → γ (join K K') x (and the symmetric case)
```

**Transfer signatures and soundness.**

```text
T_add T_sub T_mul T_and T_or T_xor : Fact N → Fact N → Fact N
T_icmp_*                           : Fact N → Fact N → Fact 1
T_shl T_lshr T_ashr                : Fact N → Fact M → Fact N
T_zext T_sext                      : Fact W → Fact N   (W ≤ N)
T_trunc                            : Fact W → Fact N   (N ≤ W)
TransferSound op T := ∀ x y A B, γ A x → γ B y → γ (T A B) (op x y)   (likewise for unary operations; the types follow op)
```

Row formulas (`m` and `v` are mask and value; `kz(K) = K.mask &&& ~~~K.value` is the known-zero set, and `K.value` is the known-one set):

```text
and  : m' = (mA & ~vA) | (mB & ~vB) | (mA & mB);            v' = vA & vB & m'
or   : m' = (mA & vA) | (mB & vB) | (mA & mB);              v' = (vA | vB) & m'
xor  : m' = mA & mB;                                        v' = (vA ^ vB) & m'
add/sub : t = mA & mB;  m' = (~t & (t+1)) - 1;              v' = (vA ± vB) & m'
mul  : z(K) = ~kz(K) & (kz(K)+1);  m' = z(A) * z(B) - 1;    v' = 0
icmp.eq  : both fullyKnown and vA = vB → one; (vA ^ vB) & mA & mB ≠ 0 → zero; otherwise top
icmp.ne  : (vA ^ vB) & mA & mB ≠ 0 → one; both fullyKnown and vA = vB → zero; otherwise top
icmp.ult : lo(A) = vA, hi(A) = vA | ~mA; hi(A) <u lo(B) → one; lo(A) ≥u hi(B) → zero; otherwise top
icmp.ule : hi(A) ≤u lo(B) → one; lo(A) >u hi(B) → zero; otherwise top
icmp.slt / sle : the two rows above, plus the condition mA.msb = mB.msb = true, with a signed comparison; otherwise top
shl  : see the split below; when s < N, m' = (mA <<< s) | ((1 <<< s) - 1), v' = vA <<< s
lshr : when s < N, m' = (mA >>> s) | ~(allOnes >>> s), v' = vA >>> s
ashr : when s < N, m' = mA.sshiftRight s, v' = vA.sshiftRight s (when the sign bit is unknown, the high bits of m' are 0 automatically; normalization guarantees vA.msb = 0)
zext : m' = zeroExtend mA | ~zeroExtend allOnes_W;           v' = zeroExtend vA
sext : m' = signExtend mA;                                  v' = signExtend vA
trunc: m' = truncate mA;                                    v' = truncate vA
```

**Proof split for shift transfers (M has no upper bound; the registry semantics do not change).** Let the shift-amount fact be `K : Fact M`:

1. `¬ fullyKnown K`: the transfer returns top, and soundness holds directly.
2. `fullyKnown K`: `γ K k` gives `k = K.value`, so `s := k.toNat = K.value.toNat`.
   - `s < N`: prove the bitwise transfer of the table above. For a fixed N, `0 ≤ s < N` has only finitely many cases; if necessary, enumerate s instead of all M-bit shift amounts.
   - `s ≥ N`: return top, as the current analysis defines.

The comparison must be the natural-number comparison `k.toNat < N`; it must not first truncate N to `BitVec M`. The shift-amount decoding lemma holds for any positive M. The time cost of this split has not been measured on 4.32.2.

**Rules.** The right side `fold.op` of an `all_const` rule is `denoteOp` itself, so the theorem is trivial, and its content goes to segment C. A `known_const` rule uses the conditional lemma `fullyKnown R ∧ γ R t → t = R.value`. R stays a separate premise and is not forced to be `R = T_op(F)`, because the fact of the result class can come through meet from other terms already proved equivalent. The complete chain that makes R trustworthy:

```text
A2 / A7   seed facts hold in the actual program
B         Lean lemmas for transfer, meet and the predicates
A3 / A7   replay associates facts with the correct class and uses the pre-merge state in the correct order
   ⇒ R does describe the value of this lhs
   ⇒ RuleSound: this known_const rewrite holds
```

The forbidden cycle: first merge the lhs with a constant, then take R from the merged class, and finally use R to prove the same merge. Stage 1 does not prove the replayer, but R is explicitly a premise that depends on the pre-merge state. `covers` rules: `γ M m → γ X x → covers M X → x &&& m = x` (the `or` dual: `x ||| m = m`). `width_min(?x,2)` is the premise `2 ≤ n`.

**Width coverage.** Prefer the parametric form (`∀ n`); where a parametric proof fails, enumerate `n ∈ 1..64`. Shift rules and transfer rows are parametric in M (the split above); casts enumerate the legal (W, N).

## 5. Trusted base and assumptions after Stage 1

| ID | Object still trusted | Guarantor | Reduced by this plan |
|---|---|---|---|
| A1 | Section 8.5 eligibility and taint: every value that enters the e-graph is total, poison-free and free of side effects | Rust, separate implementations in the producer and the checker | No |
| A2 | Function-level seed-facts fixed point (entry top, every edge, back edges, unreachable blocks) | Rust, `seed_facts.rs`, recomputed independently by the checker | No; L3b adds independent checking evidence |
| A3 | The checker's replay, extraction and reassembly, including the association of facts with the correct class and order | Rust | No; a Stage 2 target |
| A4 | Section 9 parser (shared) | Rust | No |
| A5 | Lean kernel; on the SAT path, the per-theorem native axioms and the compiler | Lean 4.32.2 | New, disclosed |
| A6 | The registry decoder and the Lean semantic definitions themselves; the audit script | Generator + manual review | New; round-trip comparison |
| A7 | Agreement of the Rust semantics (interpreter, fold, transfer) with the Lean semantics | Differential test evidence | New; segment C reports evidence only |

Manual trust that the Lean proofs replace: the mathematical correctness of the 55 registry rules; the soundness of the Section 8.10 transfer table, meet/join and the four predicates.

## 6. Deliverables, order, acceptance

Run only one heavy job at a time. The kernel milestone and the front-end lane are not affected.

**L0 Real triples (first; does not wait for the Lean package).** Add an example to `evocc-eqopt` that, for a given `.evo`, writes `before.evo / after.evo / proof.evp` and the event counts. Take one case from the reduced Csmith cases and one from the `eq.c` demo, and seal them, with the exit status of `evo-proofcheck`, in `tests/fixtures/certificates/`. Acceptance: checker status 0; the event, region and rewrite counts go into the README of that directory.

**L1 Semantics, rules, audit (Lean lane).** `Sem/Term/Fact/Rule/Registry`, the theorems of the 55 rules (parametric first, then enumerated), `audit.py` and receipt segment A. Acceptance: `lake build` is green, with no `sorry`; each receipt row corresponds to an existing theorem, the correct type parameter space, a fixed target proposition (the audit generates it itself and checks it character for character) and the actual transitive dependencies; full coverage of segment A does not mean coverage of segment B. A ten-minute build is the target.

**L2 Analysis theorems (same lane).** Each transfer row (with the shift split), `MeetSound`, `MeetConflict`, `JoinSound`, the `covers`/`knownZero`/`fullyKnown` lemmas; receipt segment B. Section 0 measured only some rows, at 64 bits and at 32↔64. Automation for the other rows, the other widths and any M is not yet verified. The measured results stay and are not extrapolated.

**L3 Correspondence evidence.** Vector format (shared by the two sides; JSON Lines, one case per line; numbers are hexadecimal strings, without width truncation):

```text
{"kind":"value","op":"shl","n":8,"m":16,"args":["0x1","0x100"]}
{"kind":"value","op":"icmp.slt","n":32,"args":["0x80000000","0x0"]}
{"kind":"value","op":"sext","w":8,"n":32,"args":["0x80"]}
{"kind":"transfer","op":"add","n":4,"facts":[{"mask":"0xc","value":"0x8"},{"mask":"0xf","value":"0x3"}]}
{"kind":"transfer","op":"shl","n":8,"m":128,"facts":[...]}
{"kind":"meet","n":8,"facts":[...]}   {"kind":"join","n":8,"facts":[...]}   {"kind":"covers","n":8,"facts":[M,X]}
```

Each side copies the input unchanged, appends `"result"` (a value is hexadecimal; a fact is `{"mask","value"}`; a meet conflict is `"conflict"`; a predicate is `true/false`), and writes `<name>.lean.jsonl` / `<name>.rust.jsonl`; `diff.py` compares them line by line. Vector sets: transfer functions, exhaustive over normalized facts at width ≤ 4 (3^4 = 81 facts per operand); binary value operations, exhaustive over 65,536 pairs at width 8; random vectors at widths up to 64, with emphasis on uncommon widths, shift amounts of independent width, boundary shift amounts (N-1, N, N+1, 2N), sign bits, same-width and cross-width casts, and conflicting facts. Named vectors (expected values from the EvoIR definitions, not C semantics):

```text
shl(1 : bits<8>, 256 : bits<16>) = 0
shl(1 : bits<8>, 0 : bits<128>) = 1
shl(1 : bits<8>, 1 : bits<128>) = 2
shl(1 : bits<8>, 2^64 : bits<128>) = 0
ashr(128 : bits<8>, 2^64 : bits<128>) = 255
```

Also cover the shapes M = 1, N > M and M > N. Rust side: compare both the producer's `evocc-eqopt::known_bits` and the checker's `facts.rs` (which reads the vectors in a unit test inside the crate); for value operations, compare the interpreter of `evocc-evoir`. Receipt segment C records only the numbers of vectors, passes and differences, and marks them as test evidence.

**L3b (optional).** Add an inductive-invariant check of the seed-fact table to the checker. The three conditions state an explicit direction in the order where "more precise is smaller":

```text
Entry:        InitialFacts ⊑ I(entry)                 -- with no added premise, InitialFacts = top, so the entry-argument table can only be top
Instruction:  Transfer_op(I(operands)) ⊑ I(result)
Edge:         RenameAndCarry_edge(I(source)) ⊑ I(destination arguments)
```

Over the actual CFG, the checker covers every entry, instruction and edge that is structurally reachable from the entry (back edges included). It does not trust the table's own unreachable claims. It treats a missing fact as top or rejects it. Negative test: the loop argument is 0 at the entry, and the back edge passes `x+1`, but the table claims "the loop argument is always 0"; the checker must reject this table. Claim: if the checker is correct and its coverage of entries and edges is complete, it rejects a fact table that does not satisfy the inductive conditions. This stage adds independent checking evidence, not a formal proof of A2. It does not block L1/L2.

**L4 Spec and wording (architect).** 8.10 clarifies "a fully known constant is a minimal element, not the unique bottom; a conflict is not a fact"; a new 11.7 "Trusted base" (the table of Section 5); the two-kind coverage principle of R1 goes into 11.6; README/roadmap adopt the wording of Section 8.

**L5 (after the kernel milestone; decided later from measurements of the L0 triples).** No preselection of Aeneas or of full replay. The two-level statement "region replacement / back into the module" of Section 8 of the review is the target theorem framework of Stage 2.

## 7. Point-by-point response to the second review round

| Review section | Response |
|---|---|
| 1 Multi-width interface | Adopted: Γ, `Rule Γ out`, dependently typed σ/F, a separate R, transfer signatures by op; `MeetSound` becomes existential; the chain of R adds A3; the ban on circular reasoning is written down. |
| 2 Coverage | Adopted `S(r) ⊆ P(r)`; the enforcer and the old-digest rule are explicit; new analysis-instance coverage in segment B; no provenance tracking. |
| 3 M has no upper bound | Adopted: no registry change; the proof splits on whether the fact is fully known and on `k.toNat`; 128-bit shift amounts are in scope, and the vectors are completed. |
| 4 L3b | Adopted the explicit direction, the CFG coverage, the negative test and the lowered claim. |
| 5 Audit and generator | Adopted: the six-step procedure, rejection of unknown declarations, the check of the complete proposition, the four receipt bindings; the generator promise is rewritten. |
| 6 Acceptance | Adopted: row-by-row check for L1, "not yet verified" for L2, `def` kept, 30 of the 55 rules instantiate shared lemmas. |
| 7 Exhaustive-check record | Included as Section 0b; the unchecked formulas are written out in full. |

## 8. What may be written after L1–L3

May write: "The 55 P0 scalar rewrite rules listed in `p0.scalar.v4` are machine-checked against the Lean 4 model of the EvoIR scalar semantics, within the bit widths and conditions that the receipt lists; the known-bits transfers, merge and predicates of 8.10 are checked as well; the build report gives the exact proof dependencies (including the native axioms of the SAT path); the agreement between the Rust implementation and the Lean model has only differential test evidence."

May not write: verified C compiler; kernel compilation correctness is proved; the Rust checker is verified; all widths are proved (if any instance is uncovered); a rule is proved and therefore faster.

## 9. Two open points

1. Static decision of segment B coverage: if the shift transfers are parametric in M and the other rows enumerate N ≤ 64, segment B is complete for P0. If the enumeration of s in the shift split is too slow at some N, how is the gap stated?
2. The implementation of audit step 4, "check the complete proposition": the audit script generates the expected proposition text from the registry data and compares it character for character with the `#check` output. Is a stronger method necessary (a check inside Lean with `example : type_of thm = expected := rfl`)?

---

## 11. Implementation record

### 2026-09-09: L0, L3, L3b merged into main

- L0: `crates/evocc-eqopt/examples/certificate_triple.rs` produces the triple and `counts.json` from `.evo` or `.c`. Three cases are sealed in `tests/fixtures/certificates/` (`tier-a-identities` 19 regions/88 rewrites, `tier-b-known-bits` 6/8, `shift-and-width-facts` 17/62; premises 10/9/36 respectively), and `evo-proofcheck` exits 0 on each. `crates/evocc-proofcheck/tests/sealed_certificate_fixtures.rs` replays each case, derives `after.evo` and `proof.evp` again from the sealed `before.evo`, and compares them byte for byte (the architect replaced the original "lower again from `source.c` and compare", so that the fixtures do not break when the front end changes; `source.c` and the commit IDs in the README are provenance records). The reduced Csmith case depends on the untracked `csmith.h`, cannot be regenerated on a clean checkout, and is not sealed.
- L3: `tools/lean/vectors.py` generates 1,157,725 vectors (transfer 132,534; lattice 22,140; width-8 value operations 983,040; random 20,000; named 11); three Rust runners (producer `known_bits`, checker `facts` through `evo-facts-vectors`, interpreter) and `tools/lean/diff.py`. Producer against checker: 174,674 comparisons, 0 differences; the 574 `unsupported` answers are all shift transfers with M ∈ {65, 128}. The interpreter answers 983,040 + 5,788 vectors, with 209 `unsupported` (M = 65). All 11 named vectors agree with 3.4; `crates/evocc-evoir/tests/shift_amount_widths.rs` pins them. Permanent regression test `crates/evocc-proofcheck/tests/known_bits_correspondence.rs`: 13,383 vectors, 0 differences.
- L3b: `SeedFacts::inductive` checks the three conditions on every block and edge that is structurally reachable in the CFG (back edges included), and treats a missing fact as top; a violation returns the existing status 3. The negative test (entry 0, back edge x+1, the table claims always 0) is rejected; the randgate corpus has 0 violations in 635 functions.
- Gates: the full-workspace tests have only two failures, and both fail in the same way on the baseline commit (`composite_function_declarations_*`, in progress in the front-end lane); clippy, fmt and ruff pass.

### Seven findings of lane B and their rulings

1. Both eligibility implementations limit the shift-amount width to 64, which conflicts with the original sentence of 8.5. Ruling: the spec follows the implementation (1.9.87), because the two independent implementations agree, the fact lattice is 64 bits wide, and a wider region is only ineligible, not miscompiled.
2. The fact lattice can represent only ≤ 64 bits: consistent with ruling 1. A transfer wider than 64 bits has no Rust counterpart even where Lean has a theorem for it, so segment C makes no claim for it.
3. The interpreter executes only 1–64 and 128 bits (`not-executed: wide-integer`): a known limit, unrelated to P0; recorded.
4. The certificates of the default policy bind `p0.scalar.v3` (`aca1f15b…`), not v4: the receipt binds v4 and records v3 as its byte-for-byte prefix (the generator checks both digests and the prefix relation), so the v4 proofs cover the v3 certificates.
5. `Inst::Const` is 64 bits wide, and a 128-bit constant cannot be written: a 128-bit shift amount can enter only as an argument. After ruling 1, this is unrelated to the proofs.
6. The producer's `join` returns `Option`, and the checker's returns `Fact`: not observable on same-width vectors; recorded as an implementation difference.
7. The instruction condition of L3b is empty on the checker's own table (the table records only block arguments): accepted. It acts only on a table that claims instruction results, and the negative test covers that case; extension to instruction results is left for later.

### 2026-09-09: L1, L2 merged into main; segment C measured

- Lean package `lean/` (`EvoIR`), toolchain v4.32.2; a clean `lake build` takes 233 seconds (`Proofs/Transfer` takes most of it); `tools/gate-lean.sh` passes on the merged tree.
- Segment A: all 55 rules are `RuleCovered` (`RuleSound` for every width environment and result width), parametric and kernel-only (30 depend on `[propext, Quot.sound]`, 25 also on `Classical.choice`); the 18 `known_const` rules share the lemma `fullyKnownValue`, and the 12 `fold` rules are definitional equalities.
- Segment B: 29 rows. 20 rows are parametric and kernel-only; among them, the three shift rows are parametric in both N and M (the comparison is `k.toNat < n`), and the casts are parametric in both widths. 9 rows (`add/sub/mul/icmp.eq/ne/slt/sle/ult/ule`) are enumerated over N ∈ 1..64: 576 `bv_decide` theorems, native-backed, with 960 reflection axioms in total, each named after its theorem, all of type `verifyBVExpr … = true` and all declared in `EvoIR.Proofs.Transfer`; no timeout.
- The audit `tools/lean/audit.py` has six steps. The receipt `tests/contracts/lean-p0-receipt.json` binds the two digests of v4 and of its byte-for-byte prefix v3, spec revision 1.9.87 and its sha256, the `lean/` commit and the toolchain, and records the proposition text and the dependencies of each target. A change to one expected proposition causes a rejection. The proofs constrain the definitions (an and row with one clause fewer, or an add row with a mask one bit wider, each gives a counterexample).
- Generator `tools/lean/registry_to_lean.py`: a check of both digests, a prefix check (rechecked inside Lean with `#guard`), a round-trip comparison; 15 Python tests. The evaluator `lake exe evoir-sem` answers all 1,157,725 vectors in 4.7 seconds. `denoteOp_capAmount` proves that the evaluator's cap on the shift amount does not change the answer of `denoteOp`.
- Segment C (the architect ran `tools/lean/correspondence.py`; receipt `tests/contracts/lean-correspondence-receipt.json`): 1,325,045 comparisons in total of the Lean model against the producer, the checker and the interpreter, with 0 differences. By convention, abstentions are not counted: 6,571 each for the producer and the checker in random (5,997 value vectors outside their scope, 574 with M ∈ {65, 128}), and 14,212 for the interpreter (14,003 non-value vectors, 209 with M = 65). This is evidence for A7, not a proof.
- Corrections at merge time: each of the two lanes wrote its own `tools/lean/vectors.py` and `diff.py`, and the lane B versions on main stay. `diff.py` gains an `--expect` single-file mode, and an abstention now counts as neither an agreement nor a difference. Segment C of the receipt that lane A brought came from its old generator (100,359 vectors); the audit rerun on the merged tree supersedes it.

### Seven items of lane A for ruling, and the rulings

1. `covers` when the widths do not match: the checker answers false, and Lean models it as a universal quantification over proofs of width equality (vacuously true on a mismatch), so the theorem is stronger. Ruling: keep it. `WellTyped` for `and`/`or` already forces the two operands to have the same width, so the two readings coincide on legal rules; a record is enough.
2. `all_const` carries no information at the value level (`CondHolds = True`), so the twelve fold theorems hold unconditionally: accepted; the content goes to segment C.
3. `width_min(?x,K)` is modeled as `K ≤ Γ ?x`, which agrees with the way the checker reads the sort of the matched node: accepted.
4. `const_like` on a comparison result does not occur in v4; `bool(K)` is a `bits<1>` constant with the value `K mod 2`: accepted.
5. `Fact.mk'` normalization changes no row (manual check plus vector evidence): accepted.
6. A theorem guarantees the evaluator's `capAmount`: accepted.
7. The segment B target table of the audit is written by hand in `audit.py` (8.10 is prose, not a machine-readable block): accepted, listed under A6. The receipt is 674 KB because it records each of the 960 reflection axioms: accepted.

## Appendix A: full text of `p0.scalar.v4` (from `docs/evoir-spec.md` 11.6)

```text
p0_001_add_zero_rhs add(?x,const_like(?x,0)) => ?x when true
p0_002_add_zero_lhs add(const_like(?x,0),?x) => ?x when true
p0_003_mul_one_rhs mul(?x,const_like(?x,1)) => ?x when true
p0_004_mul_one_lhs mul(const_like(?x,1),?x) => ?x when true
p0_005_and_zero_rhs and(?x,const_like(?x,0)) => const_like(?x,0) when true
p0_006_and_zero_lhs and(const_like(?x,0),?x) => const_like(?x,0) when true
p0_007_or_zero_rhs or(?x,const_like(?x,0)) => ?x when true
p0_008_or_zero_lhs or(const_like(?x,0),?x) => ?x when true
p0_009_double_neg sub(const_like(?x,0),sub(const_like(?x,0),?x)) => ?x when true
p0_010_icmp_eq_self icmp.eq(?x,?x) => bool(1) when true
p0_011_icmp_ne_self icmp.ne(?x,?x) => bool(0) when true
p0_012_icmp_slt_self icmp.slt(?x,?x) => bool(0) when true
p0_013_icmp_sle_self icmp.sle(?x,?x) => bool(1) when true
p0_014_icmp_ult_self icmp.ult(?x,?x) => bool(0) when true
p0_015_icmp_ule_self icmp.ule(?x,?x) => bool(1) when true
p0_100_fold_add add(?a,?b) => fold.add(?a,?b) when all_const
p0_101_fold_sub sub(?a,?b) => fold.sub(?a,?b) when all_const
p0_102_fold_mul mul(?a,?b) => fold.mul(?a,?b) when all_const
p0_103_fold_and and(?a,?b) => fold.and(?a,?b) when all_const
p0_104_fold_or or(?a,?b) => fold.or(?a,?b) when all_const
p0_105_fold_xor xor(?a,?b) => fold.xor(?a,?b) when all_const
p0_106_fold_icmp_eq icmp.eq(?a,?b) => fold.icmp.eq(?a,?b) when all_const
p0_107_fold_icmp_ne icmp.ne(?a,?b) => fold.icmp.ne(?a,?b) when all_const
p0_108_fold_icmp_slt icmp.slt(?a,?b) => fold.icmp.slt(?a,?b) when all_const
p0_109_fold_icmp_sle icmp.sle(?a,?b) => fold.icmp.sle(?a,?b) when all_const
p0_110_fold_icmp_ult icmp.ult(?a,?b) => fold.icmp.ult(?a,?b) when all_const
p0_111_fold_icmp_ule icmp.ule(?a,?b) => fold.icmp.ule(?a,?b) when all_const
p1_200_known_and and(?a,?b) => known_const when fully_known
p1_201_known_or or(?a,?b) => known_const when fully_known
p1_202_known_xor xor(?a,?b) => known_const when fully_known
p1_203_known_add add(?a,?b) => known_const when fully_known
p1_204_known_sub sub(?a,?b) => known_const when fully_known
p1_205_known_mul mul(?a,?b) => known_const when fully_known
p1_210_known_icmp_eq icmp.eq(?a,?b) => known_const when fully_known
p1_211_known_icmp_ne icmp.ne(?a,?b) => known_const when fully_known
p1_212_known_icmp_slt icmp.slt(?a,?b) => known_const when fully_known
p1_213_known_icmp_sle icmp.sle(?a,?b) => known_const when fully_known
p1_214_known_icmp_ult icmp.ult(?a,?b) => known_const when fully_known
p1_215_known_icmp_ule icmp.ule(?a,?b) => known_const when fully_known
p1_220_and_cover_rhs and(?x,?m) => ?x when covers(?m,?x)
p1_221_and_cover_lhs and(?m,?x) => ?x when covers(?m,?x)
p1_222_or_cover_rhs or(?x,?m) => ?m when covers(?m,?x)
p1_223_or_cover_lhs or(?m,?x) => ?m when covers(?m,?x)
p1_224_xor_known_zero_rhs xor(?x,?z) => ?x when known_zero(?z)
p1_225_xor_known_zero_lhs xor(?z,?x) => ?x when known_zero(?z)
p1_206_known_shl shl(?a,?k) => known_const when fully_known
p1_207_known_lshr lshr(?a,?k) => known_const when fully_known
p1_208_known_ashr ashr(?a,?k) => known_const when fully_known
p1_230_known_zext zext(?a) => known_const when fully_known
p1_231_known_sext sext(?a) => known_const when fully_known
p1_232_known_trunc trunc(?a) => known_const when fully_known
p2_300_mul_two_rhs mul(?x,const_like(?x,2)) => add(?x,?x) when width_min(?x,2)
p2_301_mul_two_lhs mul(const_like(?x,2),?x) => add(?x,?x) when width_min(?x,2)
p2_302_add_self_shl add(?x,?x) => shl(?x,const_like(?x,1)) when width_min(?x,2)
p2_303_shl_one_add shl(?x,const_like(?x,1)) => add(?x,?x) when width_min(?x,2)
```
