import EvoIR.Core.StatementDiagram
import EvoIR.Core.SourceEquality

/-! The existing source continuation with symbolic saved values. Mapping a
frame changes only its value payloads, not the source machine or evaluation
order. Static cutpoint sites never forget a captured value. -/
namespace EvoIR.Core.Source

deriving instance DecidableEq for Slot, Frame

def Frame.map {α β : Type} (f : α → β) : Frame α → Frame β
  | .unary t op => .unary t op
  | .binaryLeft t op right => .binaryLeft t op right
  | .binaryRight t op left => .binaryRight t op (f left)
  | .choice t yes no => .choice t yes no
  | .convert t => .convert t
  | .assign t op id destination => .assign t op id destination
  | .callArgument t callee destination remaining values =>
      .callArgument t callee destination remaining (values.map f)
  | .discard => .discard
  | .return destination => .return destination
  | .initialize id destination => .initialize id destination
  | .sequence rest => .sequence rest
  | .scope n => .scope n
  | .branch yes no => .branch yes no
  | .startLoop test increment body => .startLoop test increment body
  | .loopTest test increment body => .loopTest test increment body
  | .loopBody test increment body => .loopBody test increment body
  | .loopIncrement test increment body => .loopIncrement test increment body
  | .function caller t => .function caller t

theorem Frame.map_map {α β γ : Type} (f : α → β) (g : β → γ) (frame : Frame α) :
    (frame.map f).map g = frame.map (g ∘ f) := by
  cases frame <;> simp [map, List.map_map]

/-- A static site is admissible only when no runtime value is hidden in it. -/
def Frame.static? {α : Type} : Frame α → Option Frame
  | .binaryRight .. | .callArgument .. => none
  | .unary t op => some (.unary t op)
  | .binaryLeft t op right => some (.binaryLeft t op right)
  | .choice t yes no => some (.choice t yes no)
  | .convert t => some (.convert t)
  | .assign t op id destination => some (.assign t op id destination)
  | .discard => some .discard
  | .return destination => some (.return destination)
  | .initialize id destination => some (.initialize id destination)
  | .sequence rest => some (.sequence rest)
  | .scope n => some (.scope n)
  | .branch yes no => some (.branch yes no)
  | .startLoop test increment body => some (.startLoop test increment body)
  | .loopTest test increment body => some (.loopTest test increment body)
  | .loopBody test increment body => some (.loopBody test increment body)
  | .loopIncrement test increment body => some (.loopIncrement test increment body)
  | .function caller t => some (.function caller t)

theorem Frame.static_sound {α : Type} (f : α → Value) (frame : Frame α) (out : Frame)
    (h : frame.static? = some out) : frame.map f = out := by
  cases frame <;> simp only [static?, Option.some.injEq] at h
  all_goals first | contradiction | (subst out; rfl)

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

abbrev SavedFrames (Γ : Var → Nat) := List (Source.Frame (CWord Γ))

def CWord.literal {Γ : Var → Nat} (value : Source.Value) : CWord Γ :=
  ⟨value.type, .const value.type.width value.bits.toNat⟩

@[simp] theorem CWord.literal_denote {Γ : Var → Nat} (σ : Subst Γ) (value : Source.Value) :
    (CWord.literal value).denote σ = value := by
  cases value
  simp [literal, CWord.denote]

def SavedFrames.denote {Γ : Var → Nat} (σ : Subst Γ) (frames : SavedFrames Γ) : List Source.Frame :=
  frames.map (Source.Frame.map (CWord.denote σ))

def SavedFrames.literal {Γ : Var → Nat} (frames : List Source.Frame) : SavedFrames Γ :=
  frames.map (Source.Frame.map CWord.literal)

@[simp] theorem SavedFrames.literal_denote {Γ : Var → Nat} (σ : Subst Γ) (frames : List Source.Frame) :
    (SavedFrames.literal frames).denote σ = frames := by
  simp only [literal, denote, List.map_map]
  induction frames with
  | nil => rfl
  | cons frame rest ih =>
    cases frame <;> simp_all [Source.Frame.map, Function.comp_def]

theorem SavedFrames.static_sound {Γ : Var → Nat} (σ : Subst Γ) (frames : SavedFrames Γ)
    (out : List Source.Frame) (h : frames.mapM Source.Frame.static? = some out) :
    frames.denote σ = out := by
  induction frames generalizing out with
  | nil => simpa [denote] using h.symm
  | cons frame rest ih =>
    cases a : frame.static? with
    | none => simp [List.mapM_cons, a] at h
    | some first =>
      cases b : rest.mapM Source.Frame.static? with
      | none => simp [List.mapM_cons, a, b] at h
      | some tail =>
        simp [List.mapM_cons, a, b] at h
        subst out
        simp only [denote, List.map_cons]
        rw [Source.Frame.static_sound _ _ _ a]
        exact congrArg (List.cons first) (ih tail b)

end EvoIR.Core.Symbolic
