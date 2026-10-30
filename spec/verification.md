# Verification Contract

**Revision 1.1.4** | **Pinned ground truth** | **2026-10-30**

## Status and scope

This is the contract of the checked compilation path. Sections 1 to 10 are normative; "must" and
"must not" bind the driver, the checker and the tests. The Lean sources in `lean/EvoIR/` are the
formal ground truth, and this document is their readable contract. Each Lean block copies a
definition or a theorem statement without its proof; if a copy and the source differ, the source
decides and this document must be corrected. `crates/evocc/src/proof_required.rs` decides the
command line, the exports and the record. The registry blocks, digests, certificate grammar and
EvoIR text form are in [`spec/evoir.md`](evoir.md), the EvoMIR rules in
[`spec/evomir.md`](evomir.md), and the status in `docs/evidence.md` and `TODO.md`. Lean names
omit the prefix `EvoIR.`; Lean paths are relative to `lean/EvoIR/`.

## 1. The claim

EvoCC has a proved, fail-closed compilation path for the subset `core-v1` and the target
`aarch64-unknown-linux-gnu`, with an explicit trusted base (section 8). In proof-required mode, the
driver publishes an output only if the compiled Lean checker accepts each stage and the pipeline
check for each selected entry. An accepted result is "checked from typed Core source to final
assembly text": for the selected entry, every argument list of its parameter types and every ABI
entry state, the AArch64 semantics of the published text refines the Lean semantics of the exported
typed Core source (section 5). The kernel checks the theorem once, for all inputs; a run executes
the compiled checker and sends no proof term to the kernel. Not claimed: that EvoCC compiles each C
program correctly (the compiler stays untrusted, and the checker can refuse a valid program); the C
text before the export, GNU C, Linux, the assembler, the linker, the runtime or the hardware;
ordinary output, which has no proof label; speed or size.

## 2. The checked subset `core-v1`

### 2.1 Target and options

The target must be `aarch64-unknown-linux-gnu` (LP64); the source export starts with `["core-v1",
"aarch64-unknown-linux-gnu", "checked", ...]`, where `checked` is the UB profile. The command line
must have `--emit=asm`, `--proof-required=NEW_DIR`, at least one `--core-entry=NAME`, `-O0` and
`-fno-builtin` (a checked call of `abs` calls the source definition, not a builtin).
The cc actions `-S`, `-c` and linking are not this command: with `--proof-required`
they give exit 1 before creating a proof directory or an output file. It must not
have a development pass, a pass report, a frame, selection or allocation variant, an unchecked UB
profile, or an EqSat option without `-Oeq-scalar`. The front-end profile must not have signed
wrapping, a runtime floating-point environment, a stack protector or zero-initialization of
automatic objects. The optional rewrite option is `-Oeq-scalar`. With it, the command line must
have `--eq-checker-sha256` and can have each other EqSat option that the driver admits with
`-Oeq-scalar`: `--eq-policy` or `--eq-weights`, `--eq-checker`, `--eq-checker-timeout-ms`,
`--eq-max-nodes`, `--eq-max-epochs`, `--eq-max-transactions` and `--eq-proof-dir`, within the
driver's limits: at most 100000 nodes and 1024 epochs, 1 to 8 transactions, and a checker timeout
from 1 to 300000 ms (`spec/evoir.md` section 8.11).

### 2.2 Source constructors

The Rust inventory `evocc_chir::core_profile::inspect_core`
(`crates/evocc-chir/src/core_profile.rs`) decides admission; it visits every node, dead branches
included, and folds or drops none. The exporter `crates/evocc/src/core_export.rs` writes the tree.
The Lean column names `Core.Source`.

| Row | Admitted | Lean |
|---|---|---|
| Types | `int`, `unsigned int`, `long`, `unsigned long`, `long long`, `unsigned long long`, with rank and signedness; `const` is kept | `IntType`, `QualifiedType` |
| Leaves | Integer literal of its exact type with width-sized hexadecimal bits; identifier of a parameter or an initialized block-scope object | `Expr.literal`, `Expr.ident` |
| Unary | `+`, `-`, `~`, `!` | `Expr.unary` |
| Binary | `+ - * / %`, six comparisons, `&&`, `\|\|`, `& \| ^`, `<<` with an unsigned left operand, `>>`, `,` | `Expr.binary` |
| Choice, conversion | `?:`, cast to an admitted type, comma expression | `Expr.conditional`, `Expr.cast`, `Expr.comma` |
| Assignment | `=` and the ten compound forms on a named local; `<<=` only on an unsigned local | `Expr.assign` |
| Update | Prefix and postfix `++` and `--` on a named local | `Expr.update` |
| Call | Direct call of a prototyped, nonvariadic definition in the selected closure, one argument for each parameter | `Expr.call` |
| Objects | Block-scope object with no storage class, `auto` or `register`; optional single scalar initializer of the object type | `Object`, `Storage` |
| Statements | Null, expression, `return` with a value, compound, `if`/`else`, `while`, `do`-`while`, `for` (empty, expression or declaration init), `break` and `continue` in a loop | `Stmt` |
| Functions | One definition with a prototype; integer result and parameters; no attribute, asm label or `inline` on any declaration; the body must not fall off its end | `Function`, `Parameter` |

Each other constructor is outside `core-v1`, also when unreachable and also when the ordinary
compiler supports it: floating point, `_Bool`, `char`, `short`, 128-bit integers, vectors;
`volatile`, `_Atomic`, explicit alignment; pointers, arrays, aggregates, address-of, dereference;
global, `static`, `extern` and thread-local objects; `return` without a value; `switch`, `goto`,
labels; indirect, external and variadic calls; recursion; inline assembly; signed left shift;
cleanup attributes. The output must have no data object, alias, constructor or file-scope assembly.
A construct outside `core-v1` gives exit 6.

