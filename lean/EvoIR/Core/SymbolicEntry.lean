import EvoIR.Core.SourceExpression
import EvoIR.Core.SymbolicEdge

/-! Checked function-entry bindings. Source parameters and target block
parameters consume the same ordered symbolic arguments. The source side keeps
C rank, signedness and qualifiers; the target side checks representation widths.
Neither a producer-supplied local relation nor a sampled execution is trusted. -/
namespace EvoIR.Core.Symbolic

def parameterEnvironment {Γ : Var → Nat} :
    List Source.Parameter → List (CWord Γ) → Option (CBindings Γ)
  | [], [] => some []
  | p :: ps, v :: vs => do
    if p.type.type != v.type then none else
    let rest ← parameterEnvironment ps vs
    if rest.any (fun entry => entry.1 == p.symbol) then none
    else some ((p.symbol, v) :: rest)
  | _, _ => none

theorem parameter_environment_sound {Γ : Var → Nat} (σ : Subst Γ)
    (ps : List Source.Parameter) (args : List (CWord Γ)) (env : CBindings Γ)
    (accepted : parameterEnvironment ps args = some env) :
    ∃ locals, Source.parameterLocals ps (args.map (CWord.denote σ)) = .ok locals ∧
      env.Related σ locals ∧ locals.map Source.Slot.symbol = env.map Prod.fst := by
  induction ps generalizing args env with
  | nil =>
    cases args with
    | nil =>
      cases Option.some.inj accepted
      exact ⟨[], rfl, by intro id v h; simp [List.lookup] at h, rfl⟩
    | cons _ _ => simp [parameterEnvironment] at accepted
  | cons p ps ih =>
    cases args with
    | nil => simp [parameterEnvironment] at accepted
    | cons v vs =>
      simp only [parameterEnvironment, bind, Option.bind] at accepted
      split at accepted
      · contradiction
      · rename_i same
        have typed : p.type.type = v.type := by simpa using same
        split at accepted
        · contradiction
        · rename_i rest tail
          dsimp only at accepted
          split at accepted
          · contradiction
          · rename_i fresh
            cases Option.some.inj accepted
            obtain ⟨locals, bound, related, symbols⟩ := ih vs rest tail
            have noDuplicate : locals.any (fun slot => slot.symbol == p.symbol) = false := by
              have sameAny := congrArg (fun names : List Nat => names.any (· == p.symbol)) symbols
              simp only [List.any_map] at sameAny
              exact sameAny.trans (Bool.eq_false_iff.mpr fresh)
            refine ⟨⟨p.symbol, p.type, some (v.denote σ)⟩ :: locals, ?_, ?_, ?_⟩
            · simp only [List.map_cons, Source.parameterLocals, CWord.denote,
                typed, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte,
                bound, bind, Except.bind, noDuplicate, pure, Except.pure]
            · intro id value found
              simp only [List.lookup] at found
              split at found
              · rename_i sameId
                have idEq : p.symbol = id := (beq_iff_eq.mp sameId).symm
                cases Option.some.inj found
                simp [Source.readSlot, List.find?, idEq, typed, CWord.denote, pure, Except.pure]
              · rename_i different
                have notId : (p.symbol == id) = false := by
                  apply beq_eq_false_iff_ne.mpr
                  exact Ne.symm (beq_eq_false_iff_ne.mp different)
                simpa [Source.readSlot, List.find?, notId] using related id value found
            · simpa using symbols

/-- The target has no additional hidden integer arguments. Opaque memory/world
arguments, when present, must be checked separately by the enclosing signature
contract, not fabricated by this scalar entry component. -/
def scalarEntry {Γ : Var → Nat} (source : Source.Function) (target : IR.Function)
    (args : List (CWord Γ)) : Option (CBindings Γ × Scope Γ) := do
  let env ← parameterEnvironment source.parameters args
  let block ← target.blocks.head?
  if target.parameters != args.map (fun v => IR.Ty.bits v.type.width) ||
      block.parameters.map IR.Binding.type != target.parameters then none
  else
    let scope ← bindSlots (fun _ => IR.Ty.world) block.parameters
      (args.map (fun v => Slot.word v.word))
    return (env, scope)

