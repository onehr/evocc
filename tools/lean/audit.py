#!/usr/bin/env python3
"""The six-step audit of the Lean proof layer, and the receipt it writes.

The steps are the ones the plan's section 2 (R3) fixes:

  1. Audited scope: the modules under `lean/EvoIR/`, whose imports must be
     Lean core or each other, in a package with no external requirement.
  2. Every `axiom` declaration in the built environment is enumerated.  Beyond
     the three core axioms each one is recorded with its name, its declaring
     module and its type, and anything declared inside our own modules must
     have the shape of a `bv_decide` reflection axiom.  Anything else is a
     refusal, because an axiom name is not evidence of where it came from.
  3. Each target theorem's transitive axiom dependencies are collected and the
     theorem is classified `kernel-only` or `native-backed`.
  4. The full proposition is checked: a target's type, as Lean prints it, must
     equal the proposition this script generates from the registry data (for
     the rules) or from the Section 8.10 operator table (for the analysis).
     Matching names and axioms is not a pass.
  5. The receipt binds the registry digest, the specification revision line,
     the `lean/` source commit and the toolchain version to every target's
     name, proposition and actual dependencies.
  6. The trust assumptions are written into the receipt rather than implied.

Any `sorryAx` anywhere in the audited modules fails the audit.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from registry_to_lean import extract_block, lean_ident, parse_rule  # noqa: E402

CORE_AXIOMS = {"propext", "Classical.choice", "Quot.sound"}

# Axioms Lean's own prelude declares.  None of them may appear in a target's
# dependencies; they are recorded so the receipt shows what was in the
# environment rather than only what was used.
CORE_MODULE_PREFIXES = ("Init", "Std", "Lean", "Batteries")

ALLOWED_IMPORT_PREFIXES = ("EvoIR", "Init", "Std", "Lean")

NATIVE_AXIOM_RE = re.compile(r"^EvoIR\..*\._native\.bv_decide\.ax")

# What a `bv_decide` reflection axiom asserts: that the recorded LRAT
# certificate refutes the recorded bit-blasted expression.  A name is not
# evidence -- an axiom of our own could be given the same one -- so the type
# has to have this shape as well.
NATIVE_AXIOM_TYPE_RE = re.compile(
    r"^Std\.Tactic\.BVDecide\.Reflect\.verifyBVExpr\s+\S+\s+\S+\s*=\s*true$"
)

# The Section 8.10 transfer table, as the audit's own reading of it.  Each row
# gives the Lean operator constructor, the parameters the theorem quantifies
# over, and the theorem name.
PARAMETRIC_ROWS = [
    ("and", "EvoIR.Op.bin EvoIR.BinOp.and", ["n"], "EvoIR.transfer_and"),
    ("or", "EvoIR.Op.bin EvoIR.BinOp.or", ["n"], "EvoIR.transfer_or"),
    ("xor", "EvoIR.Op.bin EvoIR.BinOp.xor", ["n"], "EvoIR.transfer_xor"),
    ("shl", "EvoIR.Op.shift EvoIR.ShOp.shl", ["n", "m"], "EvoIR.transfer_shl"),
    ("lshr", "EvoIR.Op.shift EvoIR.ShOp.lshr", ["n", "m"], "EvoIR.transfer_lshr"),
    ("ashr", "EvoIR.Op.shift EvoIR.ShOp.ashr", ["n", "m"], "EvoIR.transfer_ashr"),
    ("zext", "EvoIR.Op.cast EvoIR.CastOp.zext", ["w", "n"], "EvoIR.transfer_zext"),
    ("sext", "EvoIR.Op.cast EvoIR.CastOp.sext", ["w", "n"], "EvoIR.transfer_sext"),
    ("trunc", "EvoIR.Op.cast EvoIR.CastOp.trunc", ["w", "n"], "EvoIR.transfer_trunc"),
    ("signed_add_defined", "EvoIR.Op.guard EvoIR.GuardOp.add", ["n"], "EvoIR.transfer_guard_add"),
    ("signed_sub_defined", "EvoIR.Op.guard EvoIR.GuardOp.sub", ["n"], "EvoIR.transfer_guard_sub"),
    ("signed_mul_defined", "EvoIR.Op.guard EvoIR.GuardOp.mul", ["n"], "EvoIR.transfer_guard_mul"),
    ("signed_div_rem_no_overflow", "EvoIR.Op.guard EvoIR.GuardOp.divRem", ["n"], "EvoIR.transfer_guard_div_rem"),
    ("divisor_nonzero", "EvoIR.Op.nonzero", ["n"], "EvoIR.transfer_guard_nonzero"),
    ("shift_count_defined.unsigned", "EvoIR.Op.count false", ["n", "m"], "EvoIR.transfer_guard_unsigned_count"),
    ("shift_count_defined.signed", "EvoIR.Op.count true", ["n", "m"], "EvoIR.transfer_guard_signed_count"),
]

ENUMERATED_ROWS = [
    ("add", "EvoIR.Op.bin EvoIR.BinOp.add", "EvoIR.transfer_add"),
    ("sub", "EvoIR.Op.bin EvoIR.BinOp.sub", "EvoIR.transfer_sub"),
    ("mul", "EvoIR.Op.bin EvoIR.BinOp.mul", "EvoIR.transfer_mul"),
    ("icmp.eq", "EvoIR.Op.cmp EvoIR.CmpOp.eq", "EvoIR.transfer_icmp_eq"),
    ("icmp.ne", "EvoIR.Op.cmp EvoIR.CmpOp.ne", "EvoIR.transfer_icmp_ne"),
    ("icmp.slt", "EvoIR.Op.cmp EvoIR.CmpOp.slt", "EvoIR.transfer_icmp_slt"),
    ("icmp.sle", "EvoIR.Op.cmp EvoIR.CmpOp.sle", "EvoIR.transfer_icmp_sle"),
    ("icmp.ult", "EvoIR.Op.cmp EvoIR.CmpOp.ult", "EvoIR.transfer_icmp_ult"),
    ("icmp.ule", "EvoIR.Op.cmp EvoIR.CmpOp.ule", "EvoIR.transfer_icmp_ule"),
]

ENUMERATED_WIDTHS = list(range(1, 65))

# What Section 8.5 admits, as the eligibility implementations enforce it: a
# root or witness is `bits<N>` with 1 <= N <= 64, a comparison consumes one
# common `bits<N>`, a shift consumes an amount of `bits<M>` with 1 <= M <= 64,
# and a width change's two widths are both inside the bound.  A parametric
# theorem proves more than this; an enumerated row proves exactly it.
PROFILE_BOUND = (
    "Section 8.5: 1 <= N <= 64 for a root or witness, 1 <= M <= 64 for a "
    "shift amount, both width-change widths inside the same bound"
)

# The lattice, merge and predicate lemmas, with the proposition each must
# state.  These are written out rather than generated: the specification
# prints them as prose, not as a machine-readable table.
LATTICE_TARGETS = [
    (
        "const",
        "EvoIR.transfer_const",
        "∀ (n : Nat) (v : BitVec n), EvoIR.gamma (EvoIR.Fact.const v) v",
        "8.10 transfer table, `const` row: a constant is fully known",
    ),
    (
        "fully_known",
        "EvoIR.fullyKnownValue",
        "∀ {n : Nat} (K : EvoIR.Fact n) (x : BitVec n), EvoIR.fullyKnown K → "
        "EvoIR.gamma K x → x = K.value",
        "11.6 `fully_known`, the shared lemma the 18 `known_const` rules use",
    ),
    (
        "known_zero",
        "EvoIR.knownZeroValue",
        "∀ {n : Nat} (K : EvoIR.Fact n) (z : BitVec n), EvoIR.knownZero K → "
        "EvoIR.gamma K z → z = 0#n",
        "11.6 `known_zero(?z)`",
    ),
    (
        "covers.and",
        "EvoIR.coversAnd",
        "∀ {n : Nat} (M X : EvoIR.Fact n) (m x : BitVec n), EvoIR.covers M X → "
        "EvoIR.gamma M m → EvoIR.gamma X x → x &&& m = x",
        "11.6 `covers(?m,?x)`, the `and` conclusion",
    ),
    (
        "covers.and.left",
        "EvoIR.coversAndLeft",
        "∀ {n : Nat} (M X : EvoIR.Fact n) (m x : BitVec n), EvoIR.covers M X → "
        "EvoIR.gamma M m → EvoIR.gamma X x → m &&& x = x",
        "11.6 `covers(?m,?x)`, the `and` conclusion with the operands swapped",
    ),
    (
        "covers.or",
        "EvoIR.coversOr",
        "∀ {n : Nat} (M X : EvoIR.Fact n) (m x : BitVec n), EvoIR.covers M X → "
        "EvoIR.gamma M m → EvoIR.gamma X x → x ||| m = m",
        "11.6 `covers(?m,?x)`, the `or` conclusion",
    ),
    (
        "covers.or.left",
        "EvoIR.coversOrLeft",
        "∀ {n : Nat} (M X : EvoIR.Fact n) (m x : BitVec n), EvoIR.covers M X → "
        "EvoIR.gamma M m → EvoIR.gamma X x → m ||| x = m",
        "11.6 `covers(?m,?x)`, the `or` conclusion with the operands swapped",
    ),
    (
        "meet.sound",
        "EvoIR.MeetSound",
        "∀ {n : Nat} (K K' : EvoIR.Fact n) (x : BitVec n), EvoIR.gamma K x → "
        "EvoIR.gamma K' x → ∃ K'', K.meet K' = some K'' ∧ EvoIR.gamma K'' x",
        "8.10 merge",
    ),
    (
        "meet.conflict",
        "EvoIR.MeetConflict",
        "∀ {n : Nat} (K K' : EvoIR.Fact n) (x : BitVec n), EvoIR.gamma K x → "
        "EvoIR.gamma K' x → K.meet K' ≠ none",
        "8.10 merge: two facts about one value never conflict",
    ),
    (
        "join.left",
        "EvoIR.JoinSoundLeft",
        "∀ {n : Nat} (K K' : EvoIR.Fact n) (x : BitVec n), EvoIR.gamma K x → "
        "EvoIR.gamma (K.join K') x",
        "8.10 join",
    ),
    (
        "join.right",
        "EvoIR.JoinSoundRight",
        "∀ {n : Nat} (K K' : EvoIR.Fact n) (x : BitVec n), EvoIR.gamma K' x → "
        "EvoIR.gamma (K.join K') x",
        "8.10 join, the other edge",
    ),
]

# The definitions that say what the theorems mean.  Their pretty-printed text
# goes into the receipt with a digest, so a reviewer reads the correctness
# definition once and then compares digests.
RECORDED_DEFINITIONS = [
    "EvoIR.Core.sourceArithmetic",
    "EvoIR.Core.irArithmetic",
    "EvoIR.Core.sourceNeg",
    "EvoIR.Core.irNeg",
    "EvoIR.Core.sourceAbs",
    "EvoIR.Core.irAbs",
    "EvoIR.Core.sourceFixedMul",
    "EvoIR.Core.irFixedMul",
    "EvoIR.Core.checkArithmeticFragment",
    "EvoIR.Core.runArithmeticFragment",
    "EvoIR.Core.Source.binaryValue",
    "EvoIR.Core.Source.unaryValue",
    "EvoIR.Core.Source.constantValue",
    "EvoIR.Core.Source.wrappingConstant",
    "EvoIR.Core.Source.readSlot",
    "EvoIR.Core.Source.writeSlot",
    "EvoIR.Core.Source.enterExpression",
    "EvoIR.Core.Source.enterStatement",
    "EvoIR.Core.Source.resumeValue",
    "EvoIR.Core.Source.resumeFlow",
    "EvoIR.Core.Source.enterFunction",
    "EvoIR.Core.Source.initial",
    "EvoIR.Core.Source.step",
    "EvoIR.Core.Transition.CheckpointSimulation",
    "EvoIR.Core.Symbolic.SourceCutRuns",
    "EvoIR.Core.Symbolic.sourceContextFrames",
    "EvoIR.Core.Symbolic.SourceCursor.denote",
    "EvoIR.Core.Symbolic.TargetCutRuns",
    "EvoIR.Core.Symbolic.TargetInstructionSpan",
    "EvoIR.Core.Symbolic.CallingCutRuns",
    "EvoIR.Core.Symbolic.callingCutDiagram",
    "EvoIR.Core.Symbolic.TargetExit.callState",
    "EvoIR.Core.Symbolic.cutResume",
    "EvoIR.Core.Symbolic.CallingLoopRelated",
    "EvoIR.Core.Transition.OpenCheckpoint",
    "EvoIR.Core.Symbolic.FunctionReturn",
    "EvoIR.Core.Symbolic.FunctionBoundary",
    "EvoIR.Core.Symbolic.ContextOpen",
    "EvoIR.Core.Symbolic.loopInputValues",
    "EvoIR.Core.Symbolic.CutpointRelated",
    "EvoIR.Core.Symbolic.CutExitAgreement",
    "EvoIR.Core.Symbolic.CheckedSpan",
    "EvoIR.Core.Symbolic.canonicalStore",
    "EvoIR.Core.Symbolic.projectLocals",
    "EvoIR.Core.Symbolic.CutpointLayout.project",
    "EvoIR.Core.Symbolic.checkSuccessor",
    "EvoIR.Core.Symbolic.checkedSpans",
    "EvoIR.Core.Symbolic.loopDiagram",
    "EvoIR.Core.Symbolic.loopPair",
    "EvoIR.Core.Symbolic.readCutpoints",
    "EvoIR.Core.Source.Steps",
    "EvoIR.Core.Source.Diverges",
    "EvoIR.Core.Source.runSteps",
    "EvoIR.Core.IR.evalScalar",
    "EvoIR.Core.IR.evalDivision",
    "EvoIR.Core.IR.evalOperation",
    "EvoIR.Core.IR.bind",
    "EvoIR.Core.IR.jump",
    "EvoIR.Core.IR.execute",
    "EvoIR.Core.IR.step",
    "EvoIR.Core.IR.Steps",
    "EvoIR.Core.IR.Diverges",
    "EvoIR.Core.IR.runSteps",
    "EvoIR.Core.IR.Reader.readProgram",
    "EvoIR.Core.IR.checkProgram",
    "EvoIR.Core.Source.exprDecEq",
    "EvoIR.Core.Source.stmtDecEq",
    "EvoIR.Core.Transition.Reaches",
    "EvoIR.Core.Transition.Infinite",
    "EvoIR.Core.Transition.Refines",
    "EvoIR.Core.Transition.Predecessor",
    "EvoIR.Core.Transition.RankedSimulation",
    "EvoIR.Core.checkedDiagnostic",
    "EvoIR.Core.observeSource",
    "EvoIR.Core.observeTarget",
    "EvoIR.Core.LoweringRefines",
    "EvoIR.Core.LoweringSimulation",
    "EvoIR.Core.AbsCorrespondence.sourceBody",
    "EvoIR.Core.AbsCorrespondence.source",
    "EvoIR.Core.AbsCorrespondence.target",
    "EvoIR.Core.AbsCorrespondence.checkPilotPair",
    "EvoIR.Core.Symbolic.scalar",
    "EvoIR.Core.Symbolic.instruction",
    "EvoIR.Core.Symbolic.instructions",
    "EvoIR.Core.Symbolic.checkFragment",
    "EvoIR.Core.Symbolic.guards",
    "EvoIR.Core.Symbolic.CWord.cast",
    "EvoIR.Core.Symbolic.arithmetic",
    "EvoIR.Core.Symbolic.checkArithmetic",
    "EvoIR.denoteOp",
    "EvoIR.denoteTerm",
    "EvoIR.Core.Symbolic.denoteTerm",
    "EvoIR.elabPat",
    "EvoIR.denotePat",
    "EvoIR.denoteRhs",
    "EvoIR.CondHolds",
    "EvoIR.RuleSound",
    "EvoIR.RuleCovered",
    "EvoIR.gamma",
    "EvoIR.covers",
    "EvoIR.fullyKnown",
    "EvoIR.knownZero",
    "EvoIR.Fact.meet",
    "EvoIR.Fact.join",
    "EvoIR.transfer",
    "EvoIR.TransferSound",
    "EvoIR.TransferRowSound",
    "EvoIR.capAmount",
]

CORE_PRIMITIVE_TARGETS = [
    (
        "EvoIR.Core.Source.LocalState.Realizes.after_write",
        "∀ (state : EvoIR.Core.Source.LocalState) (locals updated : EvoIR.Core.Source.Locals), "
        "state.Realizes locals → ∀ (id : Nat) (destination : EvoIR.Core.Source.QualifiedType) "
        "(value : EvoIR.Core.Source.Value) (initialization : Bool), "
        "EvoIR.Core.Source.writeSlot locals id destination value initialization = Except.ok updated → "
        "∀ (initialized : List Nat), (∀ (other : Nat), other ∈ initialized → other = id ∨ other ∈ state.initialized) → "
        "{ bindings := state.bindings, initialized := initialized }.Realizes updated",
    ),
    (
        "EvoIR.Core.Source.LocalState.Realizes.declare",
        "∀ (state : EvoIR.Core.Source.LocalState) (locals : EvoIR.Core.Source.Locals), state.Realizes locals → "
        "∀ (id : Nat) (destination : EvoIR.Core.Source.QualifiedType), "
        "Option.map Prod.snd (List.find? (fun pair => pair.fst == id) state.bindings) = none → "
        "{ bindings := (id, destination) :: state.bindings, initialized := state.initialized }.Realizes "
        "({ symbol := id, type := destination, value := none } :: locals)",
    ),
    (
        "EvoIR.Core.Source.checked_assignment_enter",
        "∀ (callees : List EvoIR.Core.Source.Function) (fuel : Nat) (static : EvoIR.Core.Source.LocalState) "
        "(state : EvoIR.Core.Source.State) (type : EvoIR.Core.Source.IntType) (op : EvoIR.Core.Source.Assignment) "
        "(id : Nat) (destination : EvoIR.Core.Source.QualifiedType) (target rhs : EvoIR.Core.Source.Expr) "
        "(facts : EvoIR.Core.Source.ExpressionFacts), "
        "EvoIR.Core.Source.checkExpr callees fuel static (EvoIR.Core.Source.Expr.assign type op id destination target rhs) = "
        "Except.ok facts → static.Realizes state.locals → "
        "EvoIR.Core.Source.enterExpression state (EvoIR.Core.Source.Expr.assign type op id destination target rhs) = "
        "{ control := EvoIR.Core.Source.Control.expression rhs, locals := state.locals, "
        "continuation := EvoIR.Core.Source.Frame.assign EvoIR.Core.Source.Value type op id destination :: state.continuation, "
        "functionId := state.functionId }",
    ),
    (
        "EvoIR.Core.Source.checked_assignment_continuation_safe",
        "∀ (callees : List EvoIR.Core.Source.Function) (fuel : Nat) (static : EvoIR.Core.Source.LocalState) "
        "(state : EvoIR.Core.Source.State) (type : EvoIR.Core.Source.IntType) (op : EvoIR.Core.Source.Assignment) "
        "(id : Nat) (destination : EvoIR.Core.Source.QualifiedType) (target rhs : EvoIR.Core.Source.Expr) "
        "(facts : EvoIR.Core.Source.ExpressionFacts) (value : EvoIR.Core.Source.Value), "
        "EvoIR.Core.Source.checkExpr callees fuel static (EvoIR.Core.Source.Expr.assign type op id destination target rhs) = "
        "Except.ok facts → static.Realizes state.locals → "
        "EvoIR.Core.Source.WriteOutcome state { bindings := static.bindings, initialized := id :: static.initialized } type "
        "(EvoIR.Core.Source.resumeValue state "
        "(EvoIR.Core.Source.Frame.assign EvoIR.Core.Source.Value type op id destination) value)",
    ),
    (
        "EvoIR.Core.Source.checked_update_safe",
        "∀ (callees : List EvoIR.Core.Source.Function) (fuel : Nat) (static : EvoIR.Core.Source.LocalState) "
        "(state : EvoIR.Core.Source.State) (type : EvoIR.Core.Source.IntType) (op : EvoIR.Core.Source.Update) (id : Nat) "
        "(destination : EvoIR.Core.Source.QualifiedType) (target : EvoIR.Core.Source.Expr) "
        "(facts : EvoIR.Core.Source.ExpressionFacts), "
        "EvoIR.Core.Source.checkExpr callees fuel static (EvoIR.Core.Source.Expr.update type op id destination target) = "
        "Except.ok facts → static.Realizes state.locals → EvoIR.Core.Source.WriteOutcome state static type "
        "(EvoIR.Core.Source.enterExpression state (EvoIR.Core.Source.Expr.update type op id destination target))",
    ),
    (
        "EvoIR.Core.Source.initialization_continuation_safe",
        "∀ (static : EvoIR.Core.Source.LocalState) (state : EvoIR.Core.Source.State), static.Realizes state.locals → "
        "∀ (id : Nat) (destination : EvoIR.Core.Source.QualifiedType) (slot : EvoIR.Core.Source.Slot), "
        "List.find? (fun x => x.symbol == id) state.locals = some slot → slot.type = destination → "
        "(destination.isConst = true → slot.value = none) → ∀ (value : EvoIR.Core.Source.Value), "
        "∃ locals, { bindings := static.bindings, initialized := id :: static.initialized }.Realizes locals ∧ "
        "EvoIR.Core.Source.resumeValue state "
        "(EvoIR.Core.Source.Frame.initialize EvoIR.Core.Source.Value id destination) value = "
        "{ control := EvoIR.Core.Source.Control.flow EvoIR.Core.Source.Flow.normal, locals := locals, "
        "continuation := state.continuation, functionId := state.functionId }",
    ),
    (
        "EvoIR.Core.Source.caller_restore_safe",
        "∀ (static : EvoIR.Core.Source.LocalState) (locals : EvoIR.Core.Source.Locals), static.Realizes locals → "
        "∀ (state : EvoIR.Core.Source.State) (callerId : Nat) (type : EvoIR.Core.Source.IntType) (value : EvoIR.Core.Source.Value), "
        "value.type = type → EvoIR.Core.Source.resumeFlow state "
        "(EvoIR.Core.Source.Frame.function EvoIR.Core.Source.Value (some (callerId, locals)) type) "
        "(EvoIR.Core.Source.Flow.returned value) = "
        "{ control := EvoIR.Core.Source.Control.value value, locals := locals, continuation := state.continuation, "
        "functionId := callerId } ∧ static.Realizes (EvoIR.Core.Source.resumeFlow state "
        "(EvoIR.Core.Source.Frame.function EvoIR.Core.Source.Value (some (callerId, locals)) type) "
        "(EvoIR.Core.Source.Flow.returned value)).locals",
    ),
    (
        "EvoIR.Core.LoweringRefines.source_no_malformed",
        "∀ {s : EvoIR.Core.Source.Program} {t : EvoIR.Core.IR.Program} {resultType : EvoIR.Core.Source.IntType} "
        "{s₀ : EvoIR.Core.Source.State} {t₀ : EvoIR.Core.IR.State}, "
        "EvoIR.Core.LoweringRefines s t resultType s₀ t₀ → "
        "∀ (message : String) (locals : EvoIR.Core.Source.Locals) (continuation : List EvoIR.Core.Source.Frame) "
        "(functionId : Nat), ¬EvoIR.Core.Source.Steps s s₀ "
        "{ control := EvoIR.Core.Source.Control.halted (Except.error (EvoIR.Core.Source.Fault.malformed message)), "
        "locals := locals, continuation := continuation, functionId := functionId }",
    ),
    (
        "EvoIR.Core.Source.checked_identifier_step",
        "∀ (program : EvoIR.Core.Source.Program) (callees : List EvoIR.Core.Source.Function) (fuel : Nat) "
        "(state : EvoIR.Core.Source.LocalState) (locals : EvoIR.Core.Source.Locals) (type : EvoIR.Core.Source.IntType) "
        "(id : Nat) (facts : EvoIR.Core.Source.ExpressionFacts) (continuation : List EvoIR.Core.Source.Frame) "
        "(functionId : Nat), "
        "EvoIR.Core.Source.checkExpr callees fuel state (EvoIR.Core.Source.Expr.ident type id) = Except.ok facts → "
        "state.Realizes locals → ∃ value, value.type = type ∧ EvoIR.Core.Source.Step program "
        "{ control := EvoIR.Core.Source.Control.expression (EvoIR.Core.Source.Expr.ident type id), locals := locals, "
        "continuation := continuation, functionId := functionId } "
        "{ control := EvoIR.Core.Source.Control.value value, locals := locals, continuation := continuation, "
        "functionId := functionId }",
    ),
    (
        "EvoIR.Core.Source.checked_binary_continuation_safe",
        "∀ (callees : List EvoIR.Core.Source.Function) (fuel : Nat) (static : EvoIR.Core.Source.LocalState) "
        "(state : EvoIR.Core.Source.State) (type : EvoIR.Core.Source.IntType) (op : EvoIR.Core.Source.Binary) "
        "(left right : EvoIR.Core.Source.Expr) (facts : EvoIR.Core.Source.ExpressionFacts) (a b : EvoIR.Core.Source.Value), "
        "EvoIR.Core.Source.checkExpr callees fuel static (EvoIR.Core.Source.Expr.binary type op left right) = Except.ok facts → "
        "a.type = left.type → b.type = right.type → EvoIR.Core.Source.SafeValueControl type "
        "(EvoIR.Core.Source.resumeValue state (EvoIR.Core.Source.Frame.binaryRight EvoIR.Core.Source.Value type op a) b).control",
    ),
    (
        "EvoIR.Core.Source.unary_continuation_safe",
        "∀ (state : EvoIR.Core.Source.State) (op : EvoIR.Core.Source.Unary) (value : EvoIR.Core.Source.Value), "
        "EvoIR.Core.Source.SafeValueControl "
        "(if (op == EvoIR.Core.Source.Unary.not) = true then EvoIR.Core.Source.IntType.int else value.type) "
        "(EvoIR.Core.Source.resumeValue state "
        "(EvoIR.Core.Source.Frame.unary EvoIR.Core.Source.Value "
        "(if (op == EvoIR.Core.Source.Unary.not) = true then EvoIR.Core.Source.IntType.int else value.type) op) value).control",
    ),
    (
        "EvoIR.Core.Symbolic.Term.constant_value_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) {n : Nat} "
        "(term : EvoIR.Core.Symbolic.Term Γ n) (value : BitVec n), "
        "term.constantValue = some value → EvoIR.Core.Symbolic.denoteTerm σ term = value",
    ),
    (
        "EvoIR.Core.Symbolic.Term.normalize_value_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) {n : Nat} "
        "(term : EvoIR.Core.Symbolic.Term Γ n), "
        "EvoIR.Core.Symbolic.denoteTerm σ term.normalizeValue = EvoIR.Core.Symbolic.denoteTerm σ term",
    ),
    (
        "EvoIR.Core.Transition.OpenCheckpoint.closeWithin",
        "∀ {S : Type u_1} {T : Type u_2} {O : Type u_3} {sourceStep : S → Option S} "
        "{targetStep : T → Option T} {sourceObserve : S → Option O} {targetObserve : T → Option O} "
        "{boundary nextBoundary : S → T → Prop} {s : S} {t : T}, "
        "(∀ (a : S) (b : T), boundary a b → EvoIR.Core.Transition.OpenCheckpoint "
        "sourceStep targetStep sourceObserve targetObserve (fun x y => boundary x y ∨ nextBoundary x y) a b) → "
        "boundary s t → EvoIR.Core.Transition.OpenCheckpoint "
        "sourceStep targetStep sourceObserve targetObserve nextBoundary s t",
    ),
    (
        "EvoIR.Core.Symbolic.openCallPair",
        "(source : EvoIR.Core.Source.Program) → (target : EvoIR.Core.IR.Program) → "
        "EvoIR.Core.Symbolic.CalleeProposals → Nat → "
        "Option (EvoIR.Core.Symbolic.CallFrontierPair source target)",
    ),
    (
        "EvoIR.Core.Transition.OpenCheckpoint.close",
        "∀ {S : Type u_1} {T : Type u_2} {O : Type u_3} {sourceStep : S → Option S} "
        "{targetStep : T → Option T} {sourceObserve : S → Option O} {targetObserve : T → Option O} "
        "{boundary : S → T → Prop} {s : S} {t : T}, "
        "(∀ (a : S) (b : T), boundary a b → EvoIR.Core.Transition.OpenCheckpoint "
        "sourceStep targetStep sourceObserve targetObserve boundary a b) → boundary s t → "
        "EvoIR.Core.Transition.Refines sourceStep targetStep sourceObserve targetObserve s t",
    ),
    (
        "EvoIR.Core.Transition.OpenCheckpoint.bind",
        "∀ {S : Type u_1} {T : Type u_2} {O : Type u_3} {sourceStep : S → Option S} "
        "{targetStep : T → Option T} {sourceObserve : S → Option O} {targetObserve : T → Option O} "
        "{boundary nextBoundary : S → T → Prop} {s : S} {t : T}, "
        "EvoIR.Core.Transition.OpenCheckpoint sourceStep targetStep sourceObserve targetObserve boundary s t → "
        "(∀ (a : S) (b : T), boundary a b → nextBoundary a b ∨ "
        "EvoIR.Core.Transition.OpenCheckpoint sourceStep targetStep sourceObserve targetObserve nextBoundary a b) → "
        "EvoIR.Core.Transition.OpenCheckpoint sourceStep targetStep sourceObserve targetObserve nextBoundary s t",
    ),
    (
        "EvoIR.Core.Symbolic.callingLoopPair",
        "(source : EvoIR.Core.Source.Program) → (target : EvoIR.Core.IR.Program) → "
        "EvoIR.Core.Symbolic.CalleeProposals → Nat → "
        "Option (EvoIR.Core.Symbolic.CallFrontierPair source target)",
    ),
    (
        "EvoIR.Core.Symbolic.cutMatchUnder",
        "{Γ : EvoIR.Var → Nat} → {α β : Type} → {related : EvoIR.Subst Γ → α → β → Prop} → "
        "((path : EvoIR.Core.Symbolic.PathConditions Γ) → (a : α) → (b : β) → "
        "Option (PLift (∀ (σ : EvoIR.Subst Γ), EvoIR.Core.Symbolic.PathConditions.Holds σ path → related σ a b))) → "
        "Nat → (path : EvoIR.Core.Symbolic.PathConditions Γ) → (a : EvoIR.Core.Symbolic.CutTree Γ α) → "
        "(b : EvoIR.Core.Symbolic.CutTree Γ β) → Option (PLift (EvoIR.Core.Symbolic.CutMatchUnder related path a b))",
    ),
    (
        "EvoIR.Core.Symbolic.PathConditions.for_return_sound",
        "∀ (σ : EvoIR.Subst EvoIR.Core.Symbolic.inputWidths) (value : EvoIR.Core.Source.Value) "
        "(path : EvoIR.Core.Symbolic.PathConditions EvoIR.Core.Symbolic.inputWidths), "
        "EvoIR.Core.Symbolic.PathConditions.Holds (EvoIR.Core.Symbolic.returnSubst σ value) path.forReturn ↔ "
        "EvoIR.Core.Symbolic.PathConditions.Holds σ path",
    ),
    (
        "EvoIR.Core.Transition.refinement_simulation",
        "∀ {S : Type u_1} {T : Type u_2} {O : Type u_3} (sourceStep : S → Option S) "
        "(targetStep : T → Option T) (sourceObserve : S → Option O) (targetObserve : T → Option O), "
        "EvoIR.Core.Transition.CheckpointSimulation sourceStep targetStep sourceObserve targetObserve "
        "(EvoIR.Core.Transition.Refines sourceStep targetStep sourceObserve targetObserve)",
    ),
    (
        "EvoIR.Core.Symbolic.saved_frames_for_return",
        "∀ (σ : EvoIR.Subst EvoIR.Core.Symbolic.inputWidths) (value : EvoIR.Core.Source.Value) "
        "(frames : EvoIR.Core.Symbolic.SavedFrames EvoIR.Core.Symbolic.inputWidths), "
        "EvoIR.Core.Symbolic.SavedFrames.denote (EvoIR.Core.Symbolic.returnSubst σ value) frames.forReturn = "
        "EvoIR.Core.Symbolic.SavedFrames.denote σ frames",
    ),
    (
        "EvoIR.Core.Symbolic.checkedSuffix",
        "(source : EvoIR.Core.Source.Program) → (target : EvoIR.Core.IR.Program) → (fid : Nat) → "
        "(f : EvoIR.Core.IR.Function) → Bool → (type : EvoIR.Core.Source.IntType) → "
        "EvoIR.Core.Symbolic.CalleeProposals → Nat → "
        "EvoIR.Core.Symbolic.ContinuationBuilder source target fid f type",
    ),
    (
        "EvoIR.Core.Symbolic.CallFrontierPair.refines",
        "∀ {source : EvoIR.Core.Source.Program} {target : EvoIR.Core.IR.Program} "
        "(pair : EvoIR.Core.Symbolic.CallFrontierPair source target) (args : List EvoIR.Core.Source.Value), "
        "List.map EvoIR.Core.Source.Value.type args = List.map (fun p => p.type.type) pair.sourceFunction.parameters → "
        "∃ sourceEntry targetEntry, EvoIR.Core.Source.initial source pair.sourceFunction.symbol args = "
        "Except.ok sourceEntry ∧ EvoIR.Core.IR.initial target pair.targetFunction.name "
        "(EvoIR.Core.Symbolic.loopInputValues pair.tokens args) = Except.ok targetEntry ∧ "
        "EvoIR.Core.LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry",
    ),
    # Audit the proof-producing executable too, not only the pair's projection theorem.
    (
        "EvoIR.Core.Symbolic.callFrontierPair",
        "(source : EvoIR.Core.Source.Program) → (target : EvoIR.Core.IR.Program) → "
        "EvoIR.Core.Symbolic.CalleeProposals → Nat → "
        "Option (EvoIR.Core.Symbolic.CallFrontierPair source target)",
    ),
    (
        "EvoIR.Core.Symbolic.call_target_conditional",
        "∀ {Γ : EvoIR.Var → Nat} (p : EvoIR.Core.IR.Program) (f : EvoIR.Core.IR.Function) (b yesBlock "
        "noBlock : EvoIR.Core.IR.Block) (scope yesScope noScope : EvoIR.Core.Symbolic.Scope Γ) (id : "
        "Nat) (condition : EvoIR.Core.Symbolic.Term Γ 1) (yes no : EvoIR.Core.IR.Edge) (a c : "
        "EvoIR.Core.Symbolic.ValueTree Γ), List.find? (fun x => x.name == f.name) p.functions = some f → "
        "List.find? (fun x => x.name == b.name) f.blocks = some b → b.terminator = "
        "EvoIR.Core.IR.Terminator.conditional id yes no → yesBlock.name = yes.block → noBlock.name = "
        "no.block → scope.word id = some ⟨1, condition⟩ → EvoIR.Core.Symbolic.edge "
        "EvoIR.Core.Symbolic.callTokenTypes f scope yes = some yesScope → EvoIR.Core.Symbolic.edge "
        "EvoIR.Core.Symbolic.callTokenTypes f scope no = some noScope → "
        "EvoIR.Core.Symbolic.CallTargetDiagram p f yesBlock 0 yesScope a → "
        "EvoIR.Core.Symbolic.CallTargetDiagram p f noBlock 0 noScope c → "
        "EvoIR.Core.Symbolic.CallTargetDiagram p f b b.instructions.length scope "
        "(EvoIR.Core.Symbolic.ValueTree.branch condition a c)",
    ),
    (
        "EvoIR.Core.Symbolic.call_target_call",
        "∀ {Γ : EvoIR.Var → Nat} (p : EvoIR.Core.IR.Program) (f callee : EvoIR.Core.IR.Function) (b : "
        "EvoIR.Core.IR.Block) (pc : Nat) (scope : EvoIR.Core.Symbolic.Scope Γ) (inst : "
        "EvoIR.Core.IR.Instruction) (args : List (EvoIR.Core.Symbolic.Slot Γ)) (called result : "
        "EvoIR.Core.Symbolic.ValueTree Γ), List.find? (fun x => x.name == f.name) p.functions = some f → "
        "List.find? (fun x => x.name == b.name) f.blocks = some b → b.instructions[pc]? = some inst → "
        "inst.op = EvoIR.Core.IR.Operation.call callee.name → List.find? (fun x => x.name == "
        "callee.name) p.functions = some callee → List.mapM (fun id => List.lookup id scope) "
        "inst.operands = some args → List.map EvoIR.Core.IR.Binding.type inst.results = callee.returns → "
        "∀ (body : EvoIR.Core.Symbolic.CallFunctionDiagram p callee args called), "
        "EvoIR.Core.Symbolic.ValueBind (fun word tree => ∃ next, scope.appendSlots inst.results "
        "(EvoIR.Core.Symbolic.callResults word) = some next ∧ EvoIR.Core.Symbolic.CallTargetDiagram p f "
        "b (pc + 1) next tree) called result → EvoIR.Core.Symbolic.CallTargetDiagram p f b pc scope "
        "result",
    ),
    (
        "EvoIR.Core.Symbolic.context_short_diagram",
        "∀ {Γ : EvoIR.Var → Nat} (p : EvoIR.Core.Source.Program) (locals : "
        "EvoIR.Core.Symbolic.LocalStore Γ) (isOr : Bool) (left right : EvoIR.Core.Source.Expr) (first "
        "output : EvoIR.Core.Symbolic.StateTree Γ), EvoIR.Core.Symbolic.ContextDiagram p locals left "
        "first → EvoIR.Core.Symbolic.TreeBind (EvoIR.Core.Symbolic.ContextShort p isOr right) first "
        "output → EvoIR.Core.Symbolic.ContextDiagram p locals (EvoIR.Core.Source.Expr.binary "
        "EvoIR.Core.Source.IntType.int (if isOr = true then EvoIR.Core.Source.Binary.or else "
        "EvoIR.Core.Source.Binary.and) left right) output",
    ),
    (
        "EvoIR.Core.Symbolic.Term.normalize_casts_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) {n : Nat} (term : EvoIR.Core.Symbolic.Term Γ n), "
        "EvoIR.Core.Symbolic.denoteTerm σ term.normalizeCasts = EvoIR.Core.Symbolic.denoteTerm σ term",
    ),
    (
        "EvoIR.Core.Symbolic.branch_condition_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (term : EvoIR.Core.Symbolic.Term Γ 1), "
        "((EvoIR.Core.Symbolic.denoteTerm σ (EvoIR.Core.Symbolic.branchCondition term)).toNat != 0) = "
        "((EvoIR.Core.Symbolic.denoteTerm σ term).toNat != 0)",
    ),
    (
        "EvoIR.Core.Symbolic.call_flow_sound",
        "∀ {Γ : EvoIR.Var → Nat} (p : EvoIR.Core.Source.Program) (fuel : Nat) (locals : "
        "EvoIR.Core.Symbolic.LocalStore Γ) (control : EvoIR.Core.Source.Control) (tree : "
        "EvoIR.Core.Symbolic.FlowTree Γ), Option.map Subtype.val (EvoIR.Core.Symbolic.callFlow p "
        "fuel locals control) = some tree → EvoIR.Core.Symbolic.ContextFlowDiagram p locals control tree",
    ),
    (
        "EvoIR.Core.Symbolic.call_target_uninitialized",
        "∀ {Γ : EvoIR.Var → Nat} (p : EvoIR.Core.IR.Program) (f : EvoIR.Core.IR.Function) (b : "
        "EvoIR.Core.IR.Block) (pc width : Nat) (scope next : EvoIR.Core.Symbolic.Scope Γ) (inst : "
        "EvoIR.Core.IR.Instruction) (result : EvoIR.Core.Symbolic.ValueTree Γ), List.find? "
        "(fun x => x.name == f.name) p.functions = some f → List.find? (fun x => x.name == b.name) "
        "f.blocks = some b → b.instructions[pc]? = some inst → inst.op = "
        "EvoIR.Core.IR.Operation.uninitialized (EvoIR.Core.IR.Ty.bits width) → inst.operands = [] → "
        "scope.appendSlots inst.results [EvoIR.Core.Symbolic.Slot.opaque (width + 2)] = some next → "
        "EvoIR.Core.Symbolic.CallTargetDiagram p f b (pc + 1) next result → "
        "EvoIR.Core.Symbolic.CallTargetDiagram p f b pc scope result",
    ),
    (
        "EvoIR.Core.Symbolic.call_target_branch",
        "∀ {Γ : EvoIR.Var → Nat} (p : EvoIR.Core.IR.Program) (f : EvoIR.Core.IR.Function) (b "
        "nextBlock : EvoIR.Core.IR.Block) (scope next : EvoIR.Core.Symbolic.Scope Γ) (successor : "
        "EvoIR.Core.IR.Edge) (result : EvoIR.Core.Symbolic.ValueTree Γ), List.find? (fun x "
        "=> x.name == f.name) p.functions = some f → List.find? (fun x => x.name == b.name) "
        "f.blocks = some b → b.terminator = EvoIR.Core.IR.Terminator.branch successor → "
        "nextBlock.name = successor.block → EvoIR.Core.Symbolic.edge "
        "EvoIR.Core.Symbolic.callTokenTypes f scope successor = some next → "
        "EvoIR.Core.Symbolic.CallTargetDiagram p f nextBlock 0 next result → "
        "EvoIR.Core.Symbolic.CallTargetDiagram p f b b.instructions.length scope result",
    ),
    (
        "EvoIR.Core.Symbolic.CallPair.refines",
        "∀ {source : EvoIR.Core.Source.Program} {target : EvoIR.Core.IR.Program} (pair : "
        "EvoIR.Core.Symbolic.CallPair source target) (args : List EvoIR.Core.Source.Value), List.map "
        "EvoIR.Core.Source.Value.type args = List.map (fun p => p.type.type) "
        "pair.sourceFunction.parameters → ∃ sourceEntry targetEntry, EvoIR.Core.Source.initial source "
        "pair.sourceFunction.symbol args = Except.ok sourceEntry ∧ EvoIR.Core.IR.initial target "
        "pair.targetFunction.name (List.map EvoIR.Core.Symbolic.sourceValueIR args ++ "
        "[EvoIR.Core.IR.Value.memory { nextId := 1, objects := [] }, EvoIR.Core.IR.Value.world]) = "
        "Except.ok targetEntry ∧ EvoIR.Core.LoweringRefines source target "
        "pair.sourceFunction.returnType.type sourceEntry targetEntry",
    ),
    (
        "EvoIR.Core.Symbolic.call_function_sound",
        "∀ {Γ : EvoIR.Var → Nat} (p : EvoIR.Core.Source.Program) (fuel : Nat) (f : "
        "EvoIR.Core.Source.Function) (args : List (EvoIR.Core.Symbolic.CWord Γ)) (tree : "
        "EvoIR.Core.Symbolic.ValueTree Γ), Option.map Subtype.val (EvoIR.Core.Symbolic.callFunction p "
        "fuel f args) = some tree → EvoIR.Core.Symbolic.InvocationDiagram p f args tree",
    ),
    (
        "EvoIR.Core.Symbolic.CallFunctionDiagram.completes",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) {p : EvoIR.Core.IR.Program} {f : "
        "EvoIR.Core.IR.Function} {args : List (EvoIR.Core.Symbolic.Slot Γ)} {result : "
        "EvoIR.Core.Symbolic.ValueTree Γ} (diagram : EvoIR.Core.Symbolic.CallFunctionDiagram p "
        "f args result), List.find? (fun x => x.name == f.name) p.functions = some f → ∃ entry last, "
        "EvoIR.Core.IR.initial p f.name (List.map (EvoIR.Core.Symbolic.Slot.denote σ "
        "EvoIR.Core.Symbolic.callTokens) args) = Except.ok entry ∧ EvoIR.Core.IR.Steps p entry last ∧ "
        "last.control = EvoIR.Core.IR.Control.halted (Except.map (fun value => "
        "[EvoIR.Core.Symbolic.sourceValueIR value, EvoIR.Core.IR.Value.memory { nextId := 1, objects "
        ":= [] }, EvoIR.Core.IR.Value.world]) (EvoIR.Core.Symbolic.ValueTree.denote σ result))",
    ),
    (
        "EvoIR.Core.IR.call_enters",
        "∀ (p : EvoIR.Core.IR.Program) (s : EvoIR.Core.IR.State) (a entered : EvoIR.Core.IR.Activation) "
        "(f callee : EvoIR.Core.IR.Function) (b : EvoIR.Core.IR.Block) (inst : "
        "EvoIR.Core.IR.Instruction) (args : List EvoIR.Core.IR.Value), s.control = "
        "EvoIR.Core.IR.Control.running a → List.find? (fun x => x.name == a.function) p.functions = some "
        "f → List.find? (fun x => x.name == a.block) f.blocks = some b → b.instructions[a.pc]? = some "
        "inst → inst.op = EvoIR.Core.IR.Operation.call callee.name → List.find? (fun x => x.name == "
        "callee.name) p.functions = some callee → List.mapM (EvoIR.Core.IR.lookup a.values) "
        "inst.operands = Except.ok args → List.map EvoIR.Core.IR.Binding.type inst.results = "
        "callee.returns → EvoIR.Core.IR.enter callee args s.nextFrame = Except.ok entered → "
        "EvoIR.Core.IR.Step p s { control := EvoIR.Core.IR.Control.running entered, stack := { caller := "
        "{ function := a.function, block := a.block, pc := a.pc + 1, values := a.values, owner := "
        "a.owner }, results := inst.results } :: s.stack, nextFrame := s.nextFrame + 1 }",
    ),
    (
        "EvoIR.Core.IR.call_returns",
        "∀ (p : EvoIR.Core.IR.Program) (s : EvoIR.Core.IR.State) (a : EvoIR.Core.IR.Activation) (f : "
        "EvoIR.Core.IR.Function) (b : EvoIR.Core.IR.Block) (ids : List Nat) (values : List "
        "EvoIR.Core.IR.Value) (frame : EvoIR.Core.IR.ReturnFrame) (rest : List "
        "EvoIR.Core.IR.ReturnFrame) (bound : EvoIR.Core.IR.Bindings), s.control = "
        "EvoIR.Core.IR.Control.running a → List.find? (fun x => x.name == a.function) p.functions = some "
        "f → List.find? (fun x => x.name == a.block) f.blocks = some b → a.pc = b.instructions.length → "
        "b.terminator = EvoIR.Core.IR.Terminator.return ids → List.mapM (EvoIR.Core.IR.lookup a.values) "
        "ids = Except.ok values → List.map EvoIR.Core.IR.Value.type values = f.returns → s.stack = frame "
        ":: rest → EvoIR.Core.IR.appendResults frame.caller.values frame.results "
        "(EvoIR.Core.IR.returnedValues a.owner values) = Except.ok bound → EvoIR.Core.IR.Step p s { "
        "control := EvoIR.Core.IR.Control.running (have __src := frame.caller; { function := "
        "__src.function, block := __src.block, pc := __src.pc, values := bound, owner := __src.owner }), "
        "stack := rest, nextFrame := s.nextFrame }",
    ),
    (
        "EvoIR.Core.Source.invoke_body",
        "∀ (p : EvoIR.Core.Source.Program) (callee : EvoIR.Core.Source.Function) "
        "(args : List EvoIR.Core.Source.Value) (calleeLocals callerLocals : EvoIR.Core.Source.Locals) "
        "(caller : Nat) (outcome : EvoIR.Core.Source.StateOutcome), "
        "List.find? (fun x => x.symbol == callee.symbol) p.functions = some callee → "
        "EvoIR.Core.Source.parameterLocals callee.parameters args = Except.ok calleeLocals → "
        "EvoIR.Core.Source.ReturnsState p calleeLocals callee.symbol callee.body outcome → "
        "(∀ (value : EvoIR.Core.Source.Value), outcome.snd = Except.ok value → "
        "value.type = callee.returnType.type) → "
        "EvoIR.Core.Source.Invokes p callerLocals caller callee args outcome.snd",
    ),
    (
        "EvoIR.Core.Source.arguments_cons_ok",
        "∀ {p : EvoIR.Core.Source.Program} {locals next : EvoIR.Core.Source.Locals} {fid : Nat} "
        "(type : EvoIR.Core.Source.IntType) (callee : Nat) (destination : EvoIR.Core.Source.QualifiedType) "
        "(expr : EvoIR.Core.Source.Expr) "
        "(pending : List (EvoIR.Core.Source.QualifiedType × EvoIR.Core.Source.Expr)) "
        "(reverseValues : List EvoIR.Core.Source.Value) (value : EvoIR.Core.Source.Value) "
        "(outcome : EvoIR.Core.Source.Locals × Except EvoIR.Core.Source.Fault (List EvoIR.Core.Source.Value)), "
        "EvoIR.Core.Source.EvaluatesContext p locals fid expr (next, Except.ok value) → "
        "EvoIR.Core.Source.ArgumentsRun p next fid type callee pending "
        "(EvoIR.Core.Source.Value.cast destination.type value :: reverseValues) outcome → "
        "EvoIR.Core.Source.ArgumentsRun p locals fid type callee ((destination, expr) :: pending) reverseValues outcome",
    ),
    (
        "EvoIR.Core.Source.evaluates_call",
        "∀ {p : EvoIR.Core.Source.Program} {locals next : EvoIR.Core.Source.Locals} {fid : Nat} "
        "(callee : EvoIR.Core.Source.Function) "
        "(arguments : List (EvoIR.Core.Source.QualifiedType × EvoIR.Core.Source.Expr)) "
        "(values : List EvoIR.Core.Source.Value) (result : EvoIR.Core.Source.Result), "
        "EvoIR.Core.Source.ArgumentsRun p locals fid callee.returnType.type callee.symbol arguments [] "
        "(next, Except.ok values) → EvoIR.Core.Source.Invokes p next fid callee values result → "
        "EvoIR.Core.Source.EvaluatesContext p locals fid "
        "(EvoIR.Core.Source.Expr.call callee.returnType.type callee.symbol arguments) (next, result)",
    ),
    (
        "EvoIR.Core.Symbolic.Term.of_p0_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) {n : Nat} (term : EvoIR.Term Γ n), "
        "EvoIR.Core.Symbolic.denoteTerm σ (EvoIR.Core.Symbolic.Term.ofP0 term) = "
        "EvoIR.denoteTerm σ term",
    ),
    (
        "EvoIR.Core.Symbolic.division_trace_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) {n : Nat} "
        "(signed remainder : Bool) (a b : EvoIR.Core.Symbolic.Term Γ n), "
        "EvoIR.Core.IR.evalDivision signed remainder n "
        "[EvoIR.Core.IR.Value.bits n (EvoIR.Core.Symbolic.denoteTerm σ a), "
        "EvoIR.Core.IR.Value.bits n (EvoIR.Core.Symbolic.denoteTerm σ b)] = "
        "do EvoIR.Core.Symbolic.guards σ (EvoIR.Core.Symbolic.divisionGuards signed remainder a b) "
        "pure (EvoIR.Core.IR.Value.bits n "
        "(EvoIR.Core.Symbolic.denoteTerm σ (EvoIR.Core.Symbolic.Term.division signed remainder a b)))",
    ),
    (
        "EvoIR.Core.Symbolic.division_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (remainder : Bool) "
        "(a b : EvoIR.Core.Symbolic.CWord Γ), "
        "(do EvoIR.Core.Symbolic.guards σ (EvoIR.Core.Symbolic.division remainder a b).fst "
        "pure (EvoIR.Core.Symbolic.CWord.denote σ (EvoIR.Core.Symbolic.division remainder a b).snd)) = "
        "EvoIR.Core.Source.binaryValue (if remainder = true then EvoIR.Core.Source.Binary.rem "
        "else EvoIR.Core.Source.Binary.div) "
        "(EvoIR.Core.Symbolic.CWord.denote σ a) (EvoIR.Core.Symbolic.CWord.denote σ b)",
    ),
    (
        "EvoIR.Core.Symbolic.normalized_traces_equal",
        "∀ {Γ : EvoIR.Var → Nat} (a b : EvoIR.Core.Symbolic.ValueTree Γ), "
        "a.normalizeTrace = b.normalizeTrace → "
        "∀ (σ : EvoIR.Subst Γ), EvoIR.Core.Symbolic.ValueTree.denote σ a = EvoIR.Core.Symbolic.ValueTree.denote σ b",
    ),
    (
        "EvoIR.Core.Symbolic.canonical_store_covers",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (locals : EvoIR.Core.Symbolic.LocalStore Γ), "
        "EvoIR.Core.Symbolic.LocalStore.denote (EvoIR.Core.Symbolic.storeSubst σ locals) "
        "(EvoIR.Core.Symbolic.canonicalStore (List.map EvoIR.Core.Symbolic.LocalSlot.shape locals)) = "
        "EvoIR.Core.Symbolic.LocalStore.denote σ locals",
    ),
    (
        "EvoIR.Core.Symbolic.LoopPair.refines",
        "∀ {source : EvoIR.Core.Source.Program} {target : EvoIR.Core.IR.Program} "
        "(pair : EvoIR.Core.Symbolic.LoopPair source target) (args : List EvoIR.Core.Source.Value), "
        "List.map EvoIR.Core.Source.Value.type args = "
        "List.map (fun p => p.type.type) pair.sourceFunction.parameters → "
        "∃ sourceEntry targetEntry, "
        "EvoIR.Core.Source.initial source pair.sourceFunction.symbol args = Except.ok sourceEntry ∧ "
        "EvoIR.Core.IR.initial target pair.targetFunction.name "
        "(EvoIR.Core.Symbolic.loopInputValues pair.diagram.tokens args) = Except.ok targetEntry ∧ "
        "EvoIR.Core.LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry",
    ),
    (
        "EvoIR.Core.Symbolic.LoopDiagram.call_refines",
        "∀ (σ : EvoIR.Subst EvoIR.Core.Symbolic.inputWidths) {source : EvoIR.Core.Source.Function} "
        "{target : EvoIR.Core.IR.Function} (s : EvoIR.Core.Source.Program) (t : EvoIR.Core.IR.Program) "
        "(d : EvoIR.Core.Symbolic.LoopDiagram s t source target) (observationType : EvoIR.Core.Source.IntType) "
        "(caller : EvoIR.Core.Symbolic.SourceReturnContext) (a : EvoIR.Core.IR.Activation) (nextFrame : Nat) "
        "(stack : List EvoIR.Core.IR.ReturnFrame) (f : EvoIR.Core.IR.Function) (b : EvoIR.Core.IR.Block) "
        "(inst : EvoIR.Core.IR.Instruction), "
        "List.find? (fun x => x.symbol == source.symbol) s.functions = some source → "
        "List.find? (fun x => x.name == a.function) t.functions = some f → "
        "List.find? (fun x => x.name == a.block) f.blocks = some b → "
        "b.instructions[a.pc]? = some inst → inst.op = EvoIR.Core.IR.Operation.call target.name → "
        "List.find? (fun x => x.name == target.name) t.functions = some target → "
        "List.mapM (EvoIR.Core.IR.lookup a.values) inst.operands = Except.ok "
        "(EvoIR.Core.Symbolic.loopInputValues d.tokens (List.map (EvoIR.Core.Symbolic.CWord.denote σ) "
        "(EvoIR.Core.Symbolic.parameterWords source.parameters))) → "
        "List.map EvoIR.Core.IR.Binding.type inst.results = target.returns → "
        "∀ (resumed : EvoIR.Core.Source.State → EvoIR.Core.IR.State → Prop), "
        "EvoIR.Core.Transition.CheckpointSimulation (EvoIR.Core.Source.step s) (EvoIR.Core.IR.step t) "
        "EvoIR.Core.observeSource (EvoIR.Core.observeTarget observationType) resumed → "
        "(∀ (τ : EvoIR.Subst EvoIR.Core.Symbolic.inputWidths) "
        "(word : EvoIR.Core.Symbolic.CWord EvoIR.Core.Symbolic.inputWidths) (afterCalls : Nat), "
        "word.type = source.returnType.type → resumed "
        "{ control := EvoIR.Core.Source.Control.value (EvoIR.Core.Symbolic.CWord.denote τ word), "
        "locals := caller.locals, continuation := caller.continuation, functionId := caller.functionId } "
        "(EvoIR.Core.Symbolic.TargetExit.callState τ d.tokens target nextFrame afterCalls "
        "(EvoIR.Core.Symbolic.TargetExit.returned word) "
        "({ caller := { function := a.function, block := a.block, pc := a.pc + 1, values := a.values, "
        "owner := a.owner }, results := inst.results } :: stack))) → "
        "EvoIR.Core.LoweringRefines s t observationType "
        "{ control := EvoIR.Core.Source.Control.call source.symbol source.returnType.type "
        "(List.map (EvoIR.Core.Symbolic.CWord.denote σ) (EvoIR.Core.Symbolic.parameterWords source.parameters)), "
        "locals := caller.locals, continuation := caller.continuation, functionId := caller.functionId } "
        "{ control := EvoIR.Core.IR.Control.running a, stack := stack, nextFrame := nextFrame }",
    ),
    (
        "EvoIR.Core.Symbolic.ReturnContinuation.refines",
        "∀ {source : EvoIR.Core.Source.Program} {ir : EvoIR.Core.IR.Program} {fid : Nat} {target : "
        "EvoIR.Core.IR.Function} {tokens : Bool} {type : EvoIR.Core.Source.IntType} {cursor : "
        "EvoIR.Core.Symbolic.SourceCursor EvoIR.Core.Symbolic.inputWidths} {label : String} {pc : Nat} {scope : "
        "EvoIR.Core.Symbolic.Scope EvoIR.Core.Symbolic.inputWidths} "
        "{path : EvoIR.Core.Symbolic.PathConditions EvoIR.Core.Symbolic.inputWidths} "
        "(d : EvoIR.Core.Symbolic.ReturnContinuation source ir "
        "fid target tokens type cursor label pc scope path) (σ : EvoIR.Subst EvoIR.Core.Symbolic.inputWidths), "
        "EvoIR.Core.Symbolic.PathConditions.Holds σ path → ∀ (owner nextFrame "
        ": Nat), EvoIR.Core.LoweringRefines source ir type (EvoIR.Core.Symbolic.SourceCursor.denote σ fid cursor) "
        "(EvoIR.Core.Symbolic.cutTargetState σ target label pc owner nextFrame scope [])",
    ),
    (
        "EvoIR.Core.Symbolic.LoopDiagram.call_checked_return",
        "∀ (argumentsσ callerσ : EvoIR.Subst EvoIR.Core.Symbolic.inputWidths) {source : EvoIR.Core.Source.Function} "
        "{target : EvoIR.Core.IR.Function} (s : EvoIR.Core.Source.Program) (t : EvoIR.Core.IR.Program) (d : "
        "EvoIR.Core.Symbolic.LoopDiagram s t source target) (observationType : EvoIR.Core.Source.IntType) (fid : Nat) "
        "(locals : EvoIR.Core.Symbolic.LocalStore EvoIR.Core.Symbolic.inputWidths) "
        "(frames : EvoIR.Core.Symbolic.SavedFrames EvoIR.Core.Symbolic.inputWidths) "
        "(scope : EvoIR.Core.Symbolic.Scope EvoIR.Core.Symbolic.inputWidths) (owner nextFrame pc "
        ": Nat) (f : EvoIR.Core.IR.Function) (b : EvoIR.Core.IR.Block) (inst : EvoIR.Core.IR.Instruction) (callerTokens : "
        "Bool), List.find? (fun x => x.symbol == source.symbol) s.functions = some source → List.find? (fun x => x.name "
        "== f.name) t.functions = some f → List.find? (fun x => x.name == b.name) f.blocks = some b → b.instructions[pc]? "
        "= some inst → inst.op = EvoIR.Core.IR.Operation.call target.name → List.find? (fun x => x.name == target.name) "
        "t.functions = some target → List.mapM (EvoIR.Core.IR.lookup (EvoIR.Core.Symbolic.Scope.denote callerσ "
        "EvoIR.Core.Symbolic.callTokens scope)) inst.operands = Except.ok (EvoIR.Core.Symbolic.loopInputValues d.tokens "
        "(List.map (EvoIR.Core.Symbolic.CWord.denote argumentsσ) (EvoIR.Core.Symbolic.parameterWords source.parameters))) "
        "→ List.map EvoIR.Core.IR.Binding.type inst.results = target.returns → "
        "∀ (path : EvoIR.Core.Symbolic.PathConditions EvoIR.Core.Symbolic.inputWidths) (resumption : "
        "EvoIR.Core.Symbolic.CheckedReturn s t fid f callerTokens observationType source.returnType.type locals frames "
        "b.name (pc + 1) scope inst.results d.tokens path), "
        "EvoIR.Core.Symbolic.PathConditions.Holds callerσ path → EvoIR.Core.LoweringRefines s t observationType { control := "
        "EvoIR.Core.Source.Control.call source.symbol source.returnType.type (List.map (EvoIR.Core.Symbolic.CWord.denote "
        "argumentsσ) (EvoIR.Core.Symbolic.parameterWords source.parameters)), locals := "
        "EvoIR.Core.Symbolic.LocalStore.denote callerσ locals, "
        "continuation := EvoIR.Core.Symbolic.SavedFrames.denote callerσ frames, functionId := fid } "
        "(EvoIR.Core.Symbolic.cutTargetState callerσ f b.name pc owner nextFrame scope [])",
    ),
    (
        "EvoIR.Core.Symbolic.source_cut_diagram_sound",
        "∀ {Γ : EvoIR.Var → Nat} (p : EvoIR.Core.Source.Program) (cutpoints : List EvoIR.Core.Symbolic.SourceSite) "
        "(budget : Nat) (advance : Bool) (cursor : EvoIR.Core.Symbolic.SourceCursor Γ) "
        "(tree : EvoIR.Core.Symbolic.CutTree Γ (EvoIR.Core.Symbolic.SourceCursor Γ)), "
        "Option.map Subtype.val (EvoIR.Core.Symbolic.sourceCutDiagram p cutpoints budget advance cursor) = some tree → "
        "EvoIR.Core.Symbolic.SourceCutRuns p advance cursor tree",
    ),
    (
        "EvoIR.Core.Symbolic.target_cut_diagram_sound",
        "∀ {Γ : EvoIR.Var → Nat} (types : Nat → EvoIR.Core.IR.Ty) (f : EvoIR.Core.IR.Function) "
        "(type : EvoIR.Core.Source.IntType) (cutpoints : List String) (budget : Nat) (advance : Bool) "
        "(scope : EvoIR.Core.Symbolic.Scope Γ) (label : String) "
        "(tree : EvoIR.Core.Symbolic.CutTree Γ (EvoIR.Core.Symbolic.TargetExit Γ)), "
        "Option.map Subtype.val (EvoIR.Core.Symbolic.targetCutDiagram types f type cutpoints budget advance scope label) = some tree → "
        "EvoIR.Core.Symbolic.TargetCutDiagram types f type advance scope label tree",
    ),
    (
        "EvoIR.Core.Symbolic.checked_cutpoint_refines",
        "∀ {Γ : EvoIR.Var → Nat} (entries : List (EvoIR.Core.Symbolic.JointPoint Γ)) "
        "(fid : Nat) (type : EvoIR.Core.Source.IntType) (source : EvoIR.Core.Source.Program) "
        "(program : EvoIR.Core.IR.Program) (target : EvoIR.Core.IR.Function) (tokens : Bool) "
        "(spans : (point : EvoIR.Core.Symbolic.JointPoint Γ) → point ∈ entries → "
        "EvoIR.Core.Symbolic.CheckedSpan source program entries fid target tokens type point) "
        "(s : EvoIR.Core.Source.State) (t : EvoIR.Core.IR.State), "
        "EvoIR.Core.Symbolic.CutpointRelated entries fid target s t → "
        "EvoIR.Core.LoweringRefines source program type s t",
    ),
    (
        "EvoIR.Core.Symbolic.flow_diagram_sound",
        "∀ {Γ : EvoIR.Var → Nat} (depth : Nat) (locals : EvoIR.Core.Symbolic.LocalStore Γ) "
        "(control : EvoIR.Core.Source.Control) (tree : EvoIR.Core.Symbolic.FlowTree Γ), "
        "Option.map Subtype.val (EvoIR.Core.Symbolic.flowDiagram depth locals control) = some tree → "
        "EvoIR.Core.Symbolic.FlowDiagram locals control tree",
    ),
    (
        "EvoIR.Core.Symbolic.normalized_trees_equal",
        "∀ {Γ : EvoIR.Var → Nat} (a b : EvoIR.Core.Symbolic.ValueTree Γ), "
        "a.normalizeBranches = b.normalizeBranches → ∀ (σ : EvoIR.Subst Γ), "
        "EvoIR.Core.Symbolic.ValueTree.denote σ a = EvoIR.Core.Symbolic.ValueTree.denote σ b",
    ),
    (
        "EvoIR.Core.Symbolic.state_expression_sound",
        "∀ {Γ : EvoIR.Var → Nat} (depth : Nat) (locals : EvoIR.Core.Symbolic.LocalStore Γ) "
        "(expr : EvoIR.Core.Source.Expr) (tree : EvoIR.Core.Symbolic.StateTree Γ), "
        "Option.map Subtype.val (EvoIR.Core.Symbolic.stateExpression depth locals expr) = some tree → "
        "EvoIR.Core.Symbolic.StateDiagram locals expr tree",
    ),
    (
        "EvoIR.Core.Symbolic.parameter_store_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ps : List EvoIR.Core.Source.Parameter) "
        "(args : List (EvoIR.Core.Symbolic.CWord Γ)) (locals : EvoIR.Core.Symbolic.LocalStore Γ), "
        "EvoIR.Core.Symbolic.parameterStore ps args = some locals → "
        "EvoIR.Core.Source.parameterLocals ps (List.map (EvoIR.Core.Symbolic.CWord.denote σ) args) = "
        "Except.ok (EvoIR.Core.Symbolic.LocalStore.denote σ locals)",
    ),
    (
        "EvoIR.Core.Symbolic.local_write_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) "
        "(locals updated : EvoIR.Core.Symbolic.LocalStore Γ) (symbol : Nat) "
        "(destination : EvoIR.Core.Source.QualifiedType) (value : EvoIR.Core.Symbolic.CWord Γ) "
        "(initialization : Bool), locals.write symbol destination value initialization = some updated → "
        "EvoIR.Core.Source.writeSlot (EvoIR.Core.Symbolic.LocalStore.denote σ locals) symbol destination "
        "(EvoIR.Core.Symbolic.CWord.denote σ value) initialization = "
        "Except.ok (EvoIR.Core.Symbolic.LocalStore.denote σ updated)",
    ),
    (
        "EvoIR.Core.Symbolic.comparison_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) "
        "(op : EvoIR.Core.Symbolic.Comparison) (a b : EvoIR.Core.Symbolic.CWord Γ), "
        "EvoIR.Core.Source.binaryValue op.source "
        "(EvoIR.Core.Symbolic.CWord.denote σ a) (EvoIR.Core.Symbolic.CWord.denote σ b) = "
        "Except.ok (EvoIR.Core.Symbolic.CWord.denote σ (EvoIR.Core.Symbolic.comparison op a b))",
    ),
    (
        "EvoIR.Core.Symbolic.truth_term_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (value : EvoIR.Core.Symbolic.CWord Γ), "
        "((EvoIR.Core.Symbolic.denoteTerm σ value.truthTerm).toNat != 0) = "
        "(EvoIR.Core.Symbolic.CWord.denote σ value).truth",
    ),
    (
        "EvoIR.Core.Symbolic.unary_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (op : EvoIR.Core.Source.Unary) "
        "(value : EvoIR.Core.Symbolic.CWord Γ), "
        "(do EvoIR.Core.Symbolic.guards σ (EvoIR.Core.Symbolic.unary op value).fst "
        "pure (EvoIR.Core.Symbolic.CWord.denote σ (EvoIR.Core.Symbolic.unary op value).snd)) = "
        "EvoIR.Core.Source.unaryValue op (EvoIR.Core.Symbolic.CWord.denote σ value)",
    ),
    (
        "EvoIR.Core.Symbolic.ReturnPair.refines",
        "∀ {source : EvoIR.Core.Source.Program} {target : EvoIR.Core.IR.Program} "
        "(pair : EvoIR.Core.Symbolic.ReturnPair source target) (args : List EvoIR.Core.Source.Value), "
        "List.map EvoIR.Core.Source.Value.type args = "
        "List.map (fun p => p.type.type) pair.sourceFunction.parameters → "
        "∃ sourceEntry targetEntry, "
        "EvoIR.Core.Source.initial source pair.sourceFunction.symbol args = Except.ok sourceEntry ∧ "
        "EvoIR.Core.IR.initial target pair.targetFunction.name "
        "(List.map EvoIR.Core.Symbolic.sourceValueIR args) = Except.ok targetEntry ∧ "
        "EvoIR.Core.LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry",
    ),
    (
        "EvoIR.Core.Symbolic.bitwise_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) "
        "(op : EvoIR.Core.Symbolic.Bitwise) "
        "(a b : EvoIR.Core.Symbolic.CWord Γ), "
        "EvoIR.Core.Source.binaryValue op.source "
        "(EvoIR.Core.Symbolic.CWord.denote σ a) (EvoIR.Core.Symbolic.CWord.denote σ b) = "
        "Except.ok (EvoIR.Core.Symbolic.CWord.denote σ (EvoIR.Core.Symbolic.bitwise op a b))",
    ),
    (
        "EvoIR.Core.Symbolic.parameter_words_cover",
        "∀ (ps : List EvoIR.Core.Source.Parameter) (values : List EvoIR.Core.Source.Value), "
        "List.map EvoIR.Core.Source.Value.type values = List.map (fun p => p.type.type) ps → "
        "List.map (EvoIR.Core.Symbolic.CWord.denote (EvoIR.Core.Symbolic.inputSubst values)) "
        "(EvoIR.Core.Symbolic.parameterWords ps) = values",
    ),
    (
        "EvoIR.Core.Symbolic.scalar_entry_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) "
        "(source : EvoIR.Core.Source.Function) (target : EvoIR.Core.IR.Function) "
        "(args : List (EvoIR.Core.Symbolic.CWord Γ)) (env : EvoIR.Core.Symbolic.CBindings Γ) "
        "(scope : EvoIR.Core.Symbolic.Scope Γ) (owner : Nat), "
        "EvoIR.Core.Symbolic.scalarEntry source target args = some (env, scope) → "
        "∃ locals block, EvoIR.Core.Source.parameterLocals source.parameters "
        "(List.map (EvoIR.Core.Symbolic.CWord.denote σ) args) = Except.ok locals ∧ "
        "EvoIR.Core.Symbolic.CBindings.Related σ env locals ∧ target.blocks.head? = some block ∧ "
        "EvoIR.Core.IR.enter target "
        "(List.map (fun v => EvoIR.Core.Symbolic.sourceValueIR "
        "(EvoIR.Core.Symbolic.CWord.denote σ v)) args) owner = Except.ok "
        "{ function := target.name, block := block.name, pc := 0, "
        "values := EvoIR.Core.Symbolic.Scope.denote σ (fun x => EvoIR.Core.IR.Value.world) scope, "
        "owner := owner }",
    ),
    (
        "EvoIR.Core.Symbolic.LeafPair.refines",
        "∀ {source : EvoIR.Core.Source.Program} {target : EvoIR.Core.IR.Program} "
        "(pair : EvoIR.Core.Symbolic.LeafPair source target) (args : List EvoIR.Core.Source.Value), "
        "List.map EvoIR.Core.Source.Value.type args = "
        "List.map (fun p => p.type.type) pair.sourceFunction.parameters → "
        "∃ sourceEntry targetEntry, "
        "EvoIR.Core.Source.initial source pair.sourceFunction.symbol args = Except.ok sourceEntry ∧ "
        "EvoIR.Core.IR.initial target pair.targetFunction.name "
        "(List.map EvoIR.Core.Symbolic.sourceValueIR args) = Except.ok targetEntry ∧ "
        "EvoIR.Core.LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry",
    ),
    (
        "EvoIR.Core.Symbolic.edge_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ρ : Nat → EvoIR.Core.IR.Value) "
        "(types : Nat → EvoIR.Core.IR.Ty), (∀ (id : Nat), (ρ id).type = types id) → "
        "∀ (f : EvoIR.Core.IR.Function) (a : EvoIR.Core.IR.Activation) "
        "(scope next : EvoIR.Core.Symbolic.Scope Γ) (successor : EvoIR.Core.IR.Edge), "
        "a.values = EvoIR.Core.Symbolic.Scope.denote σ ρ scope → "
        "EvoIR.Core.Symbolic.edge types f scope successor = some next → "
        "EvoIR.Core.IR.jump f a successor = Except.ok "
        "{ function := a.function, block := successor.block, pc := 0, "
        "values := EvoIR.Core.Symbolic.Scope.denote σ ρ next, owner := a.owner }",
    ),
    (
        "EvoIR.Core.Symbolic.branch_execution_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ρ : Nat → EvoIR.Core.IR.Value) "
        "(types : Nat → EvoIR.Core.IR.Ty), (∀ (id : Nat), (ρ id).type = types id) → "
        "∀ (p : EvoIR.Core.IR.Program) (s : EvoIR.Core.IR.State) "
        "(a : EvoIR.Core.IR.Activation) (f : EvoIR.Core.IR.Function) (b : EvoIR.Core.IR.Block) "
        "(scope next : EvoIR.Core.Symbolic.Scope Γ) (successor : EvoIR.Core.IR.Edge), "
        "s.control = EvoIR.Core.IR.Control.running a → "
        "List.find? (fun x => x.name == a.function) p.functions = some f → "
        "List.find? (fun x => x.name == a.block) f.blocks = some b → "
        "a.pc = b.instructions.length → b.terminator = EvoIR.Core.IR.Terminator.branch successor → "
        "a.values = EvoIR.Core.Symbolic.Scope.denote σ ρ scope → "
        "EvoIR.Core.Symbolic.checkEdge types f scope successor next = true → "
        "EvoIR.Core.IR.step p s = some { control := EvoIR.Core.IR.Control.running "
        "{ function := a.function, block := successor.block, pc := 0, "
        "values := EvoIR.Core.Symbolic.Scope.denote σ ρ next, owner := a.owner }, "
        "stack := s.stack, nextFrame := s.nextFrame }",
    ),
    (
        "EvoIR.Core.Symbolic.conditional_execution_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ρ : Nat → EvoIR.Core.IR.Value) "
        "(types : Nat → EvoIR.Core.IR.Ty), (∀ (id : Nat), (ρ id).type = types id) → "
        "∀ (p : EvoIR.Core.IR.Program) (s : EvoIR.Core.IR.State) "
        "(a : EvoIR.Core.IR.Activation) (f : EvoIR.Core.IR.Function) (b : EvoIR.Core.IR.Block) "
        "(scope yesScope noScope : EvoIR.Core.Symbolic.Scope Γ) (id : Nat) "
        "(condition : EvoIR.Core.Symbolic.Term Γ 1) (yes no : EvoIR.Core.IR.Edge), "
        "s.control = EvoIR.Core.IR.Control.running a → "
        "List.find? (fun x => x.name == a.function) p.functions = some f → "
        "List.find? (fun x => x.name == a.block) f.blocks = some b → "
        "a.pc = b.instructions.length → b.terminator = EvoIR.Core.IR.Terminator.conditional id yes no → "
        "a.values = EvoIR.Core.Symbolic.Scope.denote σ ρ scope → "
        "scope.word id = some ⟨1, condition⟩ → "
        "EvoIR.Core.Symbolic.checkEdge types f scope yes yesScope = true → "
        "EvoIR.Core.Symbolic.checkEdge types f scope no noScope = true → "
        "EvoIR.Core.IR.step p s = some { control := EvoIR.Core.IR.Control.running "
        "{ function := a.function, "
        "block := if ((EvoIR.Core.Symbolic.denoteTerm σ condition).toNat == 0) = true then no.block else yes.block, "
        "pc := 0, values := if ((EvoIR.Core.Symbolic.denoteTerm σ condition).toNat == 0) = true then "
        "EvoIR.Core.Symbolic.Scope.denote σ ρ noScope else EvoIR.Core.Symbolic.Scope.denote σ ρ yesScope, "
        "owner := a.owner }, stack := s.stack, nextFrame := s.nextFrame }",
    ),
    (
        "EvoIR.Core.Symbolic.shift_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (left : Bool) "
        "(a b : EvoIR.Core.Symbolic.CWord Γ), "
        "left = false ∨ a.type.signed = false → "
        "(do EvoIR.Core.Symbolic.guards σ (EvoIR.Core.Symbolic.shift left a b).fst "
        "pure (EvoIR.Core.Symbolic.CWord.denote σ (EvoIR.Core.Symbolic.shift left a b).snd)) = "
        "EvoIR.Core.Source.binaryValue "
        "(if left = true then EvoIR.Core.Source.Binary.shl else EvoIR.Core.Source.Binary.shr) "
        "(EvoIR.Core.Symbolic.CWord.denote σ a) (EvoIR.Core.Symbolic.CWord.denote σ b)",
    ),
    (
        "EvoIR.Core.Symbolic.ExpressionDiagram.sound",
        "∀ {Γ : EvoIR.Var → Nat} {env : EvoIR.Core.Symbolic.CBindings Γ} "
        "{e : EvoIR.Core.Source.Expr} {r : EvoIR.Core.Symbolic.ExpressionResult Γ}, "
        "EvoIR.Core.Symbolic.ExpressionDiagram env e r → "
        "∀ (σ : EvoIR.Subst Γ) (p : EvoIR.Core.Source.Program) "
        "(locals : EvoIR.Core.Source.Locals) (functionId : Nat), "
        "EvoIR.Core.Symbolic.CBindings.Related σ env locals → "
        "EvoIR.Core.Source.Evaluates p locals functionId e "
        "(EvoIR.Core.Symbolic.ExpressionResult.denote σ r)",
    ),
    (
        "EvoIR.Core.Symbolic.expression_execution_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ρ : Nat → EvoIR.Core.IR.Value) "
        "(env : EvoIR.Core.Symbolic.CBindings Γ) (e : EvoIR.Core.Source.Expr) "
        "(scope : EvoIR.Core.Symbolic.Scope Γ) (code : List EvoIR.Core.IR.Instruction) "
        "(output : Nat) (source : EvoIR.Core.Source.Program) "
        "(locals : EvoIR.Core.Source.Locals) (functionId : Nat) "
        "(target : EvoIR.Core.IR.Program) (s : EvoIR.Core.IR.State) "
        "(a : EvoIR.Core.IR.Activation) (f : EvoIR.Core.IR.Function) (b : EvoIR.Core.IR.Block), "
        "EvoIR.Core.Symbolic.CBindings.Related σ env locals → "
        "s.control = EvoIR.Core.IR.Control.running a → "
        "List.find? (fun x => x.name == a.function) target.functions = some f → "
        "List.find? (fun x => x.name == a.block) f.blocks = some b → "
        "(∀ (i : Nat) (inst : EvoIR.Core.IR.Instruction), "
        "code[i]? = some inst → b.instructions[a.pc + i]? = some inst) → "
        "a.values = EvoIR.Core.Symbolic.Scope.denote σ ρ scope → "
        "EvoIR.Core.Symbolic.checkExpression env e scope code output = true → "
        "∃ result finalState, EvoIR.Core.Source.Evaluates source locals functionId e result ∧ "
        "EvoIR.Core.IR.Steps target s finalState ∧ "
        "match result with "
        "| Except.error fault => finalState.control = EvoIR.Core.IR.Control.halted (Except.error fault) "
        "| Except.ok v => ∃ bindings, "
        "finalState = { control := EvoIR.Core.IR.Control.running "
        "{ function := a.function, block := a.block, pc := a.pc + code.length, "
        "values := bindings, owner := a.owner }, stack := s.stack, nextFrame := s.nextFrame } ∧ "
        "EvoIR.Core.IR.lookup bindings output = Except.ok (EvoIR.Core.Symbolic.sourceValueIR v)",
    ),
    (
        "EvoIR.Core.Symbolic.cast_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (t : EvoIR.Core.Source.IntType) "
        "(v : EvoIR.Core.Symbolic.CWord Γ), "
        "EvoIR.Core.Symbolic.CWord.denote σ (EvoIR.Core.Symbolic.CWord.cast t v) = "
        "EvoIR.Core.Source.Value.cast t (EvoIR.Core.Symbolic.CWord.denote σ v)",
    ),
    (
        "EvoIR.Core.Symbolic.arithmetic_fragment_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ρ : Nat → EvoIR.Core.IR.Value) "
        "(owner : Nat) (op : EvoIR.Core.SignedArithmetic) (a b : EvoIR.Core.Symbolic.CWord Γ) "
        "(scope : EvoIR.Core.Symbolic.Scope Γ) (code : List EvoIR.Core.IR.Instruction) (output : Nat), "
        "EvoIR.Core.Symbolic.checkArithmetic op a b scope code output = true → "
        "(do let values ← EvoIR.Core.Symbolic.concreteInstructions owner "
        "(EvoIR.Core.Symbolic.Scope.denote σ ρ scope) code "
        "List.mapM (EvoIR.Core.IR.lookup values) [output]) = "
        "do let value ← EvoIR.Core.Source.binaryValue (EvoIR.Core.Source.arithmeticSourceOp op) "
        "(EvoIR.Core.Symbolic.CWord.denote σ a) (EvoIR.Core.Symbolic.CWord.denote σ b) "
        "pure [EvoIR.Core.Symbolic.sourceValueIR value]",
    ),
    (
        "EvoIR.Core.Symbolic.segment_execution_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ρ : Nat → EvoIR.Core.IR.Value) "
        "{p : EvoIR.Core.IR.Program} (s : EvoIR.Core.IR.State) (a : EvoIR.Core.IR.Activation) "
        "(f : EvoIR.Core.IR.Function) (b : EvoIR.Core.IR.Block) "
        "(scope finalScope : EvoIR.Core.Symbolic.Scope Γ) (code : List EvoIR.Core.IR.Instruction) "
        "(checks : List (EvoIR.Core.Symbolic.Guard Γ)), "
        "s.control = EvoIR.Core.IR.Control.running a → "
        "List.find? (fun x => x.name == a.function) p.functions = some f → "
        "List.find? (fun x => x.name == a.block) f.blocks = some b → "
        "(∀ (i : Nat) (inst : EvoIR.Core.IR.Instruction), "
        "code[i]? = some inst → b.instructions[a.pc + i]? = some inst) → "
        "a.values = EvoIR.Core.Symbolic.Scope.denote σ ρ scope → "
        "EvoIR.Core.Symbolic.instructions scope code = some (checks, finalScope) → "
        "∃ finalState, EvoIR.Core.IR.Steps p s finalState ∧ "
        "match EvoIR.Core.Symbolic.guards σ checks with "
        "| Except.error fault => finalState.control = EvoIR.Core.IR.Control.halted (Except.error fault) "
        "| Except.ok a_1 => finalState = { control := EvoIR.Core.IR.Control.running "
        "{ function := a.function, block := a.block, pc := a.pc + code.length, "
        "values := EvoIR.Core.Symbolic.Scope.denote σ ρ finalScope, owner := a.owner }, "
        "stack := s.stack, nextFrame := s.nextFrame }",
    ),
    (
        "EvoIR.Core.Symbolic.scalar_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (op : EvoIR.Op) "
        "(args : List (EvoIR.Core.Symbolic.Word Γ)) (result : EvoIR.Core.Symbolic.Word Γ), "
        "EvoIR.Core.Symbolic.scalar op args = some result → "
        "EvoIR.Core.IR.evalScalar op (List.map (EvoIR.Core.Symbolic.Word.denote σ) args) = "
        "Except.ok (EvoIR.Core.Symbolic.Word.denote σ result)",
    ),
    (
        "EvoIR.Core.Symbolic.instruction_execution_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ρ : Nat → EvoIR.Core.IR.Value) "
        "{p : EvoIR.Core.IR.Program} {s : EvoIR.Core.IR.State} {a : EvoIR.Core.IR.Activation} "
        "{f : EvoIR.Core.IR.Function} {b : EvoIR.Core.IR.Block} {inst : EvoIR.Core.IR.Instruction} "
        "(scope next : EvoIR.Core.Symbolic.Scope Γ) (checks : List (EvoIR.Core.Symbolic.Guard Γ)), "
        "List.find? (fun x => x.name == a.function) p.functions = some f → "
        "List.find? (fun x => x.name == a.block) f.blocks = some b → "
        "b.instructions[a.pc]? = some inst → a.values = EvoIR.Core.Symbolic.Scope.denote σ ρ scope → "
        "EvoIR.Core.Symbolic.instruction scope inst = some (checks, next) → "
        "EvoIR.Core.IR.execute p s a = do EvoIR.Core.Symbolic.guards σ checks "
        "pure { control := EvoIR.Core.IR.Control.running "
        "{ function := a.function, block := a.block, pc := a.pc + 1, "
        "values := EvoIR.Core.Symbolic.Scope.denote σ ρ next, owner := a.owner }, "
        "stack := s.stack, nextFrame := s.nextFrame }",
    ),
    (
        "EvoIR.Core.Symbolic.fragment_sound",
        "∀ {Γ : EvoIR.Var → Nat} (σ : EvoIR.Subst Γ) (ρ : Nat → EvoIR.Core.IR.Value) "
        "(owner : Nat) (scope : EvoIR.Core.Symbolic.Scope Γ) "
        "(code : List EvoIR.Core.IR.Instruction) (outputs : List Nat) "
        "(expectedChecks : List (EvoIR.Core.Symbolic.Guard Γ)) "
        "(expectedValues : List (EvoIR.Core.Symbolic.Word Γ)), "
        "EvoIR.Core.Symbolic.checkFragment scope code outputs expectedChecks expectedValues = true → "
        "(do let values ← EvoIR.Core.Symbolic.concreteInstructions owner "
        "(EvoIR.Core.Symbolic.Scope.denote σ ρ scope) code "
        "List.mapM (EvoIR.Core.IR.lookup values) outputs) = "
        "do EvoIR.Core.Symbolic.guards σ expectedChecks "
        "pure (List.map (EvoIR.Core.Symbolic.Word.denote σ) expectedValues)",
    ),
    (
        "EvoIR.Core.Source.read_written_slot",
        "∀ (locals updated : EvoIR.Core.Source.Locals) (symbol : Nat) "
        "(destination : EvoIR.Core.Source.QualifiedType) (value : EvoIR.Core.Source.Value) "
        "(initialization : Bool), "
        "EvoIR.Core.Source.writeSlot locals symbol destination value initialization = Except.ok updated → "
        "EvoIR.Core.Source.readSlot updated symbol destination.type = Except.ok value",
    ),
    (
        "EvoIR.Core.Source.write_slot_length",
        "∀ (locals updated : EvoIR.Core.Source.Locals) (symbol : Nat) "
        "(destination : EvoIR.Core.Source.QualifiedType) (value : EvoIR.Core.Source.Value) "
        "(initialization : Bool), "
        "EvoIR.Core.Source.writeSlot locals symbol destination value initialization = Except.ok updated → "
        "List.length updated = List.length locals",
    ),
    (
        "EvoIR.Core.Source.read_other_slot",
        "∀ (locals updated : EvoIR.Core.Source.Locals) (symbol other : Nat) "
        "(destination : EvoIR.Core.Source.QualifiedType) (type : EvoIR.Core.Source.IntType) "
        "(value : EvoIR.Core.Source.Value) (initialization : Bool), other ≠ symbol → "
        "EvoIR.Core.Source.writeSlot locals symbol destination value initialization = Except.ok updated → "
        "EvoIR.Core.Source.readSlot updated other type = EvoIR.Core.Source.readSlot locals other type",
    ),
    (
        "EvoIR.Core.ranked_lowering_refines",
        "∀ (s : EvoIR.Core.Source.Program) (t : EvoIR.Core.IR.Program) "
        "(resultType : EvoIR.Core.Source.IntType) "
        "(related : EvoIR.Core.Source.State → EvoIR.Core.IR.State → Prop) "
        "(rank : EvoIR.Core.Source.State → EvoIR.Core.IR.State → Nat), "
        "EvoIR.Core.LoweringSimulation s t resultType related rank → "
        "∀ (s₀ : EvoIR.Core.Source.State) (t₀ : EvoIR.Core.IR.State), "
        "related s₀ t₀ → EvoIR.Core.LoweringRefines s t resultType s₀ t₀",
    ),
    (
        "EvoIR.Core.Transition.Refines.trans",
        "∀ {A : Type u_1} {B : Type u_2} {C : Type u_3} {O : Type u_4} "
        "{firstStep : A → Option A} {middleStep : B → Option B} {lastStep : C → Option C} "
        "{firstObserve : A → Option O} {middleObserve : B → Option O} {lastObserve : C → Option O} "
        "{a : A} {b : B} {c : C}, "
        "EvoIR.Core.Transition.Refines firstStep middleStep firstObserve middleObserve a b → "
        "EvoIR.Core.Transition.Refines middleStep lastStep middleObserve lastObserve b c → "
        "EvoIR.Core.Transition.Refines firstStep lastStep firstObserve lastObserve a c",
    ),
    (
        "EvoIR.Core.checked_arithmetic_lowering",
        "∀ (op : EvoIR.Core.SignedArithmetic) {n : Nat} (x y : BitVec n), "
        "EvoIR.Core.irArithmetic op x y = EvoIR.Core.sourceArithmetic op x y",
    ),
    (
        "EvoIR.Core.checked_neg_lowering",
        "∀ {n : Nat} (x : BitVec n), EvoIR.Core.irNeg x = EvoIR.Core.sourceNeg x",
    ),
    (
        "EvoIR.Core.checked_abs_expression",
        "∀ {n : Nat} (x : BitVec n), EvoIR.Core.irAbs x = EvoIR.Core.sourceAbs x",
    ),
    (
        "EvoIR.Core.checked_fixedmul_expression",
        "∀ (x y : BitVec 32), EvoIR.Core.irFixedMul x y = EvoIR.Core.sourceFixedMul x y",
    ),
    (
        "EvoIR.Core.arithmetic_fragment_checker_sound",
        "∀ (source : EvoIR.Core.SignedArithmetic) (candidate : EvoIR.Core.ArithmeticFragment), "
        "EvoIR.Core.checkArithmeticFragment source candidate = true → "
        "∀ {n : Nat} (x y : BitVec n), EvoIR.Core.runArithmeticFragment candidate x y = "
        "EvoIR.Core.sourceArithmetic source x y",
    ),
    (
        "EvoIR.Core.Source.typed_arithmetic_lowering",
        "∀ (op : EvoIR.Core.SignedArithmetic) (t : EvoIR.Core.Source.IntType), "
        "t.signed = true → ∀ (x y : BitVec t.width), "
        "EvoIR.Core.Source.liftChecked t (EvoIR.Core.irArithmetic op x y) = "
        "EvoIR.Core.Source.binaryValue (EvoIR.Core.Source.arithmeticSourceOp op) "
        "{ type := t, bits := x } { type := t, bits := y }",
    ),
    (
        "EvoIR.Core.Source.step_deterministic",
        "∀ (p : EvoIR.Core.Source.Program) (s t u : EvoIR.Core.Source.State), "
        "EvoIR.Core.Source.Step p s t → EvoIR.Core.Source.Step p s u → t = u",
    ),
    (
        "EvoIR.Core.Source.runSteps_prefix",
        "∀ (p : EvoIR.Core.Source.Program) (fuel : Nat) (s : EvoIR.Core.Source.State), "
        "EvoIR.Core.Source.Steps p s (EvoIR.Core.Source.runSteps p fuel s)",
    ),
    (
        "EvoIR.Core.Source.diagnostic_cannot_be_overwritten",
        "∀ (p : EvoIR.Core.Source.Program) (code : Nat) (locals : EvoIR.Core.Source.Locals) "
        "(k : List EvoIR.Core.Source.Frame) (functionId fuel : Nat), "
        "EvoIR.Core.Source.runSteps p fuel "
        "{ control := EvoIR.Core.Source.Control.halted (Except.error (EvoIR.Core.Source.Fault.diagnostic code)), "
        "locals := locals, continuation := k, functionId := functionId } = "
        "{ control := EvoIR.Core.Source.Control.halted (Except.error (EvoIR.Core.Source.Fault.diagnostic code)), "
        "locals := locals, continuation := k, functionId := functionId }",
    ),
    (
        "EvoIR.Core.Source.while_true_diverges",
        "∀ (p : EvoIR.Core.Source.Program) (locals : EvoIR.Core.Source.Locals) "
        "(k : List EvoIR.Core.Source.Frame) (functionId : Nat), EvoIR.Core.Source.Diverges p "
        "{ control := EvoIR.Core.Source.Control.statement "
        "(EvoIR.Core.Source.Stmt.while (EvoIR.Core.Source.Expr.literal EvoIR.Core.Source.IntType.int 1) "
        "EvoIR.Core.Source.Stmt.null), locals := locals, continuation := k, functionId := functionId }",
    ),
    (
        "EvoIR.Core.IR.step_deterministic",
        "∀ (p : EvoIR.Core.IR.Program) (a b c : EvoIR.Core.IR.State), "
        "EvoIR.Core.IR.Step p a b → EvoIR.Core.IR.Step p a c → b = c",
    ),
    (
        "EvoIR.Core.IR.runSteps_prefix",
        "∀ (p : EvoIR.Core.IR.Program) (n : Nat) (s : EvoIR.Core.IR.State), "
        "EvoIR.Core.IR.Steps p s (EvoIR.Core.IR.runSteps p n s)",
    ),
    (
        "EvoIR.Core.AbsCorrespondence.whole_body_refines",
        "∀ (x : BitVec 32), EvoIR.Core.LoweringRefines "
        "EvoIR.Core.AbsCorrespondence.source EvoIR.Core.AbsCorrespondence.target "
        "EvoIR.Core.Source.IntType.int (EvoIR.Core.AbsCorrespondence.sourceEntry x) "
        "(EvoIR.Core.AbsCorrespondence.targetEntry x)",
    ),
    (
        "EvoIR.Core.AbsCorrespondence.pilot_pair_refines",
        "∀ (s : EvoIR.Core.Source.Program) (t : EvoIR.Core.IR.Program), "
        "EvoIR.Core.AbsCorrespondence.checkPilotPair s t = true → ∀ (x : BitVec 32), "
        "∃ s₀ t₀, "
        "EvoIR.Core.Source.initial s 0 [{ type := EvoIR.Core.Source.IntType.int, bits := x }] = Except.ok s₀ ∧ "
        "EvoIR.Core.IR.initial t \"abs\" [EvoIR.Core.IR.Value.bits 32 x] = Except.ok t₀ ∧ "
        "EvoIR.Core.LoweringRefines s t EvoIR.Core.Source.IntType.int s₀ t₀",
    ),
]

EVALUATOR_TARGETS = [
    (
        "EvoIR.denoteOp_capAmount",
        "∀ (o : EvoIR.ShOp) (n m : Nat) (x : BitVec n) (k : BitVec m), "
        "EvoIR.denoteOp (EvoIR.Op.shift o n m) (x, EvoIR.capAmount n k) = "
        "EvoIR.denoteOp (EvoIR.Op.shift o n m) (x, k)",
        "the evaluator may cap a shift amount it cannot shift by, and still "
        "compute this file's `denoteOp`",
    ),
]

TRUST = [
    "The audit script itself, and the fixed Lean modules under lean/EvoIR/.",
    "The Lean 4 kernel, at the pinned toolchain version.",
    "The Lean compiler and the SAT/LRAT path, for every theorem this receipt "
    "classifies native-backed.",
    "The registry decoder of tools/lean/registry_to_lean.py and the Lean "
    "semantic definitions themselves; the round trip and Rule.render check "
    "that the two conversions agree, not that either is faithful.",
    "Sections 8.5 eligibility, the seed-fact fixed point, the checker's replay "
    "and the Section 9 parser, none of which this layer touches.",
    "The correspondence between the Rust implementation and these Lean "
    "definitions, for which only differential evidence exists.",
]

DUMP_TEMPLATE = """\
import Lean
import EvoIR