### 2.3 Entries and proposals

Each `--core-entry=NAME` selects one entry, a C identifier. For each entry, the driver exports one
source tree with the entry and its direct-call closure, callees first; each pipeline check has
exactly one selected entry (`Pipeline.selectEntry`). A selected internal function is retained
through emission. Its argument domain must not be narrowed to constants from its C callers.
Each function in the output must be an entry. A
proposal is an untrusted file of loop cutpoints, `--proof-proposal=NAME=PATH`: `core-cutpoints-v1`
or `-v2` (the entry) or `core-call-cutpoints-v1` (closure functions by symbol), with a budget from 1
to 4096, read by `Core.Symbolic.readCutpoints` and `readCallCutpoints` (`Core/CutpointInput.lean`).
Without a proposal, the request is finite, and a loop entry is rejected (exit 3). An undecodable
proposal is unsupported (exit 4); the checker must not fall back to the finite request.

## 3. Semantics

### 3.1 Levels

| Level | Namespace | Syntax and reader | Semantics | Trusted |
|---|---|---|---|---|
| Typed Core source | `Core.Source` | `Core/Source.lean` | `Core/Execution.lean` | Yes |
| EvoIR | `Core.IR` | `Core/IR.lean`, `Core/IRReader.lean` | `Core/IRExecution.lean` | No |
| SSA and post-SSA EvoMIR | `Machine` | `Machine.lean`, `MachineReader.lean` | `MachineExecution.lean` | No |
| Allocated EvoMIR | `Alloc` | `Alloc/Syntax.lean`, `Alloc/Reader.lean` | `Alloc/Semantics.lean` | No |
| AArch64 subset | `Asm` | `Asm/Syntax.lean`, `Asm/Reader.lean`, `Asm/Print.lean` | `Asm/Semantics.lean` | Yes |

Each level has a deterministic `step : State → Option State` (`none`: halted) and no fuel. The
theorem mentions only the first and the last level. The source semantics must match C for `core-v1`
under the checked UB profile. The AArch64 state has `x0` to `x30` with 32-bit views, `sp`, NZCV and
byte memory; an access outside `[low, high)` faults; `brk #imm` halts with diagnostic `imm`.

### 3.2 Observation

`Core.Observation` (`Core/Correspondence.lean`) is `returned type bits` or `diagnostic code`;
`Core.checkedDiagnostic` admits only `E3002` (division by zero), `E3003` (remainder by zero),
`E3004` (signed overflow) and `E3005` (invalid shift count) of `spec/evoir.md` section 6.3.
`Core.observeSource` gives `.returned v.type v.bits.toNat` for a source halt with value `v`;
`Core.observeTarget type` gives `.returned type bits.toNat` for an EvoIR halt with `[.bits n bits]`,
with optional tokens, if `n = type.width`. `Machine.observe`, `Alloc.observe` and `Asm.observe` give
the low bits of the result type. Other states, malformed halts included, have none.

### 3.3 Refinement

From `Core/Transition.lean`; `Reaches step s t` means that `t` follows `s` in zero or more steps,
and `Infinite step s` means that an infinite trace starts at `s`:

```lean
structure Refines (sourceStep : S → Option S) (targetStep : T → Option T)
    (sourceObserve : S → Option O) (targetObserve : T → Option O)
    (sourceEntry : S) (targetEntry : T) : Prop where
  terminal : ∀ t, Reaches targetStep targetEntry t → targetStep t = none →
    ∃ s o, Reaches sourceStep sourceEntry s ∧ sourceStep s = none ∧
      sourceObserve s = some o ∧ targetObserve t = some o
  divergence : Infinite targetStep targetEntry → Infinite sourceStep sourceEntry
  progress : ∀ t, Reaches targetStep targetEntry t →
    (∃ next, targetStep t = some next) ∨ ∃ o, targetObserve t = some o
```

From related entry states: (1) each halted target state has the observation of a halted source
state; (2) if the target runs forever, the source runs forever; (3) no reachable target state is
stuck or malformed. `Core.Transition.Refines.trans` (`Core/Simulation.lean`) composes refinements;
`Core.LoweringRefines` is `Refines` from the source to EvoIR.

## 4. Stage checkers

### 4.1 Stage table

| Stage | Checks | Runs | Roles |
|---|---|---|---|
| `source-to-evoir` | Source to EvoIR (4.2) | Once, on the EvoIR before the first rewrite step | `source`, `ir`, optional `proposal` |
| `rewrite` | Rewrite steps (4.3) | Once for each rewrite step | `source`, `before`, `after`, `certificate` |
| `lowering` | Lowering (4.4) | Once, on the EvoIR after the last step | `source`, `ir`, `ssa`, `postssa` |
| `allocation` | Allocation (4.5) | Once | `source`, `postssa`, `allocation` |
| `frame-emission` | Frame and emission (4.6) | Once | `source`, `allocation`, `asm`, `text` |

Each stage has one identifier, in the first column. It is the stage argument of `--check-stage`,
the `stage=` word of the checker output, the `id` of the record row (7.4), the reader name in the
digest chain (7.3) and the prefix of the log files (7.2). The Lean table is `Eval.stages`
(`Eval.lean`); the Rust table `STAGES` must have the same rows, identifiers and roles, plus the
final row `pipeline` (5.1). `evoir-sem --check-stage STAGE ROLE=PATH ...` runs one row on one
entry, for example `evoir-sem --check-stage source-to-evoir source=PATH ir=PATH`. It prints
`STAGE_ACCEPTED stage=STAGE entry=NAME ...` (exit 0), `STAGE_REJECTED stage=STAGE ...` (exit 1) or
`STAGE_INPUT_UNSUPPORTED stage=STAGE ...` (exit 2). An unknown stage or a wrong role list gives
`STAGE_COMMAND_INVALID` (exit 64). `--list-stages` lists the identifiers and their roles.

