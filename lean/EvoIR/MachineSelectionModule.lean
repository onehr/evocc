import EvoIR.MachineSelectionCaller

/-! Production bottom-up selection over the actual pair. Original block
indices are only an untrusted matching proposal: every admitted table checks
actual lookups, instructions, guards, full call vectors and edges. Each pass
retains only proof-bearing functions; no unresolved callee is an assumption.
The pass bound concerns finite certificate construction, not execution. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def tokenBlockProposal (function : IR.Function) : List (String × Nat) :=
  function.blocks.mapIdx fun index block => (block.name, index)

def tokenResultType (function : IR.Function) : Option Source.IntType :=
  match function.returns with
  | [.bits 32, .memory, .world] => some .uint
  | [.bits 64, .memory, .world] => some .ulongLong
  | _ => none

private def checkTokenFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (function : IR.Function) (callees : List (TokenCallee source target)) :
    Option (TokenCallee source target) := do
  let type ← tokenResultType function
  let checked ← tokenCallerFunction source target function.name type (tokenBlockProposal function) callees
  return ⟨function.name, type, checked⟩

private def tokenModulePass (source : IR.Program) (target : EvoIR.Machine.Program) :
    List IR.Function → List (TokenCallee source target) → List (TokenCallee source target)
  | [], callees => callees
  | function :: rest, callees =>
    let next := if callees.any (fun previous => previous.name == function.name) then callees else
      match checkTokenFunction source target function callees with
      | none => callees
      | some checked => checked :: callees
    tokenModulePass source target rest next

private def tokenModulePasses (source : IR.Program) (target : EvoIR.Machine.Program) :
    Nat → List (TokenCallee source target) → List (TokenCallee source target)
  | 0, callees => callees
  | passes+1, callees => tokenModulePasses source target passes (tokenModulePass source target source.functions callees)

structure TokenModuleCheck (source : IR.Program) (target : EvoIR.Machine.Program) where
  callees : List (TokenCallee source target)
  refused : List String

/-- Refusals remain explicit. This is a collection of per-function proofs,
not a claim that every actual function or private-memory instruction passed. -/
def tokenModule (source : IR.Program) (target : EvoIR.Machine.Program) : TokenModuleCheck source target :=
  let callees := tokenModulePasses source target (source.functions.length+1) []
  ⟨callees, (source.functions.filter fun function =>
    !(callees.any fun callee => callee.name == function.name)).map IR.Function.name⟩

def tokenModuleFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) :
    Option (CheckedTokenFunction source target entry type) := do
  let function ← source.functions.find? (·.name == entry)
  tokenCallerFunction source target entry type (tokenBlockProposal function) (tokenModule source target).callees

def tokenModulePair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let function ← tokenModuleFunction source target entry type
  tokenFunctionPairUsing source target entry parameters type function

end EvoIR.Core.Symbolic.MachineSelection
