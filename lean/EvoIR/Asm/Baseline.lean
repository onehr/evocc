import EvoIR.Alloc.Soundness
import EvoIR.Asm.Soundness

/-!
The baseline compilation theorem of the checked core-v1 path: typed Core
source, EvoIR, SSA EvoMIR, post-SSA EvoMIR, allocated EvoMIR and the final
AArch64 assembly AST.

The premises `front`, `selection` and `transport` are the conclusions of the
source-to-EvoIR and machine lowering checkers. `allocChecked` is the
allocation checker and `asmChecked` is the frame and emission checker. The
safety premise of `checkAsm_sound` is the progress field of the allocation
refinement, so the composition adds no safety premise. The remaining
premises are the ABI entry state with sufficient stack memory (`AbiEntry`)
and the result view of the entry function (`ResultView`).

The assembly AST is connected to the emitted text by the print check of
`EvoIR.Asm.Print` (a trusted printer, compared byte for byte). The
assembler, the linker and the hardware are outside this theorem.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open Core.Transition (Refines)

def SourceToAsmRefines (source : Core.Source.Program) (asm : File) (type : Core.Source.IntType)
    (sourceEntry : Core.Source.State) (asmEntry : Asm.State) : Prop :=
  Refines (Core.Source.step source) (Asm.step asm) Core.observeSource (Asm.observe type)
    sourceEntry asmEntry

/-- The baseline theorem: an accepted pipeline from typed Core source to the
assembly AST refines the source program. -/
theorem source_asm_refines {source : Core.Source.Program} {ir : Core.IR.Program}
    {ssa lowered : Machine.Program} {allocated : Alloc.Program} {allocHints : Alloc.Hints}
    {asm : File} {frameHints : List FrameHint}
    {type : Core.Source.IntType} {sourceEntry : Core.Source.State} {irEntry : Core.IR.State}
    {ssaEntry loweredEntry : Machine.State} {name : String} {args : List Machine.Value}
    (front : Core.LoweringRefines source ir type sourceEntry irEntry)
    (selection : Machine.SelectionRefines ir ssa type irEntry ssaEntry)
    (transport : Machine.TransportRefines ssa lowered type ssaEntry loweredEntry)
    (started : Machine.initial lowered name args = .ok loweredEntry)
    (allocChecked : Alloc.checkAllocation lowered allocated allocHints = true)
    (asmChecked : Check.checkAsm allocated asm frameHints = true)
    (asmEntry : Asm.State) (E : Nat) (abi : AbiEntry ⟨allocated, asm, frameHints⟩ name args asmEntry E)
    (resultView : ResultView allocated name type) :
    SourceToAsmRefines source asm type sourceEntry asmEntry := by
  obtain ⟨allocatedEntry, init, toAllocated⟩ :=
    Alloc.source_allocated_refines front selection transport started allocChecked
  have safe : Safe allocated type allocatedEntry := toAllocated.progress
  have toAsm := checkAsm_sound allocated asm frameHints asmChecked type name args allocatedEntry init
    asmEntry E abi resultView safe
  exact Refines.trans toAllocated toAsm

end EvoIR.Asm.Proof
