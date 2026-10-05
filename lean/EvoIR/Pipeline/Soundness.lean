import EvoIR.Pipeline.Check
import EvoIR.Pipeline.Text
import EvoIR.Rewrite.Soundness
import EvoIR.Asm.Baseline

/-!
Soundness of the pipeline check.

`checkPipeline_sound`: when `checkPipeline input` is true, the printed AST is
the actual text, and the source selects an entry function `f`. For each
argument list of the parameter types of `f` and each assembly entry state
that obeys the ABI (`AbiSourceEntry`), the source has an initial state, and
the assembly AST refines the source from these two states. In the entry
state, only the low `w` bits of the register or stack slot of an argument of
width `w` are equal to the argument. The other bits are arbitrary.

The theorem has no hypothesis other than the accepted check, the typed
argument list and the ABI entry state. The proof composes the stage theorems:

* V01: `check_source_target_refines` gives the source and EvoIR entries.
* V05: `source_rewrite_refines`, once for each step, keeps the EvoIR entry.
* V02: the `checked` field of `checkLowering` gives the EvoIR and post-SSA
  entries. The link checks of `refusal` make the two EvoIR entries equal.
  The post-SSA entry has the zero-extended source arguments.
* V03: `checkAllocation_sound_args`. The allocated entry has the machine
  arguments of the assembly entry state (`abiArguments`), with all 64 bits.
  `entryViewsOk` makes the two argument lists agree at the entry
  (`EntryAgrees`): a 32-bit parameter has a 32-bit register, and the write
  to this register keeps only the low 32 bits. The progress field of the V02
  refinement discharges the progress hypothesis.
* V04: `checkAsm_sound` with `AbiEntry` for the machine arguments of the
  assembly entry state. `resultViewOk` gives `ResultView`, and the progress
  field of the V03 refinement gives `Safe`.
-/
namespace EvoIR.Pipeline

open EvoIR.Core
open EvoIR.Core.Transition (Refines)

/-- An accepted rewrite chain keeps the EvoIR entry state and refines the
source from the same entry states. -/
theorem rewriteChain_refines {source : Source.Program} {rt : Source.IntType} {fn : String}
    {args : List IR.Value} {sourceEntry : Source.State} {entry : IR.State} :
    ∀ {k : Nat} {ir last : IR.Program} {steps : List RewriteStep},
      rewriteChain k ir steps = .ok last → IR.initial ir fn args = .ok entry →
      LoweringRefines source ir rt sourceEntry entry →
      IR.initial last fn args = .ok entry ∧ LoweringRefines source last rt sourceEntry entry
  | _, _, _, [], chain, init, front => by
    cases chain
    exact ⟨init, front⟩
  | _, _, _, step :: _, chain, init, front => by
    unfold rewriteChain at chain
    split at chain
    · rename_i accepted
      obtain ⟨init', front'⟩ := Rewrite.source_rewrite_refines accepted init front
      exact rewriteChain_refines chain init' front'
    · cases chain

/-- `resultViewOk` decides `ResultView`. -/
theorem resultView_of_ok {p : Alloc.Program} {entry : String} {type : Source.IntType}
    (ok : resultViewOk p entry type = true) : Asm.Proof.ResultView p entry type := by
  intro f hf hname b hb locs hterm l hl
  have row := List.all_eq_true.mp ok f hf
  simp only [hname, bne_self_eq_false, Bool.false_or] at row
  have block := List.all_eq_true.mp row b hb
  rw [hterm] at block
  simpa using List.all_eq_true.mp block l hl

