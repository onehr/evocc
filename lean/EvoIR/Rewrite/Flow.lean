import EvoIR.Rewrite.Fact

/-!
Block-argument facts and the poison and exclusion fixed point of Section 8.5.

A table gives one fact for each block parameter. `tableOk` checks that the
table is inductive: each entry block has no known bit, and on every edge the
fact of each passed value is at or below the fact of the parameter. Only this
check carries a proof obligation. `propose` computes the table by the join
fixed point of Section 8.10; its result is not trusted.
-/

namespace EvoIR.Rewrite

open EvoIR.Core.IR

abbrev Table := List ((String × String) × List RFact)

def Table.param (T : Table) (f b : String) (k : Nat) : RFact :=
  ((T.lookup (f, b)).bind (·[k]?)).getD default

/-- The fact of a pure instruction result, from the facts of its operands. -/
def instFact (env : List (Nat × RFact)) (inst : Instruction) : Option (Nat × RFact) :=
  match inst.op, inst.results with
  | .constant w v, [r] => some (r.id, constFact w v)
  | .scalar o, [r] => some (r.id, rawTransfer o (inst.operands.map fun id => (env.lookup id).getD default))
  | _, _ => none

def addFacts (env : List (Nat × RFact)) : List Instruction → List (Nat × RFact)
  | [] => env
  | inst :: rest => addFacts ((instFact env inst).elim env (· :: env)) rest

def paramFacts (T : Table) (f : String) (b : Block) : List (Nat × RFact) :=
  b.parameters.zipIdx.map fun (p, k) => (p.id, T.param f b.name k)

/-- The facts of the values of block `b` after its first `k` instructions. -/
def blockEnv (T : Table) (f : String) (b : Block) (k : Nat) : List (Nat × RFact) :=
  addFacts (paramFacts T f b) (b.instructions.take k)

def edges : Terminator → List Edge
  | .branch e => [e]
  | .conditional _ yes no => [yes, no]
  | .return _ => []

def passed (T : Table) (f : String) (b : Block) (e : Edge) : List RFact :=
  let env := blockEnv T f b b.instructions.length
  e.arguments.map fun id => (env.lookup id).getD default

def edgeOk (T : Table) (f : Function) (b : Block) (e : Edge) : Bool :=
  match f.blocks.find? (·.name == e.block) with
  | none => true
  | some tb => (passed T f.name b e).zipIdx.all fun (a, k) => a.le (T.param f.name tb.name k)

def entryOk (T : Table) (f : Function) : Bool :=
  match f.blocks.head? with
  | none => true
  | some b => (List.range b.parameters.length).all fun k => (T.param f.name b.name k).mask == 0

def tableOk (p : Program) (T : Table) : Bool :=
  p.functions.all fun f => entryOk T f && f.blocks.all fun b => (edges b.terminator).all (edgeOk T f b)

/-! ## The proposed table -/

def joinInto (T : Table) (key : String × String) (fs : List RFact) : Table :=
  match T.lookup key with
  | none => (key, fs) :: T
  | some old =>
    let joined := (old.zip fs).map fun (a, b) => a.join b
    (key, joined) :: T.filter (·.1 != key)

def proposePass (p : Program) (T : Table) : Table :=
  p.functions.foldl (fun T f => f.blocks.foldl (fun T b =>
    if (T.lookup (f.name, b.name)).isNone && f.blocks.head?.map (·.name) != some b.name then T
    else (edges b.terminator).foldl (fun T e =>
      match f.blocks.find? (·.name == e.block) with
      | some tb => joinInto T (f.name, tb.name) (passed T f.name b e)
      | none => T) T) T) T

def entryTable (p : Program) : Table :=
  p.functions.filterMap fun f => f.blocks.head?.map fun b =>
    ((f.name, b.name), b.parameters.map fun prm => match prm.type with
      | .bits n => RFact.top n
      | _ => default)

def propose (p : Program) : Nat → Table → Table
  | 0, T => T
  | k+1, T =>
    let T' := proposePass p T
    if T' == T then T else propose p k T'

/-! ## Poison and exclusion

A value is bad when it is the result of `uninitialized` or of an instruction
outside the operator allowlist, when it is not a bit vector of width 1 to 64,
when an operand of its instruction is bad, or when it is a block argument
that receives a bad value on any edge. This is the union of the two monotone
fixed points of Section 8.5. -/

def allowed : Operation → Bool
  | .constant w _ => 1 ≤ w && w ≤ 64
  | .scalar (.bin _ n) | .scalar (.cmp _ n) => 1 ≤ n && n ≤ 64
  | .scalar (.shift _ n m) => 1 ≤ n && n ≤ 64 && 1 ≤ m && m ≤ 64
  | .scalar (.cast _ w n) => 1 ≤ w && w ≤ 64 && 1 ≤ n && n ≤ 64
  | _ => false

def scalarBinding (b : Binding) : Bool :=
  match b.type with
  | .bits n => 1 ≤ n && n ≤ 64
  | _ => false

def badPass (f : Function) (bad : List Nat) : List Nat :=
  f.blocks.foldl (fun bad b =>
    let bad := b.parameters.foldl (fun bad prm => if scalarBinding prm then bad else prm.id :: bad) bad
    let bad := b.instructions.foldl (fun bad inst =>
      if !allowed inst.op || inst.operands.any bad.contains then
        inst.results.map (·.id) ++ bad else bad) bad
    (edges b.terminator).foldl (fun bad e =>
      match f.blocks.find? (·.name == e.block) with
      | some tb => (e.arguments.zip tb.parameters).foldl (fun bad (a, prm) =>
          if bad.contains a then prm.id :: bad else bad) bad
      | none => bad) bad) bad

def badValues (f : Function) : Nat → List Nat → List Nat
  | 0, bad => bad
  | k+1, bad =>
    let next := (badPass f bad).eraseDups
    if next.length == bad.length then bad else badValues f k next

end EvoIR.Rewrite