### 4.2 Source to EvoIR: `source-to-evoir`

The checkers are `Core.Source.checkWellFormed` (`Core/WellFormed.lean`), then
`Core.Symbolic.checkSourceTargetPair` (`Core/SourceTargetCheck.lean`). The validator checks ranks
and result types, bindings, places, conversions, `const` writes, signatures, scopes, definite
initialization, unsequenced conflicting local accesses and the call order (a call must name an
earlier definition); it also refuses some defined inputs, such as nested writes to an assignment
target. The request is `.finite`, or `.cutpoints` or `.closure` with a proposal. An accepted result
is a route (for reports only) and a `Core.Symbolic.CallFrontierPair`. The theorem:

```lean
theorem check_source_target_refines {source : Source.Program} {target : IR.Program}
    (request : SourceTargetRequest) (route : SourceTargetRoute)
    (pair : CallFrontierPair source target)
    (_accepted : checkSourceTargetPair source target request = some (route, pair))
    (args : List Source.Value)
    (typed : args.map Source.Value.type = pair.sourceFunction.parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      Source.initial source pair.sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial target pair.targetFunction.name (loopInputValues pair.tokens args) = .ok targetEntry ∧
      LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry
```

The hypotheses are the accepted check and typed arguments; `loopInputValues` adds the memory and
world tokens if `tokens` holds. `Core.Source.checked_source_safety` (`Core/ValidatorSafety.lean`):
if `checkWellFormed` accepts, no reachable state of a selected, typed entry is malformed.

### 4.3 Rewrite steps: `rewrite`

`Rewrite.checkRewrite` (`Rewrite/Check.lean`) replays each certificate region, rebuilds the program
after the step with `replaceAll` and compares it with the given one (section 9). In
`Rewrite/Soundness.lean`:

```lean
theorem checkRewrite_refines {before after : Program} {cert : Certificate}
    (accepted : checkRewrite before after cert = true) {rt : Source.IntType} {fn : String}
    {args : List Value} {entry : State} (init : IR.initial before fn args = .ok entry)
    (safe : ∀ s, Transition.Reaches (IR.step before) entry s →
      (∃ s', IR.step before s = some s') ∨ ∃ o, observeTarget rt s = some o) :
    IR.initial after fn args = .ok entry ∧
      Transition.Refines (IR.step before) (IR.step after) (observeTarget rt) (observeTarget rt)
        entry entry

theorem source_rewrite_refines {source : Source.Program} {before after : Program}
    {cert : Certificate} (accepted : checkRewrite before after cert = true) {rt : Source.IntType}
    {fn : String} {args : List Value} {sourceEntry : Source.State} {entry : State}
    (init : IR.initial before fn args = .ok entry)
    (front : LoweringRefines source before rt sourceEntry entry) :
    IR.initial after fn args = .ok entry ∧ LoweringRefines source after rt sourceEntry entry
```

The hypothesis `safe` (each reachable state before the step steps or has an observation) is the
field `progress` of the source refinement. `Rewrite.check_source_rewrite_refines` composes one step.

### 4.4 Lowering: `lowering`

`Machine.checkLowering` (`MachineLowering.lean`) checks instruction selection (EvoIR to SSA EvoMIR)
and SSA removal (to post-SSA) on the same SSA program and entry state, with the always-on dead-code
elimination and dead poison normalization; `Machine.Transport.checkProgram` checks the untrusted
plan of `Machine.Transport.proposeProgram`. An accepted result carries its theorem in `checked`:

```lean
structure CheckedLowering (source : IR.Program) (ssa target : Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) where
  tokens : Bool
  checked : ∀ values, values.map Source.Value.type = parameters.map (fun p => p.type.type) →
    ∃ sourceEntry targetEntry,
      IR.initial source entry (loweringArguments tokens values) = .ok sourceEntry ∧
      initial target entry (values.map sourceArgument) = .ok targetEntry ∧
      IRToMachineRefines source target type sourceEntry targetEntry
```

The hypothesis is a typed value list; `sourceArgument` zero-extends to 64 bits.
`Machine.source_machine_refines` (`MachineCorrespondence.lean`) composes it with the source result.

The selection check compares a source word and a machine word through their canonical forms
(`checkTerm`, `MachineSelectionNormal.lean`). When the canonical forms differ, a second proved pass
lifts the unsigned width changes (`zext`, `trunc`) out of each bitwise operator (`liftCasts`,
`lift_casts_sound`) and compares the lifted forms. The pass runs only after the first comparison
fails, so every pair that the first comparison accepts stays accepted. It accepts, for example, the
guard of a signed division, a one-bit OR in EvoIR, against the 32-bit OR of two zero-extended `cset`
results that the backend emits (Section 10).

### 4.5 Allocation: `allocation`

`Alloc.checkAllocation` (`Alloc/Check.lean`) walks each post-SSA block and the allocated block with
the same identifier in lockstep, from untrusted hints (one abstract state per block entry). It
tracks registers and spill slots through inserted moves, spills, reloads, stack arguments, deleted
moves and calls (clobbers, ABI locations). A branch must keep its target and carry no argument, so
edge bridge blocks are refused. A post-SSA copy in the export that differs from `postssa` is
rejected. From `Alloc/Soundness.lean`:

```lean
theorem checkAllocation_sound_args (pre : Machine.Program) (post : Program) (hints : Hints)
    (accepted : checkAllocation pre post hints = true) (type : Core.Source.IntType)
    (name : String) (preArgs args : List Machine.Value) (memory : Machine.Memory)
    (preEntry : Machine.State) (started : Machine.initial pre name preArgs memory = .ok preEntry)
    (agrees : EntryAgrees pre name preArgs args)
    (progress : Machine.ObservedProgress .postSSA pre type preEntry) :
    ∃ postEntry, initial post name args memory = .ok postEntry ∧
      AllocationRefines pre post type preEntry postEntry
```

