import EvoIR.Core.WellFormed
import EvoIR.Core.CheckedInt

/-!
Source integer semantics for the admitted LP64 types, not IR normalization.
The source rank survives every value; conversions use its signed mathematical
value and the destination representation. Diagnostics and malformed-machine
states are different outcomes. The latter must be ruled out by V01 soundness.

The executed source-constant exception in the EvoIR specification's September
10 ruling is explicit below: literal-only add/sub/mul/neg may wrap. This does
not turn object reads into constants or suppress an executed div/rem failure.
-/
namespace EvoIR.Core.Source

structure Value where
  type : IntType
  bits : BitVec type.width
  deriving DecidableEq, Repr

def Value.number (v : Value) : Int :=
  if v.type.signed then v.bits.toInt else v.bits.toNat

def Value.ofInt (t : IntType) (n : Int) : Value := ⟨t, BitVec.ofInt t.width n⟩
def Value.ofNat (t : IntType) (n : Nat) : Value := ⟨t, BitVec.ofNat t.width n⟩
def Value.cast (t : IntType) (v : Value) : Value := .ofInt t v.number
def Value.truth (v : Value) : Bool := v.bits.toNat != 0
def Value.boolean (b : Bool) : Value := .ofNat .int (if b then 1 else 0)

inductive Fault where
  | diagnostic (code : Nat)
  | malformed (message : String)
  deriving DecidableEq, Repr

abbrev Result := Except Fault Value

deriving instance DecidableEq for Except

def arithmeticResult (t : IntType) (z : Int) (constant : Bool) : Result :=
  if t.signed && !constant && !(decide (signedRepresentable t.width z)) then
    .error (.diagnostic 3004)
  else .ok (.ofInt t z)

def unaryValue (op : Unary) (v : Value) (constant : Bool := false) : Result :=
  match op with
  | .plus => .ok v
  | .neg => arithmeticResult v.type (-v.number) constant
  | .bitNot => .ok ⟨v.type, ~~~v.bits⟩
  | .not => .ok (.boolean (!v.truth))

/-- Integer promotion has already selected one of the six source types.
Shift counts keep their independent type/width; they are never UAC-converted
to the shifted type or masked before the definedness check. -/
def binaryValue (op : Binary) (a b : Value) (constant : Bool := false) : Result := do
  match op with
  | .comma => return b
  | .and => return .boolean (a.truth && b.truth)
  | .or => return .boolean (a.truth || b.truth)
  | .shl | .shr =>
    let count := b.number
    if count < 0 || count ≥ (a.type.width : Int) then
      throw (.diagnostic 3005)
    if op == .shl then
      if a.type.signed then throw (.malformed "signed left shift is outside core-v1")
      return ⟨a.type, a.bits <<< count.toNat⟩
    else if a.type.signed then
      return ⟨a.type, a.bits.sshiftRight count.toNat⟩
    else return ⟨a.type, a.bits >>> count.toNat⟩
  | _ =>
    let t := usualType a.type b.type
    let x := (a.cast t).number
    let y := (b.cast t).number
    match op with
    | .add => arithmeticResult t (x + y) constant
    | .sub => arithmeticResult t (x - y) constant
    | .mul => arithmeticResult t (x * y) constant
    | .div | .rem =>
      if y == 0 then throw (.diagnostic (if op == .div then 3002 else 3003))
      if t.signed && x == signedMin t.width && y == -1 then throw (.diagnostic 3004)
      -- Int.tdiv/tmod truncate toward zero, unlike Lean's Euclidean / and %.
      return .ofInt t (if op == .div then x.tdiv y else x.tmod y)
    | .lt => return .boolean (x < y)
    | .le => return .boolean (x ≤ y)
    | .gt => return .boolean (x > y)
    | .ge => return .boolean (x ≥ y)
    | .eq => return .boolean (x == y)
    | .ne => return .boolean (x != y)
    | .bitAnd => return ⟨t, (BitVec.ofInt t.width x) &&& (BitVec.ofInt t.width y)⟩
    | .bitOr => return ⟨t, (BitVec.ofInt t.width x) ||| (BitVec.ofInt t.width y)⟩
    | .bitXor => return ⟨t, (BitVec.ofInt t.width x) ^^^ (BitVec.ofInt t.width y)⟩
    | _ => throw (.malformed "unexpected integer operator")

def Assignment.binary : Assignment → Option Binary
  | .set => none
  | .add => some .add | .sub => some .sub | .mul => some .mul
  | .div => some .div | .rem => some .rem | .shl => some .shl | .shr => some .shr
  | .bitAnd => some .bitAnd | .bitOr => some .bitOr | .bitXor => some .bitXor

/-- Source-literal evaluation only. Object reads, assignments, updates and calls
never supply constant operands, including reads of const-qualified objects. -/
def constantValue : Expr → Option Value
  | .literal t n => some (.ofNat t n)
  | .unary _ op a => do (unaryValue op (← constantValue a) true).toOption
  | .binary _ op a b => do
    if op == .comma then none else do
      let x ← constantValue a
      if op == .and && !x.truth then return .boolean false
      if op == .or && x.truth then return .boolean true
      (binaryValue op x (← constantValue b) true).toOption
  | .cast _ destination a => return (← constantValue a).cast destination.type
  | .conditional t c a b => do
    let v ← if (← constantValue c).truth then constantValue a else constantValue b
    return v.cast t
  | _ => none

/-- Only the explicitly approved wrapping cases bypass runtime evaluation.
Other constants still follow the ordinary expression transitions. -/
def wrappingConstant : Expr → Option Value
  | .unary _ .neg a => do
    let x ← constantValue a
    if unaryValue .neg x = .error (.diagnostic 3004) then
      (unaryValue .neg x true).toOption
    else none
  | .binary _ op a b => do
    if !(op == .add || op == .sub || op == .mul) then none else do
      let x ← constantValue a
      let y ← constantValue b
      if binaryValue op x y = .error (.diagnostic 3004) then
        (binaryValue op x y true).toOption
      else none
  | _ => none

-- These reductions distinguish source conversions from host integer casts.
example : (Value.ofNat .int 0xffffffff).cast .ulong = Value.ofNat .ulong 0xffffffffffffffff := by decide
example : (Value.ofNat .uint 0xffffffff).cast .long = Value.ofNat .long 0xffffffff := by decide
example : binaryValue .div (.ofInt .int (-7)) (.ofInt .int 3) = .ok (.ofInt .int (-2)) := by decide
example : binaryValue .rem (.ofInt .int (-7)) (.ofInt .int 3) = .ok (.ofInt .int (-1)) := by decide
example : binaryValue .rem (.ofNat .int 0x80000000) (.ofInt .int (-1)) = .error (.diagnostic 3004) := by decide
example : binaryValue .shl (.ofNat .uint 1) (.ofNat .ulong 0x100000001) = .error (.diagnostic 3005) := by decide
example : binaryValue .shr (.ofNat .int 0x80000000) (.ofInt .long (-1)) = .error (.diagnostic 3005) := by decide

end EvoIR.Core.Source
