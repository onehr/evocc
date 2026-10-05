import EvoIR.Rewrite.Fact
import EvoIR.Rewrite.Certificate
import EvoIR.Rewrite.Rules

/-!
Replay of one certificate region in Lean. The replay keeps a union-find
partition of the region's classes. Each union is a rule instance with a
`RuleCovered` theorem or a congruence step. Each node of the program before
the rewrite must be the instruction at its witness position.

The result of a region is one replacement instruction for its root. The
replay accepts only an extraction that gives one new instruction at the root
position, with operands that are seeds or earlier witness results. Other
extraction forms are refused.
-/

namespace EvoIR.Rewrite

open EvoIR.Core.IR

structure RNode where
  owner : Nat
  op : Operation
  children : List Nat
  /-- The result value of a witness instruction. A derived node has none. -/
  result : Option Nat
  deriving DecidableEq, Repr

structure St where
  parent : List Nat
  sorts : List Nat
  nodes : List RNode
  witnessCount : Nat
  derived : Bool
  deriving Repr

structure Ctx where
  block : Block
  region : Region
  rules : List CoveredRule
  guards : Bool
  seedFacts : List RFact

def findF (parent : List Nat) : Nat → Nat → Nat
  | 0, c => c
  | k+1, c => match parent[c]? with
    | some p => if p = c then c else findF parent k p
    | none => c

def St.find (st : St) (c : Nat) : Nat := findF st.parent st.parent.length c

def St.sort (st : St) (c : Nat) : Nat := st.sorts.getD c 0

def St.union (st : St) (l r : Nat) : St :=
  { st with parent := st.parent.set (max l r) (min l r) }

def validUnion (st : St) (l r res : Nat) : Bool :=
  l < st.parent.length && r < st.parent.length && st.find l == l && st.find r == r &&
    l != r && res == min l r && st.sort l == st.sort r

/-! ## Node shapes -/

def scalarOp (name : String) (sort : Nat) (cs : List Nat) : Option Op :=
  let w := cs.headD 0
  let m := cs.getD 1 0
  match name with
  | "add" => some (.bin .add w) | "sub" => some (.bin .sub w) | "mul" => some (.bin .mul w)
  | "and" => some (.bin .and w) | "or" => some (.bin .or w) | "xor" => some (.bin .xor w)
  | "icmp.eq" => some (.cmp .eq w) | "icmp.ne" => some (.cmp .ne w)
  | "icmp.slt" => some (.cmp .slt w) | "icmp.sle" => some (.cmp .sle w)
  | "icmp.ult" => some (.cmp .ult w) | "icmp.ule" => some (.cmp .ule w)
  | "shl" => some (.shift .shl w m) | "lshr" => some (.shift .lshr w m)
  | "ashr" => some (.shift .ashr w m)
  | "zext" => some (.cast .zext w sort) | "sext" => some (.cast .sext w sort)
  | "trunc" => some (.cast .trunc w sort)
  | "signed_add_defined" => some (.guard .add w) | "signed_sub_defined" => some (.guard .sub w)
  | "signed_mul_defined" => some (.guard .mul w)
  | "signed_div_rem_no_overflow" => some (.guard .divRem w)
  | "divisor_nonzero" => some (.nonzero w)
  | "shift_count_defined.signed" => some (.count true w m)
  | "shift_count_defined.unsigned" => some (.count false w m)
  | _ => none

def isGuardOp : Op → Bool
  | .guard _ _ | .nonzero _ | .count _ _ _ => true
  | _ => false

/-- The operation of a node. Its child sorts must be the operator's operand
widths and its sort must be the result width. A guard node is admitted only
in `g0.ub.v1` and only from the program before the rewrite. -/
def nodeOp (guards : Bool) (nd : NodeRec) (cs : List Nat) : Option Operation :=
  if nd.op == "const" then
    match nd.value, cs with
    | some v, [] => if nd.sort != 0 && v < 2 ^ nd.sort then some (.constant nd.sort v) else none
    | _, _ => none
  else
    match scalarOp nd.op nd.sort cs with
    | some o =>
      if nd.value.isNone && opInputs o == cs && o.res == nd.sort &&
          (!isGuardOp o || (guards && nd.source == .before)) then some (.scalar o) else none
    | none => none

