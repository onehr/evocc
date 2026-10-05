import EvoIR.MachineExecution

/-!
Exact binding laws for the actual machine. Argument binding captures values
before writing destinations, retains the complete call vector, and normalizes
32-bit destinations. These laws apply to entry, return and SSA edge bindings;
they do not assume that an allocator or argument-demand analysis was correct.
-/
namespace EvoIR.Machine

def bindValues (regs : RegisterFile) (values : List (VReg × Value)) : RegisterFile :=
  values.foldl (fun regs (reg, value) => write regs reg value) regs

theorem bind_exact (regs : RegisterFile) (params : List VReg) (args : List Value)
    (arity : params.length = args.length)
    (unique : (params.map VReg.id).eraseDups.length = params.length) :
    bind regs params args = .ok (bindValues regs (params.zip args)) := by
  simp [bind, arity, unique, bindValues]
  rfl

theorem bindValues_lookup_other (regs : RegisterFile) (values : List (VReg × Value))
    (id : Nat) (absent : ∀ p ∈ values, id ≠ p.1.id) :
    (bindValues regs values).lookup id = regs.lookup id := by
  induction values generalizing regs with
  | nil => rfl
  | cons p rest ih =>
    rcases p with ⟨reg, value⟩
    have different : id ≠ reg.id := absent (reg, value) (by simp)
    have tail := ih (write regs reg value) (by intro p hp; exact absent p (by simp [hp]))
    exact tail.trans (readSlot_writeSlot_other regs reg ⟨id, .gpr64⟩ _ different)

theorem bindValues_lookup_member (regs : RegisterFile) (values : List (VReg × Value))
    (unique : (values.map (fun p => p.1.id)).Nodup)
    (reg : VReg) (value : Value) (member : (reg, value) ∈ values) :
    (bindValues regs values).lookup reg.id = some (.value (normalize reg.class value)) := by
  induction values generalizing regs with
  | nil => simp at member
  | cons p rest ih =>
    rcases p with ⟨head, input⟩
    simp only [List.map_cons, List.nodup_cons] at unique
    rcases List.mem_cons.mp member with same | later
    · cases same
      have untouched := bindValues_lookup_other (write regs reg value) rest reg.id (by
        intro p hp equal
        exact unique.1 (by rw [equal]; exact List.mem_map.mpr ⟨p, hp, rfl⟩))
      simpa [bindValues, List.foldl_cons, write, writeSlot] using untouched
    · exact ih (write regs head input) unique.2 later

/-- Only the named registers participate in this relation. In particular,
scratch registers and undemanded arguments do not acquire asserted values. -/
def RegistersAgree (needed : List Nat) (left right : RegisterFile) : Prop :=
  ∀ id, id ∈ needed → left.lookup id = right.lookup id

theorem RegistersAgree.write (needed : List Nat) (left right : RegisterFile)
    (agree : RegistersAgree needed left right) (reg : VReg) (a b : Value)
    (same : reg.id ∈ needed → normalize reg.class a = normalize reg.class b) :
    RegistersAgree needed (write left reg a) (write right reg b) := by
  intro id used
  by_cases equal : id = reg.id
  · subst id
    simp [Machine.write, writeSlot, same used]
  · change readSlot (Machine.write left reg a) ⟨id, .gpr64⟩ =
      readSlot (Machine.write right reg b) ⟨id, .gpr64⟩
    simp only [Machine.write]
    rw [readSlot_writeSlot_other left reg ⟨id, .gpr64⟩ _ equal,
      readSlot_writeSlot_other right reg ⟨id, .gpr64⟩ _ equal]
    exact agree id used

inductive BindingAgreement (needed : List Nat) :
    List (VReg × Value) → List (VReg × Value) → Prop where
  | nil : BindingAgreement needed [] []
  | cons (reg : VReg) (a b : Value) {as bs : List (VReg × Value)}
      (same : reg.id ∈ needed → normalize reg.class a = normalize reg.class b)
      (rest : BindingAgreement needed as bs) :
      BindingAgreement needed ((reg, a) :: as) ((reg, b) :: bs)

theorem bindValues_agree (needed : List Nat) (left right : RegisterFile)
    (agree : RegistersAgree needed left right) (a b : List (VReg × Value))
    (correspondence : BindingAgreement needed a b) :
    RegistersAgree needed (bindValues left a) (bindValues right b) := by
  induction correspondence generalizing left right with
  | nil => exact agree
  | cons reg av bv values rest ih =>
    exact ih _ _ (RegistersAgree.write needed left right agree reg av bv values)

theorem bind_agree (needed : List Nat) (left right : RegisterFile)
    (agree : RegistersAgree needed left right) (params : List VReg) (a b : List Value)
    (leftArity : params.length = a.length) (rightArity : params.length = b.length)
    (unique : (params.map VReg.id).eraseDups.length = params.length)
    (correspondence : BindingAgreement needed (params.zip a) (params.zip b)) :
    ∃ afterLeft afterRight,
      bind left params a = .ok afterLeft ∧ bind right params b = .ok afterRight ∧
      RegistersAgree needed afterLeft afterRight :=
  ⟨_, _, bind_exact left params a leftArity unique,
    bind_exact right params b rightArity unique,
    bindValues_agree needed left right agree _ _ correspondence⟩

def sourceArgument (value : Core.Source.Value) : Value :=
  .int (value.bits.setWidth 64)

def sourceArgumentClass : Core.Source.IntType → RegClass
  | .int | .uint => .gpr32
  | _ => .gpr64

theorem source_argument_normalized (value : Core.Source.Value) :
    normalize (sourceArgumentClass value.type) (sourceArgument value) = sourceArgument value := by
  rcases value with ⟨type, bits⟩
  cases type <;> simp [sourceArgumentClass, sourceArgument, normalize,
    Core.Source.IntType.width, BitVec.setWidth_setWidth_of_le]

theorem source_argument_conforms (value : Core.Source.Value) :
    (sourceArgument value).Conforms (sourceArgumentClass value.type) := by
  rcases value with ⟨type, bits⟩
  cases type <;> simp [sourceArgumentClass, sourceArgument, Value.Conforms,
    Core.Source.IntType.width, BitVec.setWidth_setWidth_of_le]

end EvoIR.Machine
