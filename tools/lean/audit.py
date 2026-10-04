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
    "EvoIR.Core.checkedDiagnostic",
    "EvoIR.Core.observeSource",
    "EvoIR.Core.observeTarget",
    "EvoIR.Core.LoweringRefines",
    "EvoIR.Core.AbsCorrespondence.sourceBody",
    "EvoIR.Core.AbsCorrespondence.source",
    "EvoIR.Core.AbsCorrespondence.target",
    "EvoIR.Core.AbsCorrespondence.checkPilotPair",
    "EvoIR.denoteOp",
    "EvoIR.denoteTerm",
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
        "∃ (s₀ : EvoIR.Core.Source.State) (t₀ : EvoIR.Core.IR.State), "
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
                     "no proof of source export, input-validation soundness, source/IR "
                     "simulation, whole-function checker or compilation yet",
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
