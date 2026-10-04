import EvoIR.Core.IR

/-! Independent reader for the canonical text emitted on the core path.
No Rust-parsed AST or producer typing claim is trusted. Unsupported syntax,
metadata, regions and address spaces refuse; nothing is silently dropped.
Reading and local type checks are not a source/target compilation proof. -/
namespace EvoIR.Core.IR.Reader

abbrev P := StateT (List String) (Except String)

def tokens (line : String) : List String := Id.run do
  let mut out := []
  let mut word := ""
  for c in line.toList do
    if c.isWhitespace || "()[]{}<>,:=".contains c then
      if !word.isEmpty then out := word :: out; word := ""
      if !c.isWhitespace then out := String.singleton c :: out
    else word := word.push c
  if !word.isEmpty then out := word :: out
  return out.reverse

def take : P String := do
  let x :: xs ← get | throw "unexpected end of EvoIR line"
  set xs
  return x

def expect (wanted : String) : P Unit := do
  let got ← take
  if got != wanted then throw s!"expected '{wanted}', got '{got}'"

def peek : P (Option String) := return (← get).head?

def natural : P Nat := do
  let word ← take
  let some n := word.toNat? | throw s!"not a natural: {word}"
  if toString n != word then throw "noncanonical integer"
  return n

def name (mark : String) : P String := do
  let word ← take
  if !word.startsWith mark then throw s!"expected {mark} name"
  let body := (word.drop mark.length).toString
  if body.isEmpty || !body.toList.all (fun c => c.isAlphanum || c == '_' || c == '.') then
    throw "unsupported symbol spelling"
  return body

def value : P Nat := do
  let word ← name "%v"
  let some n := word.toNat? | throw "invalid SSA value name"
  if toString n != word then throw "noncanonical SSA value name"
  return n

def ty : P Ty := do
  match ← take with
  | "bits" =>
    expect "<"
    let n ← natural
    expect ">"
    if n != 1 && n != 32 && n != 64 then throw "outside core integer widths"
    return .bits n
  | "ptr" => expect "<"; expect "generic"; expect ">"; return .pointer
  | "mem" => expect "<"; expect "all"; expect ">"; return .memory
  | "world" => return .world
  | other => throw s!"unsupported core type: {other}"

def width : P Nat := do
  expect "<"
  let n ← natural
  expect ">"
  if n != 1 && n != 32 && n != 64 then throw "outside core integer widths"
  return n

def commaList (item : P α) (closing : String) : Nat → P (List α)
  | 0 => throw "unterminated list"
  | n+1 => do
    if (← peek) == some closing then expect closing; return []
    let a ← item
    if (← peek) == some closing then expect closing; return [a]
    expect ","
    if (← peek) == some closing then throw "trailing comma"
    return a :: (← commaList item closing n)

def list (item : P α) (closing : String) : P (List α) := do
  commaList item closing ((← get).length+1)

def restList (item : P α) : P (List α) := do
  modify (· ++ ["END"])
  list item "END"

def run (parser : P α) (line : String) : Except String α := do
  let (a, rest) ← parser.run (tokens line)
  if !rest.isEmpty then throw s!"unexpected remaining syntax: {rest}"
  return a

def boolean : P Bool := do
  match ← take with
  | "true" => return true
  | "false" => return false
  | _ => throw "expected boolean"

def regions : P Bool := do
  expect "{"
  if (← peek) == some "}" then expect "}"; return false
  expect "all"; expect "}"; return true

def field (key : String) (parser : P α) : P α := do
  expect key; expect "="; parser

def effects : P Effects := do
  expect "effects"; expect "{"
  let readsMemory ← field "reads" regions
  expect ","
  let writesMemory ← field "writes" regions
  expect ","
  let volatile ← field "volatile" boolean
  expect ","
  let atomic ← field "atomic" boolean
  expect ","
  let io ← field "io" boolean
  expect ","
  let fenv ← field "fenv" boolean
  expect ","
  let mayTrap ← field "may_trap" boolean
  expect ","
  let mayUnwind ← field "may_unwind" boolean
  expect "}"
  return ⟨readsMemory, writesMemory, volatile, atomic, io, fenv, mayTrap, mayUnwind⟩