def insertNode (ctx : Ctx) (st : St) (nd : NodeRec) : Option St := do
  guard (nd.node == st.nodes.length && nd.owner == st.parent.length &&
    nd.children.all (· < st.parent.length))
  let op ← nodeOp ctx.guards nd (nd.children.map st.sort)
  match nd.source with
  | .derived =>
    some { st with parent := st.parent ++ [nd.owner], sorts := st.sorts ++ [nd.sort],
                   nodes := st.nodes ++ [⟨nd.owner, op, nd.children, none⟩], derived := true }
  | .before =>
    guard !st.derived
    let j ← ctx.region.witnesses[st.witnessCount]?
    let inst ← ctx.block.instructions[j]?
    let [r] := inst.results | none
    guard (inst.op == op && r.type == .bits nd.sort)
    some { st with parent := st.parent ++ [nd.owner], sorts := st.sorts ++ [nd.sort],
                   nodes := st.nodes ++ [⟨nd.owner, op, nd.children, some r.id⟩],
                   witnessCount := st.witnessCount + 1 }

/-! ## Class facts -/

def setFact (fs : List RFact) (c : Nat) (t : RFact) : Option (List RFact) := do
  let old ← fs[c]?
  let m ← old.meet t
  some (fs.set c m)

def nodeFact (st : St) (fs : List RFact) (nd : RNode) : RFact :=
  match nd.op with
  | .constant w v => constFact w v
  | .scalar o => rawTransfer o (nd.children.map fun c => fs.getD (st.find c) default)
  | _ => default

def factPass (st : St) (fs : List RFact) : Option (List RFact) :=
  st.nodes.foldlM (fun fs nd => setFact fs (st.find nd.owner) (nodeFact st fs nd)) fs

/-- The Section 8.10 fact of every class root: the meet of the seed facts
and the node transfers, for a fixed number of passes. -/
def classFacts (ctx : Ctx) (st : St) : Option (List RFact) := do
  let fs ← (ctx.seedFacts.zipIdx).foldlM (fun fs (f, i) => setFact fs (st.find i) f)
    (st.sorts.map RFact.top)
  (List.range (st.nodes.length + 1)).foldlM (fun fs _ => factPass st fs) fs

def premiseFact (st : St) (p : Premise) : RFact := ⟨st.sort p.cls, p.mask, p.value⟩

def premiseOk (st : St) (fs : List RFact) (p : Premise) : Bool :=
  let w := st.sort p.cls
  p.cls < st.parent.length && p.digits == (w + 3) / 4 && p.mask < 2 ^ w && p.value < 2 ^ w &&
    (p.value &&& p.mask) == p.value && (fs.getD (st.find p.cls) default).le (premiseFact st p)

def premiseFor (st : St) (ps : List Premise) (c : Nat) : RFact :=
  match ps.find? (fun p => st.find p.cls == st.find c) with
  | some p => premiseFact st p
  | none => default

/-! ## Rule instances -/

def bound (σ : List (String × Nat)) (v : Var) : Nat := (σ.lookup v).getD 0

def widths (st : St) (σ : List (String × Nat)) (v : Var) : Nat := st.sort (bound σ v)

def factEnv (st : St) (σ : List (String × Nat)) (ps : List Premise) : FactEnv (widths st σ) :=
  fun v => match σ.lookup v with
    | some c => (premiseFor st ps c).at (widths st σ v)
    | none => Fact.top _

/-- A valid constant node of width `w` in the class of `c`, and its value. -/
def constNode (st : St) (c w : Nat) (nd : RNode) : Option Nat :=
  match nd.op with
  | .constant w' v =>
    if w' == w && w != 0 && v < 2 ^ w && nd.children.isEmpty && st.find nd.owner == st.find c
    then some v else none
  | _ => none

def hasConst (st : St) (c w : Nat) (ok : Nat → Bool) : Bool :=
  st.nodes.any fun nd => match constNode st c w nd with
    | some v => ok v
    | none => false

def constIn (st : St) (c w : Nat) : Option Nat := st.nodes.findSome? (constNode st c w)

def hasOp (st : St) (c : Nat) (o : Op) (k : List Nat → Bool) : Bool :=
  st.nodes.any fun nd => nd.op == .scalar o && st.find nd.owner == st.find c && k nd.children

/-- The constant a fold pattern denotes, from the constants of its classes. -/
def foldConst (st : St) (σ : List (String × Nat)) : Pat → Nat → Option Nat
  | .var v, n => if widths st σ v == n then constIn st (bound σ v) n else none
  | .constLike v k, n => if widths st σ v == n then some (BitVec.ofNat n k).toNat else none
  | .bin o _ a b, n => do
    let x ← foldConst st σ a n
    let y ← foldConst st σ b n
    some (denoteOp (.bin o n) (BitVec.ofNat n x, BitVec.ofNat n y)).toNat
  | .cmp o _ a b, 1 => match a.width (widths st σ) with
    | some w => do
      let x ← foldConst st σ a w
      let y ← foldConst st σ b w
      some (denoteOp (.cmp o w) (BitVec.ofNat w x, BitVec.ofNat w y)).toNat
    | none => none
  | _, _ => none

def two (k : Nat → Nat → Bool) : List Nat → Bool
  | [a, b] => k a b
  | _ => false

