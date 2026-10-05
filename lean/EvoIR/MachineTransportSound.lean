import EvoIR.MachineTransportSimulation

/-! Verified two-input admission for the actual SSA and postSSA programs.
The only semantic premise is progress of the preceding actual SSA program,
which is supplied by the independently proved selection stage. No target
equivalence, invariant, bounded test or producer replay is a premise. -/
namespace EvoIR.Machine.Transport

theorem initial_related (source target : Program) (annotations : ProgramPlan)
    (accepted : checkProgram source target annotations = true)
    (type : Core.Source.IntType) (name : String) (values : List Value) (memory : Memory)
    (entry : State) (started : initial source name values memory = .ok entry) :
    initial target name values memory = .ok entry ∧ StateRelated source target type entry entry entry := by
  cases functionAt : source.functions.find? (·.name == name) with
  | none => simp [initial, functionAt, MonadExcept.throw, throwThe, MonadExceptOf.throw] at started
  | some function =>
    cases enteredAt : enter function values 1 with
    | error fault => simp [initial, functionAt, enteredAt, Bind.bind, Except.bind] at started
    | ok activation =>
      have entryShape : ({control := .running activation, stack := [], nextFrame := 2, memory} : State) = entry := by
        simpa [initial, functionAt, enteredAt, Bind.bind, Except.bind, Pure.pure, Except.pure] using started
      subst entry
      cases blockAt : findBlock function 0 with
      | none =>
        change function.blocks.find? (·.id == 0) = none at blockAt
        simp [enter, blockAt, MonadExcept.throw, throwThe, MonadExceptOf.throw] at enteredAt
      | some block =>
        obtain ⟨ctx, sourceFunction, sourceBlock⟩ := contextForBlock source target annotations accepted name function 0 block
          functionAt blockAt
        have blockId : ctx.sourceBlock.id = 0 := by
          rw [sourceBlock]
          simpa [findBlock] using List.find?_some blockAt
        obtain ⟨targetEntered, active⟩ := activeEntry_sameBank ctx values 1 activation blockId
          (by simpa only [sourceFunction] using enteredAt)
        have targetFunctionAt : target.functions.find? (·.name == name) = some ctx.targetFunction := by
          have functionName : function.name = name := by simpa using List.find?_some functionAt
          simpa only [sourceFunction, functionName] using ctx.targetFunctionAt
        refine ⟨?_, ⟨.refl _, .nil, rfl, rfl, activation, activation, rfl, rfl, ctx, active⟩⟩
        simp [initial, targetFunctionAt, targetEntered, Bind.bind, Except.bind, Pure.pure, Except.pure]

/-- Accepted actual SSA/postSSA pairs refine for every logical input vector
and private memory with proved source progress. This quantification is not a
finite runner or a sampled differential-execution claim. -/
theorem sound (source target : Program) (annotations : ProgramPlan)
    (accepted : checkProgram source target annotations = true)
    (type : Core.Source.IntType) (name : String) (values : List Value) (memory : Memory)
    (entry : State) (started : initial source name values memory = .ok entry)
    (progress : ObservedProgress .ssa source type entry) :
    initial target name values memory = .ok entry ∧ TransportRefines source target type entry entry := by
  obtain ⟨targetStarted, related⟩ := initial_related source target annotations accepted type name values memory entry started
  exact ⟨targetStarted, (checkpointSimulation source target annotations accepted type entry progress).sound related⟩

end EvoIR.Machine.Transport
