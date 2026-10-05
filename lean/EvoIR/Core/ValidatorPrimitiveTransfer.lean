import EvoIR.Core.ValidatorStoreTransfer

/-! Actual scalar/write/initializer continuation transitions of the one
validator-machine invariant. The operand/store premises are obtained from the
typed frame and its delivery protocol, not inputs to final source safety. -/
namespace EvoIR.Core.Source

theorem validator_diagnostic_invariant (program : Program) (entryType : IntType)
    (state : State) (code : Nat) (admitted : code ∈ [3002, 3003, 3004, 3005]) :
    ValidatorMachineInvariant program entryType (fail state (.diagnostic code)) :=
  .terminal _ (.error (.diagnostic code)) rfl admitted

theorem validator_result_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (environment : ValidatorEnvironment)
    (models : environment.Models state.locals) (type : IntType) (result : Result)
    (safe : ResultSafe type result)
    (tail : ValidatorContinuation program entryType state.functionId (.value environment type)
      state.continuation) :
    ValidatorMachineInvariant program entryType (valueResult state type result) := by
  cases result with
  | error fault =>
    cases fault with
    | malformed message => exact False.elim safe
    | diagnostic code => exact validator_diagnostic_invariant program entryType state code safe
  | ok value =>
    have typed : value.type = type := safe
    simp only [valueResult, typed, BEq.rfl, ↓reduceIte]
    exact .running _ (.value environment type) checked
      (.delivered _ _ _ ⟨typed, models⟩) tail

def primitiveValueFrame : Frame → Bool
  | .unary .. | .binaryRight .. | .convert .. | .assign ..
  | .discard | .return .. | .initialize .. => true
  | _ => false

theorem validator_primitive_value_frame_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (value : Value)
    (nextFunctionId : Nat) (frame : Frame) (input output : ValidatorProtocol)
    (typed : ValidatorFrame program entryType state.functionId nextFunctionId frame input output)
    (tail : ValidatorContinuation program entryType nextFunctionId output state.continuation)
    (delivered : input.Models (.value value) state.locals)
    (primitive : primitiveValueFrame frame = true) :
    ValidatorMachineInvariant program entryType (resumeValue state frame value) := by
  rcases state with ⟨control, locals, continuation, functionId⟩
  cases typed <;> try simp [primitiveValueFrame] at primitive
  case unary environment inputType op =>
    change value.type = inputType ∧ environment.Models locals at delivered
    obtain ⟨valueType, models⟩ := delivered
    have safe := unary_result_safe op value false
    rw [valueType] at safe
    exact validator_result_transfer program entryType checked _ environment models _ _ safe tail
  case binaryRight environment op left rightType admitted =>
    change value.type = rightType ∧ environment.Models locals at delivered
    obtain ⟨valueType, models⟩ := delivered
    have safe := binary_result_safe op left value false admitted
    rw [valueType] at safe
    exact validator_result_transfer program entryType checked _ environment models _ _ safe tail
  case convert environment inputType type =>
    change value.type = inputType ∧ environment.Models locals at delivered
    exact .running _ (.value environment type) checked
      (.delivered _ _ _ ⟨rfl, delivered.2⟩) tail
  case assign environment inputType op id destination binding writable initialized admitted =>
    change value.type = inputType ∧ environment.Models locals at delivered
    have transferred := validator_assignment_transfer environment
      ⟨control, locals, continuation, functionId⟩ delivered.2 op id destination value
      binding writable initialized admitted
    rcases transferred with ⟨result, updated, resultType, models, result⟩ | ⟨code, admitted, result⟩
    · rw [result]
      exact .running _
        (.value {environment with state.initialized := id :: environment.state.initialized} destination.type)
        checked (.delivered _ _ _ ⟨resultType, models⟩) tail
    · rw [result]
      exact validator_diagnostic_invariant program entryType _ code admitted
  case discard environment type depth result =>
    change value.type = type ∧ environment.Models locals at delivered
    refine .running _ _ checked (.delivered _ _ _ ?_) tail
    exact ⟨rfl, delivered.2⟩
  case returned environment type depth destination =>
    change value.type = type ∧ environment.Models locals at delivered
    refine .running _ _ checked (.delivered _ _ _ ?_) tail
    exact ⟨rfl, rfl, delivered.2⟩
  case «initialize» before inputType id destination binding fresh after afterBindings
      afterLayout afterInitialized afterFresh depth result =>
    change value.type = inputType ∧ before.Models locals at delivered
    obtain ⟨updated, written, models⟩ := delivered.2.initialize before after locals id destination
      binding fresh afterBindings afterLayout afterInitialized afterFresh value
    simp only [resumeValue, written]
    exact .running _ (.flow ⟨after, after, true, depth, result.type, true, false⟩)
      checked (.delivered _ _ _ ⟨rfl, models⟩) tail

end EvoIR.Core.Source
