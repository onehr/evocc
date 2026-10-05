import EvoIR.MachinePrivateBody
import EvoIR.MachineSelectionDemandCallEntry

/-!
Demand-aware calls from real private states. Every scalar argument position is
retained. The source enters its unchanged full vector and current source
memory; the actual BL enters its checked zero-substituted vector and retains
the independently modeled target memory. Saved pointer-containing callers are
literal snapshots, not scalar reconstructions.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def privateNativeWords {Γ : Var → Nat} (bank : Bank Γ) :
    (args : List EvoIR.Machine.VReg) →
    Option {words : List (WordExpr Γ) // ∀ σ regs, bank.Models σ regs →
      args.mapM (EvoIR.Machine.read regs) =
        .ok (words.map fun word => EvoIR.Machine.Value.int (word.denote σ))}
  | [] => some ⟨[], by intro σ regs models; rfl⟩
  | reg :: rest => do
    let first ← wordOperand bank reg
    if present : (bank.lookup reg.id).isSome = true then
      let tail ← privateNativeWords bank rest
      return ⟨first.val :: tail.val, by
        intro σ regs models
        have agrees := bank_models_operand σ bank regs models reg present
        have lifted : EvoIR.Machine.operandValue (EvoIR.Machine.readOperand regs) reg =
            EvoIR.Machine.operandValue (EvoIR.Machine.readOperand (bankDenote σ bank)) reg := by
          simp only [EvoIR.Machine.operandValue, agrees]
        have read := lifted.trans (word_operand_value bank reg first.val first.property σ)
        rw [EvoIR.Machine.operandValue_readOperand] at read
        simp only [List.mapM_cons, read, tail.property σ regs models, List.map_cons,
          Bind.bind, Except.bind, Pure.pure, Except.pure]⟩
    else none

def PrivateDemandCallEntryDiagram {Γ : Var → Nat}
    (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (before : PrivateBank Γ)
    (inst : IR.Instruction) (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (results : List EvoIR.Machine.VReg) (sc : IR.Function) (tc : EvoIR.Machine.Function)
    (plan : EvoIR.Machine.Demand.Plan) (entry : TokenDemandCallEntryResult Γ sc tc plan)
    (checks : List (Guard Γ)) : Prop :=
  ∀ σ values incoming owner frame targetMemory regs logicalInputs sourceStack targetStack sourceMemory,
    before.Models σ values incoming owner frame targetMemory regs → guards σ checks = .ok () →
    before.source.memoryValue values = some sourceMemory →
    IR.step source (privateState σ values sf sb.name spc owner frame before.source sourceStack) =
      some ⟨.running ⟨sc.name, entry.original.sourceBlock.name, 0,
        entry.original.sourceScope.denote σ (tokenEnvironment sourceMemory), frame⟩,
        ⟨⟨sf.name, sb.name, spc+1, before.source.scope.denote σ (privateToken values), owner⟩,
          inst.results⟩ :: sourceStack, frame+1⟩ ∧
    EvoIR.Machine.step .ssa target
      (privateMachineState tf tb tpc regs owner frame logicalInputs targetStack targetMemory) =
      some ⟨.running ⟨tc.name, entry.substituted.targetBlock.id, 0,
        bankDenote σ entry.substituted.targetBank, frame, machineInputs σ entry.substituted.arguments⟩,
        ⟨⟨tf.name, tb.id, tpc+1, regs, owner, logicalInputs⟩, results⟩ :: targetStack,
        frame+1, targetMemory⟩

def privateDemandCallEntry {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (before : PrivateBank Γ)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (sc : IR.Function) (tc : EvoIR.Machine.Function) (plan : EvoIR.Machine.Demand.Plan)
    (checks : List (Guard Γ)) :
    Option {pair : IR.Instruction × List EvoIR.Machine.VReg × TokenDemandCallEntryResult Γ sc tc plan //
      PrivateDemandCallEntryDiagram source target sf sb spc before pair.1 tf tb tpc pair.2.1 sc tc
        plan pair.2.2 checks} := do
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
                        let sourceArgs ← readTokenArguments before.source.scope inst.val.operands
                          before.source.memory before.source.world
                        let targetArgs ← privateNativeWords before.words args
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
                          entryBlock := entry.val, entryFound := entry.property,
                          widths := matched.widths, agreement := matched.agreement }
                        return ⟨(inst.val, results, result), by
                          intro σ values incoming owner frame targetMemory regs logicalInputs
                            sourceStack targetStack sourceMemory models passed current
                          constructor
                          · have step := IR.call_enters source
                              (privateState σ values sf sb.name spc owner frame before.source sourceStack)
                              ⟨sf.name, sb.name, spc, before.source.scope.denote σ (privateToken values), owner⟩
                              ⟨sc.name, sourceEntry.val.1.name, 0,
                                sourceEntry.val.2.denote σ (tokenEnvironment sourceMemory), frame⟩
                              sf sc sb inst.val (tokenInputValues σ sourceArgs.val sourceMemory)
                              rfl sourceFunction sourceBlock inst.property call sourceCallee
                              (sourceArgs.property σ (privateToken values) sourceMemory
                                (before.source.current_token current) models.source.2.2.1)
                              resultTypes (sourceEntry.property σ sourceMemory frame)
                            cases step with
                            | execute equation => exact equation
                          · have readArgs := targetArgs.property σ regs models.words
                            rw [matched.actual σ passed] at readArgs
                            exact EvoIR.Machine.step_bl .ssa target
                              (privateMachineState tf tb tpc regs owner frame logicalInputs targetStack targetMemory)
                              ⟨tf.name, tb.id, tpc, regs, owner, logicalInputs⟩ tf tb symbol args results
                              (machineInputs σ matched.substituted) tc
                              ⟨tc.name, targetEntry.val.1.id, 0, bankDenote σ targetEntry.val.2,
                                frame, machineInputs σ matched.substituted⟩ rfl
                              targetFunction targetBlock instruction readArgs
                              (by simpa only [symbolName] using targetCallee) (targetEntry.property σ frame)⟩
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
