import EvoIR.MachineSelectionCallerSpan

/-! The actual call leaf composes a checked callee's open return with the
separately checked actual caller suffix. A returned word is universally bound
as a fresh input; no execution bound or whole-caller proof is assumed. -/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem token_return_suffix (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type returnType : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (spc : Nat) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank inputWidths) (types : List IR.Ty)
    (checks : List (Guard inputWidths)) (sourceResults : List IR.Binding) (results : List EvoIR.Machine.VReg)
    (binding : ResumeBindings types scope bank sourceResults results returnType)
    (suffix : TokenSuffixDiagram source target sf tf type table sb spc binding.scope
      tb tpc binding.bank (types ++ [.memory, .world]) types.length (types.length+1) (guardsForReturn checks))
    (σ : Subst inputWidths) (values : List IR.Value) (typed : values.map IR.Value.type = types)
    (owner calleeOwner : Nat) (incoming : List EvoIR.Machine.Value)
    (sourceStack : List IR.ReturnFrame) (targetStack : List EvoIR.Machine.ReturnFrame)
    (observationType : Source.IntType) (passed : guards σ checks = .ok ())
    (sourceState : IR.State) (targetState : EvoIR.Machine.State)
    (boundary : TokenReturnBoundary returnType calleeOwner
      (⟨⟨sf.name, sb.name, spc, scope.denote σ (privateToken values), owner⟩, sourceResults⟩ :: sourceStack)
      (⟨⟨tf.name, tb.id, tpc, bankDenote σ bank, owner, incoming⟩, results⟩ :: targetStack)
      sourceState targetState) :
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (TokenCallerBoundary type table owner sourceStack targetStack) sourceState targetState := by
  obtain ⟨calleeSubst, word, afterMemory, afterFrame, width, rfl, rfl⟩ := boundary
  let returned := returnWordValue calleeSubst returnType word width
  have resumed := call_resume_exact types scope bank sourceResults results returnType binding
    sf sb spc tf tb tpc σ values typed owner calleeOwner afterFrame incoming sourceStack targetStack
    calleeSubst word width afterMemory
  dsimp only at resumed
  rw [resumed.1, resumed.2]
  have length : values.length = types.length := by
    have same := congrArg List.length typed
    simpa only [List.length_map] using same
  have appendedTypes : (values ++ [IR.Value.memory (afterMemory.endFrame calleeOwner), IR.Value.world]).map IR.Value.type =
      types ++ [.memory, .world] := by
    simp only [List.map_append, typed, List.map_cons, List.map_nil, IR.Value.type]
  have newMemory : privateToken (values ++ [.memory (afterMemory.endFrame calleeOwner), .world]) types.length =
      .memory (afterMemory.endFrame calleeOwner) := by
    rw [← length]
    simpa only [Nat.add_zero, privateToken, List.getElem?_cons_zero, Option.getD_some] using
      private_token_new values [.memory (afterMemory.endFrame calleeOwner), .world] 0
  have newWorld : privateToken (values ++ [.memory (afterMemory.endFrame calleeOwner), .world]) (types.length+1) = .world := by
    rw [← length]
    simpa only [privateToken, List.getElem?_cons_succ, List.getElem?_cons_zero, Option.getD_some] using
      private_token_new values [.memory (afterMemory.endFrame calleeOwner), .world] 1
  have freshPassed : guards (returnSubst σ returned) (guardsForReturn checks) = .ok () := by
    rw [guards_for_return]
    exact passed
  exact suffix (returnSubst σ returned) (values ++ [.memory (afterMemory.endFrame calleeOwner), .world]) appendedTypes
    (afterMemory.endFrame calleeOwner) owner afterFrame incoming sourceStack targetStack observationType
    freshPassed newMemory newWorld

