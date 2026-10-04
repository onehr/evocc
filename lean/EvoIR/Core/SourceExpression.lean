import EvoIR.Core.SymbolicSource
import EvoIR.Core.SourceOperators
import EvoIR.Core.Execution

/-! Source-tree-driven expression diagrams. The checker reads the original
expression, including its evaluation order and literal-only wrapping rule.
It does not accept a producer-supplied expected arithmetic expression.

This component handles read-only straight-line expressions. Calls, writes,
short-circuit control and other unsupported rows refuse here; their enclosing
CFG/state diagrams remain required for whole-function admission. -/
namespace EvoIR.Core.Source

theorem Steps.trans {p : Program} {a b c : State}
    (first : Steps p a b) (second : Steps p b c) : Steps p a c := by
  induction first with
  | refl => exact second
  | next step _ ih => exact .next step (ih second)

/-- An expression finishes before consuming the surrounding continuation.
A diagnostic retains the actual continuation at the first failing operation;
it need not be the caller's continuation. No evaluator fuel occurs here. -/
def Evaluates (p : Program) (locals : Locals) (functionId : Nat)
    (e : Expr) (result : Result) : Prop :=
  ∀ k, match result with
  | .ok value => Steps p ⟨.expression e, locals, k, functionId⟩
      ⟨.value value, locals, k, functionId⟩
  | .error fault => ∃ failedK, Steps p ⟨.expression e, locals, k, functionId⟩
      ⟨.halted (.error fault), locals, failedK, functionId⟩

theorem evaluates_wrapping (p : Program) (locals : Locals) (functionId : Nat)
    (e : Expr) (v : Value) (wrapped : wrappingConstant e = some v)
    (typed : v.type = e.type) : Evaluates p locals functionId e (.ok v) := by
  intro k
  exact .next (by simp [Step, step, enterExpression, wrapped, valueResult, typed]) (.refl _)

theorem evaluates_literal (p : Program) (locals : Locals) (functionId : Nat)
    (t : IntType) (n : Nat) (canonical : n < 2^t.width) :
    Evaluates p locals functionId (.literal t n) (.ok (.ofNat t n)) := by
  intro k
  exact .next (by simp [Step, step, enterExpression, wrappingConstant, canonical,
    valueResult, Value.ofNat]) (.refl _)

theorem evaluates_ident (p : Program) (locals : Locals) (functionId : Nat)
    (t : IntType) (id : Nat) (v : Value)
    (read : readSlot locals id t = .ok v) (typed : v.type = t) :
    Evaluates p locals functionId (.ident t id) (.ok v) := by
  intro k
  exact .next (by simp [Step, step, enterExpression, wrappingConstant, read,
    valueResult, typed]) (.refl _)

theorem evaluates_cast (p : Program) (locals : Locals) (functionId : Nat)
    (destination : QualifiedType) (e : Expr) (r : Result)
    (operand : Evaluates p locals functionId e r) :
    Evaluates p locals functionId (.cast destination.type destination e)
      (r.map (Value.cast destination.type)) := by
  intro k
  have first : Step p
      ⟨.expression (.cast destination.type destination e), locals, k, functionId⟩
      ⟨.expression e, locals, .convert destination.type :: k, functionId⟩ := by
    simp [Step, step, enterExpression, wrappingConstant]
  cases r with
  | error fault =>
    obtain ⟨failedK, path⟩ := operand (.convert destination.type :: k)
    exact ⟨failedK, .next first path⟩
  | ok value =>
    exact .next first ((operand (.convert destination.type :: k)).trans
      (.next (by rfl) (.refl _)))

