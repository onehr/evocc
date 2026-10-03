import Lean.Data.Json

/-!
The lossless Core C-HIR input tree. This reader only checks the wire grammar,
constructor whitelist, identifier range, and exact literal representations.
It is NOT the independent well-formedness or source-to-EvoIR checker. In
particular, decoding does not establish typing, lexical scope, initialization,
sequencing, or any compilation theorem.

Ranks remain distinct even when widths coincide. Implicit conversion sites
retain destination types and their complete source expressions. Blocks retain
declarations and statement order; loops and both conditional arms are retained.
-/

namespace EvoIR.Core.Source

open Lean (Json)

inductive IntType where
  | int | uint | long | ulong | longLong | ulongLong
  deriving DecidableEq, Repr

def IntType.width : IntType → Nat
  | .int | .uint => 32
  | _ => 64

def IntType.signed : IntType → Bool
  | .int | .long | .longLong => true
  | _ => false

def IntType.rank : IntType → Nat
  | .int | .uint => 3
  | .long | .ulong => 4
  | .longLong | .ulongLong => 5

structure QualifiedType where
  type : IntType
  isConst : Bool
  deriving DecidableEq, Repr

inductive Unary where
  | plus | neg | bitNot | not
  deriving DecidableEq, Repr

inductive Binary where
  | add | sub | mul | div | rem
  | lt | le | gt | ge | eq | ne
  | and | or | bitAnd | bitOr | bitXor | shl | shr | comma
  deriving DecidableEq, Repr

inductive Assignment where
  | set | add | sub | mul | div | rem | shl | shr | bitAnd | bitOr | bitXor
  deriving DecidableEq, Repr

inductive Update where
  | preInc | preDec | postInc | postDec
  deriving DecidableEq, Repr

inductive Expr where
  | literal (type : IntType) (bits : Nat)
  | ident (type : IntType) (symbol : Nat)
  | unary (type : IntType) (op : Unary) (operand : Expr)
  | binary (type : IntType) (op : Binary) (left right : Expr)
  | conditional (type : IntType) (condition yes no : Expr)
  | cast (type : IntType) (destination : QualifiedType) (operand : Expr)
  | comma (type : IntType) (left right : Expr)
  | assign (type : IntType) (op : Assignment) (symbol : Nat)
      (destination : QualifiedType) (target value : Expr)
  | update (type : IntType) (op : Update) (symbol : Nat)
      (destination : QualifiedType) (target : Expr)
  | call (type : IntType) (callee : Nat) (arguments : List (QualifiedType × Expr))
  deriving Repr

def Expr.type : Expr → IntType
  | .literal t _ | .ident t _ | .unary t _ _ | .binary t _ _ _
  | .conditional t _ _ _ | .cast t _ _ | .comma t _ _
  | .assign t _ _ _ _ _ | .update t _ _ _ _ | .call t _ _ => t

inductive Storage where
  | none | auto | register
  deriving DecidableEq, Repr

structure Object where
  symbol : Nat
  name : String
  type : QualifiedType
  storage : Storage
  initializer : Option (QualifiedType × Expr)
  deriving Repr

inductive ForInit where
  | empty
  | expression (value : Expr)
  | declarations (objects : List Object)
  deriving Repr

mutual
  inductive Stmt where
    | null | break | continue
    | expression (value : Expr)
    | return (destination : QualifiedType) (value : Expr)
    | compound (items : List BlockItem)
    | ite (condition : Expr) (yes : Stmt) (no : Option Stmt)
    | while (condition : Expr) (body : Stmt)
    | doWhile (body : Stmt) (condition : Expr)
    | for (init : ForInit) (condition increment : Option Expr) (body : Stmt)
    deriving Repr

  inductive BlockItem where
    | declaration (object : Object)
    | statement (stmt : Stmt)
    deriving Repr
end

structure Parameter where
  symbol : Nat
  type : QualifiedType
  storage : Storage
  deriving Repr

structure Function where
  symbol : Nat
  name : String
  returnType : QualifiedType
  internal : Bool
  parameters : List Parameter
  body : Stmt
  deriving Repr

structure Program where
  entries : List Nat
  functions : List Function
  deriving Repr

private def string (j : Json) : Option String := j.getStr?.toOption
private def array (j : Json) : Option (List Json) := j.getArr?.toOption.map Array.toList
private def boolean (j : Json) : Option Bool := j.getBool?.toOption

private def identifier (j : Json) : Option Nat := do
  let n ← j.getNat?.toOption
  if n < 2^32 then some n else none

