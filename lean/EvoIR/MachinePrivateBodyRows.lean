import EvoIR.MachinePrivateBody

/-! Effect rows for the actual paired private body. The existing IR private
checker supplies exact token-history/cell updates; independent machine effect
theorems supply actual mutable-register and memory updates. Result names and
operand shapes are checked on both concrete inputs. -/
namespace EvoIR.Core.Symbolic.MachineSelection

private theorem model_memory {Γ : Var → Nat} (before : PrivateBank Γ) (σ : Subst Γ)
    (values : List IR.Value) (incoming source target : IR.Memory) (owner frame : Nat)
    (regs : EvoIR.Machine.RegisterFile)
    (models : before.Models σ values incoming owner frame target regs)
    (current : before.source.memoryValue values = some source) :
    EvoIR.Machine.PrivateMemory.LiveRelated source target := by
  obtain ⟨actual, actualCurrent, related⟩ := models.memory
  have same := Option.some.inj (actualCurrent.symm.trans current)
  exact same ▸ related

private theorem source_appended {Γ : Var → Nat} (before : PrivateBank Γ)
    (after : PrivateConfig Γ) (inst : IR.Instruction) (σ : Subst Γ)
    (values nextValues outputs : List IR.Value) (owner : Nat) (slots : List (Slot Γ))
    (operands : inst.operands.mapM (fun id => before.source.scope.lookup id) = some slots)
    (operation : IR.evalOperation owner inst.op (slots.map (Slot.denote σ (privateToken values))) = .ok outputs)
    (executed : concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst =
      .ok (after.scope.denote σ (privateToken nextValues))) :
    IR.appendResults (before.source.scope.denote σ (privateToken values)) inst.results outputs =
      .ok (after.scope.denote σ (privateToken nextValues)) := by
  simpa only [concreteInstruction, slots_lookup_sound σ (privateToken values)
    before.source.scope inst.operands slots operands, operation, Bind.bind, Except.bind] using executed

private theorem actual_alive (memory : IR.Memory) (object : IR.Object)
    (safe : memory.WellAllocated) (found : memory.objects.find? (·.id == object.id) = some object)
    (active : object.live = true) : IR.liveObject memory object.id = .ok object := by
  exact EvoIR.Machine.PrivateMemory.liveObject_of_info memory object.id object
    (Nat.ne_of_gt (safe.2 object (List.mem_of_find?_eq_some found)).1) found active

private def nativePointer {Γ : Var → Nat} (before : PrivateBank Γ) (source : Nat)
    (target : EvoIR.Machine.VReg) : Option (PLift (∀ bindings regs id,
      PrivatePointer.Models before.pointers bindings regs →
      IR.lookup bindings source = .ok (.pointer id 0) →
      EvoIR.Machine.readOperand regs target = .ok (.value (.pointer id 0)))) := do
  let pointer : {entry // before.pointers.find? (fun entry =>
      decide (entry.source = source ∧ entry.target = target)) = some entry} ←
    match found : before.pointers.find? (fun entry => decide (entry.source = source ∧ entry.target = target)) with
    | none => none
    | some entry => some ⟨entry, found⟩
  return ⟨by
    intro bindings regs id models sourceRead
    have names : pointer.val.source = source ∧ pointer.val.target = target := by
      simpa only [decide_eq_true_eq] using List.find?_some pointer.property
    obtain ⟨actualId, sourcePointer, targetPointer⟩ := models pointer.val (List.mem_of_find?_eq_some pointer.property)
    rw [names.1, sourceRead] at sourcePointer
    have same : actualId = id := (IR.Value.pointer.inj (Except.ok.inj sourcePointer.symm)).1
    simpa only [names.2, same] using targetPointer⟩

private def nativeWord {Γ : Var → Nat} (bank : Bank Γ) (reg : EvoIR.Machine.VReg) :
    Option {word : WordExpr Γ // ∀ σ regs, bank.Models σ regs →
      EvoIR.Machine.readOperand regs reg = .ok (.value (.int (word.denote σ)))} :=
  match found : bank.lookup reg.id with
  | some (.word word) => some ⟨word, by
      intro σ regs models
      have actual := models reg.id (.word word) found
      simp only [EvoIR.Machine.readOperand, EvoIR.Machine.readSlot, actual, BankSlot.denote]⟩
  | _ => none