/-- Both operands execute in source order. Only the short-circuit operators
are excluded: those require two successor diagrams instead of this one. -/
theorem evaluates_binary (p : Program) (locals : Locals) (functionId : Nat)
    (t : IntType) (op : Binary) (left right : Expr) (a b : Result)
    (notAnd : op ≠ .and) (notOr : op ≠ .or)
    (noWrap : wrappingConstant (.binary t op left right) = none)
    (leftPath : Evaluates p locals functionId left a)
    (rightPath : Evaluates p locals functionId right b)
    (typed : ∀ x y v, a = .ok x → b = .ok y →
      binaryValue op x y = .ok v → v.type = t) :
    Evaluates p locals functionId (.binary t op left right)
      (do let x ← a; let y ← b; binaryValue op x y) := by
  intro k
  have first : Step p
      ⟨.expression (.binary t op left right), locals, k, functionId⟩
      ⟨.expression left, locals, .binaryLeft t op right :: k, functionId⟩ := by
    simp [Step, step, enterExpression, noWrap]
  cases a with
  | error fault =>
    obtain ⟨failedK, path⟩ := leftPath (.binaryLeft t op right :: k)
    exact ⟨failedK, .next first path⟩
  | ok x =>
    have next : Step p
        ⟨.value x, locals, .binaryLeft t op right :: k, functionId⟩
        ⟨.expression right, locals, .binaryRight t op x :: k, functionId⟩ := by
      simp [Step, step, resumeValue, notAnd, notOr]
    cases b with
    | error fault =>
      obtain ⟨failedK, path⟩ := rightPath (.binaryRight t op x :: k)
      exact ⟨failedK, (Steps.next first (leftPath (.binaryLeft t op right :: k))).trans
        (.next next path)⟩
    | ok y =>
      have path := (Source.Steps.next first (leftPath (.binaryLeft t op right :: k))).trans
        (Source.Steps.next next (rightPath (.binaryRight t op x :: k)))
      cases result : binaryValue op x y with
      | error fault =>
        simp only [bind, Except.bind, result]
        refine ⟨k, path.trans (.next ?_ (.refl _))⟩
        simp [Step, step, resumeValue, valueResult, result, fail]
      | ok value =>
        have type := typed x y value rfl rfl result
        simp only [bind, Except.bind, result]
        exact path.trans (.next (by
          simp [Step, step, resumeValue, valueResult, result, type]) (.refl _))

theorem evaluates_unary (p : Program) (locals : Locals) (functionId : Nat)
    (t : IntType) (op : Unary) (e : Expr) (r : Result)
    (noWrap : wrappingConstant (.unary t op e) = none)
    (operand : Evaluates p locals functionId e r)
    (typed : ∀ x v, r = .ok x → unaryValue op x = .ok v → v.type = t) :
    Evaluates p locals functionId (.unary t op e)
      (do let x ← r; unaryValue op x) := by
  intro k
  have first : Step p ⟨.expression (.unary t op e), locals, k, functionId⟩
      ⟨.expression e, locals, .unary t op :: k, functionId⟩ := by
    simp [Step, step, enterExpression, noWrap]
  cases r with
  | error fault =>
    obtain ⟨failedK, path⟩ := operand (.unary t op :: k)
    exact ⟨failedK, .next first path⟩
  | ok x =>
    have path := Steps.next first (operand (.unary t op :: k))
    cases result : unaryValue op x with
    | error fault =>
      simp only [bind, Except.bind, result]
      exact ⟨k, path.trans (.next (by
        simp [Step, step, resumeValue, valueResult, result, fail]) (.refl _))⟩
    | ok v =>
      simp only [bind, Except.bind, result]
      exact path.trans (.next (by
        simp [Step, step, resumeValue, valueResult, result, typed x v rfl result]) (.refl _))

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

abbrev CBindings (Γ : Var → Nat) := List (Nat × CWord Γ)
abbrev ExpressionResult (Γ : Var → Nat) := List (Guard Γ) × CWord Γ

def ExpressionResult.denote {Γ : Var → Nat} (σ : Subst Γ)
    (r : ExpressionResult Γ) : Source.Result := do
  guards σ r.1
  return r.2.denote σ

/-- The outer state relation must establish each read against actual local
storage. Unmentioned locals are unrestricted, not silently removed. -/
def CBindings.Related {Γ : Var → Nat} (σ : Subst Γ) (env : CBindings Γ)
    (locals : Source.Locals) : Prop :=
  ∀ id v, env.lookup id = some v →
    Source.readSlot locals id v.type = .ok (v.denote σ)

def CWord.constant {Γ : Var → Nat} (v : Source.Value) : CWord Γ :=
  ⟨v.type, .const v.type.width v.bits⟩