def emptyEffects : Effects := ⟨false, false, false, false, false, false, false, false⟩

def header : P Function := do
  expect "func"
  let n ← name "@"
  expect "("
  let parameters ← list (do
    let id ← name "%arg"
    let some _ := id.toNat? | throw "invalid formal argument name"
    expect ":"; return (id, ← ty)) ")"
  if parameters.map Prod.fst != (List.range parameters.length).map toString then
    throw "formal argument names are not ordered canonical names"
  expect "-"; expect ">"; expect "("
  let returns ← list ty ")"
  expect "cc"; expect "="; expect "target_default"
  let internal ← if (← peek) == some "visibility" then do
    expect "visibility"; expect "="; expect "internal"; pure true
    else pure false
  return ⟨n, parameters.map Prod.snd, returns, internal, emptyEffects, []⟩

def binding : P Binding := do
  let id ← value
  expect ":"
  return ⟨id, ← ty⟩

def blockHeader : P (String × List Binding) := do
  let n ← name "^"
  expect "("
  let params ← list binding ")"
  expect ":"
  return (n, params)

def edge : P Edge := do
  let n ← name "^"
  expect "("
  return ⟨n, ← list value ")"⟩

def terminator : P Terminator := do
  match ← take with
  | "br" => return .branch (← edge)
  | "cond_br" =>
    let c ← value
    expect ","
    let yes ← edge
    expect ","
    return .conditional c yes (← edge)
  | "ret" => return .return (← restList value)
  | _ => throw "unsupported core terminator"

def reason : String → Except String Reason
  | "ub.assume.division_by_zero" => .ok .divisionByZero
  | "ub.assume.remainder_by_zero" => .ok .remainderByZero
  | "ub.assume.signed_overflow" => .ok .signedOverflow
  | "ub.assume.invalid_shift_count" => .ok .invalidShiftCount
  | _ => .error "unsupported UB obligation"

def operandType (bindings : List Binding) (id : Nat) : Except String Ty := do
  let some b := bindings.find? (·.id == id) | throw "SSA use outside current block"
  return b.type

def operandWidth (bindings : List Binding) (id : Nat) : Except String Nat := do
  let .bits n ← operandType bindings id | throw "integer operand required"
  return n

def scalar (name : String) (n m : Nat) : Except String Op :=
  match name with
  | "add.bits" => .ok (.bin .add n)
  | "sub.bits" => .ok (.bin .sub n)
  | "mul.bits" => .ok (.bin .mul n)
  | "and.bits" => .ok (.bin .and n)
  | "or.bits" => .ok (.bin .or n)
  | "xor.bits" => .ok (.bin .xor n)
  | "shl.bits" => .ok (.shift .shl n m)
  | "lshr.bits" => .ok (.shift .lshr n m)
  | "ashr.bits" => .ok (.shift .ashr n m)
  | "icmp.eq" => .ok (.cmp .eq n)
  | "icmp.ne" => .ok (.cmp .ne n)
  | "icmp.slt" => .ok (.cmp .slt n)
  | "icmp.sle" => .ok (.cmp .sle n)
  | "icmp.ult" => .ok (.cmp .ult n)
  | "icmp.ule" => .ok (.cmp .ule n)
  | "signed_add_defined" => .ok (.guard .add n)
  | "signed_sub_defined" => .ok (.guard .sub n)
  | "signed_mul_defined" => .ok (.guard .mul n)
  | "signed_div_rem_no_overflow" => .ok (.guard .divRem n)
  | "shift_count_defined.signed" => .ok (.count true n m)
  | "shift_count_defined.unsigned" => .ok (.count false n m)
  | _ => .error s!"unsupported core scalar opcode: {name}"

def scalarInputs : Op → List Ty
  | .bin _ n | .cmp _ n | .guard _ n => [.bits n, .bits n]
  | .shift _ n m | .count _ n m => [.bits n, .bits m]
  | .cast _ n _ | .nonzero n => [.bits n]

