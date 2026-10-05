import EvoIR.Core.ValidatorInitial
import EvoIR.Core.ValidatorForTransfer
import EvoIR.Core.ValidatorObjectTransfer
import EvoIR.Core.ValidatorSequenceTransfer

/-! Independent source-validator safety. The public admission theorem has only
the actual checker result, selected entry, actual function lookup and typed
arguments as premises. The concrete machine invariant and every suspended-frame
fact are derived internally; no accepted compilation pair is used. -/
namespace EvoIR.Core.Source

theorem validator_value_frame_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (value : Value)
    (nextFunctionId : Nat) (frame : Frame) (input output : ValidatorProtocol)
    (typed : ValidatorFrame program entryType state.functionId nextFunctionId frame input output)
    (tail : ValidatorContinuation program entryType nextFunctionId output state.continuation)
    (delivered : input.Models (.value value) state.locals) :
    ValidatorMachineInvariant program entryType (resumeValue state frame value) := by
  have primitive := validator_primitive_value_frame_preserves program entryType checked state value
    nextFunctionId frame input output typed tail delivered
  rcases state with ⟨control, locals, continuation, functionId⟩
  cases typed <;> try simp [ValidatorProtocol.Models, ValidatorItemsTask.output,
    ValidatorStatementTask.output] at delivered
  case unary => exact primitive rfl
  case binaryRight => exact primitive rfl
  case convert => exact primitive rfl
  case assign => exact primitive rfl
  case discard => exact primitive rfl
  case returned => exact primitive rfl
  case «initialize» => exact primitive rfl
  case binaryLeft callees type op left right context parent checkedLeft fuel =>
    exact validator_binary_left_transfer program entryType checked _ callees type op left right context
      parent checkedLeft fuel value delivered tail
  case choice callees type condition yes no context parent checkedCondition fuel =>
    exact validator_choice_transfer program entryType checked _ callees type condition yes no context
      parent checkedCondition fuel value delivered tail
  case commaLeft callees type left right context parent checkedLeft fuel =>
    exact validator_comma_left_transfer program entryType checked _ callees type left right context
      parent checkedLeft fuel value delivered tail
  case branch callees condition yes no context parent result checkedCondition fuel =>
    exact validator_branch_value_transfer program entryType checked _ callees condition yes no context
      parent result checkedCondition fuel value delivered tail
  case loopTest callees context loop result =>
    exact validator_loop_test_transfer program entryType checked _ callees context loop result value delivered tail
  case loopIncrement callees context loop result expression present =>
    exact validator_loop_increment_transfer program entryType checked _ callees context loop result
      expression present value delivered tail
  case argument callees type callee destination expression remaining reverseValues context parent checkedArgument fuel =>
    exact validator_call_argument_preserves program entryType checked _ callees context type callee destination
      expression remaining reverseValues parent checkedArgument fuel value delivered.2 tail
  case sequence callees first remaining context parent functionResult child environment input result depth fuel =>
    cases child <;> simp [ValidatorProtocol.Models, ValidatorItemTask.output,
      ValidatorObjectTask.output, ValidatorStatementTask.output] at delivered