theorem token_suffix_call (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (spc : Nat) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank inputWidths) (types : List IR.Ty)
    (memoryKey worldKey : Nat) (checks : List (Guard inputWidths))
    (callee : TokenCallee source target) (inst : IR.Instruction) (results : List EvoIR.Machine.VReg)
    (entry : TokenCallEntryResult inputWidths callee.checked.sourceFunction callee.checked.targetFunction)
    (called : TokenCallEntryDiagram source target sf sb spc scope inst tf tb tpc bank results
      callee.checked.sourceFunction callee.checked.targetFunction entry checks memoryKey worldKey)
    (calleeBody : TokenCalleeArgumentsDiagram callee.checked entry.arguments)
    (binding : ResumeBindings types scope bank inst.results results callee.type)
    (suffix : TokenSuffixDiagram source target sf tf type table sb (spc+1) binding.scope
      tb (tpc+1) binding.bank (types ++ [.memory, .world]) types.length (types.length+1)
      (guardsForReturn checks)) :
    TokenSuffixDiagram source target sf tf type table sb spc scope tb tpc bank types memoryKey worldKey checks := by
  intro σ values typed memory owner nextFrame incoming sourceStack targetStack observationType passed current world
  let savedSource : List IR.ReturnFrame :=
    ⟨⟨sf.name, sb.name, spc+1, scope.denote σ (privateToken values), owner⟩, inst.results⟩ :: sourceStack
  let savedTarget : List EvoIR.Machine.ReturnFrame :=
    ⟨⟨tf.name, tb.id, tpc+1, bankDenote σ bank, owner, incoming⟩, results⟩ :: targetStack
  have steps := called σ (privateToken values) memory owner nextFrame incoming sourceStack targetStack passed current world
  have opened := calleeBody σ memory nextFrame (nextFrame+1) savedSource savedTarget observationType
    ⟨callee.checked.sourceFunction.name, entry.sourceBlock.name, 0,
      entry.sourceScope.denote σ (tokenEnvironment memory), nextFrame⟩
    ⟨callee.checked.targetFunction.name, entry.targetBlock.id, 0, bankDenote σ entry.targetBank,
      nextFrame, machineInputs σ entry.arguments⟩
    (entry.sourceEntry σ memory nextFrame) (entry.targetEntry σ nextFrame)
  have continued := opened.bind (nextBoundary := TokenCallerBoundary type table owner sourceStack targetStack) (by
    rintro _ _ ⟨calleeSubst, word, afterMemory, afterFrame, width, rfl, rfl⟩
    right
    let returned := returnWordValue calleeSubst callee.type word width
    have resumed := call_resume_exact types scope bank inst.results results callee.type binding
      sf sb (spc+1) tf tb (tpc+1) σ values typed owner nextFrame afterFrame incoming sourceStack targetStack
      calleeSubst word width afterMemory
    dsimp only at resumed
    rw [resumed.1, resumed.2]
    have length : values.length = types.length := by
      have same := congrArg List.length typed
      simpa only [List.length_map] using same
    have appendedTypes : (values ++ [IR.Value.memory (afterMemory.endFrame nextFrame), IR.Value.world]).map IR.Value.type =
        types ++ [.memory, .world] := by
      simp only [List.map_append, typed, List.map_cons, List.map_nil, IR.Value.type]
    have newMemory : privateToken (values ++ [.memory (afterMemory.endFrame nextFrame), .world]) types.length =
        .memory (afterMemory.endFrame nextFrame) := by
      rw [← length]
      simpa only [Nat.add_zero, privateToken, List.getElem?_cons_zero, Option.getD_some] using
        private_token_new values [.memory (afterMemory.endFrame nextFrame), .world] 0
    have newWorld : privateToken (values ++ [.memory (afterMemory.endFrame nextFrame), .world]) (types.length+1) =
        .world := by
      rw [← length]
      simpa only [privateToken, List.getElem?_cons_succ, List.getElem?_cons_zero, Option.getD_some] using
        private_token_new values [.memory (afterMemory.endFrame nextFrame), .world] 1
    have freshPassed : guards (returnSubst σ returned) (guardsForReturn checks) = .ok () := by
      rw [guards_for_return]
      exact passed
    exact suffix (returnSubst σ returned) (values ++ [.memory (afterMemory.endFrame nextFrame), .world]) appendedTypes
      (afterMemory.endFrame nextFrame) owner afterFrame incoming sourceStack targetStack observationType
      freshPassed newMemory newWorld)
  exact continued.prepend (.next steps.1 (.refl _)) (.next steps.2 (.refl _))

end EvoIR.Core.Symbolic.MachineSelection
