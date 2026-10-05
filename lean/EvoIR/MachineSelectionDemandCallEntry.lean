import EvoIR.MachineSelectionDemandArguments

/-! Actual call entry with a complete positional demand-checked argument
vector. The original and zero-substituted auxiliary entries retain proofs of
their real enter operations for later all-input progress instantiation.
Only the target actual BL enters the substituted vector. -/
namespace EvoIR.Core.Symbolic.MachineSelection

structure TokenDemandCallEntryResult (Γ : Var → Nat) (source : IR.Function)
    (target : EvoIR.Machine.Function) (plan : EvoIR.Machine.Demand.Plan) where
  original : TokenCallEntryResult Γ source target
  substituted : TokenCallEntryResult Γ source target
  entryBlock : EvoIR.Machine.Block
  entryFound : target.blocks.find? (·.id == 0) = some entryBlock
  widths : substituted.arguments.map Sigma.fst = original.arguments.map Sigma.fst
  agreement : ∀ σ, EvoIR.Machine.BindingAgreement (EvoIR.Machine.Demand.needed plan target.name)
    (entryBlock.args.zip (machineInputs σ original.arguments))
    (entryBlock.args.zip (machineInputs σ substituted.arguments))

def TokenDemandCallEntryDiagram {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (scope : Scope Γ) (inst : IR.Instruction)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank Γ)
    (results : List EvoIR.Machine.VReg) (sc : IR.Function) (tc : EvoIR.Machine.Function)
    (plan : EvoIR.Machine.Demand.Plan) (entry : TokenDemandCallEntryResult Γ sc tc plan)
    (checks : List (Guard Γ)) (memoryKey worldKey : Nat) : Prop :=
  ∀ σ (ρ : Nat → IR.Value) memory owner nextFrame incoming sourceStack targetStack,
    guards σ checks = .ok () → ρ memoryKey = .memory memory → ρ worldKey = .world →
    IR.step source (sourceState σ ρ sf sb spc scope owner nextFrame sourceStack) =
      some ⟨.running ⟨sc.name, entry.original.sourceBlock.name, 0,
        entry.original.sourceScope.denote σ (tokenEnvironment memory), nextFrame⟩,
        ⟨⟨sf.name, sb.name, spc+1, scope.denote σ ρ, owner⟩, inst.results⟩ :: sourceStack, nextFrame+1⟩ ∧
    EvoIR.Machine.step .ssa target (machineState σ tf tb tpc bank owner nextFrame incoming targetStack memory) =
      some ⟨.running ⟨tc.name, entry.substituted.targetBlock.id, 0,
        bankDenote σ entry.substituted.targetBank, nextFrame, machineInputs σ entry.substituted.arguments⟩,
        ⟨⟨tf.name, tb.id, tpc+1, bankDenote σ bank, owner, incoming⟩, results⟩ :: targetStack,
        nextFrame+1, memory⟩

def tokenDemandCallEntry {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (scope : Scope Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank Γ)
    (sc : IR.Function) (tc : EvoIR.Machine.Function) (plan : EvoIR.Machine.Demand.Plan)
    (checks : List (Guard Γ)) (memoryKey worldKey : Nat) :
    Option {pair : IR.Instruction × List EvoIR.Machine.VReg × TokenDemandCallEntryResult Γ sc tc plan //
      TokenDemandCallEntryDiagram source target sf sb spc scope pair.1 tf tb tpc bank pair.2.1 sc tc
        plan pair.2.2 checks memoryKey worldKey} := do
  if sourceFunction : source.functions.find? (·.name == sf.name) = some sf then
    if sourceBlock : sf.blocks.find? (·.name == sb.name) = some sb then
      if targetFunction : target.functions.find? (·.name == tf.name) = some tf then
        if targetBlock : tf.blocks.find? (·.id == tb.id) = some tb then
          if sourceCallee : source.functions.find? (·.name == sc.name) = some sc then
            if targetCallee : target.functions.find? (·.name == tc.name) = some tc then
              if sameName : sc.name = tc.name then
                let entry : {b // tc.blocks.find? (·.id == 0) = some b} ←
                  match found : tc.blocks.find? (·.id == 0) with
                  | none => none
                  | some b => some ⟨b, found⟩
                let inst : {inst // sb.instructions[spc]? = some inst} ←
                  match found : sb.instructions[spc]? with
                  | none => none
                  | some inst => some ⟨inst, found⟩
                if call : inst.val.op = .call sc.name then
                  if resultTypes : inst.val.results.map IR.Binding.type = sc.returns then
                    match instruction : tb.insts[tpc]? with
                    | some (.bl symbol args results) => do
                      if symbolName : symbol = tc.name then
                        let sourceArgs ← readTokenArguments scope inst.val.operands memoryKey worldKey
                        let targetArgs ← readWords bank args
                        let matched ← checkDemandArguments (EvoIR.Machine.Demand.needed plan tc.name)
                          checks entry.val.args sourceArgs.val targetArgs.val
                        let sourceEntry ← sourceTokenEntry sc sourceArgs.val
                        let originalTarget ← machineEntry tc sourceArgs.val
                        let auxiliarySource ← sourceTokenEntry sc matched.substituted
                        let targetEntry ← machineEntry tc matched.substituted
                        let result : TokenDemandCallEntryResult Γ sc tc plan := {
                          original := ⟨sourceArgs.val, sourceEntry.val.1, sourceEntry.val.2,
                            originalTarget.val.1, originalTarget.val.2, sourceEntry.property, originalTarget.property⟩
                          substituted := ⟨matched.substituted, auxiliarySource.val.1, auxiliarySource.val.2,
                            targetEntry.val.1, targetEntry.val.2, auxiliarySource.property, targetEntry.property⟩
                          entryBlock := entry.val, entryFound := entry.property
                          widths := matched.widths, agreement := matched.agreement }
                        return ⟨(inst.val, results, result), by
                          intro σ ρ memory owner nextFrame incoming sourceStack targetStack passed current world
                          constructor
                          · have step := IR.call_enters source
                              (sourceState σ ρ sf sb spc scope owner nextFrame sourceStack)
                              ⟨sf.name, sb.name, spc, scope.denote σ ρ, owner⟩
                              ⟨sc.name, sourceEntry.val.1.name, 0,
                                sourceEntry.val.2.denote σ (tokenEnvironment memory), nextFrame⟩
                              sf sc sb inst.val (tokenInputValues σ sourceArgs.val memory)
                              rfl sourceFunction sourceBlock inst.property call sourceCallee
                              (sourceArgs.property σ ρ memory current world) resultTypes (sourceEntry.property σ memory nextFrame)
                            cases step with
                            | execute equation => exact equation
                          · have readArgs := targetArgs.property σ
                            rw [matched.actual σ passed] at readArgs
                            exact EvoIR.Machine.step_bl .ssa target
                              (machineState σ tf tb tpc bank owner nextFrame incoming targetStack memory)
                              ⟨tf.name, tb.id, tpc, bankDenote σ bank, owner, incoming⟩ tf tb symbol args results
                              (machineInputs σ matched.substituted) tc
                              ⟨tc.name, targetEntry.val.1.id, 0, bankDenote σ targetEntry.val.2,
                                nextFrame, machineInputs σ matched.substituted⟩ rfl
                              targetFunction targetBlock instruction readArgs
                              (by simpa only [symbolName] using targetCallee) (targetEntry.property σ nextFrame)⟩
                      else none
                    | _ => none
                  else none
                else none
              else none
            else none
          else none
        else none
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