`Alloc.checkAllocation_sound` is the same with one argument list and no `agrees`. The hypotheses are
`progress` (the program before allocation is never stuck or malformed; the lowering refinement
discharges it) and `agrees` (`Alloc.EntryAgrees`: equal after register-class normalization).

`checkAllocation` checks the given hints first (`checkHinted`). If that check fails, and each given
entry hint is a subset of the ABI entry state of its entry block (`entryHintsHold`), it checks again
with that ABI entry state as the hint of each entry block (`entryHints`); the hints of the other
blocks do not change. The soundness theorems hold for every hint, and `checkHinted_of_accepted` gives
the hints that an accepted check used, so the retry adds no assumption. It accepts, for example, an
allocated entry that copies a dead parameter (Section 10).

### 4.6 Frame and emission: `frame-emission`

The checkers are `Asm.Check.checkAsm` (`Asm/Check.lean`), `Asm.printMatches` (`Asm/Print.lean`) and
`Pipeline.resultViewOk`. `checkAsm` checks the allocated program, the AST and untrusted frame hints:
frame areas inside the frame and disjoint, and a save slot for each callee-saved register that the
function writes; only `x0` to `x15` and `x19` to `x28` allocated; operand views that agree with
instruction widths, except the zero-extension mask `and xD, xN, #imm` with a 32-bit source view
and `imm < 2^32` (`zeroMask`); the entry structure: the first block is the entry block with
identifier 0, block identifiers are unique, and no branch goes to the entry block; no private frame
object or stack release; a stack bound for the frame and each callee; a body equal to the emitter
templates with optional variants off.
`printMatches file actual` is `file.print == actual`. From `Asm/Soundness.lean`:

```lean
theorem checkAsm_sound (p : Alloc.Program) (asm : File) (hints : List FrameHint)
    (checked : checkAsm p asm hints = true) (type : Core.Source.IntType) (entry : String)
    (args : List Machine.Value) (s0 : Alloc.State) (hs0 : Alloc.initial p entry args = .ok s0)
    (t0 : Asm.State) (E : Nat) (abi : AbiEntry ⟨p, asm, hints⟩ entry args t0 E)
    (rv : ResultView p entry type) (safe : Safe p type s0) :
    Refines (Alloc.step p) (Asm.step asm) (Alloc.observe type) (Asm.observe type) s0 t0
```

The hypotheses are `abi` (`Asm.Proof.AbiEntry`: an ABI entry state with 64-bit machine arguments),
`rv` (`Asm.Proof.ResultView`: each returned location is at least as wide as the result type) and
`safe` (`Asm.Proof.Safe`: each reachable allocated state steps or has an observation).
`Asm.Proof.source_asm_refines` (`Asm/Baseline.lean`) composes the stages to the AST. The pipeline
discharges `safe` by allocation, `rv` by `Pipeline.resultView_of_ok`, and replaces `abi` (5.3).

## 5. The pipeline theorem

### 5.1 The pipeline check

`Pipeline.checkPipeline` (`Pipeline/Check.lean`) is `(refusal input).isNone`. `Pipeline.refusal`
runs these checks in order on the values of one `Pipeline.Input` and returns the first refusal: (1)
`checkWellFormed`; (2) `selectEntry`; (3) `checkSourceTargetPair` on the EvoIR before the rewrite
steps; (4) `Pipeline.pairLinked`: the pair has the symbol, name, parameter types and result type of
the selected entry; (5) `Pipeline.rewriteChain`: `checkRewrite` on each step in order, possibly
none; (6) `checkLowering` on the EvoIR after the last step; (7) the token check: `lowered.tokens`
must equal `pair.tokens`, so the lowering starts from the EvoIR arguments of the source result; (8)
`Pipeline.entryViewsOk`: each 32-bit entry parameter has a 32-bit register in block 0 of the
post-SSA entry; (9) `checkAllocation`; (10) `checkAsm`; (11) `resultViewOk`; (12) `printMatches`.
Each check is a Boolean or option test; no check trusts a producer annotation.

`Pipeline.checkPipelineTexts` (`Pipeline/Text.lean`) reads the texts with `Pipeline.readInput` and
runs `refusal`: `accepted`, `rejected stage reason` or `unsupported reason`. An undecodable
certificate and an allocation export with a wrong post-SSA copy are rejections. `evoir-sem
--check-pipeline ROLE=PATH ...` runs it, with `proposal` at most once, `after` and `certificate`
once for each step in step order, and the other roles once; it prints `PIPELINE_ACCEPTED entry=NAME
...` (exit 0), `PIPELINE_REJECTED stage=STAGE ...` (exit 1) or `PIPELINE_INPUT_UNSUPPORTED` (exit
2), or `PIPELINE_COMMAND_INVALID` (exit 64) for a wrong role list. `STAGE` is the identifier (4.1)
of the first stage that refuses. The driver runs this command as the row `pipeline`.

### 5.2 The theorems

From `Pipeline/Soundness.lean`:

```lean
theorem checkPipeline_sound {input : Input} (accepted : checkPipeline input = true) :
    input.asm.print = input.text ∧
    ∃ f, selectEntry input.source = some f ∧
      ∀ args : List Source.Value, args.map Source.Value.type = f.parameters.map (·.type.type) →
      ∀ (asmEntry : Asm.State) (E : Nat),
        Asm.Proof.AbiSourceEntry ⟨input.allocated, input.asm, input.frameHints⟩ f.name args
          asmEntry E →
        ∃ sourceEntry, Source.initial input.source f.symbol args = .ok sourceEntry ∧
          Asm.Proof.SourceToAsmRefines input.source input.asm f.returnType.type sourceEntry
            asmEntry

theorem checkPipelineTexts_sound {texts : Texts} {report : String}
    (accepted : checkPipelineTexts texts = .accepted report) :
    ∃ input, readInput texts = .ok input ∧ input.asm.print = input.text ∧
    ∃ f, selectEntry input.source = some f ∧
      ∀ args : List Source.Value, args.map Source.Value.type = f.parameters.map (·.type.type) →
      ∀ (asmEntry : Asm.State) (E : Nat),
        Asm.Proof.AbiSourceEntry ⟨input.allocated, input.asm, input.frameHints⟩ f.name args
          asmEntry E →
        ∃ sourceEntry, Source.initial input.source f.symbol args = .ok sourceEntry ∧
          Asm.Proof.SourceToAsmRefines input.source input.asm f.returnType.type sourceEntry
            asmEntry
```

`Asm.Proof.SourceToAsmRefines` (`Asm/Baseline.lean`) is `Refines` from the source to AArch64 with
`observeSource` and `Asm.observe`. The only hypotheses are the accepted check, typed arguments and
the ABI entry state. The proof joins `check_source_target_refines`, `Pipeline.rewriteChain_refines`,
`checked`, `checkAllocation_sound_args` and `checkAsm_sound` with `Refines.trans`.

### 5.3 The ABI entry assumption

From `Asm/Soundness.lean`:

```lean
structure AbiSourceEntry (ctx : Ctx) (entry : String) (args : List Core.Source.Value)
    (t0 : Asm.State) (E : Nat) : Prop where
  function : ∃ i f, ctx.p.functions[i]? = some f ∧ f.name = entry ∧ t0.control = .running i 0
  sp : t0.sp = BitVec.ofNat 64 E
  aligned : E % 16 = 0
  registers : ∀ n v, n < 8 → args[n]? = some v → (t0.regs n).setWidth v.type.width = v.bits
  stackArgs : ∀ k v, args[8 + k]? = some v →
    (load64 t0.memory (E + 8 * k)).setWidth v.type.width = v.bits ∧ E + 8 * k + 8 ≤ t0.high
  returnAddress : decodeAddress (t0.regs 30) = none
  stackLow : t0.low + needOf ctx.p ctx.hints entry ≤ E
  stackHigh : E ≤ t0.high
  addressSpace : t0.high < 2^64
```

`function`: control is at line 0 of the entry, function `i` of the allocated program. `sp`: the
stack pointer is `E`. `aligned`: `E` is a multiple of 16. `registers`: for argument `n < 8` of width
`w`, the low `w` bits of `x<n>` are the argument; the other bits are arbitrary. `stackArgs`:
argument `8 + k` is in the low bits of the 8-byte slot at `E + 8 * k`, in the stack.
`returnAddress`: `x30` is outside the program. `stackLow`: the stack has `needOf` bytes below `E`.
`stackHigh`: `E` is at most the stack top. `addressSpace`: the top is below `2^64`.

The caller must obey AAPCS64 for the low bits of each argument and give sufficient stack.
`AbiSourceEntry.abiEntry` gives `AbiEntry` for the machine arguments (`Asm.Proof.abiArguments`);
`Pipeline.entryAgrees_of_ok` uses `entryViewsOk` to match them with the zero-extended source
arguments. By `Pipeline.abiSourceEntry_of_abiEntry`, zero-extended `AbiEntry` implies it.

### 5.4 Axioms and how to verify

`#print axioms` must give exactly `[propext, Classical.choice, Quot.sound]` for
`Pipeline.checkPipeline_sound`, `Pipeline.checkPipelineTexts_accepted`,
`Pipeline.checkPipelineTexts_sound`, `Rewrite.checkRewrite_refines`,
`Rewrite.source_rewrite_refines` and `Rewrite.check_source_rewrite_refines`. No `sorry`, `axiom`
declaration, `native_decide` or new `bv_decide` may enter the checked path; the `bv_decide` rows of
`Proofs/TransferCarry.lean`, `Proofs/TransferMul.lean` and `Proofs/TransferCompare.lean` are outside it. `Pipeline/Tests.lean` and `Rewrite/Tests.lean` put `#print
axioms` inside `#guard_msgs`, so a further axiom fails the module and `lake build`; run them from
`lean/` with `lake env lean FILE`. `tools/gate-lean.sh` runs the registry generator check, `lake
build` on the pinned toolchain, the self-test, the differential vectors and `tools/lean/audit.py`,
which fails on any `sorryAx`, classifies each audited theorem as `kernel-only` or `native-backed`,
and writes `tests/contracts/lean-p0-receipt.json` with the Lean commit, the toolchain, the registry
digests and the revision line and SHA-256 of `spec/evoir.md`.
`python3 tools/render-status.py --check` fails when that revision or SHA-256 differs from the
`spec/evoir.md` pin in `spec/pins.json`, so a receipt for another text of `spec/evoir.md` is stale
until `tools/gate-lean.sh` rewrites it.

## 6. Exports and readers

The driver writes these files to `NEW_DIR/inputs/`; `ENTRY` is an entry, `NNN` a step number.