inductive ExpressionDiagram {Γ : Var → Nat} (env : CBindings Γ) :
    Source.Expr → ExpressionResult Γ → Prop where
  | wrapped (e : Source.Expr) (v : Source.Value)
      (wrap : Source.wrappingConstant e = some v) (typed : v.type = e.type) :
      ExpressionDiagram env e ([], CWord.constant v)
  | literal (t : Source.IntType) (n : Nat) (canonical : n < 2^t.width) :
      ExpressionDiagram env (.literal t n) ([], CWord.constant (.ofNat t n))
  | ident (id : Nat) (v : CWord Γ) (found : env.lookup id = some v) :
      ExpressionDiagram env (.ident v.type id) ([], v)
  | cast (destination : Source.QualifiedType) (e : Source.Expr)
      (r : ExpressionResult Γ) (operand : ExpressionDiagram env e r) :
      ExpressionDiagram env (.cast destination.type destination e) (r.1, r.2.cast destination.type)
  | arithmetic (op : SignedArithmetic) (left right : Source.Expr)
      (a b : ExpressionResult Γ)
      (noWrap : Source.wrappingConstant
        (.binary (arithmetic op a.2 b.2).2.type (Source.arithmeticSourceOp op) left right) = none)
      (first : ExpressionDiagram env left a) (second : ExpressionDiagram env right b) :
      ExpressionDiagram env
        (.binary (arithmetic op a.2 b.2).2.type (Source.arithmeticSourceOp op) left right)
        (a.1 ++ b.1 ++ (arithmetic op a.2 b.2).1, (arithmetic op a.2 b.2).2)
  | shift (toLeft : Bool) (left right : Source.Expr) (a b : ExpressionResult Γ)
      (admitted : toLeft = false ∨ a.2.type.signed = false)
      (first : ExpressionDiagram env left a) (second : ExpressionDiagram env right b) :
      ExpressionDiagram env (.binary a.2.type (if toLeft then .shl else .shr) left right)
        (a.1 ++ b.1 ++ (shift toLeft a.2 b.2).1, (shift toLeft a.2 b.2).2)
  | bitwise (op : Bitwise) (left right : Source.Expr) (a b : ExpressionResult Γ)
      (first : ExpressionDiagram env left a) (second : ExpressionDiagram env right b) :
      ExpressionDiagram env (.binary (bitwise op a.2 b.2).type op.source left right)
        (a.1 ++ b.1, bitwise op a.2 b.2)
  | comparison (op : Comparison) (left right : Source.Expr) (a b : ExpressionResult Γ)
      (first : ExpressionDiagram env left a) (second : ExpressionDiagram env right b) :
      ExpressionDiagram env (.binary .int op.source left right)
        (a.1 ++ b.1, comparison op a.2 b.2)
  | unary (op : Source.Unary) (operand : Source.Expr) (r : ExpressionResult Γ)
      (noWrap : Source.wrappingConstant (.unary (unary op r.2).2.type op operand) = none)
      (inner : ExpressionDiagram env operand r) :
      ExpressionDiagram env (.unary (unary op r.2).2.type op operand)
        (r.1 ++ (unary op r.2).1, (unary op r.2).2)

theorem expression_result_value {Γ : Var → Nat} (σ : Subst Γ)
    (r : ExpressionResult Γ) (v : Source.Value)
    (value : r.denote σ = .ok v) : v = r.2.denote σ := by
  unfold ExpressionResult.denote at value
  cases h : guards σ r.1 <;> simp_all [bind, Except.bind, pure, Except.pure]

theorem expression_arithmetic_result {Γ : Var → Nat} (σ : Subst Γ)
    (op : SignedArithmetic) (a b : ExpressionResult Γ) :
    ExpressionResult.denote σ
        (a.1 ++ b.1 ++ (arithmetic op a.2 b.2).1, (arithmetic op a.2 b.2).2) =
      (do let x ← a.denote σ; let y ← b.denote σ;
          Source.binaryValue (Source.arithmeticSourceOp op) x y) := by
  simp only [ExpressionResult.denote, guards_append]
  cases guards σ a.1 with
  | error fault => rfl
  | ok u =>
    cases u
    cases guards σ b.1 with
    | error fault => rfl
    | ok u =>
      cases u
      exact arithmetic_sound σ op a.2 b.2

