import EvoIR.IRPoisonInstructions

/-! Actual call-control and saved-frame correspondence for dead-poison
normalization. Neither the call stack nor its full result bindings are erased.
The only unequal components are checked inactive values of the same type. -/
namespace EvoIR.Core.IR.Poison

inductive FramesRelated (plan : DemandPlan) : List ReturnFrame → List ReturnFrame → Prop where
  | nil : FramesRelated plan [] []
  | cons {sourceCaller targetCaller results sourceRest targetRest} :
      ActivationsRelated plan sourceCaller targetCaller →
      FramesRelated plan sourceRest targetRest →
      FramesRelated plan (⟨sourceCaller, results⟩ :: sourceRest) (⟨targetCaller, results⟩ :: targetRest)

theorem FramesRelated.refl (plan : DemandPlan) (stack : List ReturnFrame) :
    FramesRelated plan stack stack := by
  induction stack with
  | nil => exact .nil
  | cons frame rest ih => exact .cons (ActivationsRelated.refl plan frame.caller) ih

def ControlsRelated (plan : DemandPlan) : Control → Control → Prop
  | .running source, .running target => ActivationsRelated plan source target
  | .halted source, .halted target => source = target
  | _, _ => False

structure StatesRelated (plan : DemandPlan) (source target : State) : Prop where
  control : ControlsRelated plan source.control target.control
  stack : FramesRelated plan source.stack target.stack
  nextFrame : source.nextFrame = target.nextFrame

theorem StatesRelated.refl (plan : DemandPlan) (state : State) : StatesRelated plan state state := by
  refine ⟨?_, FramesRelated.refl plan state.stack, rfl⟩
  cases state.control with
  | running activation => exact ActivationsRelated.refl plan activation
  | halted result => rfl

theorem StatesRelated.replaceRunning {plan : DemandPlan} {source target : State}
    (states : StatesRelated plan source target) {sourceActivation targetActivation : Activation}
    (active : ActivationsRelated plan sourceActivation targetActivation) :
    StatesRelated plan {source with control := .running sourceActivation}
      {target with control := .running targetActivation} := ⟨active, states.stack, states.nextFrame⟩

theorem StatesRelated.replaceHalted {plan : DemandPlan} {source target : State}
    (states : StatesRelated plan source target) (result : Except Source.Fault (List Value)) :
    StatesRelated plan {source with control := .halted result}
      {target with control := .halted result} := ⟨rfl, states.stack, states.nextFrame⟩

theorem ActivationsRelated.advance {plan : DemandPlan} {source target : Activation}
    (active : ActivationsRelated plan source target) :
    ActivationsRelated plan {source with pc := source.pc+1} {target with pc := target.pc+1} :=
  ⟨active.function, active.block, congrArg (·+1) active.pc, active.owner, active.values⟩

theorem ActivationsRelated.replaceValues {plan : DemandPlan} {source target : Activation}
    (active : ActivationsRelated plan source target) {sourceValues targetValues : Bindings}
    (values : BanksRelated (neededFor plan source.function) sourceValues targetValues) :
    ActivationsRelated plan {source with values := sourceValues} {target with values := targetValues} :=
  ⟨active.function, active.block, active.pc, active.owner, values⟩

theorem return_related (plan : DemandPlan) (source target : State)
    (states : StatesRelated plan source target) (values : List Value) :
    ExceptRelated (StatesRelated plan)
      (match source.stack with
      | [] =>
        if values.any (fun value => match value with | .poison _ => true | _ => false) then
          .error (.diagnostic 3001)
        else .ok {source with control := .halted (.ok values)}
      | frame :: rest => do
        let bound ← appendResults frame.caller.values frame.results values
        return {source with control := .running {frame.caller with values := bound}, stack := rest})
      (match target.stack with
      | [] =>
        if values.any (fun value => match value with | .poison _ => true | _ => false) then
          .error (.diagnostic 3001)
        else .ok {target with control := .halted (.ok values)}
      | frame :: rest => do
        let bound ← appendResults frame.caller.values frame.results values
        return {target with control := .running {frame.caller with values := bound}, stack := rest}) := by
  cases source with | mk sourceControl sourceStack sourceNext =>
   cases target with | mk targetControl targetStack targetNext =>
    rcases states with ⟨controls, stacks, counter⟩
    cases stacks with
  | nil =>
    dsimp only
    split
    · rfl
    · exact ⟨rfl, .nil, counter⟩
  | @cons sourceCaller targetCaller results sourceRest targetRest callers rest =>
    dsimp only
    apply except_bind_related (callers.values.append_same results values)
    intro before after banks
    exact ⟨callers.replaceValues banks, rest, counter⟩

end EvoIR.Core.IR.Poison