def signature (functions : List Function) : Operation → Except String (List Ty × List Ty)
  | .constant n _ => .ok ([], [.bits n])
  | .uninitialized t => .ok ([], [t])
  | .scalar op => .ok (scalarInputs op, [.bits op.res])
  | .division _ _ n => .ok ([.bits n, .bits n], [.bits n])
  | .assume _ => .ok ([.bits 1], [])
  | .call n => do
    let some f := functions.find? (·.name == n) | throw "external call outside core"
    return (f.parameters, f.returns)
  | .allocate _ _ _ => .ok ([.memory, .world], [.pointer, .memory, .world])
  | .endLifetime => .ok ([.memory, .world, .pointer], [.memory, .world])
  | .access _ _ => .ok ([.memory, .pointer], [.memory])
  | .load n => .ok ([.memory, .world, .pointer], [.bits n, .memory, .world])
  | .store => .error "store signature depends on value type"

def layout : P (Nat × Nat) := do
  let size ← field "size" natural
  expect ","
  let align ← field "align" natural
  expect "]"
  if size == 0 || size > 2^63-1 || align == 0 || (align &&& (align-1)) != 0 then
    throw "invalid memory layout"
  return (size, align)

def operation (bindings : List Binding) : P (Operation × List Nat) := do
  let opcode ← take
  if opcode.startsWith "ub.assume." then return (.assume (← reason opcode), [← value])
  match opcode with
  | "const.bits" =>
    let n ← width
    let bits ← natural
    if bits ≥ 2^n then throw "out-of-width constant"
    return (.constant n bits, [])
  | "uninitialized.bits" => return (.uninitialized (.bits (← width)), [])
  | "call" =>
    let f ← name "@"
    expect "("
    return (.call f, ← list value ")")
  | "alloca" =>
    let m ← value; expect ","; let w ← value
    expect "["
    let origin ← field "origin" natural
    expect ","
    let (size, align) ← layout
    return (.allocate origin size align, [m, w])
  | "access_in_bounds" =>
    let m ← value; expect ","; let p ← value
    expect "["
    let (size, align) ← layout
    return (.access size align, [m, p])
  | "lifetime.end" | "load" | "store" =>
    let m ← value; expect ","; let w ← value; expect ","; let p ← value
    if opcode == "load" then
      expect ":"
      let .bits n ← ty | throw "noninteger load"
      if n == 1 then throw "non-object load width"
      return (.load n, [m, w, p])
    if opcode == "store" then expect ","; return (.store, [m, w, p, ← value])
    return (.endLifetime, [m, w, p])
  | "divisor_nonzero" =>
    let a ← value
    return (.scalar (.nonzero (← operandWidth bindings a)), [a])
  | "zext.bits" | "sext.bits" | "trunc.bits" =>
    let n ← width
    let a ← value
    let w ← operandWidth bindings a
    if (opcode == "trunc.bits" && w ≤ n) || (opcode != "trunc.bits" && n ≤ w) then
      throw "invalid cast width direction"
    let op := if opcode == "zext.bits" then CastOp.zext
      else if opcode == "sext.bits" then .sext else .trunc
    return (.scalar (.cast op w n), [a])
  | _ =>
    let annotated ← if opcode.endsWith ".bits" then some <$> width else pure none
    let a ← value; expect ","; let b ← value
    let n ← operandWidth bindings a
    let m ← operandWidth bindings b
    if let some w := annotated then
      if w != n then throw "opcode width differs from operand"
    if ["sdiv.bits", "udiv.bits", "srem.bits", "urem.bits"].contains opcode then
      return (.division (opcode.startsWith "s") (opcode.endsWith "rem.bits") n, [a, b])
    return (.scalar (← scalar opcode n m), [a, b])

