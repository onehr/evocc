import EvoIR.Core.ValidatorAdmission

/-! The entry case of independent-validator safety. Every local-store,
parameter uniqueness, function context and initial continuation witness comes
from `checkWellFormed` and the declared types of the supplied arguments. -/
namespace EvoIR.Core.Source

theorem checked_source_initial (program : Program) (entry : Nat) (function : Function)
    (arguments : List Value) (accepted : checkWellFormed program = .ok ())
    (selected : entry ∈ program.entries)
    (defined : program.functions.find? (·.symbol == entry) = some function)
    (typed : arguments.map Value.type = function.parameters.map (fun parameter => parameter.type.type)) :
    ∃ state, Source.initial program entry arguments = .ok state ∧
      ValidatorMachineInvariant program function.returnType.type state := by
  classical
  obtain ⟨checkedProgram⟩ := checked_source_program program accepted
  let checkedFunction := checkedProgram.functions function (List.mem_of_find?_eq_some defined)
  obtain ⟨locals, parameters, models⟩ := checked_parameters_models program function checkedFunction
    arguments typed
  let environment : ValidatorEnvironment :=
    ⟨checkedFunction.parameterState, localLayout locals, []⟩
  let outputState : LocalState := ⟨checkedFunction.body.facts.state.bindings,
    union environment.state.initialized checkedFunction.body.facts.state.initialized⟩
  let outputEnvironment : ValidatorEnvironment := ⟨outputState, environment.layout, []⟩
  let task : ValidatorStatementTask checkedFunction.preceding function.body := {
    environment, checkedInput := checkedFunction.parameterState, bindings := rfl,
    initialized := fun _ member => member,
    result := function.returnType, depth := 0, checked := checkedFunction.body,
    outputEnvironment, outputBindings := rfl, outputInitialized := rfl,
    outputLayout := rfl, outputFresh := rfl }
  let context : ValidatorFunctionContext program entry checkedFunction.preceding := {
    function, defined, symbol := by simpa using List.find?_some defined,
    checked := checkedFunction, preceding := rfl }
  let state : State := ⟨.statement function.body, locals, [.function none function.returnType.type], entry⟩
  refine ⟨state, ?_, ?_⟩
  · simp [Source.initial, selected, defined, parameters, state, bind, Except.bind, pure, Except.pure]
  · refine .running state task.output checkedProgram
      (.statement checkedFunction.preceding function.body context task rfl locals models) ?_
    unfold ValidatorStatementTask.output
    exact .frame (.entry entry _ checkedFunction.returns rfl rfl) (.terminal entry)

end EvoIR.Core.Source
