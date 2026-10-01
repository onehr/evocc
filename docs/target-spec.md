# EvoCC internal target description: contract

Revision 1 — 2026-10-01. **Implementation is staged.** N03a now has canonical
`targets/aarch64/regfile` and `insts` components, their deterministic generator
and shared register/operand-fact consumers. Step-1 cross-revision objects are
byte-identical; generated operand facts, generic SSA destruction, allocation
and its independent verifier have focused tests, not final acceptance. The
remaining N03a ABI/frame extraction, complete-object comparisons and full
workspace gate remain open. N07 is still design work
under the [codegen quality plan](codegen-quality-plan.md).
[EvoMIR](evomir-spec.md)
stays the normative specification of the machine IR; until a fact has been
migrated under this contract, the code that holds it today (Appendix A) remains
its authority.

## 1. Purpose

A vendor architecture manual cannot be verified; it is a source. What EvoCC can
do is keep **one internal, machine-readable description per target** and make
it the only place a target fact is written down. Backend tables, Lean
definitions, tests and documentation are generated from it, and every backend
operation is shown correct **against it**. The vendor material is then checked
once, at the boundary, by differential testing, instead of being re-read and
re-typed in every pass.

This is the pattern the rule registry already follows: registry text in the
specification, a digest, `tools/lean/registry_to_lean.py` generating Lean data,
Rust recomputing the digest, an audit binding theorem names to rule text. The
target description applies it one level down.

## 2. Components

Each component has a different source of truth and therefore a different way
of being right. They are kept in separate files and never merged.

| Component | Contents | Source | How it is validated | Consumed by |
| --- | --- | --- | --- | --- |
| `regfile` | Physical registers, classes, sub-width views, reserved registers, flags register | Vendor manual plus ABI | Schema and consistency tests | Allocator pools, verifier, emitter names |
| `insts` | Per machine instruction: operand roles (use, def, tied, fixed, early-clobber), clobbers including flags, whether it is a move, call or terminator | EvoMIR instruction set | Allocation verifier and the MIR interpreter agree on every corpus program | Allocator, out-of-SSA, verifier (one copy, not three) |
| `encodings` | Immediate and addressing-mode legality, assembly syntax | Vendor encoding tables | Round trip through the platform assembler and disassembler on generated instances | Selection legality checks, emitter |
| `abi-<os>` | Argument and return classification, stack alignment, slot size, variadic rules, callee-saved set, frame requirements | AAPCS64, Darwin arm64 notes, psABI: prose, transcribed by hand | Differential against GCC-14 and Clang-19 on generated call shapes; existing layout and ABI probes | Frame and call driver |
| `profile-<name>` | Project choices: kernel stack guard, patchable entry, PIC, TLS model, symbol prefix | EvoCC decisions | Tests | Driver, emitter |
| `patterns` | Lowering rules: EvoIR form, machine sequence, side conditions, the claim | Engineers and, later, agents | Section 5 | Instruction selection |
| `costs` | Bytes per instruction, optional move and spill weights | Vendor guides, measurement | None: **policy, never correctness** | Size estimates, allocator heuristics |
| `isa-semantics` | State transformer per instruction form, sliced to the integer user-level subset in use | Vendor machine-readable model (Arm ASL via Sail; RISC-V Sail), pinned by upstream commit | The model's emulator against hardware and QEMU on the corpus | Proof and SMT obligations for `patterns`; emulator lane |

## 3. Layout and form

```text
targets/
  aarch64/
    regfile      insts      encodings     costs
    abi-darwin   abi-linux  profile-kernel
    patterns
    isa/         (pinned slice of the upstream model, with its provenance)
```

- **Form.** Line-oriented text with one fact per line and a canonical byte
  spelling, the way registry blocks are written, so a digest is meaningful and
  a diff is reviewable. The concrete grammar of each file is fixed by the RED
  that introduces it; this contract does not prescribe one beyond the examples
  in Section 5.
- **No runtime parser.** Generators write checked-in sources: Rust tables under
  the per-ISA module, Lean definitions, test vectors, the ISA appendix of the
  EvoMIR document. A gate step regenerates and fails on any difference, as
  `tools/gate-lean.sh` does for `Registry.lean`. Generated files say so in
  their first line.
- **Provenance.** Every file carries its digest; imported semantics also carry
  the upstream repository commit and the digest of the slicing script. A
  change to a target file is a reviewed change with a digest bump.

## 4. Rules

1. **One fact, one place.** A register set, a clobber list, an immediate range
   or an ABI rule is written in the target description and generated
   everywhere else. Hand-written copies are defects.
2. **The boundary is tested.** Target-independent modules contain no ISA
   names; a workspace test greps for them. Per-ISA hand-written code contains
   no literal register sets, immediate ranges or clobber lists; those come
   from generated tables.
3. **The verifier and the allocator read the same `insts`.** Two hand-written
   copies of instruction facts can agree on a wrong answer; one generated copy
   cannot disagree with itself, and its content is checked independently by
   the MIR interpreter and the native differential.