| File | Role | Content | Writer (Rust) | Reader (Lean) |
|---|---|---|---|---|
| `source-ENTRY.json` | `source` | Typed Core tree, `core-v1` | `core_export::export_core` (`crates/evocc/src/core_export.rs`) | `Core.Source.readProgram` (`Core/Source.lean`) |
| `proposal-ENTRY.json` | `proposal` | Copy of the `--proof-proposal` file | `proof_required::export` | `Core.Symbolic.readCutpoints`, `readCallCutpoints` |
| `evoir.evo` | `ir` | EvoIR before the first step (`spec/evoir.md` section 9) | `evocc_evoir::render_module` | `Core.IR.Reader.readProgram` (`Core/IRReader.lean`) |
| `rewrite-NNN.before.evo`, `rewrite-NNN.after.evo` | `before`, `after` | EvoIR before and after step `NNN` | `evocc_eqopt::ProofBundle` (`crates/evocc-eqopt/src/chain.rs`) | `Core.IR.Reader.readProgram` |
| `rewrite-NNN.proof.evp` | `certificate` | `evp2` certificate (`spec/evoir.md` section 11.3) | `evocc_eqopt::ProofBundle` | `Rewrite.Reader.certificate` (`Rewrite/Certificate.lean`) |
| `evoir-rewritten.evo` | `ir` | EvoIR after the last step; only with steps | `evocc_evoir::render_module` | `Core.IR.Reader.readProgram` |
| `ssa-evomir.json`, `post-ssa-evomir.json` | `ssa`, `postssa` | `core-mir-v1`, mode `ssa` or `post-ssa` | `evocc_mir::core_machine_export::export_core_machine` | `Machine.Reader.readProgram` (`MachineReader.lean`) |
| `allocation.json` | `allocation` | `core-alloc-pair-v1`: post-SSA copy, allocated program, hints | `evocc_mir::verify_alloc::export_core_allocation` | `Alloc.Reader.readPair` (`Alloc/Reader.lean`) |
| `asm-ast.json` | `asm` | `core-asm-v1`: assembly AST and frame hints | `evocc_mir::asm_ast::export_core_asm_with_entries` | `Asm.Reader.readFile` (`Asm/Reader.lean`) |
| `assembly.s` | `text` | The text that the driver publishes | `assemble` (`crates/evocc/src/main.rs`) | Compared with `Asm.File.print` |

The SSA and post-SSA programs must come from the ordinary backend
(`evocc_mir::lower_module_for_profile`, checked UB profile, then
`evocc_mir::regalloc::checked_out_of_ssa`, which validates its SSA input and runs the E4021
transport check on the result).
Snapshots, hints and proposals are untrusted. The readers are trusted (section 8): they decode,
refuse unknown forms, and do not repair. They do not refuse every noncanonical spelling. The JSON
readers parse with Lean's `Json.parse`, which skips spaces, tabs, carriage returns and line feeds
around each token and decodes string escapes. `Core.IR.Reader.readProgram` trims each line and skips
empty lines. `Rewrite.Reader.certificate` splits at each line feed and each space and trims nothing.
A noncanonical spelling of a value therefore decodes to that value; the digest chain (7.3) binds
the bytes that each reader reads to the bytes that the driver wrote. A source reader fault changes
the claimed program; another reader fault changes compared values.

## 7. Proof-required mode

### 7.1 Command line

```text
evocc --emit=asm --proof-required=NEW_DIR --core-entry=NAME [--core-entry=NAME ...] -O0 -fno-builtin
      [--proof-checker=PATH] [--proof-checker-sha256=SHA256] [--proof-timeout-ms=N]
      [--proof-proposal=NAME=PATH ...] [-Oeq-scalar --eq-checker-sha256=SHA256 ...] FILE.c
```

`NEW_DIR` must not exist. `--proof-checker` (else `EVOCC_PROOF_CHECKER`) names the compiled checker
`evoir-sem`; with `--proof-checker-sha256`, it must have this SHA-256. `--proof-timeout-ms` limits
one run, from 1 to 3600000 (default 120000). `--proof-proposal` must name a `--core-entry`. Each
option may occur once (a proposal once per entry), and only with `--proof-required`.

### 7.2 Order of checks

(1) Admission (section 2.1). (2) Compilation; with `-Oeq-scalar`, the Rust certificate checker
checks each step. (3) Export (section 6), with the output checks of section 2.2 and at least one
step with `-Oeq-scalar`. (4) The assembly text, then the checker location and digest. (5) The digest
chain. (6) `NEW_DIR`, `inputs/` and `checks/`; each input is written once. (7) The driver rows in
order: `source-to-evoir`, `rewrite`, `lowering`, `allocation`, `frame-emission`, `pipeline`; each
row for each entry, the row `rewrite` also for each step; logs in `checks/STAGE-ENTRY[-NNN].out` and
`.err`. (8) A second digest of each input: the checker must not change an input. (9) `output.s` and
`record.json`, each created new, then the text on standard output. `pipeline` is the final decision;
each row before it must also accept. Checker exit 0 is an acceptance only with exactly one output
line that is, or starts with, `STAGE_ACCEPTED stage=STAGE entry=ENTRY` (`PIPELINE_ACCEPTED
entry=ENTRY` for `pipeline`). Exit 1 is a rejection, exit 2 an unsupported input, anything else (a
signal, the time limit, a missing line) a checker failure.

### 7.3 Digest chain

Two adjacent readers must read the same bytes. Before a checker runs, the driver compares SHA-256
digests: the EvoIR of `source-to-evoir` and the `before` file of step 0; the `after` file of step
`k` and the `before` file of step `k + 1`; the `after` file of the last step and the EvoIR of
`lowering` (without steps, `evoir.evo` for both); `post-ssa-evomir.json` for `lowering` and
`allocation`; `allocation.json` for `allocation` and `frame-emission`; `assembly.s` for
`frame-emission` and the published `output.s`. A different digest is a gap and gives exit 4. In the
record, a reader is its stage identifier, `rewrite step K` for step `K`, or `publication` for
`output.s`. The `pipeline` row reads `evoir.evo` and the `after` and `certificate` files; the chain
binds the other EvoIR files to them.

### 7.4 Record