theorem expression_shift_result {Γ : Var → Nat} (σ : Subst Γ)
    (left : Bool) (a b : ExpressionResult Γ)
    (admitted : left = false ∨ a.2.type.signed = false) :
    ExpressionResult.denote σ
        (a.1 ++ b.1 ++ (shift left a.2 b.2).1, (shift left a.2 b.2).2) =
      (do let x ← a.denote σ; let y ← b.denote σ;
          Source.binaryValue (if left then .shl else .shr) x y) := by
  simp only [ExpressionResult.denote, guards_append]
  cases guards σ a.1 with
  | error fault => rfl
  | ok u =>
    cases u
    cases guards σ b.1 with
    | error fault => rfl
    | ok u =>
      cases u
      exact shift_sound σ left a.2 b.2 admitted

theorem expression_bitwise_result {Γ : Var → Nat} (σ : Subst Γ)
    (op : Bitwise) (a b : ExpressionResult Γ) :
    ExpressionResult.denote σ (a.1 ++ b.1, bitwise op a.2 b.2) =
      (do let x ← a.denote σ; let y ← b.denote σ; Source.binaryValue op.source x y) := by
  simp only [ExpressionResult.denote, guards_append]
  cases guards σ a.1 with
  | error fault => rfl
  | ok u =>
    cases u
    cases guards σ b.1 with
    | error fault => rfl
    | ok u =>
      cases u
      exact (bitwise_sound σ op a.2 b.2).symm

theorem expression_comparison_result {Γ : Var → Nat} (σ : Subst Γ)
    (op : Comparison) (a b : ExpressionResult Γ) :
    ExpressionResult.denote σ (a.1 ++ b.1, comparison op a.2 b.2) =
      (do let x ← a.denote σ; let y ← b.denote σ; Source.binaryValue op.source x y) := by
  simp only [ExpressionResult.denote, guards_append]
  cases guards σ a.1 with
  | error fault => rfl
  | ok u =>
    cases u
    cases guards σ b.1 with
    | error fault => rfl
    | ok u =>
      cases u
      exact (comparison_sound σ op a.2 b.2).symm

theorem expression_unary_result {Γ : Var → Nat} (σ : Subst Γ)
    (op : Source.Unary) (r : ExpressionResult Γ) :
    ExpressionResult.denote σ (r.1 ++ (unary op r.2).1, (unary op r.2).2) =
      (do let x ← r.denote σ; Source.unaryValue op x) := by
  simp only [ExpressionResult.denote, guards_append]
  cases guards σ r.1 with
  | error fault => rfl
  | ok u => cases u; exact unary_sound σ op r.2