/-- The facts of an input that no stage refuses. -/
theorem refusal_none {input : Input} (none_ : refusal input = none) :
    ∃ (f : Source.Function) (route : Symbolic.SourceTargetRoute)
      (pair : Symbolic.CallFrontierPair input.source input.ir) (last : IR.Program)
      (lowered : Machine.CheckedLowering last input.ssa input.postSSA f.name f.parameters
        f.returnType.type),
      selectEntry input.source = some f ∧
      Symbolic.checkSourceTargetPair input.source input.ir input.request = some (route, pair) ∧
      pairLinked f pair = true ∧
      rewriteChain 0 input.ir input.rewrites = .ok last ∧
      lowered.tokens = pair.tokens ∧
      entryViewsOk input.postSSA f = true ∧
      Alloc.checkAllocation input.postSSA input.allocated input.allocHints = true ∧
      Asm.Check.checkAsm input.allocated input.asm input.frameHints = true ∧
      resultViewOk input.allocated f.name f.returnType.type = true ∧
      Asm.printMatches input.asm input.text = true := by
  unfold refusal at none_
  split at none_
  · cases none_
  split at none_
  · cases none_
  rename_i f hf
  split at none_
  · cases none_
  rename_i route pair hpair
  split at none_
  · cases none_
  rename_i linked
  split at none_
  · cases none_
  rename_i last hchain
  split at none_
  · cases none_
  rename_i lowered _
  split at none_
  · cases none_
  rename_i tokens
  split at none_
  · cases none_
  rename_i views
  split at none_
  · cases none_
  rename_i alloc
  split at none_
  · cases none_
  rename_i asm
  split at none_
  · cases none_
  rename_i view
  split at none_
  · cases none_
  rename_i print
  simp only [Bool.not_eq_true', Bool.not_eq_false] at linked views alloc asm view print
  simp only [bne_iff_ne, ne_eq, Decidable.not_not] at tokens
  exact ⟨f, route, pair, last, lowered, hf, hpair, linked, hchain, tokens, views, alloc, asm, view,
    print⟩

/-- A register of class `regClass` gets the same value from a 64-bit
argument `raw` and from the zero-extended source value `value` when the low
bits of `raw` are `value` and the class keeps no more bits than the width of
`value`. -/
theorem normalize_low_bits (regClass : Machine.RegClass) (value : Source.Value)
    (raw : BitVec 64) (low : raw.setWidth value.type.width = value.bits)
    (narrow : value.type.width = 64 ∨ regClass = .gpr32) :
    Machine.normalize regClass (.int raw) =
      Machine.normalize regClass (Machine.sourceArgument value) := by
  rcases value with ⟨type, bits⟩
  simp only at low narrow
  subst low
  cases type
  all_goals first
    | (rcases narrow with narrow | rfl
       · simp [Source.IntType.width] at narrow
       · simp [Machine.sourceArgument, Machine.normalize, Source.IntType.width])
    | simp [Machine.sourceArgument, Machine.normalize, Source.IntType.width]

/-- The low bits of a zero-extended source value are the value. -/
theorem sourceArgument_low (value : Source.Value) :
    (value.bits.setWidth 64).setWidth value.type.width = value.bits := by
  rcases value with ⟨type, bits⟩
  cases type <;> simp [Source.IntType.width]

/-- The entry assumption of the pipeline theorem is weaker than `AbiEntry`
with the zero-extended source arguments. -/
theorem abiSourceEntry_of_abiEntry {ctx : Asm.Proof.Ctx} {entry : String}
    {args : List Source.Value} {t0 : Asm.State} {E : Nat}
    (abi : Asm.Proof.AbiEntry ctx entry (args.map Machine.sourceArgument) t0 E) :
    Asm.Proof.AbiSourceEntry ctx entry args t0 E where
  function := abi.function
  sp := abi.sp
  aligned := abi.aligned
  registers n v small found := by
    have same := abi.registers n (Machine.sourceArgument v) small (by simp [found])
    simp only [Machine.sourceArgument, Machine.Value.int.injEq] at same
    rw [← same]
    exact sourceArgument_low v
  stackArgs k v found := by
    obtain ⟨same, bound⟩ := abi.stackArgs k (Machine.sourceArgument v) (by simp [found])
    simp only [Machine.sourceArgument, Machine.Value.int.injEq] at same
    rw [← same]
    exact ⟨sourceArgument_low v, bound⟩
  returnAddress := abi.returnAddress
  stackLow := abi.stackLow
  stackHigh := abi.stackHigh
  addressSpace := abi.addressSpace

/-- `entryViewsOk` makes the zero-extended source arguments and the machine
arguments of an ABI entry state agree at the post-SSA entry. -/
theorem entryAgrees_of_ok {p : Machine.Program} {f : Source.Function}
    {args : List Source.Value} {ctx : Asm.Proof.Ctx} {t0 : Asm.State} {E : Nat}
    (ok : entryViewsOk p f = true)
    (typed : args.map Source.Value.type = f.parameters.map (·.type.type))
    (abi : Asm.Proof.AbiSourceEntry ctx f.name args t0 E) :
    Alloc.EntryAgrees p f.name (args.map Machine.sourceArgument)
      (Asm.Proof.abiArguments t0 E args.length) := by
  refine ⟨by simp [Asm.Proof.abiArguments], ?_⟩
  intro g block hg hb i h h' h''
  simp only [entryViewsOk, hg, hb] at ok
  have hi : i < args.length := by simpa using h'
  have hp : i < f.parameters.length := by
    have := congrArg List.length typed
    simp only [List.length_map] at this
    omega
  have row := List.all_eq_true.mp ok (block.args[i], f.parameters[i]) (by
    have lt : i < (block.args.zip f.parameters).length := by simp; omega
    simpa [List.getElem_zip] using List.getElem_mem lt)
  have type : args[i].type = f.parameters[i].type.type := by
    have := congrArg (·[i]?) typed
    simpa [hi, hp] using this
  obtain ⟨raw, hraw, low⟩ := abi.low i hi
  simp only [Asm.Proof.abiArguments, List.getElem_map, List.getElem_range, hraw]
  apply normalize_low_bits _ _ raw low
  simpa [type] using row

/-- **Soundness of the pipeline check.** An accepted check gives the print of
the AST as the actual text and, for the selected entry `f`, every typed
argument list and every ABI entry state, the refinement of the source by the
assembly AST. The ABI entry state fixes only the low `w` bits of the
register or stack slot of each argument of width `w`. -/
theorem checkPipeline_sound {input : Input} (accepted : checkPipeline input = true) :
    input.asm.print = input.text ∧
    ∃ f, selectEntry input.source = some f ∧
      ∀ args : List Source.Value, args.map Source.Value.type = f.parameters.map (·.type.type) →
      ∀ (asmEntry : Asm.State) (E : Nat),
        Asm.Proof.AbiSourceEntry ⟨input.allocated, input.asm, input.frameHints⟩ f.name args
          asmEntry E →
        ∃ sourceEntry, Source.initial input.source f.symbol args = .ok sourceEntry ∧
          Asm.Proof.SourceToAsmRefines input.source input.asm f.returnType.type sourceEntry
            asmEntry := by
  have none_ : refusal input = none := by simpa [checkPipeline] using accepted
  obtain ⟨f, route, pair, last, lowered, hf, hpair, linked, chain, tokens, views, alloc, asm,
    view, print⟩ := refusal_none none_
  refine ⟨by simpa [Asm.printMatches] using print, f, hf, ?_⟩
  intro args typed asmEntry E abi
  simp only [pairLinked, Bool.and_eq_true, beq_iff_eq] at linked
  obtain ⟨⟨⟨symbol, name⟩, parameters⟩, result⟩ := linked
  -- V01 and V05: the source refines the EvoIR after the last step.
  obtain ⟨sourceEntry, irEntry, started, irStarted, front⟩ :=
    Symbolic.check_source_target_refines _ route pair hpair args (by rw [parameters]; exact typed)
  obtain ⟨lastStarted, front⟩ := rewriteChain_refines chain irStarted front
  rw [result] at front
  -- V02: the same EvoIR entry state starts the lowering refinement.
  obtain ⟨irEntry', postEntry, irStarted', postStarted, lowering⟩ := lowered.checked args typed
  have same : IR.initial last f.name (Machine.loweringArguments lowered.tokens args) =
      .ok irEntry := by
    rw [tokens, ← name]
    exact lastStarted
  cases (Except.ok.inj (irStarted'.symm.trans same))
  -- V03 from the zero-extended arguments to the machine arguments of the
  -- assembly entry state, then V04 with these machine arguments.
  obtain ⟨allocEntry, allocStarted, allocation⟩ := Alloc.checkAllocation_sound_args _ _ _ alloc
    f.returnType.type f.name _ _ _ postEntry postStarted (entryAgrees_of_ok views typed abi)
    lowering.progress
  have toAsm := Asm.Proof.checkAsm_sound _ _ _ asm f.returnType.type f.name _ allocEntry
    allocStarted asmEntry E abi.abiEntry (resultView_of_ok view) allocation.progress
  refine ⟨sourceEntry, by rw [← symbol]; exact started, ?_⟩
  exact Refines.trans (Refines.trans (Refines.trans front lowering) allocation) toAsm

/-- The pipeline check on the texts: an accepted verdict gives the conclusion
of `checkPipeline_sound` for the input that the readers give. -/
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
            asmEntry := by
  obtain ⟨input, read, checked⟩ := checkPipelineTexts_accepted accepted
  exact ⟨input, read, checkPipeline_sound checked⟩

end EvoIR.Pipeline