open Lean Lean.Elab Lean.Elab.Command

private def auditModuleOf (env : Environment) (n : Name) : Name :=
  match env.getModuleIdxFor? n with
  | some idx => env.header.moduleNames[idx.toNat]!
  | none => Name.mkSimple "<current>"

private def auditTargets : List Name := [%(targets)s]

private def auditDefinitions : List Name := [%(definitions)s]

#eval show CommandElabM Unit from do
  let env ← getEnv
  let mut axioms : Array Json := #[]
  for (n, ci) in env.constants.toList do
    match ci with
    | .axiomInfo ax =>
        let ty ← liftTermElabM (do return toString (← Lean.Meta.ppExpr ax.type))
        axioms := axioms.push (Json.mkObj
          [("name", toString n), ("module", toString (auditModuleOf env n)), ("type", ty)])
    | _ => pure ()
  let mut targets : Array Json := #[]
  for t in auditTargets do
    match env.find? t with
    | none => targets := targets.push (Json.mkObj [("name", toString t), ("missing", true)])
    | some ci =>
        let ty ← liftTermElabM (do return toString (← Lean.Meta.ppExpr ci.type))
        let deps ← liftTermElabM (Lean.collectAxioms t)
        targets := targets.push (Json.mkObj
          [("name", toString t), ("type", ty),
           ("axioms", Json.arr (deps.map (fun d => Json.str (toString d))))])
  let mut defs : Array Json := #[]
  for d in auditDefinitions do
    match env.find? d with
    | none => defs := defs.push (Json.mkObj [("name", toString d), ("missing", true)])
    | some ci =>
        let ty ← liftTermElabM (do return toString (← Lean.Meta.ppExpr ci.type))
        let body ← match ci.value? with
          | some v => liftTermElabM (do return toString (← Lean.Meta.ppExpr v))
          | none => pure "<opaque>"
        defs := defs.push (Json.mkObj
          [("name", toString d), ("type", ty), ("body", body)])
  -- every theorem the audited modules declare, so an unused sorry cannot hide
  let mut sorried : Array Json := #[]
  for (n, ci) in env.constants.toList do
    if (`EvoIR).isPrefixOf n && !n.isInternal then
      match ci with
      | .thmInfo _ =>
          let deps ← liftTermElabM (Lean.collectAxioms n)
          if deps.contains ``sorryAx then
            sorried := sorried.push (Json.str (toString n))
      | _ => pure ()
  let out := Json.mkObj
    [("lean_version", Json.str Lean.versionString),
     ("axioms", Json.arr axioms),
     ("targets", Json.arr targets),
     ("definitions", Json.arr defs),
     ("sorried", Json.arr sorried),
     ("registry", Json.arr ((EvoIR.registry.map (fun r => Json.str r.render)).toArray)),
     ("guard_registry", Json.arr ((EvoIR.Guards.registry.map (fun r => Json.str r.render)).toArray))]
  IO.println "===AUDIT-JSON==="
  IO.println out.compress
"""


class AuditFailure(Exception):
    pass


def lean_name_list(names: list[str]) -> str:
    return ", ".join("`" + n for n in names)


def run_dump(lean_dir: Path, targets: list[str]) -> dict:
    # The dump reads the built environment, so a stale build would let the
    # audit pass against olean files the sources no longer produce.
    build = subprocess.run(["lake", "build"], cwd=lean_dir, capture_output=True, text=True)
    if build.returncode != 0:
        raise AuditFailure("lake build failed:\n" + build.stdout + build.stderr)
    source = DUMP_TEMPLATE % {
        "targets": lean_name_list(targets),
        "definitions": lean_name_list(RECORDED_DEFINITIONS),
    }
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "AuditDump.lean"
        path.write_text(source, encoding="utf-8")
        proc = subprocess.run(
            ["lake", "env", "lean", str(path)],
            cwd=lean_dir,
            capture_output=True,
            text=True,
        )
    if proc.returncode != 0:
        raise AuditFailure(
            "the environment dump did not elaborate:\n" + proc.stdout + proc.stderr
        )
    marker = "===AUDIT-JSON==="
    if marker not in proc.stdout:
        raise AuditFailure("the environment dump printed no JSON:\n" + proc.stdout)
    payload = proc.stdout.split(marker, 1)[1].strip()
    return json.loads(payload)


def check_scope(repo: Path) -> dict:
    lean_dir = repo / "lean"
    lakefile = (lean_dir / "lakefile.toml").read_text(encoding="utf-8")
    if "require" in lakefile:
        raise AuditFailure("the Lake package requires an external dependency")
    toolchain = (lean_dir / "lean-toolchain").read_text(encoding="utf-8").strip()
    modules = sorted(
        p.relative_to(repo).as_posix()
        for p in (lean_dir / "EvoIR").rglob("*.lean")
    )
    modules += [
        p.relative_to(repo).as_posix()
        for p in [lean_dir / "EvoIR.lean", lean_dir / "Main.lean"]
        if p.exists()
    ]
    imports: dict[str, list[str]] = {}
    for rel in sorted(modules):
        text = (repo / rel).read_text(encoding="utf-8")
        found = re.findall(r"^import\s+([A-Za-z0-9_.]+)\s*$", text, re.MULTILINE)
        imports[rel] = found
        for name in found:
            head = name.split(".")[0]
            if head not in ALLOWED_IMPORT_PREFIXES:
                raise AuditFailure(f"{rel} imports {name}, which is not Lean core")
    return {
        "audited_modules": sorted(modules),
        "imports": imports,
        "toolchain": toolchain,
        "external_requirements": [],
        "lakefile_sha256": hashlib.sha256(lakefile.encode("utf-8")).hexdigest(),
        "note": "the package declares one library rooted at EvoIR and one "
        "executable rooted at Main, so lake builds exactly the modules listed "
        "here and their Lean core imports",
    }


def classify_axioms(dump: dict) -> list[dict]:
    rows = []
    for entry in dump["axioms"]:
        name = entry["name"]
        module = entry["module"]
        if name in CORE_AXIOMS:
            kind = "core"
        elif NATIVE_AXIOM_RE.match(name):
            kind = "native-reflection"
            if not module.startswith("EvoIR"):
                raise AuditFailure(
                    f"axiom {name} looks like a reflection axiom but is declared in {module}"
                )
            flat = " ".join(entry["type"].split())
            if not NATIVE_AXIOM_TYPE_RE.match(flat):
                raise AuditFailure(
                    f"axiom {name} is named like a bv_decide reflection axiom but its "
                    f"type is not one: {flat}"
                )
        elif module.startswith(CORE_MODULE_PREFIXES):
            kind = "lean-core-declared"
        else:
            raise AuditFailure(
                f"axiom {name} is declared in {module}, outside the audited modules "
                "and outside Lean core"
            )
        rows.append(
            {
                "name": name,
                "module": module,
                "type": " ".join(entry["type"].split()),
                "class": kind,
            }
        )
    for entry in rows:
        if entry["class"] == "native-reflection":
            continue
        if entry["class"] == "core":
            continue
        if entry["module"].startswith("EvoIR"):
            raise AuditFailure(
                f"axiom {entry['name']} is declared inside the audited modules but is "
                "not a bv_decide reflection axiom"
            )
    return rows


def proof_kind(axioms: list[str]) -> str:
    extra = [a for a in axioms if a not in CORE_AXIOMS]
    return "kernel-only" if not extra else "native-backed"


def target_entry(byname: dict, name: str, expected: str) -> dict:
    if name not in byname:
        raise AuditFailure(f"the environment has no declaration {name}")
    info = byname[name]
    if info.get("missing"):
        raise AuditFailure(f"the environment has no declaration {name}")
    actual = " ".join(info["type"].split())
    want = " ".join(expected.split())
    if actual != want:
        raise AuditFailure(
            f"{name} does not state the audited proposition\n"
            f"  expected: {want}\n"
            f"  actual:   {actual}"
        )
    axioms = list(info["axioms"])
    if "sorryAx" in axioms:
        raise AuditFailure(f"{name} depends on sorryAx")
    return {
        "theorem": name,
        "proposition": actual,
        "proof_kind": proof_kind(axioms),
        "core_axioms": sorted(a for a in axioms if a in CORE_AXIOMS),
        "native_axioms": sorted(a for a in axioms if a not in CORE_AXIOMS),
    }


VECTOR_FILES = [
    "transfers-small",
    "lattice-small",
    "values-width8",
    "random",
    "named",
    "guards",
]


def vector_evidence(repo: Path) -> dict:
    """What this side answered, for segment C.  The files are written by
    `tools/lean/vectors.py` and are not tracked, so an absent file is recorded
    as absent rather than as a pass."""
    out: dict = {}
    for name in VECTOR_FILES:
        path = repo / "lean" / "vectors" / f"{name}.lean.jsonl"
        if not path.exists():
            out[name] = {"status": "not run"}
            continue
        answered = 0
        abstained = 0
        wrong = 0
        checked = 0
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            result = row.get("result")
            if result in ("unsupported", "skipped"):
                abstained += 1
            else:
                answered += 1
            if "expect" in row:
                checked += 1
                if result != row["expect"]:
                    wrong += 1
        entry = {"answered": answered, "abstained": abstained}
        if checked:
            entry["named_checked"] = checked
            entry["named_wrong"] = wrong
            if wrong:
                raise AuditFailure(
                    f"{path} disagrees with {wrong} of its own expected values"
                )
        out[name] = entry
    return out


def git(repo: Path, *args: str) -> str:
    proc = subprocess.run(
        ["git", *args], cwd=repo, capture_output=True, text=True, check=False
    )
    return proc.stdout.strip()


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".")
    parser.add_argument("--spec", default="docs/evoir-spec.md")
    parser.add_argument("--registry", default="p0.scalar.v4")
    parser.add_argument("--prefix-registry", default="p0.scalar.v3")
    parser.add_argument("--out", default="tests/contracts/lean-p0-receipt.json")
    args = parser.parse_args(argv)

    repo = Path(args.repo).resolve()
    spec_path = repo / args.spec
    spec = spec_path.read_text(encoding="utf-8")

    # -- step 5 material: what the receipt binds ---------------------------
    block, digest = extract_block(spec, args.registry)
    if hashlib.sha256(block.encode("utf-8")).hexdigest() != digest:
        raise AuditFailure("the registry block does not hash to the stated digest")
    prefix_block, prefix_digest = extract_block(spec, args.prefix_registry)
    if hashlib.sha256(prefix_block.encode("utf-8")).hexdigest() != prefix_digest:
        raise AuditFailure(
            f"the {args.prefix_registry} block does not hash to the stated digest"
        )
    if not block.startswith(prefix_block):
        raise AuditFailure(
            f"{args.prefix_registry} is not a byte prefix of {args.registry}"
        )
    rules = [parse_rule(line) for line in block.split("\n") if line]
    guard_block, guard_digest = extract_block(spec, "g0.ub.v1")
    if hashlib.sha256(guard_block.encode()).hexdigest() != guard_digest:
        raise AuditFailure("the g0 registry digest does not match its rule text")
    guard_rules = [parse_rule(line) for line in guard_block.splitlines()]
    prefix_rules = [parse_rule(line) for line in prefix_block.split("\n") if line]
    revision = next(
        line.strip() for line in spec.split("\n") if line.startswith("**Revision ")
    )

    # -- step 1 -----------------------------------------------------------
    scope = check_scope(repo)

    # -- steps 2 to 4 -----------------------------------------------------
    rule_targets = [f"EvoIR.sound_{r.ident}" for r in rules]
    rule_targets += [f"EvoIR.sound_{r.ident}" for r in guard_rules]
    transfer_targets = [t[3] for t in PARAMETRIC_ROWS]
    for _, _, base in ENUMERATED_ROWS:
        transfer_targets += [f"{base}_{n}" for n in ENUMERATED_WIDTHS]
    lattice_targets = [t[1] for t in LATTICE_TARGETS]
    evaluator_targets = [t[0] for t in EVALUATOR_TARGETS]
    core_targets = [t[0] for t in CORE_PRIMITIVE_TARGETS]
    all_targets = rule_targets + transfer_targets + lattice_targets + evaluator_targets + core_targets

    dump = run_dump(repo / "lean", all_targets)

    if dump["registry"] != block.split("\n")[:-1]:
        raise AuditFailure(
            "the Lean registry values do not render back to the specification block"
        )
    if dump["guard_registry"] != guard_block.splitlines():
        raise AuditFailure("the Lean g0 data does not render back to the reviewed text")

    axiom_rows = classify_axioms(dump)
    if dump["sorried"]:
        names = sorted(dump["sorried"])
        shown = ", ".join(names[:10])
        more = "" if len(names) <= 10 else f", and {len(names) - 10} more"
        raise AuditFailure(
            f"{len(names)} audited theorems depend on sorryAx: {shown}{more}"
        )

    byname = {t["name"]: t for t in dump["targets"]}

    segment_a = []
    for rule in rules:
        name = f"EvoIR.sound_{rule.ident}"
        expected = f"EvoIR.RuleCovered EvoIR.{lean_ident(rule.ident)}"
        entry = target_entry(byname, name, expected)
        entry["rule"] = rule.ident
        entry["registry_line"] = rule.render()
        entry["instances"] = {
            "kind": "parametric",
            "space": "every width environment and every result width",
            "profile_bound": PROFILE_BOUND,
            "covers_profile": True,
        }
        segment_a.append(entry)

    for rule in guard_rules:
        name = f"EvoIR.sound_{rule.ident}"
        expected = f"EvoIR.RuleCovered EvoIR.Guards.{lean_ident(rule.ident)}"
        entry = target_entry(byname, name, expected)
        if entry["proof_kind"] != "kernel-only":
            raise AuditFailure("g0 rule is not kernel-only: " + name)
        entry.update(rule=rule.ident, registry_line=rule.render(), registry="g0.ub.v1",
                     instances=dict(kind="parametric", space="every width environment and result width",
                                    profile_bound="g0: N/M each 1..64", covers_profile=True))
        segment_a.append(entry)

    segment_b = []
    for key, name, expected, why in LATTICE_TARGETS:
        entry = target_entry(byname, name, expected)
        entry["row"] = key
        entry["source"] = why
        entry["instances"] = {
            "kind": "parametric",
            "space": "every width",
            "profile_bound": PROFILE_BOUND,
            "covers_profile": True,
        }
        segment_b.append(entry)

    for row, ctor, params, name in PARAMETRIC_ROWS:
        binder = " ".join(params)
        expected = f"∀ ({binder} : Nat), EvoIR.TransferRowSound ({ctor} {binder})"
        entry = target_entry(byname, name, expected)
        entry["row"] = row
        entry["source"] = "g0.ub.v1 transfer table" if "guard_" in name else "8.10 transfer table"
        if "guard_" in name:
            entry["assumption"] = "A2"
        if "guard_" in name and entry["proof_kind"] != "kernel-only":
            raise AuditFailure("g0 transfer is not kernel-only: " + name)
        entry["instances"] = {
            "kind": "parametric",
            "space": "every " + " and every ".join(params),
            "profile_bound": PROFILE_BOUND,
            "covers_profile": True,
        }
        segment_b.append(entry)

    for row, ctor, base in ENUMERATED_ROWS:
        widths = []
        kinds = set()
        native = set()
        for n in ENUMERATED_WIDTHS:
            name = f"{base}_{n}"
            expected = f"EvoIR.TransferRowSound ({ctor} {n})"
            entry = target_entry(byname, name, expected)
            kinds.add(entry["proof_kind"])
            native.update(entry["native_axioms"])
            widths.append(
                {
                    "n": n,
                    "theorem": name,
                    "proposition": entry["proposition"],
                    "proof_kind": entry["proof_kind"],
                    "native_axioms": entry["native_axioms"],
                }
            )
        segment_b.append(
            {
                "row": row,
                "source": "8.10 transfer table",
                "instances": {
                    "kind": "enumerated",
                    "widths": ENUMERATED_WIDTHS,
                    "profile_bound": PROFILE_BOUND,
                    "covers_profile": True,
                },
                "proof_kind": "native-backed" if "native-backed" in kinds else "kernel-only",
                "native_axioms": sorted(native),
                "theorems": widths,
            }
        )

    evaluator = [
        target_entry(byname, name, expected) | {"source": why}
        for name, expected, why in EVALUATOR_TARGETS
    ]

    core_primitives = []
    for name, expected in CORE_PRIMITIVE_TARGETS:
        entry = target_entry(byname, name, expected)
        if entry["proof_kind"] != "kernel-only":
            raise AuditFailure("compilation-core primitive is not kernel-only: " + name)
        core_primitives.append(entry)

    definitions = []
    for entry in dump["definitions"]:
        if entry.get("missing"):
            raise AuditFailure(f"the environment has no definition {entry['name']}")
        text = entry["type"] + " := " + entry["body"]
        definitions.append(
            {
                "name": entry["name"],
                "type": " ".join(entry["type"].split()),
                "sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
            }
        )

    receipt = {
        "receipt": "lean-p0",
        "generated_by": "tools/lean/audit.py",
        "binding": {
            "registry_name": args.registry,
            "registry_digest": digest,
            "registry_rules": len(rules),
            "guard_registry": {"name": "g0.ub.v1", "digest": guard_digest,
                               "rules": len(guard_rules), "independent_of_p0": True},
            "verified_prefix": {
                "registry_name": args.prefix_registry,
                "registry_digest": prefix_digest,
                "registry_rules": len(prefix_rules),
                "relation": "byte prefix of "
                + args.registry
                + ": the first "
                + str(len(prefix_rules))
                + " rules and their exact bytes are the same, so a certificate "
                "naming "
                + args.prefix_registry
                + " fires only rules this receipt covers",
            },
            "spec_revision": revision,
            "spec_sha256": hashlib.sha256(spec.encode("utf-8")).hexdigest(),
            "lean_commit": git(repo, "log", "-1", "--format=%H", "--", "lean"),
            "lean_worktree_clean": git(repo, "status", "--porcelain", "--", "lean") == "",
            "toolchain": scope["toolchain"],
            "lean_version": dump["lean_version"],
        },
        "scope": scope,
        "environment_axioms": axiom_rows,
        "definitions": definitions,
        "segments": {
            "A": {
                "claim": "every rule of the registry is sound under the fixed RuleSound",
                "targets": segment_a,
            },
            "B": {
                "claim": "the Section 8.10 transfer table, merge, join and the "
                "side-condition predicates are sound",
                "targets": segment_b,
            },
            "C": {
                "claim": "the Rust implementation agrees with these definitions",
                "status": "differential evidence only; this side's answers are "
                "counted here, and the comparison against the Rust runners' "
                "<name>.rust.jsonl is tools/lean/diff.py, not this receipt",
                "vectors": vector_evidence(repo),
            },
        },
        "evaluator": evaluator,
        "compilation_core_primitives": {
            "claim": "Checked arithmetic primitive correspondence and source/IR-machine "
                     "determinism, finite-prefix and terminal-diagnostic properties; "
                     "structural finite-statement source/IR checker soundness, including local "
                     "declarations, scope, stateful expressions, conditional CFGs, checked "
                     "cyclic diagrams, div/rem, finite calls and private-leaf substitution. "
                     "Full core-v1 constructor/call/loop composition remains incomplete. "
                     "No proof of source export, input-validation execution safety, backend "
                     "compilation or verified-release acceptance",
            "targets": core_primitives,
        },
        "trust": TRUST,
        "status": "pass",
    }

    out_path = repo / args.out
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    enumerated = sum(
        len(t["theorems"]) for t in segment_b if t["instances"]["kind"] == "enumerated"
    )
    print(f"audit passed: {len(segment_a)} rule theorems, "
          f"{len(segment_b)} analysis rows ({enumerated} enumerated instances), "
          f"{len(axiom_rows)} axioms in the environment")
    print(f"receipt written to {args.out}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except AuditFailure as failure:
        print(f"AUDIT FAILED: {failure}", file=sys.stderr)
        raise SystemExit(1)
