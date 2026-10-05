import EvoIR.Rewrite.Replay
import EvoIR.Rewrite.Flow
import EvoIR.Core.IRReader

/-!
The rewrite checker. It takes the EvoIR program before the rewrite, the
program after the rewrite and the certificate. It replays every region,
builds the program that Section 8.8 reassembles, and compares that program
with the given program after the rewrite. Any difference outside the replaced
roots, a deleted obligation included, makes the comparison false.
-/

namespace EvoIR.Rewrite

open EvoIR.Core.IR

structure Root where
  function : String
  block : String
  index : Nat
  inst : Instruction
  deriving DecidableEq, Repr

def rootAt (roots : List Root) (f b : String) (j : Nat) : Option Instruction :=
  (roots.find? fun r => r.function == f && r.block == b && r.index == j).map (·.inst)

def replaceBlock (roots : List Root) (f : String) (b : Block) : Block :=
  { b with instructions := b.instructions.mapIdx fun j inst => (rootAt roots f b.name j).getD inst }

def replaceFunction (roots : List Root) (f : Function) : Function :=
  { f with blocks := f.blocks.map (replaceBlock roots f.name) }

def replaceAll (p : Program) (roots : List Root) : Program :=
  ⟨p.functions.map (replaceFunction roots)⟩

def isPure : Operation → Bool
  | .constant _ _ | .scalar _ => true
  | _ => false

def seedFacts (T : Table) (f : String) (b : Block) (region : Region) : List RFact :=
  region.seeds.map fun s => match b.parameters.findIdx? (·.id == s.value) with
    | some k => T.param f b.name k
    | none => default

/-- Section 8.5: no seed and no witness result is poison or excluded, and
every seed is an operand of a witness instruction. The root of a guard fold
is a guard, so only its operands are checked. -/
def cleanRegion (bad : List Nat) (b : Block) (region : Region) (guards : Bool) : Bool :=
  let insts := region.witnesses.filterMap (b.instructions[·]?)
  insts.length == region.witnesses.length &&
  region.seeds.all (fun s => !bad.contains s.value && insts.any (·.operands.contains s.value)) &&
  (insts.dropLast.all fun i => i.results.all fun r => !bad.contains r.id) &&
  (guards || insts.getLast?.all fun i => i.results.all fun r => !bad.contains r.id)

def valueCount (f : Function) : Nat :=
  f.blocks.foldl (fun n b => n + b.parameters.length +
    b.instructions.foldl (fun n i => n + i.results.length) 0) 0

def regionRoot (before : Program) (T : Table) (rules : List CoveredRule) (guards : Bool)
    (region : Region) : Option Root := do
  let f ← before.functions.find? (·.name == region.function)
  let b ← f.blocks.find? (·.name == region.block)
  let old ← b.instructions[region.index]?
  guard (cleanRegion (badValues f (valueCount f + 1) []) b region guards)
  let ctx : Ctx := ⟨b, region, rules, guards, seedFacts T f.name b region⟩
  let st ← replay ctx (← start region b) region.events
  let (op, operands) ← finish ctx st
  guard (isPure op && isPure old.op)
  some ⟨f.name, b.name, region.index, ⟨old.results, op, operands⟩⟩

def paramCount (p : Program) : Nat :=
  p.functions.foldl (fun n f => f.blocks.foldl (fun n b => n + b.parameters.length) n) 0

/-- The table, and the replacement of every region. -/
def rewriteRoots (before : Program) (cert : Certificate) : Option (Table × List Root) := do
  let rules ← registryRules cert.registry cert.digest
  let T := propose before (65 * (paramCount before + 1)) (entryTable before)
  guard (tableOk before T)
  let roots ← cert.regions.mapM (regionRoot before T rules (isGuardRegistry cert.registry))
  let keys := roots.map fun r => (r.function, r.block, r.index)
  guard (keys.eraseDups == keys)
  some (T, roots)

def checkRewrite (before after : Program) (cert : Certificate) : Bool :=
  match rewriteRoots before cert with
  | some (_, roots) => after == replaceAll before roots
  | none => false

/-- The checker on the three texts: `before.evo`, `after.evo` and `proof.evp`. -/
def checkRewriteText (before after cert : String) : Except String Bool := do
  let b ← Reader.readProgram before
  let a ← Reader.readProgram after
  let c ← Rewrite.Reader.certificate cert
  return checkRewrite b a c

end EvoIR.Rewrite