4. **Costs are policy.** Nothing in `costs` may be used to justify that a
   transformation is correct.
5. **Slices are pinned.** The imported ISA model is a named subset at a named
   upstream commit. Re-slicing is a reviewed change that reruns the emulator
   differential.
6. **Migration is behavior-preserving.** Moving a fact from code into the
   description must leave every emitted object byte-identical; the S1 two-build
   identity on the six S5 units and the native corpora is the test.

## 5. What "this backend operation is correct" means

Backend correctness is not one statement. Each kind of operation has its own
claim and its own way of being established.

| Row | Operation | Claim | Established by | Kind |
| --- | --- | --- | --- | --- |
| B1 | Lowering pattern | For every input in the pattern's domain, the EvoIR operation's value equals the effect of the machine sequence on the result register, and no state outside the declared set changes | Bounded SMT per width against `isa-semantics`; Lean proof where a fixed template closes it | Proved or bounded |
| B2 | Immediate and addressing legality | The chosen encoding exists for the value | Decision functions generated from `encodings`; assembler round trip on generated instances | Oracle |
| B3 | Register allocation | The allocated function implements the virtual one: no live value overwritten, every constraint met, spills and reloads paired | The allocation verifier, **per compiled function**, reading `insts` | Translation validation |
| B4 | Frame and calls | Argument placement, callee-saved preservation and alignment follow `abi-<os>` | Generated conformance cases against GCC and Clang; verifier check of callee-saved preservation | Oracle and per-instance |
| B5 | Emission | The printed text assembles to the intended instruction | Assembler round trip | Oracle |
| B6 | Machine-level peephole | As B1 | As B1 | Proved or bounded |
| B7 | The description itself | `isa-semantics` matches the processor; `abi-<os>` matches the platform | Emulator and native differentials; reference-compiler differentials | Tested, trusted |

B3 is the important design choice: the allocator is not proved. Each of its
outputs is checked, as regalloc2's checker does, and `allocverify.rs` already
plays that role.

A pattern entry, and the four things generated from it:

```text
pattern add_imm
  evoir   %r = add.bits<64> %x, const c      where 0 <= c < 4096
  minst   ADD Xd, Xn, #c                     flags untouched
  claim   exec(ADD Xd,Xn,#c)(s[Xn:=x]).X[d] = x + c mod 2^64,
          and the state is unchanged outside X[d]
```

```text
  gen_rust   -> selection table entry and its immediate predicate
  gen_tests  -> bounded SMT query per width; assembler round-trip instance
  gen_lean   -> the theorem statement (proof by fixed template, or UNKNOWN)
  gen_docs   -> a row in the EvoMIR lowering appendix
```

Patterns are data. That is what lets an agent propose one without being
trusted: the proposal is typed data, and the verdict comes from the generated
obligations, as with `g0.ub.v1`.

## 6. Migration order

Each step is one RED and one behavior-preserving change. Steps 1–3 are N03a;
steps 4–6 are N07.

| Step | Fact moved | Exit test |
| --- | --- | --- |
| 1 | `regfile`: pools, reserved and callee-saved sets | Byte-identical objects; boundary test |
| 2 | `insts`: operand roles and clobbers, read by allocator and verifier | Byte-identical objects; verifier unchanged in verdicts |
| 3 | `abi-darwin`, `abi-linux`, `profile-kernel` | Byte-identical objects; ABI corpora |
| 4 | `encodings` legality with the assembler round trip | Generated instances all assemble; no selection change |
| 5 | `isa-semantics` slice and its emulator as a differential lane | Corpus agreement between emulator, native run and the interpreters |
| 6 | `patterns` for the existing lowering rules, with bounded SMT | Every existing rule has a verdict; a failing row is a bug report |

Lean statements for patterns start on RISC-V, where the official Sail model
and its Lean output are smallest, and only after step 6.

## 7. Non-goals

- Writing an ISA specification by hand, or modeling instructions EvoCC does
  not emit (system registers, SIMD beyond what is lowered today, privileged
  state).
- Proving the allocator, the frame builder or the emitter as algorithms.
- A generic opcode space, a lowering DSL or a TableGen-like system.
- Deriving the ABI from the ISA model. They are different documents with
  different oracles.
- x86-64 files this quarter. The layout is per ISA so that R04 adds a
  directory, not a redesign.

## Appendix A. Where the facts live today

Read-only inventory of `crates/evocc-mir/src` at `ad7549b8` (EvoMIR 0.55).
Line numbers drift; the function and constant names are the stable anchors.
This is the extraction map for N03a and N07, not a list of defects.