private theorem bank_pointer_write {Γ : Var → Nat} (bank : Bank Γ) (σ : Subst Γ)
    (regs : EvoIR.Machine.RegisterFile) (dst : EvoIR.Machine.VReg) (id : Nat)
    (models : bank.Models σ regs) (fresh : bank.lookup dst.id = none) :
    bank.Models σ (EvoIR.Machine.writeSlot regs dst (.value (.pointer id 0))) := by
  intro reg slot found
  have separate : reg ≠ dst.id := by
    intro same
    rw [same, fresh] at found
    contradiction
  simpa only [EvoIR.Machine.writeSlot, List.lookup_cons,
    beq_eq_false_iff_ne.mpr separate, Bool.false_eq_true, ↓reduceIte] using models reg slot found

def privateAllocate {Γ : Var → Nat} (before : PrivateBank Γ)
    (inst : IR.Instruction) (native : EvoIR.Machine.Instruction) :
    Option (CheckedPrivateRow before (some inst) (some native)) := do
  match op : inst.op, nativeOp : native with
  | .allocate origin size align, .frameSlot dst nativeOrigin nativeSize nativeAlign =>
    if same : nativeOrigin = origin ∧ nativeSize = size ∧ nativeAlign = align ∧ dst.class = .gpr64 then
      if layout : size ≠ 0 ∧ align ≠ 0 then
        let source ← privateInstruction before.source inst
        if noChecks : source.checks = [] then
          if operands : inst.operands.mapM (fun id => before.source.scope.lookup id) =
              some [.opaque before.source.memory, .opaque before.source.world] then
            if freshDst : before.words.lookup dst.id = none then
              if separate : before.pointers.all (fun pointer => decide (pointer.target.id ≠ dst.id)) then
                let outputs ← privateOutputs before source.next inst [.pointer, .memory, .world] 1
                let pointerResult ← inst.results[0]?
                if pointerName : outputs.fresh.lookup pointerResult.id = some (.opaque 0) then
                  let next : PrivateBank Γ := ⟨source.next, before.words,
                    ⟨pointerResult.id, dst⟩ :: before.pointers⟩
                  return ⟨[], next, by
                    intro σ values incoming owner frame targetMemory regs models passed
                    obtain ⟨memory, current, related⟩ := models.memory
                    have transferred := EvoIR.Machine.PrivateMemory.allocate_transfer related owner origin size align
                      dst (EvoIR.Machine.readOperand regs) layout.1 layout.2
                    let added : List IR.Value := [.pointer memory.nextId 0,
                      .memory (IR.allocatedMemory memory owner origin size align), .world]
                    obtain ⟨nextValues, sourceCode, sourceNext⟩ := source.sound σ values incoming owner frame models.source
                    have sourcePassed : guards σ source.checks = .ok () := by simp [noChecks, guards]
                    have realized := sourceNext sourcePassed
                    have executed : concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst =
                        .ok (source.next.scope.denote σ (privateToken nextValues)) := by
                      simpa only [sourcePassed, Bind.bind, Except.bind, Pure.pure, Except.pure] using sourceCode
                    have world := models.source.2.2.1
                    have operation : IR.evalOperation owner inst.op
                        ([.opaque before.source.memory, .opaque before.source.world].map
                          (Slot.denote σ (privateToken values))) = .ok added := by
                      simpa only [op, List.map_cons, List.map_nil, Slot.denote,
                        before.source.current_token current, world, added] using transferred.1
                    have appended := source_appended before source.next inst σ values nextValues added owner _ operands operation executed
                    have nextMemory := PrivateOutputs.memory_eq before source.next inst [.pointer, .memory, .world] 1 outputs
                      σ values added nextValues (IR.allocatedMemory memory owner origin size align) rfl rfl rfl appended
                    let nextRegs := EvoIR.Machine.writeSlot regs dst (.value (.pointer memory.nextId 0))
                    have evaluated : EvoIR.Machine.evalInstruction owner native regs targetMemory =
                        .ok (nextRegs, IR.allocatedMemory targetMemory owner origin size align) := by
                      have pointerNormalized : EvoIR.Machine.normalize dst.class (.pointer memory.nextId 0) =
                          .pointer memory.nextId 0 := by cases dst.class <;> rfl
                      simp only [EvoIR.Machine.evalInstruction, nativeOp, same.1, same.2.1, same.2.2.1,
                        transferred.2.1, Bind.bind, Except.bind, Pure.pure, Except.pure,
                        EvoIR.Machine.valueWrite, pointerNormalized, EvoIR.Machine.applyWrites, List.foldl_cons, List.foldl_nil, nextRegs]
                    have oldSource := PrivateOutputs.pointers before source.next inst [.pointer, .memory, .world] 1 outputs
                      σ values added nextValues regs rfl rfl appended models.pointers
                    have oldPointers : PrivatePointer.Models before.pointers
                        (source.next.scope.denote σ (privateToken nextValues)) nextRegs := by
                      apply pointer_target_preserved before.pointers _ regs nextRegs oldSource
                      intro pointer member
                      apply eval_pointer_preserved owner native regs nextRegs targetMemory _ evaluated pointer.target
                      intro defined definedMember
                      have only : defined = dst := by
                        simpa only [nativeOp, EvoIR.Machine.Instruction.defs, List.mem_singleton] using definedMember
                      subst defined
                      exact of_decide_eq_true (List.all_eq_true.mp separate pointer member)
                    have concrete := PrivateOutputs.concrete before source.next inst [.pointer, .memory, .world] 1 outputs
                      σ values added nextValues rfl rfl appended
                    have newSource : IR.lookup (source.next.scope.denote σ (privateToken nextValues)) pointerResult.id =
                        .ok (.pointer memory.nextId 0) := by
                      have freshPointer : (outputs.fresh.denote σ (privateToken added)).lookup pointerResult.id =
                          some (IR.Value.pointer memory.nextId 0) := by
                        simp only [lookup_denote, pointerName, Option.map_some, Slot.denote]
                        rfl
                      have found := lookup_append_some _ (before.source.scope.denote σ (privateToken values))
                        pointerResult.id (IR.Value.pointer memory.nextId 0) freshPointer
                      simp only [IR.lookup, concrete, found]
                    refine ⟨nextValues, IR.allocatedMemory targetMemory owner origin size align, nextRegs,
                      executed, evaluated, realized, bank_pointer_write before.words σ regs dst memory.nextId
                        models.words freshDst, ?_, _, nextMemory, transferred.2.2⟩
                    intro pointer member
                    rcases List.mem_cons.mp member with rfl | old
                    · refine ⟨memory.nextId, newSource, ?_⟩
                      simp only [nextRegs, EvoIR.Machine.readOperand, EvoIR.Machine.readSlot_writeSlot]
                    · exact oldPointers pointer old⟩
                else none
              else none
            else none
          else none
        else none
      else none
    else none
  | _, _ => none