`NEW_DIR/record.json` has these top-level fields, in order: `schema`
(`evocc-proof-required-record-v1`); `result` and `scope` (section 7.5); `all_stages_checked`;
`profile` (`core-v1`); `target`; `source`, `compiler`, `checker` (`path` and `sha256` of the C file,
the driver and the checker); `checker_digest_pinned`; `checker_timeout_ms`; `rewrite_checker` (only
with `-Oeq-scalar`); `entries` (`entry` and `closure`, its function names); `stages` (one row per
row of the driver table); `chain` (one link per pair of section 7.3: `sha256` and `read_by`, two
objects with `stage`, `role`, `path`); `output` (`path`, `sha256`, `checked`, `note`:
"frame-emission checked these bytes: the printed assembly AST is equal to them."); `trusted`
(parts 1 to 7 of section 8). A checked stage row has `id` (the stage identifier of 4.1, or
`pipeline`), `status` (`CHECKED`), `theorem` and `results`; each result has `entry`, `step` (row
`rewrite`), `result` (`ACCEPTED`), `inputs` (`role`, `path`, `sha256`) and `report` (the
acceptance line). Other rows have `id`, `status`, `reason`. A stage row has no other name field.

### 7.5 Result labels

`result` is `CHECKED_TO_` and the last program of the leading integrated rows, in upper case. A row
is integrated when it has roles. `all_stages_checked` and `output.checked` are `true` exactly when
every row of the driver table is integrated, whether or not each row ran. Every current row is
integrated, so `result` must be `CHECKED_TO_FINAL_ASSEMBLY`, `scope` must be "checked from typed
Core source to final assembly text", and `all_stages_checked` and `output.checked` must be `true`.
A row without roles is `NOT_CHECKED` ("not integrated"); its result must not be called checked to
assembly text. Without `-Oeq-scalar`, the row `rewrite` is `NOT_CHECKED` with the reason "no rewrite
ran; the compilation has no -Oeq-scalar", and `all_stages_checked` stays `true`: there is no
rewrite step to check, and the row `pipeline` checks a chain of zero steps (5.1, check 5).

### 7.6 Exit codes

| Exit | Meaning |
|---|---|
| 0 | Each row accepted each entry. `output.s` and `record.json` exist; the text is on standard output. |
| 1 | An incorrect command line, a refused admission, or a compilation error before the proof inputs exist, a refusal of a rewrite step by the Rust certificate checker included. |
| 3 | A stage checker rejected an entry (checker exit 1). Standard error names the row: `stage STAGE for entry ...`, where `STAGE` is a stage identifier or `pipeline`. |
| 4 | A proof input is missing, changed, not readable or not writable; the chain has a gap; `NEW_DIR` exists; the checker reported an unsupported input (checker exit 2); `-Oeq-scalar` gave no step; or `checked_out_of_ssa` refused the SSA removal of the export (section 6). |
| 5 | The checker is missing or not readable, has another digest than the pinned one, failed, timed out, or gave no acceptance line. |
| 6 | The source or the output is outside `core-v1`, or the driver refused to lower the module or to export a snapshot, except the exit 4 of `checked_out_of_ssa`. |

### 7.7 Fail-closed rule

On exits 3 to 6, standard output must be empty, `NEW_DIR/output.s` and `NEW_DIR/record.json` must
not exist, and standard error must contain "no output was written"; `NEW_DIR/inputs/` and
`NEW_DIR/checks/` can exist as diagnosis. Exit 1 before the proof inputs exist leaves no `NEW_DIR`;
exit 1 after the record exists means that standard output could not be written. There is no fallback
to ordinary compilation, to the program without a rewrite step, or to the finite request. `output.s`
must hold exactly the bytes that `frame-emission` and `pipeline` read.

## 8. Trusted base

The record field `trusted` lists parts 1 to 7. Tests check the parts but do not prove them.

1. **Preprocessing, parsing, elaboration and the typed source export**: no checker compares the tree
   with the C text. Tested by the GCC 14 and Clang 19 differential tests (`tests/acceptance/`), the
   Rust interpreters and the gcc-14 cross-check of `tools/proof-native/`.
2. **The Lean kernel, the compiled checker, the Lean compiler, its runtime and the host toolchain**:
   each run executes compiled code. Tested by `tools/gate-lean.sh` and `proof_required.rs`.
3. **The readers of the input files**: the theorem is about decoded values. Tested by the Lean
   reader tests and the changed-input tests of `proof_required.rs`.
4. **The printer `Asm.File.print`**: it fixes the text meaning. Tested by `Asm/PrintTests.lean`.
5. **The Lean semantics of typed Core C and of the AArch64 subset**: the meaning at both ends.
   Tested by agreement with the Rust interpreters and by the native run of `tools/proof-native/`.
6. **The ABI entry assumption `Asm.Proof.AbiSourceEntry`**: tested by the upper-bits probe.
7. **The assembler, the linker, the runtime libraries and the hardware**: outside the theorem.
   Tested by the native run with GNU as and ld, and with clang and ld.lld.
8. **The publication path of the driver** in `crates/evocc/src/proof_required.rs`: the checker run
   and its acceptance line (`locate_checker`, `check`), the digest chain (`chain`, section 7.3), the
   second digest of each input (section 7.2, step 8) and the writes of `output.s` and `record.json`.
   The rule of section 7.7, that `output.s` holds the bytes that the checker read, rests on this
   code alone. Tested by the checker-response, chain, changed-input and publication-failure tests
   of `proof_required.rs`.

Not trusted: the rest of the Rust compiler, the snapshots, hints and proposals, the Lean semantics
of EvoIR, EvoMIR and allocated EvoMIR, and the Rust certificate checker, which runs in addition.

## 9. Certificates and rewrites