def one (k : Nat → Bool) : List Nat → Bool
  | [a] => k a
  | _ => false

/-- The class `c` holds a value the pattern denotes at width `n`. -/
def matchPat (st : St) (σ : List (String × Nat)) : Pat → Nat → Nat → Bool
  | .var v, n, c => (σ.lookup v).isSome && bound σ v < st.sorts.length &&
      st.find (bound σ v) == st.find c && widths st σ v == n
  | .constLike v k, n, c => widths st σ v == n &&
      hasConst st c n (fun x => BitVec.ofNat n x == BitVec.ofNat n k)
  | .boolLit k, n, c => n == 1 && hasConst st c 1 (fun x => BitVec.ofNat 1 x == BitVec.ofNat 1 k)
  | .bin o f a b, n, c =>
    hasOp st c (.bin o n) (two fun x y => matchPat st σ a n x && matchPat st σ b n y) ||
      (f && match foldConst st σ (.bin o f a b) n with
        | some x => hasConst st c n (fun y => BitVec.ofNat n y == BitVec.ofNat n x)
        | none => false)
  | .cmp o f a b, n, c => n == 1 && match a.width (widths st σ) with
    | some w =>
      hasOp st c (.cmp o w) (two fun x y => matchPat st σ a w x && matchPat st σ b w y) ||
        (f && match foldConst st σ (.cmp o f a b) 1 with
          | some x => hasConst st c 1 (fun y => BitVec.ofNat 1 y == BitVec.ofNat 1 x)
          | none => false)
    | none => false
  | .shift o a k, n, c => match k.width (widths st σ) with
    | some m => hasOp st c (.shift o n m) (two fun x y => matchPat st σ a n x && matchPat st σ k m y)
    | none => false
  | .cast o a, n, c => match a.width (widths st σ) with
    | some w => hasOp st c (.cast o w n) (one fun x => matchPat st σ a w x)
    | none => false
  | .guard o a b, n, c => n == 1 && match a.width (widths st σ) with
    | some w => hasOp st c (.guard o w) (two fun x y => matchPat st σ a w x && matchPat st σ b w y)
    | none => false
  | .nonzero a, n, c => n == 1 && match a.width (widths st σ) with
    | some w => hasOp st c (.nonzero w) (one fun x => matchPat st σ a w x)
    | none => false
  | .count s a k, n, c => n == 1 && match a.width (widths st σ), k.width (widths st σ) with
    | some w, some m =>
      hasOp st c (.count s w m) (two fun x y => matchPat st σ a w x && matchPat st σ k m y)
    | _, _ => false

def condCheck (Γ : Var → Nat) (F : FactEnv Γ) {out : Nat} (R : Fact out) : Cond → Bool
  | .tt => true
  | .allConst => true
  | .fullyKnown => decide (fullyKnown R)
  | .covers m x => if h : Γ m = Γ x then decide (covers (h ▸ F m) (F x)) else true
  | .knownZero z => decide (knownZero (F z))
  | .widthMin v k => decide (k ≤ Γ v)

def rhsOk (st : St) (σ : List (String × Nat)) (rhs : Rhs) (out c : Nat) (R : Fact out) : Bool :=
  match rhs with
  | .pat p => matchPat st σ p out c
  | .knownConst => hasConst st c out (fun v => BitVec.ofNat out v == R.value)

def refOk (st : St) : ProofRef → Nat → Bool
  | .node k, c => match st.nodes[k]? with
    | some nd => st.find nd.owner == c
    | none => false
  | .cls k, c => k < st.parent.length && st.find k == c

def rewriteStep (ctx : Ctx) (st : St) (rw : RewriteRec) : Option St := do
  guard (validUnion st rw.lhs rw.rhs rw.result && refOk st rw.lhsRef rw.lhs &&
    refOk st rw.rhsRef rw.rhs)
  let r ← ctx.rules.find? (·.1.id == rw.rule)
  let fs ← classFacts ctx st
  guard (rw.premises.all (premiseOk st fs) && rw.bindings.all (·.2 < st.sorts.length))
  let σ := rw.bindings
  let out := st.sort rw.lhs
  let R := (premiseFor st rw.premises rw.lhs).at out
  guard (matchPat st σ r.1.lhs out rw.lhs)
  guard (rhsOk st σ r.1.rhs out rw.rhs R)
  guard (condCheck (widths st σ) (factEnv st σ rw.premises) R r.1.cond)
  some (st.union rw.lhs rw.rhs)

def congruenceStep (st : St) (cg : CongruenceRec) : Option St := do
  guard (validUnion st cg.lhs cg.rhs cg.result)
  let a ← st.nodes[cg.left]?
  let b ← st.nodes[cg.right]?
  guard (st.find a.owner == cg.lhs && st.find b.owner == cg.rhs && a.op == b.op &&
    a.children.length == b.children.length &&
    (a.children.zip b.children).all (fun (x, y) => st.find x == st.find y))
  some (st.union cg.lhs cg.rhs)