def privateErasedLifetime {Γ : Var → Nat} (before : PrivateBank Γ) (inst : IR.Instruction) :
    Option (CheckedPrivateRow before (some inst) none) := do
  if op : inst.op = .endLifetime then
    let source ← privateInstruction before.source inst
    if noChecks : source.checks = [] then
      let cell ← privateCellOperand before.source inst 2
      if active : cell.val.live = true then
        if operands : inst.operands.mapM (fun id => before.source.scope.lookup id) =
            some [.opaque before.source.memory, .opaque before.source.world, .opaque cell.val.pointer] then
          let outputs ← privateOutputs before source.next inst [.memory, .world] 0
          return ⟨[], {before with source := source.next}, by
            intro σ values incoming owner frame targetMemory regs models passed
            obtain ⟨shape, typed, world, incomingSafe, memory, current, safe, frameBound,
              ownerBound, owned, distinct, cells⟩ := models.source
            have admitted := (before.source.cell_shape shape cell.val cell.property).2
            obtain ⟨object, access, identity, objectOwner, bytes, live, alignment, found⟩ :=
              private_cell_access σ values incoming memory owner cell.val (cells cell.val cell.property)
                safe admitted active
            have objectFound : memory.objects.find? (·.id == object.id) = some object := by
              simpa only [identity] using found
            have alive := actual_alive memory object safe objectFound (live.trans active)
            have transferred := EvoIR.Machine.PrivateMemory.erased_lifetime_transfer
              (model_memory before σ values incoming memory targetMemory owner frame regs models current)
              owner object alive objectOwner
            let added : List IR.Value := [.memory (memory.replace {object with live := false}), .world]
            obtain ⟨nextValues, sourceCode, sourceNext⟩ := source.sound σ values incoming owner frame models.source
            have sourcePassed : guards σ source.checks = .ok () := by simp [noChecks, guards]
            have realized := sourceNext sourcePassed
            have executed : concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst =
                .ok (source.next.scope.denote σ (privateToken nextValues)) := by
              simpa only [sourcePassed, Bind.bind, Except.bind, Pure.pure, Except.pure] using sourceCode
            have pointer := (cells cell.val cell.property).1
            have operation : IR.evalOperation owner inst.op
                ([.opaque before.source.memory, .opaque before.source.world, .opaque cell.val.pointer].map
                  (Slot.denote σ (privateToken values))) = .ok added := by
              simpa only [op, List.map_cons, List.map_nil, Slot.denote, before.source.current_token current,
                world, pointer, ← identity, added] using transferred.1
            have appended := source_appended before source.next inst σ values nextValues added owner _ operands operation executed
            have nextMemory := PrivateOutputs.memory_eq before source.next inst [.memory, .world] 0 outputs
              σ values added nextValues (memory.replace {object with live := false}) rfl rfl rfl appended
            refine ⟨nextValues, targetMemory, regs, executed, rfl,
              realized, models.words, ?_, _, nextMemory, transferred.2⟩
            exact PrivateOutputs.pointers before source.next inst [.memory, .world] 0 outputs
              σ values added nextValues regs rfl rfl appended models.pointers⟩
        else none
      else none
    else none
  else none

