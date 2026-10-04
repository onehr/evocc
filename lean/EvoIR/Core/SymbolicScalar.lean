import EvoIR.Core.IR
import EvoIR.Term

/-!
Symbolic scalar evaluation for the independent local-diagram checker. Reuse
the existing width-indexed terms and their single `denoteOp` authority. The
environment is an arbitrary valuation, not a set of sampled arguments.

This component admits initialized integer values only. It cannot assert that
an actual SSA value is initialized, erase an assumption, or certify a CFG;
those obligations stay with the enclosing source/IR checker.
-/
namespace EvoIR.Core.Symbolic

deriving instance DecidableEq for Term

abbrev Word (Γ : Var → Nat) := (width : Nat) × Term Γ width

def Word.denote {Γ : Var → Nat} (σ : Subst Γ) (w : Word Γ) : IR.Value :=
  .bits w.1 (denoteTerm σ w.2)

def Word.at {Γ : Var → Nat} (w : Word Γ) (n : Nat) : Option (Term Γ n) :=
  if h : w.1 = n then some (h ▸ w.2) else none

/-- Syntactic equality is a sufficient, deliberately incomplete check. Widths
and variable identities are checked, never only a rendering or digest. -/
def sameWord {Γ : Var → Nat} (a b : Word Γ) : Bool := decide (a = b)

theorem same_word_sound {Γ : Var → Nat} (a b : Word Γ)
    (accepted : sameWord a b = true) (σ : Subst Γ) : a.denote σ = b.denote σ := by
  have same : a = b := by simpa [sameWord] using accepted
  exact congrArg (Word.denote σ) same

theorem word_at_sound {Γ : Var → Nat} (σ : Subst Γ) (w : Word Γ)
    (n : Nat) (term : Term Γ n) (accepted : w.at n = some term) :
    (w.denote σ).asBits n = .ok (denoteTerm σ term) := by
  rcases w with ⟨width, body⟩
  simp only [Word.at] at accepted
  split at accepted
  · rename_i same
    subst width
    simp only [Option.some.injEq] at accepted
    subst term
    simp [Word.denote, IR.Value.asBits]
  · contradiction

def scalar {Γ : Var → Nat} : Op → List (Word Γ) → Option (Word Γ)
  | .bin op n, [a, b] => do return ⟨n, .bin op (← a.at n) (← b.at n)⟩
  | .cmp op n, [a, b] => do return ⟨1, .cmp op (← a.at n) (← b.at n)⟩
  | .shift op n m, [a, b] => do return ⟨n, .shift op (← a.at n) (← b.at m)⟩
  | .cast op n m, [a] => do return ⟨m, .cast op m (← a.at n)⟩
  | .guard op n, [a, b] => do return ⟨1, .guard op (← a.at n) (← b.at n)⟩
  | .nonzero n, [a] => do return ⟨1, .nonzero (← a.at n)⟩
  | .count signed n m, [a, b] => do return ⟨1, .count signed (← a.at n) (← b.at m)⟩
  | _, _ => none

/-- Every accepted symbolic operator commutes with actual EvoIR execution for
every valuation, including independent shift widths. Refused shapes grant no
fact. Runtime guards remain terms rather than assumptions of defined C input. -/
theorem scalar_sound {Γ : Var → Nat} (σ : Subst Γ) (op : Op)
    (args : List (Word Γ)) (result : Word Γ)
    (accepted : scalar op args = some result) :
    IR.evalScalar op (args.map (Word.denote σ)) = .ok (result.denote σ) := by
  cases op <;> cases args with
  | nil => simp [scalar] at accepted
  | cons a rest =>
    cases rest with
    | nil =>
      simp only [scalar, bind, Option.bind, pure] at accepted
      all_goals first | contradiction | skip
      all_goals split at accepted
      all_goals first | contradiction | skip
      all_goals rename_i term hterm
      all_goals simp only [Option.some.injEq] at accepted
      all_goals subst result
      all_goals simp only [List.map_cons, List.map_nil, IR.evalScalar,
        word_at_sound σ _ _ _ hterm]
      all_goals rfl
    | cons b rest =>
      cases rest with
      | cons c rest => simp [scalar] at accepted
      | nil =>
        simp only [scalar, bind, Option.bind, pure] at accepted
        all_goals first | contradiction | skip
        all_goals split at accepted
        all_goals first | contradiction | skip
        all_goals rename_i left hleft
        all_goals dsimp only at accepted
        all_goals split at accepted
        all_goals first | contradiction | skip
        all_goals rename_i right hright
        all_goals simp only [Option.some.injEq] at accepted
        all_goals subst result
        all_goals simp only [List.map_cons, List.map_nil, IR.evalScalar,
          word_at_sound σ _ _ _ hleft, word_at_sound σ _ _ _ hright]
        all_goals rfl

end EvoIR.Core.Symbolic