| Component | Lives today in | Notes for extraction |
| --- | --- | --- |
| `regfile` | `linscan.rs`: `CALLER_SAVED` (x0–x15), `CALLEE_SAVED` (x19–x28), `SCRATCH` (x16), `SIMD_BASE` (64), `SIMD_SCRATCH` (v30), `SIMD_CALLER_SAVED`; `lib.rs`: `RegClass` (six classes), `VReg`; `emit.rs`: `SCRATCH_OFFSET`/`SCRATCH_UNIT` (x16/x17), register spelling in `physical()` and the operand printers | Physical registers are untyped `u8`. `sp`, the zero register, `x29`, `x30` and `x18` have no entry in any set; they are safe by omission or hard-coded in the emitter |
| `insts` (operand roles) | `lib.rs`: `inst_defs`, `inst_uses`, `term_uses`, `term_edges`, `writes_flags` (read-only facts); `linscan.rs`: `operand_slots`, `term_slots` (a second, mutable copy, 46 exhaustive arms); `regalloc.rs`: `discardable_computation` | The verifier reads the `lib.rs` copy, the allocator the `linscan.rs` copy. One generated table should replace both |
| `insts` (clobbers, pins) | `linscan.rs`: `pin_boundaries`, `pin_call`, `pin_assembly`, `asm_interferences`, `tied_pairs`, `early_clobbers`; `allocverify.rs`: `CLOBBERED` (x0–x17), `INDIRECT_RESULT`, `apply`, `transfer` | Call clobbers are a list only in the verifier; the allocator expresses them through `spans_call` and the pool choice |
| Flags | `lib.rs`: `RegClass::Flags`, `writes_flags`; `validate.rs`: adjacency, single consumer; `linscan.rs` and `regalloc.rs`: keep producer and consumer together | Flags are virtual, never a physical register |
| `encodings` | `immediate.rs`: `logical_mask`; `lower.rs`: imm12 selection in `simple` and `lower_compare`, offset limits in `memory_address`; `validate.rs`: compare-immediate limit; `emit.rs`: `IMM12`, `IMM12_SHIFTED`, `SCALED_LIMIT`, `emit_add_immediate`, the `movz`/`movk` sequence; `lib.rs`: `BinOp::has_immediate_form` | The same legality facts are decided in four files |
| `abi-<os>` (classification) | `lower.rs`: `record_plan`, `Passing`, `call_arguments`, `call_record_result`, `named_argument_extent`, `lower_va_start`, `MAX_REGISTER_ARGUMENTS`, `RECORD_REGISTER_LIMIT`, `refuses_stack_arguments`; `linscan.rs`: `ArgumentCursor`, `BOUNDARY`, `INDIRECT_RESULT` | Limits of 8 registers per bank appear in `lower.rs`, `linscan.rs`, `allocverify.rs` and `validate.rs` |
| `abi-<os>` (frame) | `emit.rs`: `frame_of`, `emit_prologue`, `emit_epilogue`, `REGISTER_SAVE_BYTES`, `VECTOR_SAVE_BYTES`, the 16-byte alignment and 8-byte slot constants | The frame pointer is always kept; no leaf special case, no red zone |
| `profile-<name>` | `emit.rs`: `CodegenOptions`, `StackGuard`, `RelocationModel`, patchable-entry emission; `native.rs`: refusals of Linux-only features on Darwin | |
| OS syntax | `emit.rs`: `impl Target` (symbol and label prefixes, sections, page and page-offset operands) and the remaining `Target` matches for visibility, weak symbols, initializer sections, TLS, GOT | |
| `patterns` | `lower.rs`: `lower_inst` (about 790 lines, 50 arms), `lower_block` fusions, `lower_compared_selection`, `lower_checked_result`, `take_checked_flags`, `memory_address`, `Lowerer::shift`, `sign_extend`, `zero_extend`; EvoMIR Section 5 table (44 rows) and the Section 3 immediate rules | No test ties the specification table to the code beyond substring presence |

Inventory observations to settle while migrating. Each is a pointer for the
engineer doing the step, to be confirmed against the code, not a verdict:

- Instruction facts exist in two hand-written copies with one recorded
  difference: an assembly read-write output is a definition only in
  `operand_slots`, while `AsmBody::read()` also counts it as a use.
- The call-clobber set is written twice with different contents by design
  (pool membership in `linscan.rs`, x0–x17 in `allocverify.rs`); a single
  `regfile` entry should state both the allocatable pool and the clobbered set.
- Emitter expansions use x16, x17, v30 and v31 as scratch without a
  per-instruction clobber; correctness rests on those registers never entering
  a pool. `insts` should record the scratch each expansion needs.
- `x18` is never named. On platforms that reserve it, that is correct only
  because no set contains it; `regfile` should say so explicitly.
- The EvoMIR text says the two targets "differ in four places and nowhere
  else"; the inventory counted more `Target` decision sites in `emit.rs` and
  six in `lower.rs`. `abi-<os>` and the OS syntax table are where that list
  becomes checkable.
- Some platform rules are applied without a target check (for example the
  AAPCS64 variadic save areas), while the specification's Darwin variadic note
  says the Darwin rule is still unmeasured. Splitting `abi-darwin` from
  `abi-linux` makes each such rule an explicit entry.
- Immediate legality for `BinaryImm` add and subtract is enforced at selection
  and emission but not in the validator; a generated predicate from
  `encodings` gives all three the same answer.
