import EvoIR.Term
import EvoIR.Fact

/-!
# Registry rules and the fixed soundness statement

A rule is data: the two sides as patterns and the side condition, exactly the
three fields the registry text of Section 11.6 prints.  `Registry.lean` is
generated from that text and holds nothing but values of this type.

`RuleSound` is written once, here, and every rule theorem is an instance of
it.  A generator that produced propositions could choose what "correct" means;
this way it can only choose which rule is being talked about.  It does not
make the decoding faithful: a decoder that read a rule as `lhs = lhs` would be
correctly proved sound.  The text-to-data correspondence rests on the
generator's round trip and on `Rule.render`, not on this definition.
-/

namespace EvoIR

/-- A registry pattern.  `fold` marks the constant-folding builders
`fold.add`, `fold.icmp.eq` and their kin, whose meaning under Section 11.6 is
the operator's own semantics. -/
inductive Pat where
  /-- `?x` -/
  | var (name : Var)
  /-- `const_like(?x,K)`: a constant of `?x`'s sort with raw value `K mod 2^N`. -/
  | constLike (name : Var) (k : Nat)
  /-- `bool(K)`: a `const.bits<1>`. -/
  | boolLit (k : Nat)
  | bin (o : BinOp) (isFold : Bool) (a b : Pat)
  | cmp (o : CmpOp) (isFold : Bool) (a b : Pat)
  | shift (o : ShOp) (a k : Pat)
  | cast (o : CastOp) (a : Pat)
  | guard (o : GuardOp) (a b : Pat)
  | nonzero (a : Pat)
  | count (isSigned : Bool) (a k : Pat)
  deriving DecidableEq, Repr, Inhabited

/-- A right-hand side: a pattern, or the `known_const` builder, which reads
its payload from the fact of the matched left-hand class. -/
inductive Rhs where
  | pat (p : Pat)
  | knownConst
  deriving DecidableEq, Repr, Inhabited

/-- The six side conditions the registry uses. -/
inductive Cond where
  | tt
  | allConst
  | fullyKnown
  | covers (m x : Var)
  | knownZero (z : Var)
  | widthMin (v : Var) (k : Nat)
  deriving DecidableEq, Repr, Inhabited

structure Rule where
  id : String
  lhs : Pat
  rhs : Rhs
  cond : Cond
  deriving DecidableEq, Repr, Inhabited

/-! ## Typing -/

/-- The width a pattern fixes on its own, for the operand positions a rule's
result width does not determine: a shift's amount and a comparison's operands.
A width change's target is not determined by its pattern; it is the rule's
result width. -/
def Pat.width (Γ : Var → Nat) : Pat → Option Nat
  | .var v => some (Γ v)
  | .constLike v _ => some (Γ v)
  | .boolLit _ => some 1
  | .bin _ _ a _ => a.width Γ
  | .cmp _ _ _ _ => some 1
  | .shift _ a _ => a.width Γ
  | .cast _ _ => none
  | .guard _ _ _ => some 1
  | .nonzero _ => some 1
  | .count _ _ _ => some 1

/-- Elaboration is the well-typedness judgement: a pattern types at width `n`
under `Γ` exactly when this returns a term. -/
def elabPat (Γ : Var → Nat) : (n : Nat) → Pat → Option (Term Γ n)
  | n, .var v => if h : Γ v = n then some (.var v n h) else none
  | n, .constLike v k => if Γ v = n then some (.const n (BitVec.ofNat n k)) else none
  | 1, .boolLit k => some (.const 1 (BitVec.ofNat 1 k))
  | 0, .boolLit _ => none
  | _ + 2, .boolLit _ => none
  | n, .bin o _ a b =>
      match elabPat Γ n a, elabPat Γ n b with
      | some ta, some tb => some (.bin o ta tb)
      | _, _ => none
  | 1, .cmp o _ a b =>
      match a.width Γ with
      | some w =>
          match elabPat Γ w a, elabPat Γ w b with
          | some ta, some tb => some (.cmp o ta tb)
          | _, _ => none
      | none => none
  | 0, .cmp _ _ _ _ => none
  | _ + 2, .cmp _ _ _ _ => none
  | n, .shift o a k =>
      match k.width Γ with
      | some m =>
          match elabPat Γ n a, elabPat Γ m k with
          | some ta, some tk => some (.shift o ta tk)
          | _, _ => none
      | none => none
  | n, .cast o a =>
      match a.width Γ with
      | some w =>
          match elabPat Γ w a with
          | some ta => some (.cast o n ta)
          | none => none
      | none => none
  | 1, .guard o a b =>
      match a.width Γ with
      | some w =>
          match elabPat Γ w a, elabPat Γ w b with
          | some ta, some tb => some (.guard o ta tb)
          | _, _ => none
      | none => none
  | 0, .guard _ _ _ => none
  | _ + 2, .guard _ _ _ => none
  | 1, .nonzero a =>
      match a.width Γ with
      | some w => (elabPat Γ w a).map Term.nonzero
      | none => none
  | 0, .nonzero _ => none
  | _ + 2, .nonzero _ => none
  | 1, .count s a k =>
      match a.width Γ, k.width Γ with
      | some w, some m =>
          match elabPat Γ w a, elabPat Γ m k with
          | some ta, some tk => some (.count s ta tk)
          | _, _ => none
      | _, _ => none
  | 0, .count _ _ _ => none
  | _ + 2, .count _ _ _ => none

