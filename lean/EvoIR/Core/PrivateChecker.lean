import EvoIR.Core.PrivateRead

/-! Executable, proof-producing private instruction inspection. Unsupported
effects, stale tokens, unknown pointers, invalid lifetimes and uninitialized
loads refuse. Every admitted transfer is tied to the real instruction. -/
namespace EvoIR.Core.Symbolic

def privateCellOperand {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) (position : Nat) : Option {cell // cell ∈ before.cells} := do
  let id ← inst.operands[position]?
  let .opaque pointer ← before.scope.lookup id | none
  match found : before.cells.find? (·.pointer == pointer) with
  | none => none
  | some cell => return ⟨cell, List.mem_of_find?_eq_some found⟩

set_option maxHeartbeats 1000000 in
def privateInstruction {Γ : Var → Nat} (before : PrivateConfig Γ)
    (inst : IR.Instruction) : Option (CheckedPrivateInstruction before inst) := do
  match pureCode : instruction before.scope inst with
  | some (checks, nextScope) =>
    if shape : ({before with scope := nextScope} : PrivateConfig Γ).WellShaped = true then
      return ⟨checks, {before with scope := nextScope},
        private_scalar_instruction before inst checks nextScope pureCode shape⟩
    else none
  | none =>
    match op : inst.op with
    | .allocate origin size align =>
      if admitted : size = 4 ∨ size = 8 then
        if aligned : size ≤ align then
          if operands : inst.operands.mapM (fun id => before.scope.lookup id) =
              some [.opaque before.memory, .opaque before.world] then
            match binding : before.scope.appendPrivate (before.tokens ++ [.pointer, .memory, .world]) inst.results
                [.opaque before.tokens.length, .opaque (before.tokens.length+1), .opaque (before.tokens.length+2)] with
            | none => none
            | some nextScope =>
              if shape : (privateAllocationConfig before nextScope (8*size)).WellShaped = true then
                return ⟨[], privateAllocationConfig before nextScope (8*size),
                  private_allocate_instruction before inst nextScope origin size align op admitted aligned
                    operands binding shape⟩
              else none
          else none
        else none
      else none
    | .store =>
      let cell ← privateCellOperand before inst 2
      if active : cell.val.live = true then
        let id ← inst.operands[3]?
        let word ← before.scope.word id
        let value ← word.at cell.val.width
        if operands : inst.operands.mapM (fun id => before.scope.lookup id) =
            some [.opaque before.memory, .opaque before.world, .opaque cell.val.pointer,
              .word ⟨cell.val.width, value⟩] then
          match binding : before.scope.appendPrivate (before.tokens ++ [.memory, .world]) inst.results
              [.opaque before.tokens.length, .opaque (before.tokens.length+1)] with
          | none => none
          | some nextScope =>
            if shape : (privateMutationConfig before nextScope cell.val {cell.val with value := some value}).WellShaped = true then
              return ⟨[], privateMutationConfig before nextScope cell.val {cell.val with value := some value},
                private_store_instruction before inst nextScope cell.val value op cell.property active
                  operands binding shape⟩
            else none
        else none
      else none
    | .endLifetime =>
      let cell ← privateCellOperand before inst 2
      if active : cell.val.live = true then
        if operands : inst.operands.mapM (fun id => before.scope.lookup id) =
            some [.opaque before.memory, .opaque before.world, .opaque cell.val.pointer] then
          match binding : before.scope.appendPrivate (before.tokens ++ [.memory, .world]) inst.results
              [.opaque before.tokens.length, .opaque (before.tokens.length+1)] with
          | none => none
          | some nextScope =>
            if shape : (privateMutationConfig before nextScope cell.val {cell.val with live := false}).WellShaped = true then
              return ⟨[], privateMutationConfig before nextScope cell.val {cell.val with live := false},
                private_lifetime_instruction before inst nextScope cell.val op cell.property active
                  operands binding shape⟩
            else none
        else none
      else none
    | .load width =>
      let cell ← privateCellOperand before inst 2
      if sameWidth : width = cell.val.width then
        if active : cell.val.live = true then
          match initialized : cell.val.value with
          | none => none
          | some value =>
            if operands : inst.operands.mapM (fun id => before.scope.lookup id) =
                some [.opaque before.memory, .opaque before.world, .opaque cell.val.pointer] then
              match binding : before.scope.appendPrivate
                  (before.tokens ++ [.bits cell.val.width, .memory, .world]) inst.results
                  [.word ⟨cell.val.width, value⟩, .opaque (before.tokens.length+1), .opaque (before.tokens.length+2)] with
              | none => none
              | some nextScope =>
                if shape : (privateLoadConfig before nextScope cell.val.width).WellShaped = true then
                  return ⟨[], privateLoadConfig before nextScope cell.val.width,
                    private_load_instruction before inst nextScope cell.val value
                      (by simpa only [sameWidth] using op) cell.property active initialized
                      operands binding shape⟩
                else none
            else none
        else none
      else none
    | .access size align =>
      let cell ← privateCellOperand before inst 1
      if active : cell.val.live = true then
        if sizePositive : size ≠ 0 then
          if alignPositive : align ≠ 0 then
            if sizeBound : size ≤ cell.val.width/8 then
              if alignBound : align ≤ cell.val.width/8 then
                if operands : inst.operands.mapM (fun id => before.scope.lookup id) =
                    some [.opaque before.memory, .opaque cell.val.pointer] then
                  match binding : before.scope.appendPrivate (before.tokens ++ [.memory]) inst.results
                      [.opaque before.tokens.length] with
                  | none => none
                  | some nextScope =>
                    if shape : (privateAccessConfig before nextScope).WellShaped = true then
                      return ⟨[], privateAccessConfig before nextScope,
                        private_access_instruction before inst nextScope cell.val size align op cell.property
                          active sizePositive alignPositive sizeBound alignBound operands binding shape⟩
                    else none
                else none
              else none
            else none
          else none
        else none
      else none
    | .uninitialized (.bits width) =>
      if operands : inst.operands = [] then
        match binding : before.scope.appendPrivate (before.tokens ++ [.bits width]) inst.results
            [.opaque before.tokens.length] with
        | none => none
        | some nextScope =>
          if shape : (privatePreservedConfig before nextScope [.bits width] before.memory before.world).WellShaped = true then
            return ⟨[], privatePreservedConfig before nextScope [.bits width] before.memory before.world,
              private_poison_instruction before inst nextScope width op operands binding shape⟩
          else none
      else none
    | _ => none

end EvoIR.Core.Symbolic