A rewrite step is one transaction of the scalar EqSat chain of `-Oeq-scalar`: the EvoIR before and
after the step and the certificate `proof.evp`, the triple of `--eq-proof-dir` (`spec/evoir.md`
sections 11.2 and 11.3). During compilation, the Rust certificate checker `evocc-checker`, pinned by
`--eq-checker-sha256`, checks the step; in proof-required mode, `Rewrite.checkRewrite` checks it
again, and the theorem depends only on this check. `Rewrite.Reader.certificate` accepts only `evp2`.
It keeps the registry and the regions (witnesses, seeds, `node`, `rewrite` and `congruence` events,
extraction, root class) and ignores the budget line, the epoch markers and the input and output
digests; the checker compares the complete program instead. `Rewrite.registryRules`
(`Rewrite/Rules.lean`) admits `p0.scalar.v1` (27 rules), `p0.scalar.v2` (45), `p0.scalar.v3` (51),
`p0.scalar.v4` (55) and `g0.ub.v1` (7) by name and digest (normative in `spec/evoir.md` section
11.6); `Rewrite.p0Covered` and `Rewrite.g0Covered` carry a `RuleCovered` theorem for each rule.

The Lean checker must refuse: another version or a malformed line; an unknown registry or digest;
the root-deletion (alias) form, in which the root class is extracted to a seed, such as the alias
form of `p0_001_add_zero_rhs`; an extraction with more than one new instruction (the operands of the
replacement must be seeds or earlier witness results); an extraction that reuses the root node; a
root or replacement that is not a pure constant or scalar operation; a guard region whose
replacement is not the constant 1; a seed or witness result that can be poison (`spec/evoir.md`
section 8.5); two regions with one root; any difference outside the replaced roots, a deleted
obligation included. The fact table of `Rewrite.propose` is untrusted; `Rewrite.tableOk` checks that
it is inductive. `Rewrite.kernelRow` (`Rewrite/Fact.lean`) admits the known-bits rows of `and`,
`or`, `xor`, shifts, width changes and guards. Every other row, among them `add`, `sub`, `mul` and
the comparisons, gives no known bit, because its proof uses `bv_decide`. These proofs are in
`Proofs/TransferCarry.lean`, `Proofs/TransferMul.lean` and `Proofs/TransferCompare.lean`;
`Proofs/Transfer.lean` keeps the kernel-only rows that `Rewrite.kernelRow` admits.

## 10. Conformance

- `crates/evocc/tests/proof_required.rs` runs the compiled checker on the accepted pilot entries and
  the rewrite chain, and checks each refusal with its exit code and the rule of section 7.7.
- The Lean test modules, built by `lake build`: `Pipeline/Tests.lean` (accepted fixtures, a
  rejection by the correct stage for each changed input, `AbiSourceEntry` with nonzero upper bits,
  the axiom guard), `Rewrite/Tests.lean`, `Core/SourceTargetCheckTests.lean`,
  `Core/WellFormedTests.lean`, `MachineLoweringTests.lean`, `MachineSelectionDivisionTests.lean`
  (the D06 guard pair, the cast-lifting laws and the changed operators that stay refused),
  `Alloc/AcceptanceTests.lean` (with the D06 dead-parameter entry, its refinement theorem and three
  refused changes), `Asm/CheckTests.lean`, `Asm/PrintTests.lean`.
- `crates/evocc/tests/proof_required.rs` also checks the unchanged D06 program of
  `tests/fixtures/d06-guarded-division/` to final assembly text, and its seven changed inputs exit 3.
- `tools/gate-lean.sh` and `python3 tools/proof-native/proof_native.py all WORK_DIR`.
- The receipts `tests/contracts/core-v1-pilot.json`, `tests/contracts/core-v1-native-run.json` and
  `tests/contracts/lean-p0-receipt.json`.

A change to a checker, a theorem statement, a reader, the stage tables, the exports or the record
must keep the Lean and Rust tables equal, update this document, pass these tests and gates, and
refresh each receipt that it makes stale.

## 11. Revision

Revision 1.1.4 of 2026-10-30 drops tracker IDs, archive pointers and repeated text. Sections 6 and
7.6 name the functions and exit codes that the driver uses, and Section 8 adds the publication path
of the driver as part 8 of the trusted base, which the record field `trusted` does not list.

Revision 1.1.3 of 2026-10-27 names the modules that now hold the `bv_decide` rows of the
transfer table: `Proofs/TransferCarry.lean`, `Proofs/TransferMul.lean` and `Proofs/TransferCompare.lean`. `Proofs/Transfer.lean` keeps the
kernel-only rows. No theorem, rule or check changes.

Revision 1.1.2 of 2026-10-15 makes the assembly-only command boundary explicit
and states that selected entries retain their full argument domains and bodies.
The driver removes both publication files if either final write fails, and
refuses a run if it cannot read its executable for the compiler digest.

Revision 1.1.1 of 2026-10-14 corrects the text to what the driver and Lean do: every
EqSat option that the driver admits with `-Oeq-scalar` (Section 2.1), the `zeroMask` exception, the
callee-saved save slots and the entry structure of `checkAsm` (Section 4.6), the whitespace and
escapes that the readers accept (Section 6) and the rule of `all_stages_checked` (Section 7.5).
Section 5.4 names the check that compares the Lean receipt with the `spec/evoir.md` pin. The theorem
statements and the record do not change. Revision 1.1.0 of 2026-10-14 stated the second
comparison of the selection check (Section 4.4), the entry-hint retry of the allocation check
(Section 4.5) and the Lean and Rust tests of TODO D06 (Section 10). Revision 1.0.0 of
2026-10-06 was the first pinned text. To change this document: (1) edit the text; (2) increase
the revision line (major: the claim, a theorem statement or the record schema; minor: a new stage,
export or field; patch: wording); (3) run `python3 tools/check-spec-pins.py --update`, then `python3
tools/check-spec-pins.py` and the gate of the change; (4) commit the document and `spec/pins.json`
together. The history is in git: `git log -- spec/verification.md`.