/-- Every accepted expression diagram runs on the original source machine,
for arbitrary operand bits, actual related locals and surrounding continuation.
The guard trace is ordered, so a later diagnostic cannot mask an earlier one. -/
theorem ExpressionDiagram.sound {Γ : Var → Nat} {env : CBindings Γ}
    {e : Source.Expr} {r : ExpressionResult Γ} (diagram : ExpressionDiagram env e r)
    (σ : Subst Γ) (p : Source.Program) (locals : Source.Locals) (functionId : Nat)
    (related : env.Related σ locals) :
    Source.Evaluates p locals functionId e (r.denote σ) := by
  induction diagram with
  | wrapped e v wrap typed =>
    exact Source.evaluates_wrapping p locals functionId e v wrap typed
  | literal t n canonical =>
    exact Source.evaluates_literal p locals functionId t n canonical
  | ident id v found =>
    exact Source.evaluates_ident p locals functionId v.type id (v.denote σ)
      (related id v found) rfl
  | cast destination e r _ ih =>
    have path := Source.evaluates_cast p locals functionId destination e (r.denote σ) ih
    have same : ExpressionResult.denote σ (r.1, r.2.cast destination.type) =
        (r.denote σ).map (Source.Value.cast destination.type) := by
      simp only [ExpressionResult.denote, cast_sound]
      cases guards σ r.1 <;> rfl
    rw [same]
    exact path
  | arithmetic op left right a b noWrap _ _ first second =>
    rw [expression_arithmetic_result]
    apply Source.evaluates_binary p locals functionId
      (EvoIR.Core.Symbolic.arithmetic op a.2 b.2).2.type (Source.arithmeticSourceOp op)
      left right (a.denote σ) (b.denote σ)
      (by cases op <;> decide) (by cases op <;> decide) noWrap first second
    intro x y v hx hy hv
    rw [expression_result_value σ a x hx, expression_result_value σ b y hy,
      ← arithmetic_sound σ op a.2 b.2] at hv
    cases h : guards σ (EvoIR.Core.Symbolic.arithmetic op a.2 b.2).1 with
    | error fault => simp_all [bind, Except.bind]
    | ok unitValue =>
      simp only [h, bind, Except.bind, pure, Except.pure, Except.ok.injEq] at hv
      subst v
      rfl
  | shift toLeft left right a b admitted _ _ first second =>
    rw [expression_shift_result σ toLeft a b admitted]
    apply Source.evaluates_binary p locals functionId a.2.type
      (if toLeft then .shl else .shr) left right (a.denote σ) (b.denote σ)
      (by cases toLeft <;> decide) (by cases toLeft <;> decide)
      (by cases toLeft <;> rfl) first second
    intro x y v hx hy hv
    rw [expression_result_value σ a x hx, expression_result_value σ b y hy,
      ← shift_sound σ toLeft a.2 b.2 admitted] at hv
    cases h : guards σ (EvoIR.Core.Symbolic.shift toLeft a.2 b.2).1 with
    | error fault => simp_all [bind, Except.bind]
    | ok unitValue =>
      simp only [h, bind, Except.bind, pure, Except.pure, Except.ok.injEq] at hv
      subst v
      rfl

  | bitwise op left right a b _ _ first second =>
    rw [expression_bitwise_result]
    apply Source.evaluates_binary p locals functionId
      (EvoIR.Core.Symbolic.bitwise op a.2 b.2).type op.source left right
      (a.denote σ) (b.denote σ) (by cases op <;> decide) (by cases op <;> decide)
      (by cases op <;> rfl) first second
    intro x y v hx hy hv
    rw [expression_result_value σ a x hx, expression_result_value σ b y hy,
      bitwise_sound σ op a.2 b.2] at hv
    cases Except.ok.inj hv
    rfl
  | comparison op left right a b _ _ first second =>
    rw [expression_comparison_result]
    apply Source.evaluates_binary p locals functionId .int op.source left right
      (a.denote σ) (b.denote σ) (by cases op <;> decide) (by cases op <;> decide)
      (by cases op <;> rfl) first second
    intro x y v hx hy hv
    rw [expression_result_value σ a x hx, expression_result_value σ b y hy,
      comparison_sound σ op a.2 b.2] at hv
    cases Except.ok.inj hv
    rfl
  | unary op operand r noWrap _ ih =>
    rw [expression_unary_result]
    apply Source.evaluates_unary p locals functionId (EvoIR.Core.Symbolic.unary op r.2).2.type
      op operand (r.denote σ) noWrap ih
    intro x v hx hv
    rw [expression_result_value σ r x hx, ← unary_sound σ op r.2] at hv
    cases h : guards σ (EvoIR.Core.Symbolic.unary op r.2).1 with
    | error fault => simp_all [bind, Except.bind]
    | ok unitValue =>
      simp only [h, bind, Except.bind, pure, Except.pure, Except.ok.injEq] at hv
      subst v
      rfl

private def sourceBitwise : Source.Binary → Option Bitwise
  | .bitAnd => some .and | .bitOr => some .or | .bitXor => some .xor
  | _ => none

private def sourceComparison : Source.Binary → Option Comparison
  | .eq => some .eq | .ne => some .ne | .lt => some .lt
  | .le => some .le | .gt => some .gt | .ge => some .ge
  | _ => none

private def sourceArithmetic : Source.Binary → Option SignedArithmetic
  | .add => some .add | .sub => some .sub | .mul => some .mul
  | _ => none

