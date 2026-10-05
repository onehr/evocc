import EvoIR.MachineExecution
import EvoIR.Core.Simulation

/-!
The exact semantic boundaries of virtual machine lowering. These propositions
use the independently defined machines and preserve the same observations as
the source-to-IR theorem. Composition does not establish either checker premise:
each must be derived by checking the actual programs and entry states.
-/
namespace EvoIR.Machine

def SelectionRefines (source : Core.IR.Program) (target : Program)
    (resultType : Core.Source.IntType) (sourceEntry : Core.IR.State)
    (targetEntry : State) : Prop :=
  Core.Transition.Refines (Core.IR.step source) (step .ssa target)
    (Core.observeTarget resultType) (observe resultType) sourceEntry targetEntry

def TransportRefines (source target : Program) (resultType : Core.Source.IntType)
    (sourceEntry targetEntry : State) : Prop :=
  Core.Transition.Refines (step .ssa source) (step .postSSA target)
    (observe resultType) (observe resultType) sourceEntry targetEntry

def IRToMachineRefines (source : Core.IR.Program) (target : Program)
    (resultType : Core.Source.IntType) (sourceEntry : Core.IR.State)
    (targetEntry : State) : Prop :=
  Core.Transition.Refines (Core.IR.step source) (step .postSSA target)
    (Core.observeTarget resultType) (observe resultType) sourceEntry targetEntry

def SourceToMachineRefines (source : Core.Source.Program) (target : Program)
    (resultType : Core.Source.IntType) (sourceEntry : Core.Source.State)
    (targetEntry : State) : Prop :=
  Core.Transition.Refines (Core.Source.step source) (step .postSSA target)
    Core.observeSource (observe resultType) sourceEntry targetEntry

/-- The shared intermediate machine and entry state are syntactically the
same in both premises. A second compilation or merely equal digest cannot
stand in for this composition boundary. -/
theorem selection_transport_refines
    {ir : Core.IR.Program} {ssa lowered : Program} {resultType : Core.Source.IntType}
    {irEntry : Core.IR.State} {ssaEntry loweredEntry : State}
    (selection : SelectionRefines ir ssa resultType irEntry ssaEntry)
    (transport : TransportRefines ssa lowered resultType ssaEntry loweredEntry) :
    IRToMachineRefines ir lowered resultType irEntry loweredEntry :=
  Core.Transition.Refines.trans selection transport

/-- Conditional composition with the already established source-to-IR result.
No claim about allocation, physical frames or emitted code is in this theorem. -/
theorem source_machine_refines
    {source : Core.Source.Program} {ir : Core.IR.Program} {ssa lowered : Program}
    {resultType : Core.Source.IntType} {sourceEntry : Core.Source.State}
    {irEntry : Core.IR.State} {ssaEntry loweredEntry : State}
    (front : Core.LoweringRefines source ir resultType sourceEntry irEntry)
    (selection : SelectionRefines ir ssa resultType irEntry ssaEntry)
    (transport : TransportRefines ssa lowered resultType ssaEntry loweredEntry) :
    SourceToMachineRefines source lowered resultType sourceEntry loweredEntry :=
  Core.Transition.Refines.trans front (selection_transport_refines selection transport)

end EvoIR.Machine
