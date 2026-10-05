import EvoIR.Core.PrivateTarget
import EvoIR.Core.PrivateBoundary

/-! Proof-producing private CFG inspection. Frontier mode suspends before
real calls and returns; complete mode executes the actual return. -/
namespace EvoIR.Core.Symbolic

set_option maxHeartbeats 1000000 in
def privateCutDiagram {Γ : Var → Nat} (p : IR.Program) (f : IR.Function)
    (tokens : Bool) (type : Source.IntType) (cutpoints : List String) (frontier : Bool)
    (budget : Nat) (advance : Bool) (config : PrivateConfig Γ) (label : String) (pc : Nat := 0) :
    Option {tree // PrivateCutRuns p f tokens advance label pc config tree} :=
  if stopped : advance = false ∧ label ∈ cutpoints ∧ pc = 0 then
    some ⟨.leaf (.next label pc config), stopped.1 ▸ private_cut_boundary p f tokens label pc config⟩
  else match budget with
  | 0 => none
  | budget+1 => do
    if function : p.functions.find? (·.name == f.name) = some f then
      let block : {b // f.blocks.find? (·.name == label) = some b} ←
        match h : f.blocks.find? (·.name == label) with
        | none => none
        | some b => some ⟨b, h⟩
      have name : block.val.name = label := by simpa using List.find?_some block.property
      have found : f.blocks.find? (·.name == block.val.name) = some block.val := by
        simpa only [name] using block.property
      match atPc : block.val.instructions[pc]? with
      | some inst =>
        if noncall : privateIsCall inst.op = false then
          let checked ← privateInstruction config inst
          let tail ← privateCutDiagram p f tokens type cutpoints frontier budget false checked.next label (pc+1)
          return ⟨tail.val.prefix checked.checks, by
            simpa only [name] using private_cut_instruction p f tokens advance block.val pc config
              inst checked tail.val function found atPc (private_not_call inst.op noncall)
              (by simpa only [name] using tail.property)⟩
        else if suspended : frontier = true ∧ advance = false then
          return ⟨.leaf (.next label pc config), suspended.2 ▸ private_cut_boundary p f tokens label pc config⟩
        else none
      | none =>
        if atEnd : pc = block.val.instructions.length then
          match terminator : block.val.terminator with
          | .return ids =>
            if suspended : frontier = true ∧ advance = false then
              return ⟨.leaf (.next label pc config), suspended.2 ▸ private_cut_boundary p f tokens label pc config⟩
            else
              let id :: _ := ids | none
              match lookup : config.scope.word id with
              | none => none
              | some ⟨width, term⟩ =>
                if sameWidth : width = type.width then
                  let word : CWord Γ := ⟨type, sameWidth ▸ term⟩
                  if operands : ids.mapM (fun id => config.scope.lookup id) = some (privateResults tokens config word) then
                    if returns : f.returns = cutReturnTypes tokens type then
                      return ⟨.leaf (.returned word config), by
                        simpa only [name, atEnd] using private_cut_return p f tokens advance block.val
                          config ids word function found terminator operands returns⟩
                    else none
                  else none
                else none
          | .branch successor =>
            match binding : edge (privateTokenType config.tokens) f config.scope successor with
            | none => none
            | some scope =>
              if shape : ({config with scope} : PrivateConfig Γ).WellShaped = true then
                let tail ← privateCutDiagram p f tokens type cutpoints frontier budget false
                  {config with scope} successor.block
                return ⟨tail.val, by
                  simpa only [name, atEnd] using private_cut_branch p f tokens advance block.val config
                    scope successor tail.val function found terminator binding shape tail.property⟩
              else none
          | .conditional id yes no =>
            match lookup : config.scope.word id with
            | some ⟨1, condition⟩ =>
              match firstEdge : edge (privateTokenType config.tokens) f config.scope yes,
                  secondEdge : edge (privateTokenType config.tokens) f config.scope no with
              | some yesScope, some noScope =>
                if yesShape : ({config with scope := yesScope} : PrivateConfig Γ).WellShaped = true then
                  if noShape : ({config with scope := noScope} : PrivateConfig Γ).WellShaped = true then
                    let first ← privateCutDiagram p f tokens type cutpoints frontier budget false
                      {config with scope := yesScope} yes.block
                    let second ← privateCutDiagram p f tokens type cutpoints frontier budget false
                      {config with scope := noScope} no.block
                    return ⟨.branch condition first.val second.val, by
                      simpa only [name, atEnd] using private_cut_choice p f tokens advance block.val
                        config yesScope noScope id condition yes no first.val second.val function found
                        terminator lookup firstEdge secondEdge yesShape noShape first.property second.property⟩
                  else none
                else none
              | _, _ => none
            | _ => none
        else none
    else none

end EvoIR.Core.Symbolic
