import EvoIR.Pipeline.Check
import EvoIR.Rewrite.Soundness
import EvoIR.Asm.Baseline

/-!
Soundness of the pipeline check.

`checkPipeline_sound`: when `checkPipeline input` is true, the printed AST is
the actual text, and the source selects an entry function `f`. For each
argument list of the parameter types of `f` and each assembly entry state
that obeys the ABI (`AbiEntry`), the source has an initial state, and the
assembly AST refines the source from these two states.

The theorem has no hypothesis other than the accepted check, the typed
argument list and the ABI entry state. The proof composes the stage theorems
on the same values:

* V01: `check_source_target_refines` gives the source and EvoIR entries.
* V05: `source_rewrite_refines`, once for each step, keeps the EvoIR entry.
* V02: the `checked` field of `checkLowering` gives the EvoIR and post-SSA
  entries. The link checks of `refusal` make the two EvoIR entries equal.
* V03: `checkAllocation_sound`. The progress field of the V02 refinement
  discharges its progress hypothesis.
* V04: `checkAsm_sound`. `resultViewOk` gives `ResultView`, and the progress
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
  simp only [Bool.not_eq_true', Bool.not_eq_false] at linked alloc asm view print
  simp only [bne_iff_ne, ne_eq, Decidable.not_not] at tokens
  exact ⟨f, route, pair, last, lowered, hf, hpair, linked, hchain, tokens, alloc, asm, view, print⟩

/-- **Soundness of the pipeline check.** An accepted check gives the print of
the AST as the actual text and, for the selected entry `f`, every typed
argument list and every ABI entry state, the refinement of the source by the
assembly AST. -/
theorem checkPipeline_sound {input : Input} (accepted : checkPipeline input = true) :
    input.asm.print = input.text ∧
    ∃ f, selectEntry input.source = some f ∧
      ∀ args : List Source.Value, args.map Source.Value.type = f.parameters.map (·.type.type) →
      ∀ (asmEntry : Asm.State) (E : Nat),
        Asm.Proof.AbiEntry ⟨input.allocated, input.asm, input.frameHints⟩ f.name
          (args.map Machine.sourceArgument) asmEntry E →
        ∃ sourceEntry, Source.initial input.source f.symbol args = .ok sourceEntry ∧
          Asm.Proof.SourceToAsmRefines input.source input.asm f.returnType.type sourceEntry
            asmEntry := by
  have none_ : refusal input = none := by simpa [checkPipeline] using accepted
  obtain ⟨f, route, pair, last, lowered, hf, hpair, linked, chain, tokens, alloc, asm, view,
    print⟩ := refusal_none none_
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
  -- V03 and V04.
  obtain ⟨allocEntry, allocStarted, allocation⟩ := Alloc.checkAllocation_sound _ _ _ alloc
    f.returnType.type f.name _ _ postEntry postStarted lowering.progress
  have toAsm := Asm.Proof.checkAsm_sound _ _ _ asm f.returnType.type f.name _ allocEntry
    allocStarted asmEntry E abi (resultView_of_ok view) allocation.progress
  refine ⟨sourceEntry, by rw [← symbol]; exact started, ?_⟩
  exact Refines.trans (Refines.trans (Refines.trans front lowering) allocation) toAsm

end EvoIR.Pipeline