def privateErasedAccess {Γ : Var → Nat} (before : PrivateBank Γ) (inst : IR.Instruction) :
    Option (CheckedPrivateRow before (some inst) none) := do
  match op : inst.op with
  | .access size align =>
    let source ← privateInstruction before.source inst
    if noChecks : source.checks = [] then
      let cell ← privateCellOperand before.source inst 1
      if active : cell.val.live = true then
        if layout : size ≠ 0 ∧ align ≠ 0 ∧ size ≤ cell.val.width/8 ∧ align ≤ cell.val.width/8 then
          if operands : inst.operands.mapM (fun id => before.source.scope.lookup id) =
              some [.opaque before.source.memory, .opaque cell.val.pointer] then
            let outputs ← privateOutputs before source.next inst [.memory] 0
            return ⟨[], {before with source := source.next}, by
              intro σ values incoming owner frame targetMemory regs models passed
              obtain ⟨shape, typed, world, incomingSafe, memory, current, safe, frameBound,
                ownerBound, owned, distinct, cells⟩ := models.source
              have admitted := (before.source.cell_shape shape cell.val cell.property).2
              obtain ⟨object, access, identity, objectOwner, bytes, live, alignment, found⟩ :=
                private_cell_access σ values incoming memory owner cell.val (cells cell.val cell.property)
                  safe admitted active
              have objectFound : memory.objects.find? (·.id == object.id) = some object := by
                simpa only [identity] using found
              have positive := (safe.2 object (List.mem_of_find?_eq_some objectFound)).1
              have exactAccess := IR.access_object_zero memory object size align (Nat.ne_of_gt positive)
                objectFound (live.trans active) layout.1 layout.2.1
                (by simpa only [bytes, private_cell_bytes_length] using layout.2.2.1)
                (Nat.le_trans layout.2.2.2 alignment)
              have transferred := EvoIR.Machine.PrivateMemory.erased_access_transfer
                (model_memory before σ values incoming memory targetMemory owner frame regs models current)
                owner object.id size align 0 object 0 exactAccess
              let added : List IR.Value := [.memory memory]
              obtain ⟨nextValues, sourceCode, sourceNext⟩ := source.sound σ values incoming owner frame models.source
              have sourcePassed : guards σ source.checks = .ok () := by simp [noChecks, guards]
              have realized := sourceNext sourcePassed
              have executed : concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst =
                  .ok (source.next.scope.denote σ (privateToken nextValues)) := by
                simpa only [sourcePassed, Bind.bind, Except.bind, Pure.pure, Except.pure] using sourceCode
              have pointer := (cells cell.val cell.property).1
              have operation : IR.evalOperation owner inst.op
                  ([.opaque before.source.memory, .opaque cell.val.pointer].map
                    (Slot.denote σ (privateToken values))) = .ok added := by
                simpa only [op, List.map_cons, List.map_nil, Slot.denote, before.source.current_token current,
                  pointer, ← identity, added] using transferred.1
              have appended := source_appended before source.next inst σ values nextValues added owner _ operands operation executed
              have nextMemory := PrivateOutputs.memory_eq before source.next inst [.memory] 0 outputs
                σ values added nextValues memory rfl rfl rfl appended
              refine ⟨nextValues, targetMemory, regs, executed, rfl,
                realized, models.words, ?_, memory, nextMemory, transferred.2⟩
              exact PrivateOutputs.pointers before source.next inst [.memory] 0 outputs
                σ values added nextValues regs rfl rfl appended models.pointers⟩
          else none
        else none
      else none
    else none
  | _ => none

