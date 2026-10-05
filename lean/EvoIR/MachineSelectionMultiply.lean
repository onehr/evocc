import EvoIR.MachineSelectionNormal

/-! Source-guided checking of the actual SMULL/SMULH overflow idioms. The
high multiply need not be falsely represented as a low EvoIR multiply. Its
flag-producing idiom is checked structurally and justified by MachineMultiply.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def peelWord {Γ : Var → Nat} : WordExpr Γ → WordExpr Γ
  | .normalize cls input => if cls == .gpr32 then .normalize cls input else peelWord input
  | other => other

theorem peel_word_sound {Γ : Var → Nat} (σ : Subst Γ) : (expr : WordExpr Γ) →
    (peelWord expr).denote σ = expr.denote σ
  | .normalize cls input => by
    cases cls <;> simp [peelWord, WordExpr.denote, peel_word_sound σ input]
  | .input .. | .literal .. | .binary .. | .wide .. | .extend .. | .bfx ..
  | .cset .. | .csel .. => rfl

structure MulTest {Γ : Var → Nat} (lhs rhs : WordExpr Γ) where
  width : EvoIR.Machine.Width
  left : WordExpr Γ
  right : WordExpr Γ
  correct : ∀ σ,
    EvoIR.Machine.condition (.integer (lhs.denote σ) (rhs.denote σ) .x) .eq =
      .ok ((denoteOp (.guard .mul width.bits)
        ((left.denote σ).setWidth width.bits, (right.denote σ).setWidth width.bits)).getLsbD 0)

def multiplyTest {Γ : Var → Nat} (lhs rhs : WordExpr Γ) : Option (MulTest lhs rhs) :=
  match leftShape : peelWord lhs, rightShape : peelWord rhs with
  | .wide .smull x y, .bfx true .x product 0 32 =>
    if productShape : peelWord product = .wide .smull x y then
      some ⟨.w, x, y, by
        intro σ
        have lhsDenote : lhs.denote σ = EvoIR.Machine.evalWideMultiply .smull
            (x.denote σ) (y.denote σ) := by
          rw [← peel_word_sound, leftShape]
          rfl
        have rhsDenote : rhs.denote σ = EvoIR.Machine.evalBfx true .x
            (EvoIR.Machine.evalWideMultiply .smull (x.denote σ) (y.denote σ)) 0 32 := by
          rw [← peel_word_sound, rightShape]
          simp only [WordExpr.denote]
          rw [← peel_word_sound σ product, productShape]
          rfl
        rw [lhsDenote, rhsDenote]
        have law := EvoIR.Machine.smull_guard_correct
          ((x.denote σ).setWidth 32) ((y.denote σ).setWidth 32)
        simp only [EvoIR.Machine.evalWideMultiply,
          BitVec.setWidth_setWidth_of_le _ (by decide : 32 ≤ 64), BitVec.setWidth_eq] at law ⊢
        rw [law]
        simp [EvoIR.Machine.Width.bits, denoteOp, Op.res]⟩
    else none
  | .wide .smulh x y, .binary .asr .x product count =>
    if productShape : peelWord product = .binary .mul .x x y then
      if countShape : peelWord count = .literal 63 then
        some ⟨.x, x, y, by
          intro σ
          have lhsDenote : lhs.denote σ = EvoIR.Machine.evalWideMultiply .smulh
              (x.denote σ) (y.denote σ) := by
            rw [← peel_word_sound, leftShape]
            rfl
          have rhsDenote : rhs.denote σ = EvoIR.Machine.evalBinary .asr .x
              (EvoIR.Machine.evalBinary .mul .x (x.denote σ) (y.denote σ))
              (BitVec.ofNat 64 63) := by
            rw [← peel_word_sound, rightShape]
            simp only [WordExpr.denote]
            rw [← peel_word_sound σ product, productShape,
              ← peel_word_sound σ count, countShape]
            rfl
          rw [lhsDenote, rhsDenote]
          simpa only [EvoIR.Machine.Width.bits, BitVec.setWidth_eq,
            denoteOp, Op.res, BitVec.getLsbD_ofBool]
            using EvoIR.Machine.smulh_guard_correct (x.denote σ) (y.denote σ)
              |>.trans (by simp)⟩
      else none
    else none
  | _, _ => none

def checkMultiplyGuard {Γ : Var → Nat} {n : Nat} (a b : Term Γ n) (flags : FlagExpr Γ) :
    Option (PLift (∀ σ,
      EvoIR.Machine.condition (flags.denote σ) .eq =
        .ok ((denoteTerm σ (.guard .mul a b)).getLsbD 0))) := do
  match shape : flags with
  | .comparison .x lhs rhs =>
    let test ← multiplyTest lhs rhs
    if width : n = test.width.bits then
      let left ← checkWord ⟨n, a⟩ test.left
      let right ← checkWord ⟨n, b⟩ test.right
      return ⟨by
        intro σ
        have law := test.correct σ
        rw [left.down σ, right.down σ] at law
        have le : n ≤ 64 := by
          rw [width]
          exact EvoIR.Machine.Selection.width_le_64 test.width
        rw [← width] at law
        simpa only [shape, FlagExpr.denote, denoteTerm, Op.res,
          BitVec.setWidth_setWidth_of_le _ le, BitVec.setWidth_eq] using law⟩
    else none
  | _ => none

end EvoIR.Core.Symbolic.MachineSelection