private def intType (j : Json) : Option IntType := do
  match ← string j with
  | "int" => some .int | "uint" => some .uint
  | "long" => some .long | "ulong" => some .ulong
  | "long_long" => some .longLong | "ulong_long" => some .ulongLong
  | _ => none

private def qualifiedType (j : Json) : Option QualifiedType := do
  match ← array j with
  | [t, c] => return ⟨← intType t, ← boolean c⟩
  | _ => none

private def storage (j : Json) : Option Storage := do
  match ← string j with
  | "none" => some .none | "auto" => some .auto | "register" => some .register
  | _ => none

private def unary (j : Json) : Option Unary := do
  match ← string j with
  | "plus" => some .plus | "neg" => some .neg
  | "bit_not" => some .bitNot | "not" => some .not
  | _ => none

private def binary (j : Json) : Option Binary := do
  match ← string j with
  | "add" => some .add | "sub" => some .sub | "mul" => some .mul
  | "div" => some .div | "rem" => some .rem
  | "lt" => some .lt | "le" => some .le | "gt" => some .gt | "ge" => some .ge
  | "eq" => some .eq | "ne" => some .ne
  | "and" => some .and | "or" => some .or
  | "bit_and" => some .bitAnd | "bit_or" => some .bitOr | "bit_xor" => some .bitXor
  | "shl" => some .shl | "shr" => some .shr | "comma" => some .comma
  | _ => none

private def assignment (j : Json) : Option Assignment := do
  match ← string j with
  | "set" => some .set | "add" => some .add | "sub" => some .sub | "mul" => some .mul
  | "div" => some .div | "rem" => some .rem | "shl" => some .shl | "shr" => some .shr
  | "bit_and" => some .bitAnd | "bit_or" => some .bitOr | "bit_xor" => some .bitXor
  | _ => none

private def update (j : Json) : Option Update := do
  match ← string j with
  | "pre_inc" => some .preInc | "pre_dec" => some .preDec
  | "post_inc" => some .postInc | "post_dec" => some .postDec
  | _ => none

private def literalBits (type : IntType) (j : Json) : Option Nat := do
  let text ← string j
  if text.length != type.width / 4 then none else do
    text.toList.foldlM (fun n c => do
      let digit ← if '0' ≤ c ∧ c ≤ '9' then some (c.toNat - '0'.toNat)
        else if 'a' ≤ c ∧ c ≤ 'f' then some (c.toNat - 'a'.toNat + 10) else none
      return n * 16 + digit) 0

/-- Fuel bounds JSON nesting only, never source program execution. -/
def parseExpr : Nat → Json → Option Expr
  | 0, _ => none
  | fuel + 1, j => do
    let fields ← array j
    match fields with
    | tag :: type :: rest =>
      let t ← intType type
      match ← string tag, rest with
      | "lit", [bits] => return .literal t (← literalBits t bits)
      | "ident", [id] => return .ident t (← identifier id)
      | "unary", [op, operand] => return .unary t (← unary op) (← parseExpr fuel operand)
      | "binary", [op, left, right] =>
        return .binary t (← binary op) (← parseExpr fuel left) (← parseExpr fuel right)
      | "conditional", [condition, yes, no] =>
        return .conditional t (← parseExpr fuel condition) (← parseExpr fuel yes) (← parseExpr fuel no)
      | "cast", [destination, operand] =>
        return .cast t (← qualifiedType destination) (← parseExpr fuel operand)
      | "comma", [left, right] =>
        return .comma t (← parseExpr fuel left) (← parseExpr fuel right)
      | "assign", [op, id, destination, target, value] =>
        return .assign t (← assignment op) (← identifier id) (← qualifiedType destination)
          (← parseExpr fuel target) (← parseExpr fuel value)
      | "update", [op, id, destination, target] =>
        return .update t (← update op) (← identifier id) (← qualifiedType destination)
          (← parseExpr fuel target)
      | "call", [callee, arguments] =>
        let args ← (← array arguments).mapM fun arg => do
          match ← array arg with
          | [tag, destination, value] =>
            if (← string tag) != "argument" then none else
              return (← qualifiedType destination, ← parseExpr fuel value)
          | _ => none
        return .call t (← identifier callee) args
      | _, _ => none
    | _ => none

private def optional (parse : Json → Option α) (j : Json) : Option (Option α) :=
  if j == Json.null then some none else (parse j).map some

private def object (fuel : Nat) (j : Json) : Option Object := do
  match ← array j with
  | [tag, symbol, name, type, store, initializer] =>
    if (← string tag) != "object" then none else do
      let init ← optional (fun j => do
        match ← array j with
        | [tag, destination, value] =>
          if (← string tag) != "store_scalar" then none else
            return (← qualifiedType destination, ← parseExpr fuel value)
        | _ => none) initializer
      return ⟨← identifier symbol, ← string name, ← qualifiedType type, ← storage store, init⟩
  | _ => none