/-- The `fold.` marker is rendering, not meaning: Section 11.6 gives the fold
builders the operator's own semantics, so the two sides of a fold rule
elaborate to the same term. -/
theorem elabPat_bin_fold {Γ : Var → Nat} {n : Nat} (o : BinOp) (a b : Pat) :
    elabPat Γ n (.bin o true a b) = elabPat Γ n (.bin o false a b) := rfl

theorem elabPat_cmp_fold {Γ : Var → Nat} {n : Nat} (o : CmpOp) (a b : Pat) :
    elabPat Γ n (.cmp o true a b) = elabPat Γ n (.cmp o false a b) := by
  match n with
  | 0 => rfl
  | 1 => rfl
  | _ + 2 => rfl

/-- The value a pattern denotes at width `n`, or nothing when it does not type
there. -/
def denotePat (Γ : Var → Nat) (σ : Subst Γ) (n : Nat) (p : Pat) : Option (BitVec n) :=
  (elabPat Γ n p).map (denoteTerm σ)

/-- The value a right-hand side denotes.  `known_const` reads the result
class's fact, which is a premise of the rule and not something the rule
computes. -/
def denoteRhs (Γ : Var → Nat) (σ : Subst Γ) {out : Nat} (R : Fact out) : Rhs → Option (BitVec out)
  | .pat p => denotePat Γ σ out p
  | .knownConst => some R.value

/-- A valuation of the pattern variables' facts, each at its own width. -/
abbrev FactEnv (Γ : Var → Nat) := (v : Var) → Fact (Γ v)

/-- The side condition, read off the variables' facts, the result class's fact
`R`, and the widths.

`all_const` constrains the matcher, not the values: it says the operand
classes hold constant nodes.  Whatever those constants are, the substitution
already gives them, so it adds nothing here and the `fold.*` right-hand side
is the operator's own semantics.

`R` stays an independent premise.  It is not forced to be the transfer of the
operand facts, because the result class's fact may have been merged in from
another proved-equal term; and it is the fact *before* the merge this rule
justifies, never one read back out of the class the rule is about. -/
def CondHolds (Γ : Var → Nat) (_σ : Subst Γ) (F : FactEnv Γ) {out : Nat} (R : Fact out) :
    Cond → Prop
  | .tt => True
  | .allConst => True
  | .fullyKnown => fullyKnown R
  | .covers m x => coversAt (F m) (F x)
  | .knownZero z => knownZero (F z)
  | .widthMin v k => k ≤ Γ v

/-- The fixed correctness statement of a rewrite rule, at one width
environment and one result width.

The first premise is the rule's own well-typedness together with the value it
matched; the second says every variable's fact holds; the third that the
result class's fact holds of the matched value; the fourth is the side
condition. -/
def RuleSound (Γ : Var → Nat) (out : Nat) (r : Rule) : Prop :=
  ∀ (σ : Subst Γ) (F : FactEnv Γ) (R : Fact out) (lhs : BitVec out),
    denotePat Γ σ out r.lhs = some lhs →
    (∀ v, gamma (F v) (σ v)) →
    gamma R lhs →
    CondHolds Γ σ F R r.cond →
    denoteRhs Γ σ R r.rhs = some lhs

/-- Every width environment and every result width: the whole instance space
Section 8.5 admits, and more. -/
def RuleCovered (r : Rule) : Prop := ∀ (Γ : Var → Nat) (out : Nat), RuleSound Γ out r

/-! ## Rendering back to registry text -/

def Pat.render : Pat → String
  | .var v => "?" ++ v
  | .constLike v k => "const_like(?" ++ v ++ "," ++ toString k ++ ")"
  | .boolLit k => "bool(" ++ toString k ++ ")"
  | .bin o f a b =>
      (if f then "fold." else "") ++ o.name ++ "(" ++ a.render ++ "," ++ b.render ++ ")"
  | .cmp o f a b =>
      (if f then "fold." else "") ++ o.name ++ "(" ++ a.render ++ "," ++ b.render ++ ")"
  | .shift o a k => o.name ++ "(" ++ a.render ++ "," ++ k.render ++ ")"
  | .cast o a => o.name ++ "(" ++ a.render ++ ")"
  | .guard o a b => o.name ++ "(" ++ a.render ++ "," ++ b.render ++ ")"
  | .nonzero a => "divisor_nonzero(" ++ a.render ++ ")"
  | .count s a k => "shift_count_defined." ++ (if s then "signed" else "unsigned") ++
      "(" ++ a.render ++ "," ++ k.render ++ ")"

def Rhs.render : Rhs → String
  | .pat p => p.render
  | .knownConst => "known_const"

def Cond.render : Cond → String
  | .tt => "true"
  | .allConst => "all_const"
  | .fullyKnown => "fully_known"
  | .covers m x => "covers(?" ++ m ++ ",?" ++ x ++ ")"
  | .knownZero z => "known_zero(?" ++ z ++ ")"
  | .widthMin v k => "width_min(?" ++ v ++ "," ++ toString k ++ ")"

/-- The canonical registry line for a rule.  The generator renders the same
line from the parsed data in Python and compares it with the specification
block byte for byte; this is the same rendering, done from the Lean values the
proofs are about. -/
def Rule.render (r : Rule) : String :=
  r.id ++ " " ++ r.lhs.render ++ " => " ++ r.rhs.render ++ " when " ++ r.cond.render

end EvoIR
