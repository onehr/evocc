import EvoIR.Core.ValidatorAdmission

/-! Actual call entry and function-return continuation preservation. The
callee's checked body and parameter store come from independent admission;
the caller's exact saved store is restored by the real execution machine. -/
namespace EvoIR.Core.Source

theorem validator_call_control_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State)
    (callee : Nat) (type : IntType) (arguments : List Value)
    (task : ValidatorCallTask program callee type arguments)
    (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId
      (.value task.environment type) state.continuation) :
    ValidatorMachineInvariant program entryType
      (enterFunction program state callee type arguments (some (state.functionId, state.locals))) := by
  obtain ⟨locals, parameters, parametersModel⟩ := checked_parameters_models
    program task.function task.checked arguments task.typedArguments
  let environment : ValidatorEnvironment :=
    ⟨task.checked.parameterState, localLayout locals, []⟩
  let bodyTask : ValidatorStatementTask task.checked.preceding task.function.body := {
    environment
    checkedInput := task.checked.parameterState
    bindings := rfl
    initialized := fun _ member => member
    result := task.function.returnType
    depth := 0
    checked := task.checked.body
    outputEnvironment := {
      state := ⟨task.checked.body.facts.state.bindings,
        union environment.state.initialized task.checked.body.facts.state.initialized⟩
      layout := environment.layout
      freshConst := [] }
    outputBindings := rfl
    outputInitialized := rfl
    outputLayout := rfl
    outputFresh := rfl }
  have calleeSymbol : task.function.symbol = callee :=
    by simpa using List.find?_some task.defined
  let context : ValidatorFunctionContext program callee task.checked.preceding :=
    ⟨task.function, task.defined, calleeSymbol, task.checked, rfl⟩
  have frame : ValidatorFrame program entryType callee state.functionId
      (.function (some (state.functionId, state.locals)) task.function.returnType.type)
      bodyTask.output (.value task.environment task.function.returnType.type) := by
    unfold ValidatorStatementTask.output
    exact .caller callee state.functionId
      ⟨bodyTask.environment, bodyTask.outputEnvironment, bodyTask.checked.facts.fallsThrough,
        0, task.function.returnType.type, true,
        statementAllowsAbrupt task.function.body⟩
      task.checked.returns rfl state.locals task.environment models
  have continuation : ValidatorContinuation program entryType callee bodyTask.output
      (.function (some (state.functionId, state.locals)) type :: state.continuation) :=
    .frame (by simpa only [task.result] using frame) tail
  have entered : enterFunction program state callee type arguments
      (some (state.functionId, state.locals)) =
      ⟨.statement task.function.body, locals,
        .function (some (state.functionId, state.locals)) type :: state.continuation, callee⟩ := by
    simp [enterFunction, task.defined, task.result, parameters]
  rw [entered]
  exact .running _ bodyTask.output checkedProgram
    (.statement task.checked.preceding task.function.body context bodyTask rfl locals parametersModel)
    continuation

theorem validator_entry_flow_preserves (program : Program) (entryType : IntType)
    (state : State) (protocol : ValidatorFlowProtocol)
    (noNormal : protocol.fallsThrough = false) (noEscapedLoop : protocol.loopDepth = 0)
    (result : protocol.resultType = entryType) (flow : Flow)
    (models : protocol.Models flow state.locals) :
    ValidatorMachineInvariant program entryType
      (resumeFlow state (.function none entryType) flow) := by
  cases flow with
  | normal => simp [ValidatorFlowProtocol.Models, noNormal] at models
  | «break» => simp [ValidatorFlowProtocol.Models, noEscapedLoop] at models
  | «continue» => simp [ValidatorFlowProtocol.Models, noEscapedLoop] at models
  | returned value =>
    have typed : value.type = entryType := models.2.1.trans result
    have returned : resumeFlow state (.function none entryType) (.returned value) =
        {state with control := .halted (.ok value), locals := []} := by
      simp [resumeFlow, typed]
    rw [returned]
    exact .terminal _ (.ok value) rfl typed

theorem validator_caller_flow_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State)
    (protocol : ValidatorFlowProtocol) (noNormal : protocol.fallsThrough = false)
    (noEscapedLoop : protocol.loopDepth = 0) (callerId : Nat) (saved : Locals)
    (environment : ValidatorEnvironment) (savedModels : environment.Models saved)
    (flow : Flow) (models : protocol.Models flow state.locals)
    (tail : ValidatorContinuation program entryType callerId
      (.value environment protocol.resultType) state.continuation) :
    ValidatorMachineInvariant program entryType
      (resumeFlow state (.function (some (callerId, saved)) protocol.resultType) flow) := by
  cases flow with
  | normal => simp [ValidatorFlowProtocol.Models, noNormal] at models
  | «break» => simp [ValidatorFlowProtocol.Models, noEscapedLoop] at models
  | «continue» => simp [ValidatorFlowProtocol.Models, noEscapedLoop] at models
  | returned value =>
    have typed : value.type = protocol.resultType := models.2.1
    have returned : resumeFlow state (.function (some (callerId, saved)) protocol.resultType)
        (.returned value) = {state with control := .value value, locals := saved, functionId := callerId} := by
      simp [resumeFlow, typed]
    rw [returned]
    exact .running _ (.value environment protocol.resultType) checkedProgram
      (.delivered (.value value) saved _ ⟨typed, savedModels⟩) tail

end EvoIR.Core.Source