theorem validator_flow_frame_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (flow : Flow)
    (nextFunctionId : Nat) (frame : Frame) (input output : ValidatorProtocol)
    (typed : ValidatorFrame program entryType state.functionId nextFunctionId frame input output)
    (tail : ValidatorContinuation program entryType nextFunctionId output state.continuation)
    (delivered : input.Models (.flow flow) state.locals) :
    ValidatorMachineInvariant program entryType (resumeFlow state frame flow) := by
  rcases state with ⟨control, locals, continuation, functionId⟩
  cases typed <;> try simp [ValidatorProtocol.Models, ValidatorExpressionTask.output] at delivered
  case sequence callees first remaining context parent functionResult child environment input result depth fuel =>
    exact validator_sequence_frame_preserves program entryType checked _ callees first remaining context
      parent functionResult child environment input result depth fuel flow delivered tail
  case scope callees items context parent functionResult child environment input result depth fuel =>
    exact validator_scope_frame_preserves program entryType checked _ callees items context
      parent functionResult child environment input result depth fuel flow delivered tail
  case scopeProtocol input output certificate =>
    exact validator_scope_protocol_preserves program entryType checked _ input output certificate flow delivered tail
  case loopBody callees context loop result =>
    exact validator_loop_body_transfer program entryType checked _ callees context loop result flow delivered tail
  case startLoop callees context loop result input after onlyNormal =>
    exact validator_start_loop_transfer program entryType checked _ callees context loop result
      input after onlyNormal flow delivered tail
  case entry protocol noNormal noEscaped result =>
    exact validator_entry_flow_preserves program entryType _ protocol noNormal noEscaped result flow delivered
  case caller protocol noNormal noEscaped saved environment models =>
    exact validator_caller_flow_preserves program entryType checked _ protocol noNormal noEscaped nextFunctionId
      saved environment models flow delivered tail

theorem validator_delivered_step_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (functionId : Nat) (protocol : ValidatorProtocol)
    (frames : List Frame)
    (continuation : ValidatorContinuation program entryType functionId protocol frames)
    (state next : State) (identity : state.functionId = functionId) (stack : state.continuation = frames)
    (models : protocol.Models state.control state.locals) (stepped : Step program state next) :
    ValidatorMachineInvariant program entryType next := by
  induction continuation generalizing state next with
  | terminal => simp [ValidatorProtocol.Models] at models
  | weaken entails continuation induction =>
    exact induction state next identity stack (entails state.control state.locals models) stepped
  | @frame functionId nextFunctionId input output frame rest typed tail induction =>
    have frameTyping : ValidatorFrame program entryType state.functionId nextFunctionId frame input output := by
      rw [identity]
      exact typed
    let suspended := {state with continuation := rest}
    have tailTyping : ValidatorContinuation program entryType nextFunctionId output suspended.continuation := tail
    cases control : state.control
    case expression | statement | items | object | call | halted =>
      cases input <;> simp [control, ValidatorProtocol.Models] at models
    case value value =>
      have entered : next = resumeValue suspended frame value := by
        simpa [Step, step, control, stack, suspended] using stepped.symm
      rw [entered]
      rw [control] at models
      exact validator_value_frame_preserves program entryType checked suspended value nextFunctionId frame
        input output frameTyping tailTyping models
    case flow flow =>
      have entered : next = resumeFlow suspended frame flow := by
        simpa [Step, step, control, stack, suspended] using stepped.symm
      rw [entered]
      rw [control] at models
      exact validator_flow_frame_preserves program entryType checked suspended flow nextFunctionId frame
        input output frameTyping tailTyping models

theorem validator_statement_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (statement : Stmt) (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorStatementTask callees statement) (result : task.result = context.function.returnType)
    (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state statement) := by
  cases statement with
  | null => exact validator_null_control_preserves program entryType checked state callees task models tail
  | «break» =>
    exact validator_abrupt_control_preserves program entryType checked state callees .break
      (Or.inl rfl) task models tail
  | «continue» =>
    exact validator_abrupt_control_preserves program entryType checked state callees .continue
      (Or.inr rfl) task models tail
  | expression expression =>
    exact validator_expression_statement_preserves program entryType checked state
      callees expression context task models tail
  | «return» destination expression =>
    exact validator_return_control_preserves program entryType checked state
      callees destination expression context task models tail
  | compound items =>
    exact validator_compound_control_preserves program entryType checked state callees items
      context task result models tail
  | ite condition yes no =>
    exact validator_branch_control_preserves program entryType checked state callees
      condition yes no context task result models tail
  | «while» condition body =>
    exact validator_while_control_preserves program entryType checked state callees
      condition body context task result models tail
  | doWhile body condition =>
    exact validator_do_while_control_preserves program entryType checked state callees
      body condition context task result models tail
  | «for» init condition increment body =>
    exact validator_for_control_preserves program entryType checked state
      callees init condition increment body context task result models tail