def privateLoad {Γ : Var → Nat} (before : PrivateBank Γ)
    (inst : IR.Instruction) (native : EvoIR.Machine.Instruction) :
    Option (CheckedPrivateRow before (some inst) (some native)) := do
  match op : inst.op, nativeOp : native with
  | .load width, .load align dst base offset kind volatile =>
    let source ← privateInstruction before.source inst
    if noChecks : source.checks = [] then
      let cell ← privateCellOperand before.source inst 2
      if layout : width = cell.val.width ∧ cell.val.live = true ∧ kind.bits = cell.val.width ∧
          (if align == 0 then kind.bits/8 else align) = cell.val.width/8 ∧
          offset = 0 ∧ volatile = false then
        match initialized : cell.val.value with
        | none => none
        | some value =>
          if operands : inst.operands.mapM (fun id => before.source.scope.lookup id) =
              some [.opaque before.source.memory, .opaque before.source.world, .opaque cell.val.pointer] then
            let pointerId ← inst.operands[2]?
            if pointerLookup : before.source.scope.lookup pointerId = some (.opaque cell.val.pointer) then
              let address ← nativePointer before pointerId base
              if separate : before.pointers.all (fun pointer => decide (pointer.target.id ≠ dst.id)) then
                let outputs ← privateOutputs before source.next inst [.bits cell.val.width, .memory, .world] 1
                let writes := symbolicValueWrite dst (.input ⟨cell.val.width, value⟩)
                let next : PrivateBank Γ := {before with source := source.next, words := applySymbolicWrites before.words writes}
                return ⟨[], next, by
                  intro σ values incoming owner frame targetMemory regs models passed
                  obtain ⟨shape, typed, world, incomingSafe, memory, current, safe, frameBound,
                    ownerBound, owned, distinct, cells⟩ := models.source
                  have admitted := (before.source.cell_shape shape cell.val cell.property).2
                  obtain ⟨object, access, identity, objectOwner, bytes, live, alignment, found⟩ :=
                    private_cell_access σ values incoming memory owner cell.val (cells cell.val cell.property)
                      safe admitted layout.2.1
                  have sourcePointer : IR.lookup (before.source.scope.denote σ (privateToken values)) pointerId =
                      .ok (.pointer (cell.val.id values) 0) := by
                    simp only [IR.lookup, lookup_denote, pointerLookup, Option.map_some, Slot.denote,
                      (cells cell.val cell.property).1]
                  have nativePointerRead := address.down _ regs (cell.val.id values) models.pointers sourcePointer
                  have exactBytes : object.bytes = IR.scalarBytes cell.val.width (denoteTerm σ value) := by
                    simpa only [PrivateCell.bytes, initialized] using bytes
                  have transferred := EvoIR.Machine.PrivateMemory.load_transfer
                    (model_memory before σ values incoming memory targetMemory owner frame regs models current)
                    owner object cell.val.width align (denoteTerm σ value) dst base kind
                    (EvoIR.Machine.readOperand regs) admitted layout.2.2.1 layout.2.2.2.1
                    (by simpa only [identity] using access) exactBytes
                    (by simpa only [identity] using nativePointerRead)
                  let added : List IR.Value := [.bits cell.val.width (denoteTerm σ value), .memory memory, .world]
                  obtain ⟨nextValues, sourceCode, sourceNext⟩ := source.sound σ values incoming owner frame models.source
                  have sourcePassed : guards σ source.checks = .ok () := by simp [noChecks, guards]
                  have realized := sourceNext sourcePassed
                  have executed : concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst =
                      .ok (source.next.scope.denote σ (privateToken nextValues)) := by
                    simpa only [sourcePassed, Bind.bind, Except.bind, Pure.pure, Except.pure] using sourceCode
                  have operation : IR.evalOperation owner inst.op
                      ([.opaque before.source.memory, .opaque before.source.world, .opaque cell.val.pointer].map
                        (Slot.denote σ (privateToken values))) = .ok added := by
                    simpa only [op, layout.1, List.map_cons, List.map_nil, Slot.denote,
                      before.source.current_token current, world, (cells cell.val cell.property).1,
                      ← identity, added] using transferred.1
                  have appended := source_appended before source.next inst σ values nextValues added owner _ operands operation executed
                  have nextMemory := PrivateOutputs.memory_eq before source.next inst
                    [.bits cell.val.width, .memory, .world] 1 outputs σ values added nextValues memory rfl rfl rfl appended
                  let nextRegs := EvoIR.Machine.applyWrites regs (writesDenote σ writes)
                  have evaluated : EvoIR.Machine.evalInstruction owner native regs targetMemory =
                      .ok (nextRegs, targetMemory) := by
                    simp only [EvoIR.Machine.evalInstruction, nativeOp, layout.2.2.2.2.1,
                      layout.2.2.2.2.2, transferred.2.1, Bind.bind, Except.bind, Pure.pure, Except.pure]
                    have denoted := value_write_denote σ dst (.input ⟨cell.val.width, value⟩)
                    simp only [WordExpr.denote] at denoted
                    rw [← denoted]
                  have oldSource := PrivateOutputs.pointers before source.next inst
                    [.bits cell.val.width, .memory, .world] 1 outputs
                    σ values added nextValues regs rfl rfl appended models.pointers
                  refine ⟨nextValues, targetMemory, nextRegs, executed, evaluated, realized,
                    bank_models_writes σ before.words regs models.words writes, ?_, memory, nextMemory, transferred.2.2⟩
                  apply pointer_target_preserved before.pointers _ regs nextRegs oldSource
                  intro pointer member
                  apply eval_pointer_preserved owner native regs nextRegs targetMemory _ evaluated pointer.target
                  intro defined definedMember
                  have only : defined = dst := by
                    simpa only [nativeOp, EvoIR.Machine.Instruction.defs, List.mem_singleton] using definedMember
                  subst defined
                  exact of_decide_eq_true (List.all_eq_true.mp separate pointer member)⟩
              else none
            else none
          else none
      else none
    else none
  | _, _ => none

