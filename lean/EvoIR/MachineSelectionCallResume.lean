import EvoIR.MachineSelectionResume
import EvoIR.MachineSelectionTokenReturnOpen

/-! Exact actual return-boundary reconstruction for a suspended caller.
The unknown returned value comes from the proved callee boundary; neither its
arithmetic nor termination is replaced by a sample result. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def returnWordValue (σ : Subst inputWidths) (type : Source.IntType) (word : Word inputWidths)
    (typed : word.1 = type.width) : Source.Value :=
  ⟨type, typed ▸ denoteTerm σ word.2⟩

theorem word_return_value_ir (σ : Subst inputWidths) (type : Source.IntType) (word : Word inputWidths)
    (typed : word.1 = type.width) : sourceValueIR (returnWordValue σ type word typed) = word.denote σ := by
  cases word with
  | mk width term => cases typed; rfl

theorem word_return_value_machine (σ : Subst inputWidths) (type : Source.IntType) (word : Word inputWidths)
    (typed : word.1 = type.width) :
    machineInputs σ [word] = [.int ((returnWordValue σ type word typed).bits.setWidth 64)] := by
  cases word with
  | mk width term => cases typed; rfl

def guardsForReturn (checks : List (Guard inputWidths)) : List (Guard inputWidths) :=
  checks.map fun g => ⟨g.reason, g.condition.renameInputs returnName⟩

theorem guards_for_return (σ : Subst inputWidths) (value : Source.Value)
    (checks : List (Guard inputWidths)) :
    guards (returnSubst σ value) (guardsForReturn checks) = guards σ checks := by
  induction checks with
  | nil => rfl
  | cons g rest ih =>
    simp only [guardsForReturn, List.map_cons, guards, Term.rename_inputs_sound]
    have old : (fun v => returnSubst σ value (returnName v)) = σ := by
      funext v
      exact return_subst_old σ value v
    simp only [old]
    exact congrArg (fun tail => if (denoteTerm σ g.condition).toNat == 0 then
      .error (.diagnostic g.reason.code) else tail) ih

theorem call_resume_exact (types : List IR.Ty) (scope : Scope inputWidths) (bank : Bank inputWidths)
    (sourceResults : List IR.Binding) (targetResults : List EvoIR.Machine.VReg) (type : Source.IntType)
    (binding : ResumeBindings types scope bank sourceResults targetResults type)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (tf : EvoIR.Machine.Function)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (σ : Subst inputWidths)
    (values : List IR.Value) (valueTypes : values.map IR.Value.type = types)
    (owner calleeOwner nextFrame : Nat) (incoming : List EvoIR.Machine.Value)
    (sourceStack : List IR.ReturnFrame) (targetStack : List EvoIR.Machine.ReturnFrame)
    (calleeSubst : Subst inputWidths) (word : Word inputWidths) (typed : word.1 = type.width)
    (memory : IR.Memory) :
    let returned := returnWordValue calleeSubst type word typed
    let afterMemory := memory.endFrame calleeOwner
    tokenSourceReturnState calleeSubst word memory calleeOwner nextFrame
      (⟨⟨sf.name, sb.name, spc, scope.denote σ (privateToken values), owner⟩, sourceResults⟩ :: sourceStack) =
      sourceState (returnSubst σ returned) (privateToken (values ++ [.memory afterMemory, .world]))
        sf sb spc binding.scope owner nextFrame sourceStack ∧
    machineReturnState (machineInputs calleeSubst [word]) memory calleeOwner nextFrame
      (⟨⟨tf.name, tb.id, tpc, bankDenote σ bank, owner, incoming⟩, targetResults⟩ :: targetStack) =
      machineState (returnSubst σ returned) tf tb tpc binding.bank owner nextFrame incoming targetStack afterMemory := by
  dsimp only
  let returned := returnWordValue calleeSubst type word typed
  have bound := binding.sound σ returned rfl values valueTypes (memory.endFrame calleeOwner)
  dsimp only [returned] at bound
  constructor
  · simp only [tokenSourceReturnState, cutResume, ← word_return_value_ir calleeSubst type word typed,
      bound.1, bind, Except.bind, pure, Except.pure, sourceState]
  · simp only [word_return_value_machine calleeSubst type word typed, machineReturnState, bound.2,
      machineState]

end EvoIR.Core.Symbolic.MachineSelection
