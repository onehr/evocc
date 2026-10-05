import EvoIR.MachinePrivateBinding
import EvoIR.MachineSelectionConvert
import EvoIR.MachineTransportBindings

/-! Native guard continuations have real mixed pointer/word SSA argument
vectors. The source IR activation stays at the same block and token bank;
only native parameters are rebound. Word quotation is an independently
proved scalar denotation check. Pointer identities remain Nat-valued tokens.
No scalar expression is fabricated for an object address. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def PrivateNativeInputsDiagram {Γ : Var → Nat} (before : PrivateBank Γ)
    (args params : List EvoIR.Machine.VReg) (inputs : List (PrivateInput Γ)) : Prop :=
  inputs.map PrivateInput.target = params ∧
  ∀ σ values incoming owner frame memory regs,
    before.Models σ values incoming owner frame memory regs →
    args.mapM (EvoIR.Machine.read regs) = .ok (inputs.map (PrivateInput.value σ values)) ∧
      PrivateInputsBound inputs values (before.source.scope.denote σ (privateToken values))

private def privateNativePointer {Γ : Var → Nat} (before : PrivateBank Γ)
    (src dst : EvoIR.Machine.VReg) : Option {input : PrivateInput Γ //
      input.target = dst ∧ ∀ σ values incoming owner frame memory regs,
        before.Models σ values incoming owner frame memory regs →
        EvoIR.Machine.read regs src = .ok (input.value σ values) ∧
        PrivateInputsBound [input] values (before.source.scope.denote σ (privateToken values))} := do
  let link : {link // before.pointers.find? (fun link => decide (link.target = src)) = some link} ←
    match found : before.pointers.find? (fun link => decide (link.target = src)) with
    | none => none
    | some link => some ⟨link, found⟩
  let key : {key // before.source.scope.lookup link.val.source = some (.opaque key)} ←
    match found : before.source.scope.lookup link.val.source with
    | some (.opaque key) => some ⟨key, found⟩
    | _ => none
  return ⟨.pointer link.val.source dst key.val, rfl, by
    intro σ values incoming owner frame memory regs models
    have target : link.val.target = src := by simpa using List.find?_some link.property
    obtain ⟨id, sourceRead, targetRead⟩ := models.pointers link.val (List.mem_of_find?_eq_some link.property)
    have sourceActual : IR.lookup (before.source.scope.denote σ (privateToken values)) link.val.source =
        .ok (privateToken values key.val) := by
      simp [IR.lookup, lookup_denote, key.property, Slot.denote]
    have token : privateToken values key.val = .pointer id 0 :=
      Except.ok.inj (sourceActual.symm.trans sourceRead)
    have valueRead : EvoIR.Machine.read regs src = .ok (.pointer id 0) := by
      have read : EvoIR.Machine.operandValue (EvoIR.Machine.readOperand regs) src =
          .ok (.pointer id 0) := by
        simp only [EvoIR.Machine.operandValue, ← target, targetRead, Bind.bind, Except.bind, Pure.pure, Except.pure]
      simpa only [EvoIR.Machine.operandValue_readOperand] using read
    constructor
    · simpa only [PrivateInput.value, privatePointerValue, token] using valueRead
    · intro name reg pointerKey member
      have same : PrivateInput.pointer name reg pointerKey =
          PrivateInput.pointer link.val.source dst key.val := List.mem_singleton.mp member
      cases same
      exact ⟨id, sourceRead, token⟩⟩

def privateNativeInputs {Γ : Var → Nat} (before : PrivateBank Γ) :
    (args params : List EvoIR.Machine.VReg) →
    Option {inputs : List (PrivateInput Γ) // PrivateNativeInputsDiagram before args params inputs}
  | [], [] => some ⟨[], rfl, by
      intro σ values incoming owner frame memory regs models
      exact ⟨rfl, by intro name reg key member; cases member⟩⟩
  | src :: args, dst :: params => do
    let tail ← privateNativeInputs before args params
    match slot : before.words.lookup src.id with
    | some (.word word) =>
      let quoted ← wordDiagram word
      let input := PrivateInput.word src.id dst (⟨64, quoted.val⟩ : Word Γ)
      return ⟨input :: tail.val, by
        simp only [input, List.map_cons, PrivateInput.target, tail.property.1], by
        intro σ values incoming owner frame memory regs models
        obtain ⟨reads, pointers⟩ := tail.property.2 σ values incoming owner frame memory regs models
        have nativeRead : EvoIR.Machine.read regs src = .ok (.int (word.denote σ)) := by
          have actual := models.words src.id (.word word) slot
          simp only [EvoIR.Machine.read, EvoIR.Machine.readSlot, actual, BankSlot.denote]
        have denoted : input.value σ values = .int (word.denote σ) := by
          simp only [input, PrivateInput.value, BitVec.setWidth_eq, quoted.property σ]
        constructor
        · simp only [List.mapM_cons, nativeRead, reads, List.map_cons, denoted,
            Bind.bind, Except.bind, Pure.pure, Except.pure]
        · intro name reg key member
          rcases List.mem_cons.mp member with impossible | member
          · simp only [input, reduceCtorEq] at impossible
          · exact pointers name reg key member⟩
    | _ =>
      let pointer ← privateNativePointer before src dst
      return ⟨pointer.val :: tail.val, by
        simp only [List.map_cons, pointer.property.1, tail.property.1], by
        intro σ values incoming owner frame memory regs models
        obtain ⟨first, pointerBound⟩ := pointer.property.2 σ values incoming owner frame memory regs models
        obtain ⟨reads, pointers⟩ := tail.property.2 σ values incoming owner frame memory regs models
        constructor
        · simp only [List.mapM_cons, first, reads, List.map_cons,
            Bind.bind, Except.bind, Pure.pure, Except.pure]
        · intro name reg key member
          rcases List.mem_cons.mp member with member | member
          · exact pointerBound name reg key (List.mem_singleton.mpr member)
          · exact pointers name reg key member⟩
  | _, _ => none

structure PrivateNativeJumpResult (Γ : Var → Nat) where
  block : EvoIR.Machine.Block
  bank : PrivateBank Γ

def PrivateNativeJumpDiagram {Γ : Var → Nat} (function : EvoIR.Machine.Function)
    (before : PrivateBank Γ) (destination : Nat) (args : List EvoIR.Machine.VReg)
    (result : PrivateNativeJumpResult Γ) : Prop :=
  function.blocks.find? (·.id == destination) = some result.block ∧
  result.block.id = destination ∧ result.bank.source = before.source ∧
  ∀ σ values incoming owner frame memory regs logicalInputs block pc,
    before.Models σ values incoming owner frame memory regs →
    ∃ nextRegs,
      EvoIR.Machine.jump .ssa function ⟨function.name, block, pc, regs, owner, logicalInputs⟩
        destination args = .ok ⟨function.name, destination, 0, nextRegs, owner, logicalInputs⟩ ∧
      result.bank.Models σ values incoming owner frame memory nextRegs

def privateNativeJump {Γ : Var → Nat} (function : EvoIR.Machine.Function)
    (before : PrivateBank Γ) (destination : Nat) (args : List EvoIR.Machine.VReg) :
    Option {result : PrivateNativeJumpResult Γ //
      PrivateNativeJumpDiagram function before destination args result} := do
  let block : {block // function.blocks.find? (·.id == destination) = some block} ←
    match found : function.blocks.find? (·.id == destination) with
    | none => none
    | some block => some ⟨block, found⟩
  let inputs ← privateNativeInputs before args block.val.args
  if unique : (block.val.args.map EvoIR.Machine.VReg.id).eraseDups.length = block.val.args.length then
    let bank : PrivateBank Γ := ⟨before.source, privateBindingWords inputs.val, privateBindingPointers inputs.val⟩
    return ⟨⟨block.val, bank⟩, block.property,
      by simpa using List.find?_some block.property, rfl, by
      intro σ values incoming owner frame memory regs logicalInputs sourceBlock pc models
      obtain ⟨reads, pointers⟩ := inputs.property.2 σ values incoming owner frame memory regs models
      let nextRegs := EvoIR.Machine.bindValues [] (privateBindingValues σ values inputs.val)
      have named : inputs.val.map (fun input => input.target.id) =
          block.val.args.map EvoIR.Machine.VReg.id := by
        simpa only [List.map_map, Function.comp_def] using
          congrArg (List.map EvoIR.Machine.VReg.id) inputs.property.1
      have distinct : (inputs.val.map (fun input => input.target.id)).Nodup := by
        rw [named]
        exact EvoIR.Machine.Transport.nodup_of_eraseDups_length _
          (by simpa only [List.length_map] using unique)
      obtain ⟨words, links⟩ := private_binding_models σ values inputs.val
        (before.source.scope.denote σ (privateToken values)) distinct pointers
      have arity : block.val.args.length = (inputs.val.map (PrivateInput.value σ values)).length := by
        simpa only [List.length_map] using (congrArg List.length inputs.property.1).symm
      have bound : EvoIR.Machine.bind [] block.val.args
          (inputs.val.map (PrivateInput.value σ values)) = .ok nextRegs := by
        rw [EvoIR.Machine.bind_exact [] block.val.args _ arity unique]
        apply congrArg Except.ok
        apply congrArg (EvoIR.Machine.bindValues [])
        have canonical : (inputs.val.map PrivateInput.target).zip
            (inputs.val.map (PrivateInput.value σ values)) = privateBindingValues σ values inputs.val :=
          List.zip_map'
        exact (congrArg (fun params => params.zip (inputs.val.map (PrivateInput.value σ values)))
          inputs.property.1).symm.trans canonical
      refine ⟨nextRegs, ?_, models.source, words, links, models.memory⟩
      simp only [EvoIR.Machine.jump, block.property, reads, bound, Bind.bind, Except.bind,
        Pure.pure, Except.pure]⟩
  else none

end EvoIR.Core.Symbolic.MachineSelection