def instruction (functions : List Function) (bindings : List Binding) : P Instruction := do
  let ids ← if ((← peek).getD "").startsWith "ub.assume." then pure [] else do
    let ids ← if (← peek) == some "(" then do
      expect "("; list value ")"
      else pure [← value]
    expect "="
    pure ids
  let (op, operands) ← operation bindings
  let actual ← (operands.mapM (operandType bindings) : Except String (List Ty))
  let (inputs, outputs) ← if op == .store then do
    let [.memory, .world, .pointer, .bits n] := actual | throw "store operand types"
    if n != 32 && n != 64 then throw "store object width"
    pure (actual, [.memory, .world])
    else signature functions op
  if actual != inputs then throw "instruction operand type mismatch"
  if ids.length != outputs.length then throw "instruction result arity"
  if ids.eraseDups != ids || ids.any (fun id => bindings.any (·.id == id)) then
    throw "duplicate result binding"
  return ⟨(ids.zip outputs).map (fun (id, t) => ⟨id, t⟩), op, operands⟩

structure RawFunction where
  signature : Function
  body : List String

def rawFunctions : Nat → List String → Except String (List RawFunction)
  | _, [] => .ok []
  | 0, _ => .error "function reader input limit"
  | fuel+1, h :: e :: "{" :: lines => do
    let f ← run header h
    let eff ← run effects e
    let (body, rest) := lines.span (· != "}")
    let "}" :: rest := rest | throw "unterminated function"
    let following ← rawFunctions fuel rest
    return ⟨{f with effects := eff}, body⟩ :: following
  | _, _ => .error "expected complete core function, effects and body"

def readBlocks (functions : List Function) : Nat → List String → Except String (List Block)
  | _, [] => .ok []
  | 0, _ => .error "block reader input limit"
  | fuel+1, line :: rest => do
    let (name, parameters) ← run blockHeader line
    if (parameters.map Binding.id).eraseDups != parameters.map Binding.id then
      throw "duplicate block parameter"
    let (body, following) := rest.span (fun s => !s.startsWith "^")
    let some last := body.getLast? | throw "block missing terminator"
    let term ← run terminator last
    let mut bindings := parameters
    let mut instructions := []
    for line in body.dropLast do
      let inst ← run (instruction functions bindings) line
      bindings := inst.results ++ bindings
      instructions := instructions ++ [inst]
    let _ ← match term with
      | .branch e => e.arguments.mapM (operandType bindings)
      | .conditional c a b => do
        if (← operandType bindings c) != .bits 1 then throw "nonboolean branch"
        (a.arguments ++ b.arguments).mapM (operandType bindings)
      | .return ids => ids.mapM (operandType bindings)
    return ⟨name, parameters, instructions, term⟩ :: (← readBlocks functions fuel following)

def readProgram (text : String) : Except String Program := do
  let lines := (text.splitOn "\n").map (fun s => s.trimAscii.toString) |>.filter (· != "")
  let raw ← rawFunctions (lines.length+1) lines
  let signatures := raw.map RawFunction.signature
  if signatures.isEmpty || (signatures.map Function.name).eraseDups != signatures.map Function.name then
    throw "empty module or duplicate function"
  let functions ← raw.mapM fun r => do
    let blocks ← readBlocks signatures (r.body.length+1) r.body
    if blocks.isEmpty || (blocks.map Block.name).eraseDups != blocks.map Block.name then
      throw "empty function or duplicate block"
    let ids := blocks.flatMap fun b => (b.parameters ++ b.instructions.flatMap Instruction.results).map Binding.id
    if ids.eraseDups != ids then throw "SSA value ID repeated across blocks"
    for b in blocks do
      let bindings := b.parameters ++ b.instructions.flatMap Instruction.results
      let edges ← match b.terminator with
        | .return ids => do
          if (← ids.mapM (operandType bindings)) != r.signature.returns then throw "return type mismatch"
          pure []
        | .branch e => pure [e]
        | .conditional _ a b => pure [a, b]
      for e in edges do
        let some target := blocks.find? (·.name == e.block) | throw "unknown successor"
        if (← e.arguments.mapM (operandType bindings)) != target.parameters.map Binding.type then
          throw "edge type/arity mismatch"
    let some entry := blocks.head? | throw "missing entry"
    if entry.parameters.map Binding.type != r.signature.parameters then throw "entry type mismatch"
    pure {r.signature with blocks}
  return ⟨functions⟩

end EvoIR.Core.IR.Reader