theorem scalar_entry_sound {Γ : Var → Nat} (σ : Subst Γ)
    (source : Source.Function) (target : IR.Function) (args : List (CWord Γ))
    (env : CBindings Γ) (scope : Scope Γ) (owner : Nat)
    (accepted : scalarEntry source target args = some (env, scope)) :
    ∃ locals block,
      Source.parameterLocals source.parameters (args.map (CWord.denote σ)) = .ok locals ∧
      env.Related σ locals ∧ target.blocks.head? = some block ∧
      IR.enter target (args.map (fun v => sourceValueIR (v.denote σ))) owner =
        .ok ⟨target.name, block.name, 0, scope.denote σ (fun _ => .world), owner⟩ := by
  simp only [scalarEntry, bind, Option.bind, pure] at accepted
  split at accepted
  · contradiction
  · rename_i sourceEnv hsource
    dsimp only at accepted
    split at accepted
    · contradiction
    · rename_i block hblock
      dsimp only at accepted
      split at accepted
      · contradiction
      · rename_i signatures
        split at accepted
        · contradiction
        · rename_i targetScope htarget
          dsimp only at accepted
          simp only [Option.some.injEq, Prod.mk.injEq] at accepted
          rcases accepted with ⟨rfl, rfl⟩
          obtain ⟨locals, bound, related, _⟩ := parameter_environment_sound σ
            source.parameters args sourceEnv hsource
          have boundIR := bind_slots_sound σ (fun _ => IR.Value.world) (fun _ => IR.Ty.world)
            (by intro id; rfl) block.parameters (args.map (fun v => Slot.word v.word))
            targetScope htarget
          have argsSame : (args.map (fun v => Slot.word v.word)).map
              (Slot.denote σ (fun _ => IR.Value.world)) =
              args.map (fun v => sourceValueIR (v.denote σ)) := by
            simp [List.map_map, Slot.denote, CWord.word, Word.denote, sourceValueIR, CWord.denote]
          rw [argsSame] at boundIR
          have sig : target.parameters = args.map (fun v => IR.Ty.bits v.type.width) ∧
              block.parameters.map IR.Binding.type = target.parameters := by simpa using signatures
          refine ⟨locals, block, bound, related, hblock, ?_⟩
          have argtypes : (args.map (fun v => sourceValueIR (v.denote σ))).map IR.Value.type =
              args.map (fun v => IR.Ty.bits v.type.width) := by
            simp [List.map_map, sourceValueIR, CWord.denote, IR.Value.type]
          simp [IR.enter, argtypes, ← sig.1, sig.2, hblock, boundIR,
            bind, Except.bind, pure, Except.pure]

/-- Internal variable identities are injective by string length. This avoids
trusting a hash or a producer-chosen name to distinguish independent inputs. -/
def inputName (index : Nat) : Var := String.ofList (List.replicate index 'i')
def inputWidths (_ : Var) : Nat := 64

def inputWord (type : Source.IntType) (index : Nat) : CWord inputWidths :=
  (⟨.ulongLong, .var (inputName index) 64 rfl⟩ : CWord inputWidths).cast type

def parameterWords (ps : List Source.Parameter) : List (CWord inputWidths) :=
  ps.mapIdx fun index p => inputWord p.type.type index

def inputSubst (values : List Source.Value) : Subst inputWidths := fun name =>
  ((values[name.length]?).map (fun v => v.bits.setWidth 64)).getD 0

theorem input_word_covers (values : List Source.Value) (index : Nat) (value : Source.Value)
    (found : values[index]? = some value) :
    (inputWord value.type index).denote (inputSubst values) = value := by
  rw [inputWord, cast_sound]
  have selected : inputSubst values (inputName index) = value.bits.setWidth 64 := by
    simp [inputSubst, inputName, found]
  have width : value.type.width ≤ 64 := by cases value.type <;> decide
  have extended := BitVec.toNat_setWidth_of_le (b := value.bits) width
  simp only [CWord.denote, denoteTerm, selected, Source.Value.cast, Source.Value.number,
    Source.IntType.signed, Bool.false_eq_true, ↓reduceIte]
  change Source.Value.ofInt value.type ((value.bits.setWidth 64).toNat : Int) = value
  rw [extended]
  apply congrArg (Source.Value.mk value.type)
  simpa only [BitVec.ofInt_natCast, BitVec.setWidth_eq] using
    BitVec.ofNat_toNat value.type.width value.bits

/-- Universal input coverage. Narrow arguments are zero-extended only to
construct a valuation, then recovered at their original source type. No input
bits, signed values or C integer ranks are excluded. -/
theorem parameter_words_cover (ps : List Source.Parameter) (values : List Source.Value)
    (typed : values.map Source.Value.type = ps.map (fun p => p.type.type)) :
    (parameterWords ps).map (CWord.denote (inputSubst values)) = values := by
  have length : values.length = ps.length := by
    simpa using congrArg List.length typed
  apply List.ext_getElem
  · simp [parameterWords, length]
  · intro i leftBound rightBound
    have psBound : i < ps.length := by simpa [parameterWords] using leftBound
    have type : values[i].type = ps[i].type.type := by
      have eq := congrArg (fun types => types[i]?) typed
      simpa [List.getElem?_eq_getElem, rightBound, psBound] using eq
    simp only [List.getElem_map, parameterWords, List.getElem_mapIdx]
    rw [← type]
    exact input_word_covers values i values[i] (List.getElem?_eq_getElem rightBound)

end EvoIR.Core.Symbolic