/-- Data-only input, proof-producing implementation. Proof fields erase in
the compiled checker; successful results are justified by `sound`, not by a
finite valuation corpus. This is one component of the program-pair checker. -/
def expression {Γ : Var → Nat} (env : CBindings Γ) (e : Source.Expr) :
    Option {r : ExpressionResult Γ // ExpressionDiagram env e r} :=
  match wrap : Source.wrappingConstant e with
  | some v =>
    if typed : v.type = e.type then
      some ⟨([], CWord.constant v), .wrapped e v wrap typed⟩
    else none
  | none => match e with
    | .literal t n =>
      if canonical : n < 2^t.width then
        some ⟨([], CWord.constant (.ofNat t n)), .literal t n canonical⟩
      else none
    | .ident t id =>
      match found : env.lookup id with
      | none => none
      | some v =>
        if typed : v.type = t then
          some ⟨([], v), by simpa [typed] using ExpressionDiagram.ident id v found⟩
        else none
    | .cast t destination operand => do
      if typed : t = destination.type then
        let r ← expression env operand
        return ⟨(r.val.1, r.val.2.cast destination.type), by
          subst t
          exact .cast destination operand r.val r.property⟩
      else none
    | .unary t op operand => do
      let r ← expression env operand
      if typed : (unary op r.val.2).2.type = t then
        return ⟨(r.val.1 ++ (unary op r.val.2).1, (unary op r.val.2).2), by
          have noWrap : Source.wrappingConstant
              (.unary (unary op r.val.2).2.type op operand) = none := by
            simpa only [typed] using wrap
          simpa only [typed] using ExpressionDiagram.unary op operand r.val noWrap r.property⟩
      else none
    | .binary t op left right => do
      let a ← expression env left
      let b ← expression env right
      match sourceArithmetic op with
      | some arithmeticOp =>
        if same : Source.arithmeticSourceOp arithmeticOp = op then
          if typed : (arithmetic arithmeticOp a.val.2 b.val.2).2.type = t then
            return ⟨(a.val.1 ++ b.val.1 ++ (arithmetic arithmeticOp a.val.2 b.val.2).1,
              (arithmetic arithmeticOp a.val.2 b.val.2).2), by
                have noWrap : Source.wrappingConstant
                    (.binary (arithmetic arithmeticOp a.val.2 b.val.2).2.type
                      (Source.arithmeticSourceOp arithmeticOp) left right) = none := by
                  simpa only [typed, same] using wrap
                simpa only [typed, same] using
                  ExpressionDiagram.arithmetic arithmeticOp left right a.val b.val noWrap
                    a.property b.property⟩
          else none
        else none
      | none => match sourceBitwise op with
        | some bitOp =>
          if same : bitOp.source = op then
            if typed : (bitwise bitOp a.val.2 b.val.2).type = t then
              return ⟨(a.val.1 ++ b.val.1, bitwise bitOp a.val.2 b.val.2), by
                simpa only [typed, same] using ExpressionDiagram.bitwise bitOp left right
                  a.val b.val a.property b.property⟩
            else none
          else none
        | none => match sourceComparison op with
          | some cmpOp =>
            if same : cmpOp.source = op then
              if typed : t = .int then
                return ⟨(a.val.1 ++ b.val.1, comparison cmpOp a.val.2 b.val.2), by
                  simpa only [typed, same] using ExpressionDiagram.comparison cmpOp left right
                    a.val b.val a.property b.property⟩
              else none
            else none
          | none =>
            let toLeft := op == .shl
            if same : (if toLeft then Source.Binary.shl else .shr) = op then
              if admitted : toLeft = false ∨ a.val.2.type.signed = false then
                if typed : a.val.2.type = t then
                  return ⟨(a.val.1 ++ b.val.1 ++ (shift toLeft a.val.2 b.val.2).1,
                    (shift toLeft a.val.2 b.val.2).2), by
                      simpa only [typed, same] using ExpressionDiagram.shift toLeft left right
                        a.val b.val admitted a.property b.property⟩
                else none
              else none
            else none
    | _ => none

/-- The source tree, not the producer, supplies the expected trace/result.
The relation between entry locals and target inputs is an explicit remaining
obligation of the enclosing state/CFG check. -/
def checkExpression {Γ : Var → Nat} (env : CBindings Γ) (e : Source.Expr)
    (scope : Scope Γ) (code : List IR.Instruction) (output : Nat) : Bool :=
  match expression env e with
  | none => false
  | some r => checkFragment scope code [output] r.val.1 [r.val.2.word]

theorem expression_fragment_sound {Γ : Var → Nat} (σ : Subst Γ)
    (ρ : Nat → IR.Value) (owner : Nat) (env : CBindings Γ) (e : Source.Expr)
    (scope : Scope Γ) (code : List IR.Instruction) (output : Nat)
    (p : Source.Program) (locals : Source.Locals) (functionId : Nat)
    (related : env.Related σ locals)
    (accepted : checkExpression env e scope code output = true) :
    ∃ result, Source.Evaluates p locals functionId e result ∧
      (do let values ← concreteInstructions owner (scope.denote σ ρ) code
          [output].mapM (IR.lookup values)) = result.map (fun v => [sourceValueIR v]) := by
  unfold checkExpression at accepted
  split at accepted
  · contradiction
  · rename_i r found
    refine ⟨r.val.denote σ, r.property.sound σ p locals functionId related, ?_⟩
    rw [fragment_sound σ ρ owner scope code [output] r.val.1 [r.val.2.word] accepted]
    simp only [ExpressionResult.denote]
    cases guards σ r.val.1 <;> rfl

/-- Actual source expression and actual target-PC correspondence. The outer
checker must establish entry-local/SSA correspondence and segment membership;
neither is inferred from a proposed expression or an execution sample. -/
theorem expression_execution_sound {Γ : Var → Nat} (σ : Subst Γ)
    (ρ : Nat → IR.Value) (env : CBindings Γ) (e : Source.Expr)
    (scope : Scope Γ) (code : List IR.Instruction) (output : Nat)
    (source : Source.Program) (locals : Source.Locals) (functionId : Nat)
    (target : IR.Program) (s : IR.State) (a : IR.Activation) (f : IR.Function) (b : IR.Block)
    (related : env.Related σ locals)
    (running : s.control = .running a)
    (function : target.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (codeAt : ∀ i inst, code[i]? = some inst → b.instructions[a.pc+i]? = some inst)
    (values : a.values = scope.denote σ ρ)
    (accepted : checkExpression env e scope code output = true) :
    ∃ result finalState, Source.Evaluates source locals functionId e result ∧
      IR.Steps target s finalState ∧
      match result with
      | .error fault => finalState.control = .halted (.error fault)
      | .ok v => ∃ bindings,
          finalState = {s with control := .running {a with pc := a.pc+code.length, values := bindings}} ∧
          IR.lookup bindings output = .ok (sourceValueIR v) := by
  unfold checkExpression at accepted
  split at accepted
  · contradiction
  · rename_i r found
    have sourcePath := r.property.sound σ source locals functionId related
    unfold checkFragment at accepted
    split at accepted
    · contradiction
    · rename_i checks finalScope codeChecked
      simp only [Bool.and_eq_true, decide_eq_true_eq] at accepted
      rcases accepted with ⟨trace, result⟩
      subst checks
      obtain ⟨finalState, path, outcome⟩ := segment_execution_sound σ ρ s a f b scope
        finalScope code r.val.1 running function block codeAt values codeChecked
      refine ⟨r.val.denote σ, finalState, sourcePath, path, ?_⟩
      cases hg : guards σ r.val.1 with
      | error fault =>
        simpa [ExpressionResult.denote, hg, bind, Except.bind] using outcome
      | ok unitValue =>
        simp only [ExpressionResult.denote, hg, bind, Except.bind, pure, Except.pure]
        refine ⟨finalScope.denote σ ρ, ?_, ?_⟩
        · simpa [hg] using outcome
        · have lookup := operands_sound σ ρ finalScope [output] [r.val.2.word] result
          cases value : IR.lookup (finalScope.denote σ ρ) output with
          | error fault => simp [value, List.mapM_cons, bind, Except.bind] at lookup
          | ok v =>
            simp only [List.mapM_cons, List.mapM_nil, value, bind, Except.bind,
              pure, Except.pure, Except.ok.injEq, List.map_cons, List.map_nil,
              List.cons.injEq, and_true] at lookup
            exact congrArg Except.ok lookup

end EvoIR.Core.Symbolic
