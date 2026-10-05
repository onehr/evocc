import EvoIR.Asm.Layout

/-!
Execution of assembly code in a function body. A straight-line instruction
changes the data of the state and advances the line. `effect` is that data
change; `execute_straight` proves it against `execute`. A run of straight
lines is an actual path of `step` of the same length.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)
open Core.Transition (Path)

/-- The straight-line instructions: all forms except control transfers. -/
def straight : Inst → Bool
  | .bl _ | .b _ | .bCond _ _ | .cb _ _ _ _ | .tb _ _ _ _ _ | .ret | .brk _ => false
  | _ => true

/-- The data change of a straight-line instruction. The control is unchanged. -/
def effect (s : State) : Inst → Except Fault State
  | .stpPre first second off => do
    let base ← s.baseValue .sp
    let address := base + BitVec.ofInt 64 off
    let s ← s.store address 8 (s.regs first)
    let s ← s.store (address + 8) 8 (s.regs second)
    return {s with sp := address}
  | .ldpPost first second off => do
    let base ← s.baseValue .sp
    let one ← s.load base 8
    let two ← s.load (base + 8) 8
    let s := (s.set .x (.r first) one).set .x (.r second) two
    return {s with sp := base + BitVec.ofInt 64 off}
  | .movFromSp d => return s.set .x (.r d) s.sp
  | .subSp n => return {s with sp := s.sp - BitVec.ofNat 64 n}
  | .addSp n => return {s with sp := s.sp + BitVec.ofNat 64 n}
  | .addFromSp d n => return s.set .x d (s.sp + BitVec.ofNat 64 n)
  | .ldr width t base off => do
    let address := (← s.baseValue base) + BitVec.ofInt 64 off
    let value ← s.load address (widthBytes width)
    return s.set width t value
  | .str width t base off => do
    let address := (← s.baseValue base) + BitVec.ofInt 64 off
    s.store address (widthBytes width) (s.get width t)
  | .mov width d src => return s.set width d (s.get width src)
  | .movz width d n => return s.set width d (BitVec.ofNat 64 n)
  | .movk width d n shift => return s.set width d (insertHalf (s.reg d) n shift)
  | .alu op width d lhs rhs => return s.set width d (alu op width (s.reg lhs) (s.reg rhs))
  | .aluImm op width d lhs n => return s.set width d (alu op width (s.reg lhs) (BitVec.ofNat 64 n))
  | .flagAlu op width d lhs rhs =>
    let (value, flags) := flagAlu op width (s.reg lhs) (s.reg rhs)
    return {s.set width d value with flags}
  | .wideMul op d lhs rhs => return s.set .x d (wideMul op (s.reg lhs) (s.reg rhs))
  | .msub width d n m a =>
    return s.set width d (alu .sub width (s.reg a) (alu .mul width (s.reg n) (s.reg m)))
  | .madd width d n m a =>
    return s.set width d (alu .add width (s.reg a) (alu .mul width (s.reg n) (s.reg m)))
  | .extend op width d src => return s.set width d (extend op (s.reg src))
  | .bfx signed width d src lsb bits => return s.set width d (bitfield signed width (s.reg src) lsb bits)
  | .cmp width lhs rhs => return {s with flags := (flagAlu .subs width (s.reg lhs) (s.reg rhs)).2}
  | .cmpImm width lhs n =>
    return {s with flags := (flagAlu .subs width (s.reg lhs) (BitVec.ofNat 64 n)).2}
  | .tstImm width lhs n => return {s with flags := logicalFlags width (s.reg lhs) (BitVec.ofNat 64 n)}
  | .cset width d cond => return s.set width d (if s.flags.holds cond then 1 else 0)
  | .csel width d lhs rhs cond => return s.set width d (if s.flags.holds cond then s.reg lhs else s.reg rhs)
  | _ => throw (.malformed "control transfer")

/-- Run straight-line code from line `pos` of function `fn`. Each line sets
the next line, as `execute` does. -/
def runFrom (fn : Nat) : Nat → List Inst → State → Except Fault State
  | _, [], s => .ok s
  | pos, i :: rest, s => do
    let s' ← effect s i
    runFrom fn (pos+1) rest {s' with control := .running fn (pos+1)}

theorem execute_straight (file : File) (s : State) (fn pc : Nat) (i : Inst)
    (h : straight i = true) :
    execute file s fn pc i = (effect s i).map (fun t => {t with control := .running fn (pc+1)}) := by
  cases i <;> simp [straight] at h <;>
    simp only [execute, effect, State.next, bind, Except.bind, pure, Except.pure, Except.map] <;>
    (repeat' split) <;> simp_all

theorem step_at (file : File) (s : State) (fn pos : Nat) (g : Function) (i : Inst)
    (rest : List Line) (hc : s.control = .running fn pos) (hg : file.functions[fn]? = some g)
    (hd : g.body.drop pos = .inst i :: rest) :
    step file s = some (match execute file s fn pos i with
      | .ok next => next
      | .error fault => {s with control := .halted (.error fault)}) := by
  have hl : g.body[pos]? = some (.inst i) := by
    have := congrArg (·[0]?) hd
    simpa using this
  simp [step, hc, run, hg, hl]
  rfl

theorem step_label_at (file : File) (s : State) (fn pos : Nat) (g : Function) (k : Nat)
    (rest : List Line) (hc : s.control = .running fn pos) (hg : file.functions[fn]? = some g)
    (hd : g.body.drop pos = .label k :: rest) :
    step file s = some (s.next fn pos) := by
  have hl : g.body[pos]? = some (.label k) := by
    have := congrArg (·[0]?) hd
    simpa using this
  simp [step, hc, run, hg, hl]
  rfl

/-- A run of straight-line code is an actual path of the same length. -/
theorem run_path (file : File) (fn : Nat) (g : Function) (hg : file.functions[fn]? = some g) :
    ∀ (code : List Inst) (pos : Nat) (rest : List Line) (s s' : State),
      s.control = .running fn pos → g.body.drop pos = code.map .inst ++ rest →
      code.all straight = true → runFrom fn pos code s = .ok s' →
      Path (step file) code.length s s' ∧ s'.control = .running fn (pos + code.length)
  | [], pos, rest, s, s', hc, _, _, hr => by
    simp only [runFrom, Except.ok.injEq] at hr
    subst hr
    exact ⟨.refl _, by simpa using hc⟩
  | i :: code, pos, rest, s, s', hc, hd, hs, hr => by
    simp only [List.all_cons, Bool.and_eq_true] at hs
    simp only [runFrom, bind, Except.bind] at hr
    split at hr
    · cases hr
    · rename_i mid hm
      have hstep : step file s = some {mid with control := .running fn (pos+1)} := by
        rw [step_at file s fn pos g i (code.map .inst ++ rest) hc hg (by simpa using hd),
          execute_straight file s fn pos i hs.1, hm]
        rfl
      have hd' : g.body.drop (pos+1) = code.map .inst ++ rest := by
        rw [← List.drop_drop, hd]; simp
      obtain ⟨path, ctrl⟩ := run_path file fn g hg code (pos+1) rest _ s' rfl hd' hs.2 hr
      refine ⟨.next hstep path, ?_⟩
      simpa [Nat.add_assoc, Nat.add_comm 1] using ctrl

end EvoIR.Asm.Proof
