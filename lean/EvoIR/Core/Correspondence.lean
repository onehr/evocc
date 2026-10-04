import EvoIR.Core.Execution
import EvoIR.Core.IRExecution
import EvoIR.Core.Simulation

/-! Source/IR observations and relational adapters. Private memory snapshots
are not external effects in core-v1, but malformed/poison failures are not
silently reclassified as admitted checked diagnostics. -/
namespace EvoIR.Core

inductive Observation where
  | returned (type : Source.IntType) (bits : Nat)
  | diagnostic (code : Nat)
  deriving DecidableEq, Repr

def checkedDiagnostic (code : Nat) : Option Observation :=
  if code = 3002 ∨ code = 3003 ∨ code = 3004 ∨ code = 3005 then
    some (.diagnostic code) else none

def observeSource (s : Source.State) : Option Observation :=
  match s.control with
  | .halted (.ok v) => some (.returned v.type v.bits.toNat)
  | .halted (.error (.diagnostic code)) => checkedDiagnostic code
  | _ => none

def observeTarget (type : Source.IntType) (s : IR.State) : Option Observation :=
  match s.control with
  | .halted (.ok [.bits n bits])
  | .halted (.ok [.bits n bits, .memory _, .world]) =>
    if n = type.width then some (.returned type bits.toNat) else none
  | .halted (.error (.diagnostic code)) => checkedDiagnostic code
  | _ => none

theorem Source.Steps.toReaches {p : Source.Program} {s t : Source.State}
    (h : Source.Steps p s t) : Transition.Reaches (Source.step p) s t := by
  induction h with
  | refl => exact .refl _
  | next h _ ih => exact .next h ih

theorem IR.Steps.toReaches {p : IR.Program} {s t : IR.State}
    (h : IR.Steps p s t) : Transition.Reaches (IR.step p) s t := by
  induction h with
  | refl => exact .refl _
  | next h _ ih => cases h with | execute h => exact .next h ih

/-- All observations, divergence and target progress from the related entry.
No fuel bound, hash identity, native tests or unsupported-state escape hatch
occurs in this proposition. -/
def LoweringRefines (s : Source.Program) (t : IR.Program)
    (resultType : Source.IntType) (s₀ : Source.State) (t₀ : IR.State) : Prop :=
  Transition.Refines (Source.step s) (IR.step t) observeSource
    (observeTarget resultType) s₀ t₀

/-- The local invariant obligations for the actual source/IR machines. A
relation/rank proposal alone conveys no authority: transfer, terminal and
progress obligations must all be established independently for both inputs. -/
def LoweringSimulation (s : Source.Program) (t : IR.Program)
    (resultType : Source.IntType) (related : Source.State → IR.State → Prop)
    (rank : Source.State → IR.State → Nat) : Prop :=
  Transition.RankedSimulation (Source.step s) (IR.step t) observeSource
    (observeTarget resultType) related rank

theorem ranked_lowering_refines (s : Source.Program) (t : IR.Program)
    (resultType : Source.IntType) (related : Source.State → IR.State → Prop)
    (rank : Source.State → IR.State → Nat)
    (obligations : LoweringSimulation s t resultType related rank)
    (s₀ : Source.State) (t₀ : IR.State) (entry : related s₀ t₀) :
    LoweringRefines s t resultType s₀ t₀ :=
  obligations.sound entry

end EvoIR.Core