def privateStore {Γ : Var → Nat} (checks : List (Guard Γ)) (before : PrivateBank Γ)
    (inst : IR.Instruction) (native : EvoIR.Machine.Instruction) :
    Option (CheckedPrivateRow before (some inst) (some native)) := do
  match op : inst.op, nativeOp : native with
  | .store, .store align src base offset kind volatile =>
    let source ← privateInstruction before.source inst
    if noChecks : source.checks = [] then
      let cell ← privateCellOperand before.source inst 2
      if layout : cell.val.live = true ∧ kind.bits = cell.val.width ∧
          (if align == 0 then kind.bits/8 else align) = cell.val.width/8 ∧
          offset = 0 ∧ volatile = false then
        let sourceValueId ← inst.operands[3]?
        let word ← before.source.scope.word sourceValueId
        let value ← word.at cell.val.width
        if operands : inst.operands.mapM (fun id => before.source.scope.lookup id) =
            some [.opaque before.source.memory, .opaque before.source.world, .opaque cell.val.pointer,
              .word ⟨cell.val.width, value⟩] then
          let pointerId ← inst.operands[2]?
          if pointerLookup : before.source.scope.lookup pointerId = some (.opaque cell.val.pointer) then
            let address ← nativePointer before pointerId base
            let nativeValue ← nativeWord before.words src
            let equal ← checkWordUnder checks ⟨cell.val.width, value⟩ nativeValue.val
            let outputs ← privateOutputs before source.next inst [.memory, .world] 0
            return ⟨checks, {before with source := source.next}, by
              intro σ values incoming owner frame targetMemory regs models passed
              obtain ⟨shape, typed, world, incomingSafe, memory, current, safe, frameBound,
                ownerBound, owned, distinct, cells⟩ := models.source
              have admitted := (before.source.cell_shape shape cell.val cell.property).2
              obtain ⟨object, access, identity, objectOwner, bytes, live, alignment, found⟩ :=
                private_cell_access σ values incoming memory owner cell.val (cells cell.val cell.property)
                  safe admitted layout.1
              have objectFound : memory.objects.find? (·.id == object.id) = some object := by
                simpa only [identity] using found
              have alive := actual_alive memory object safe objectFound (live.trans layout.1)
              have sourcePointer : IR.lookup (before.source.scope.denote σ (privateToken values)) pointerId =
                  .ok (.pointer (cell.val.id values) 0) := by
                simp only [IR.lookup, lookup_denote, pointerLookup, Option.map_some, Slot.denote,
                  (cells cell.val cell.property).1]
              have nativePointerRead := address.down _ regs (cell.val.id values) models.pointers sourcePointer
              have nativeValueRead := nativeValue.property σ regs models.words
              rw [equal.down σ passed] at nativeValueRead
              obtain ⟨other, objects, sourceOperation, nativeOperation, related⟩ :=
                EvoIR.Machine.PrivateMemory.store_transfer
                  (model_memory before σ values incoming memory targetMemory owner frame regs models current)
                  owner object cell.val.width align (denoteTerm σ value) src base kind
                  (EvoIR.Machine.readOperand regs) admitted layout.2.1 layout.2.2.1 alive
                  (by simpa only [identity] using access)
                  (by simpa only [bytes, private_cell_bytes_length])
                  (by simpa only [identity] using nativePointerRead) nativeValueRead
              let added : List IR.Value := [.memory (memory.replace
                {object with bytes := IR.scalarBytes cell.val.width (denoteTerm σ value)}), .world]
              obtain ⟨nextValues, sourceCode, sourceNext⟩ := source.sound σ values incoming owner frame models.source
              have sourcePassed : guards σ source.checks = .ok () := by simp [noChecks, guards]
              have realized := sourceNext sourcePassed
              have executed : concreteInstruction owner (before.source.scope.denote σ (privateToken values)) inst =
                  .ok (source.next.scope.denote σ (privateToken nextValues)) := by
                simpa only [sourcePassed, Bind.bind, Except.bind, Pure.pure, Except.pure] using sourceCode
              have operation : IR.evalOperation owner inst.op
                  ([.opaque before.source.memory, .opaque before.source.world, .opaque cell.val.pointer,
                    .word ⟨cell.val.width, value⟩].map (Slot.denote σ (privateToken values))) = .ok added := by
                simpa only [op, List.map_cons, List.map_nil, Slot.denote, Word.denote,
                  before.source.current_token current, world, (cells cell.val cell.property).1,
                  ← identity, added] using sourceOperation
              have appended := source_appended before source.next inst σ values nextValues added owner _ operands operation executed
              have nextMemory := PrivateOutputs.memory_eq before source.next inst [.memory, .world] 0 outputs
                σ values added nextValues _ rfl rfl rfl appended
              have evaluated : EvoIR.Machine.evalInstruction owner native regs targetMemory =
                  .ok (regs, targetMemory.replace
                    {other with bytes := IR.scalarBytes cell.val.width (denoteTerm σ value)}) := by
                simp only [EvoIR.Machine.evalInstruction, nativeOp, layout.2.2.2.1, layout.2.2.2.2,
                  nativeOperation, Bind.bind, Except.bind, Pure.pure, Except.pure,
                  EvoIR.Machine.applyWrites, List.foldl_nil]
              refine ⟨nextValues, _, regs, executed, evaluated, realized, models.words, ?_, _, nextMemory, related⟩
              exact PrivateOutputs.pointers before source.next inst [.memory, .world] 0 outputs
                σ values added nextValues regs rfl rfl appended models.pointers⟩
          else none
        else none
      else none
    else none
  | _, _ => none

end EvoIR.Core.Symbolic.MachineSelection