private def forInit (fuel : Nat) (j : Json) : Option ForInit := do
  if j == Json.null then return .empty
  match ← array j with
  | [tag, value] =>
    match ← string tag with
    | "expr" => return .expression (← parseExpr fuel value)
    | "declarations" => return .declarations (← (← array value).mapM (object fuel))
    | _ => none
  | _ => none

def parseStmt : Nat → Json → Option Stmt
  | 0, _ => none
  | fuel + 1, j => do
    match ← array j with
    | tag :: rest =>
      match ← string tag, rest with
      | "null", [] => return .null
      | "break", [] => return .break
      | "continue", [] => return .continue
      | "expr", [value] => return .expression (← parseExpr fuel value)
      | "return", [destination, value] =>
        return .return (← qualifiedType destination) (← parseExpr fuel value)
      | "compound", [items] =>
        let items ← (← array items).mapM fun item => do
          let tag ← (← array item).head?
          if (← string tag) == "object" then return .declaration (← object fuel item)
          else return .statement (← parseStmt fuel item)
        return .compound items
      | "if", [condition, yes, no] =>
        return .ite (← parseExpr fuel condition) (← parseStmt fuel yes)
          (← optional (parseStmt fuel) no)
      | "while", [condition, body] =>
        return .while (← parseExpr fuel condition) (← parseStmt fuel body)
      | "do_while", [body, condition] =>
        return .doWhile (← parseStmt fuel body) (← parseExpr fuel condition)
      | "for", [init, condition, increment, body] =>
        return .for (← forInit fuel init) (← optional (parseExpr fuel) condition)
          (← optional (parseExpr fuel) increment) (← parseStmt fuel body)
      | _, _ => none
    | _ => none

private def parameter (j : Json) : Option Parameter := do
  match ← array j with
  | [tag, symbol, type, store] =>
    if (← string tag) != "parameter" then none else
      return ⟨← identifier symbol, ← qualifiedType type, ← storage store⟩
  | _ => none

private def function (fuel : Nat) (j : Json) : Option Function := do
  match ← array j with
  | [tag, symbol, name, result, internal, parameters, body] =>
    if (← string tag) != "function" then none else
      return ⟨← identifier symbol, ← string name, ← qualifiedType result, ← boolean internal,
        ← (← array parameters).mapM parameter, ← parseStmt fuel body⟩
  | _ => none

/-- A decoded tree, never a verified-compilation disposition. -/
def parseProgram (fuel : Nat) (j : Json) : Option Program := do
  match ← array j with
  | [version, target, profile, entries, functions] =>
    if (← string version) != "core-v1" || (← string target) != "aarch64-unknown-linux-gnu" ||
        (← string profile) != "checked" then none else
      return ⟨← (← array entries).mapM identifier, ← (← array functions).mapM (function fuel)⟩
  | _ => none

def readProgram (text : String) : Except String Program := do
  let json ← Json.parse text
  match parseProgram 1024 json with
  | some program => return program
  | none => throw "unsupported or malformed core-v1 source tree (not a proof result)"

private def readerCases : List (String × Bool) := [
  ("[\"lit\",\"int\",\"80000000\"]", true),
  ("[\"lit\",\"ulong_long\",\"ffffffffffffffff\"]", true),
  ("[\"lit\",\"int\",\"0\"]", false),
  ("[\"lit\",\"int\",\"FFFFFFFF\"]", false),
  ("[\"lit\",\"int128\",\"00000000000000000000000000000000\"]", false),
  ("[\"ident\",\"int\",4294967296]", false),
  ("[\"ident\",\"int\",-1]", false),
  ("[\"ident\",\"int\",0,0]", false),
  ("[\"unary\",\"int\",\"abs\",[\"ident\",\"int\",0]]", false),
  ("[\"unary\",\"int\",\"neg\",[\"ident\",\"int\",0]]", true)
]

def readerSelfTest : IO UInt32 := do
  let mut failures := 0
  for (text, expected) in readerCases do
    let parsed := (Json.parse text).toOption.bind (parseExpr 1024)
    if parsed.isSome != expected then
      IO.eprintln s!"core reader mismatch: {text}"
      failures := failures + 1
  if failures == 0 then
    IO.println s!"{readerCases.length} core source-reader cases pass (no compilation claim)"
    return 0
  return 1

end EvoIR.Core.Source