theorem validator_step_preserves (program : Program) (entryType : IntType) (state next : State)
    (invariant : ValidatorMachineInvariant program entryType state) (stepped : Step program state next) :
    ValidatorMachineInvariant program entryType next := by
  cases invariant with
  | terminal result halted _ => simp [Step, step, halted] at stepped
  | running protocol checked control continuation =>
    rcases state with ⟨actual, locals, frames, functionId⟩
    cases control with
    | expression callees expression context task locals models =>
      have entered : next = enterExpression ⟨.expression expression, locals, frames, functionId⟩ expression := by
        simpa [Step, step] using stepped.symm
      rw [entered]
      exact validator_expression_control_preserves program entryType checked _ callees expression context task models continuation
    | statement callees statement context task result locals models =>
      have entered : next = enterStatement ⟨.statement statement, locals, frames, functionId⟩ statement := by
        simpa [Step, step] using stepped.symm
      rw [entered]
      exact validator_statement_control_preserves program entryType checked _ callees statement context task result models continuation
    | items callees items context task result locals models =>
      exact validator_items_control_preserves program entryType checked _ callees items context task result
        models continuation next stepped
    | object callees object context task result locals models =>
      have entered : next = enterObject ⟨.object object, locals, frames, functionId⟩ object := by
        simpa [Step, step] using stepped.symm
      rw [entered]
      exact validator_object_control_preserves program entryType checked _ callees object context task result models continuation
    | call callee type arguments task locals models =>
      have entered : next = enterFunction program ⟨.call callee type arguments, locals, frames, functionId⟩
          callee type arguments (some (functionId, locals)) := by
        simpa [Step, step] using stepped.symm
      rw [entered]
      exact validator_call_control_preserves program entryType checked _ callee type arguments task models continuation
    | delivered actual locals protocol models =>
      exact validator_delivered_step_preserves program entryType checked functionId protocol frames continuation
        _ next rfl rfl models stepped

theorem validator_reachable (program : Program) (entryType : IntType) (state next : State)
    (invariant : ValidatorMachineInvariant program entryType state) (steps : Steps program state next) :
    ValidatorMachineInvariant program entryType next := by
  induction steps with
  | refl => exact invariant
  | next step _ induction => exact induction (validator_step_preserves program entryType _ _ invariant step)

theorem validator_no_malformed (program : Program) (entryType : IntType) (state : State)
    (invariant : ValidatorMachineInvariant program entryType state) (message : String) :
    state.control ≠ .halted (.error (.malformed message)) := by
  intro halted
  exact validator_terminal_safe program entryType state invariant halted

theorem checked_source_safety (program : Program) (entry : Nat) (function : Function) (arguments : List Value)
    (accepted : checkWellFormed program = .ok ()) (selected : entry ∈ program.entries)
    (defined : program.functions.find? (·.symbol == entry) = some function)
    (typed : arguments.map Value.type = function.parameters.map (fun parameter => parameter.type.type)) :
    ∃ state, Source.initial program entry arguments = .ok state ∧
      ∀ next, Steps program state next →
        ValidatorMachineInvariant program function.returnType.type next ∧
        (∀ message, next.control ≠ .halted (.error (.malformed message))) ∧
        ((∃ result, next.control = .halted result ∧ ResultSafe function.returnType.type result) ∨
          ∃ following, Step program next following) := by
  obtain ⟨state, initial, invariant⟩ := checked_source_initial program entry function arguments accepted selected defined typed
  refine ⟨state, initial, ?_⟩
  intro next steps
  have retained := validator_reachable program _ state next invariant steps
  exact ⟨retained, validator_no_malformed program _ next retained,
    validator_progress program _ next retained⟩

end EvoIR.Core.Source