def step (ctx : Ctx) (st : St) : Event → Option St
  | .node nd => insertNode ctx st nd
  | .rewrite rw => rewriteStep ctx st rw
  | .congruence cg => congruenceStep st cg

def replay (ctx : Ctx) : St → List Event → Option St
  | st, [] => some st
  | st, e :: es => do replay ctx (← step ctx st e) es

/-! ## Region start and extraction -/

def start (region : Region) (block : Block) : Option St := do
  guard ((region.seeds.zipIdx.all fun (s, i) => s.cls == i) &&
    region.seeds.all fun s =>
      block.parameters.any fun p => p.id == s.value && p.type == .bits s.width)
  some { parent := List.range region.seeds.length, sorts := region.seeds.map (·.width),
         nodes := [], witnessCount := 0, derived := false }

/-- The final node list has the shape the semantics needs: node `k` owns
class `S + k`, its children are earlier classes, its operation types at the
sorts of its children, and the witness nodes come first. -/
def shapeOk (S : Nat) (st : St) : Bool :=
  st.parent.length == S + st.nodes.length && st.sorts.length == st.parent.length &&
  st.nodes.zipIdx.all fun (nd, k) =>
    nd.owner == S + k && nd.children.all (· < S + k) && (k < st.witnessCount) == nd.result.isSome &&
    match nd.op with
    | .constant w v => nd.children.isEmpty && w != 0 && v < 2 ^ w && st.sort (S + k) == w
    | .scalar o => opInputs o == nd.children.map st.sort && st.sort (S + k) == o.res
    | _ => false

/-- The class of an operand of witness `k`: a seed, or an earlier witness. -/
def operandClass (ctx : Ctx) (st : St) (k v : Nat) : Option Nat :=
  match ctx.region.seeds.findIdx? (·.value == v) with
  | some i => some i
  | none => ((List.range k).find? fun k' => (st.nodes[k']?.bind (·.result)) == some v).map
      (ctx.region.seeds.length + ·)

/-- Witness node `k` is the instruction at witness position `k`, and each
child has one root with the class of the operand it stands for. -/
def witnessOk (ctx : Ctx) (st : St) : Bool :=
  let ids := ctx.region.seeds.map (·.value) ++ st.nodes.filterMap (·.result)
  ids.eraseDups == ids &&
  (List.range ctx.region.witnesses.length).all fun k =>
    match ctx.region.witnesses[k]?.bind (ctx.block.instructions[·]?), st.nodes[k]? with
    | some inst, some nd =>
      inst.op == nd.op && inst.results.map (·.id) == nd.result.toList &&
      inst.operands.length == nd.children.length &&
      (inst.operands.zip nd.children).all fun (v, c) => match operandClass ctx st k v with
        | some oc => st.find oc == st.find c
        | none => false
    | _, _ => false

/-- The value that the extraction selects for class `c`: a seed or an
earlier witness result. The root node itself is not a selectable value. -/
def existing (ctx : Ctx) (st : St) (rootNode c : Nat) : Option Nat := do
  let (_, ch) ← ctx.region.extracts.find? (·.1 == c)
  match ch with
  | .seed i =>
    let s ← ctx.region.seeds[i]?
    guard (st.find i == c)
    some s.value
  | .node k =>
    let nd ← st.nodes[k]?
    guard (k != rootNode && st.find nd.owner == c)
    nd.result

/-- The replacement operation and its operand values. -/
def finish (ctx : Ctx) (st : St) : Option (Operation × List Nat) := do
  let w := ctx.region.witnesses
  guard (st.witnessCount == w.length && w.getLast? == some ctx.region.index &&
    w.dropLast.all (· < ctx.region.index) && shapeOk ctx.region.seeds.length st &&
    witnessOk ctx st)
  let rootNode := w.length - 1
  let rn ← st.nodes[rootNode]?
  let rc := ctx.region.rootClass
  guard (rn.result == some ctx.region.root && st.find rn.owner == rc && st.find rc == rc)
  let (_, .node k) ← ctx.region.extracts.find? (·.1 == rc) | none
  let nd ← st.nodes[k]?
  guard (k != rootNode && nd.result.isNone && st.find nd.owner == rc)
  let operands ← nd.children.mapM (fun ch => existing ctx st rootNode (st.find ch))
  guard (ctx.region.extracts.all fun (c, _) => c == rc || nd.children.any (st.find · == c))
  guard (!ctx.guards || (nd.op == .constant 1 1 && ctx.region.extracts.length == 1))
  some (nd.op, operands)

end EvoIR.Rewrite
