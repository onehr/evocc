# EvoCC Target Description

**Revision 1.1.0** | **Pinned ground truth** | Stabilized from revision 2 on 2026-10-06

## Status and scope

This document is the contract of the internal target description: the table
files in `targets/<isa>/` (today `targets/aarch64/`), their form, the rules
that bind the backend to them, and what "this backend operation is correct"
means (obligations B1 to B7).

**Normative and informative text.** Sections 1 to 6 are normative; "must"
states a requirement. Appendix A is informative: it maps where each fact lives
in the code, and it changes when the code changes.

**Authorities.** The tables are data. Their values are the ground truth for
register sets, operand roles, encodings, ABI facts, profile choices, patterns
and costs; this document governs their form and their use. `spec/evomir.md`
is the normative specification of the machine IR and states what the facts
mean for EvoMIR. A fact that is not yet in a table has its authority in the
code that holds it (Appendix A) until it moves.

**Machine-read parts.** No tool reads this document.
`tools/generate-target.py` reads the tables.

## 1. Purpose

A vendor architecture manual cannot be verified; it is a source. EvoCC keeps
**one internal, machine-readable description per target** and makes it the
only place a target fact is written down. Backend tables, documentation and
tests are generated from it, and every backend operation is shown correct
**against it**. The vendor material is checked once, at the boundary, by
differential testing, instead of being re-read and re-typed in every pass.

This is the pattern that the EvoIR rule registries follow: registry text in
the specification, a digest, `tools/lean/registry_to_lean.py` generating Lean
data, Rust recomputing the digest, and an audit binding theorem names to rule
text. The target description applies it one level down.

## 2. Components

Each component has a different source of truth and therefore a different way
of being right. The components are separate files and are never merged.

| Component | Contents | Source | How it is validated | Consumed by |
| --- | --- | --- | --- | --- |
| `regfile` | Register banks, views and aliases, the virtual flags register, register sets (allocatable, callee-saved, call clobbers, partially preserved, reserved, assembly-bindable), roles (scratch, frame pointer, link register, platform register), special spellings | Vendor manual plus ABI | Generator schema and consistency checks; `target_regfile.rs` | Allocator candidate sets, verifier, emitter spellings |
| `insts` | Per machine instruction and terminator: operand roles (use, def, rewrite, tied, fixed, early clobber), flags, discardability, call, copy, spill, stack-argument and indirect-result facts, edges and exits | EvoMIR instruction set | `target_insts.rs`; the allocation verifier and the MIR interpreter agree on every corpus program | SSA destruction, allocator, verifier (one generated copy) |
| `encodings` | Immediate and offset fields, shifts, scales, the instruction size, access and pair sizes, logical element sizes, and the instruction families with an immediate form | Vendor encoding tables | `target_encodings.rs`: independent boundary cases and an assembler round trip | Selection legality, validator, emitter |
| `abi-<os>` | Argument and result registers per bank, indirect-result and overflow locations, stack slot and alignment, stack-argument admission, callee-saved, clobbered and partially preserved sets, frame record and save slots, variadic save areas and `va_list` offsets, record register limit and pair rounding | AAPCS64, Apple's arm64 ABI notes, the psABI, transcribed by hand | `target_abi.rs`; differential runs against GCC-14 and Clang-19 on call shapes; ABI probes | Argument placement (`generic/abi.rs`), frame layout (`generic/frame.rs`), lowering classifier, allocator boundary, verifier |
| `profile-<name>` | Project choices: the host ABI and its digest, features admitted only on the host ABI, the stack-protector slot size | EvoCC decisions | Tests | Driver and emitter admission |
| `patterns` | Selection rules: name, root form and widths, machine sequence, guard, written state, flags and memory effect, claim | Engineers | Obligations B1 and B6 (Section 5) | Instruction selection |
| `costs` | Bytes per instruction kind | Vendor guides, measurement | None: **policy, never correctness** | Size comparison of selection candidates |
| `isa-semantics` | The state transformer of each instruction form, sliced to the integer user-level subset in use | Vendor machine-readable model (Arm ASL via Sail), pinned by upstream commit | The model's emulator against the native run, QEMU and the interpreters | Obligations B1 and B6; emulator lane |

## 3. Layout and form

```text
targets/
  aarch64/
    regfile      insts      encodings     costs
    abi-darwin   abi-linux  profile-kernel
    patterns
    isa/         (isa-semantics: the pinned model slice, its license and provenance.json)
```

- **Form.** A table is canonical ASCII with LF line ends, no tab and no
  carriage return, one fact per line and single spaces between fields, so
  that a digest is meaningful and a diff is reviewable. Line 1 is
  `version <kind>.v1`, where `<kind>` is the component name without its
  `-<os>` or `-<name>` suffix: `regfile.v1`, `insts.v1`, `encodings.v1`,
  `patterns.v1` and `costs.v1`, `abi.v1` for each `abi-<os>`, and
  `profile.v1` for each `profile-<name>`. Line 2 is `target aarch64`, and
  line 3 is `sha256 <hex>`: the SHA-256 of the file without that line. A
  table that depends on another names that table's digest: `abi-<os>` names
  the `regfile` digest, and `profile-<name>` names its host ABI and that
  ABI's digest.
- **Grammar.** The grammar of each table is the one that
  `tools/generate-target.py` accepts. The generator refuses noncanonical
  bytes, unknown facts, duplicate names, unresolved references and digest
  mismatches. It never infers a repair.
- **No runtime parser.** The generator writes checked-in sources: the Rust
  tables under `crates/evocc-mir/src/isa/aarch64/` (`regfile.rs`, `insts.rs`,
  `abi_tables.rs`, `encodings.rs`, `patterns.rs`, `costs.rs`), the register
  primitives `crates/evocc-evoir/src/assembly_registers.rs`, and the pattern
  and encoding tables of `spec/evomir.md`. Each generated file says so in its
  first line. `python3 tools/generate-target.py --check` regenerates every
  output and fails on any difference; `tools/gate-changed.sh` runs it.
- **Provenance.** Every table carries its digest. The `isa-semantics` slice
  carries the upstream repository, commit and path, the digest of each input
  and of the selector, and the byte span and digest of each selected span
  (`isa/provenance.json`); `tools/isa_model_slice.py` produces it and checks
  it with `--check`. A change to a table is a reviewed change with a new
  digest.

The field inventory of `encodings` was checked against the automatically
translated Arm 2023-03 ASL in
[`sail-arm` at `1bf2e5574ba9d704639a28401b6a387dcb113cae`](https://github.com/rems-project/sail-arm/blob/1bf2e5574ba9d704639a28401b6a387dcb113cae/arm-v9.4-a/src/instrs64.sail).
That check is a review, not an execution of the model. Generator and schema
tests are not an ISA oracle: the independent cyclic-run enumeration, the
immediate and offset boundary cases and the Clang assembly, disassembly and
reassembly round trip of `target_encodings.rs` check that boundary. The
conservative frame limit and branch-relaxation margins of emission are policy
derived from encoding facts and are not widened to the architectural maximum.

## 4. Rules

1. **One fact, one place.** A register set, a clobber list, an immediate range
   or an ABI rule is written in the target description and generated
   everywhere else. A hand-written copy is a defect.
2. **The boundary is tested.** Target-independent modules
   (`crates/evocc-mir/src/generic/`) contain no ISA names, register names or
   target types. Per-ISA hand-written code contains no literal register sets,
   immediate ranges or clobber lists; those come from generated tables.
   `crates/evocc-mir/tests/target_boundary.rs` checks the first sentence by
   a scan of `generic/` for instruction-set names, register names,
   mnemonics, target types and imports from outside `generic/`. It checks
   the second sentence for a fixed list of migrated declarations
   (register-set arrays and ABI constants and functions), and for the named
   constants in `costs.rs`, `encodings.rs` and `patterns.rs`. Each must have
   one owner; the scan does not find unnamed literal ranges or clobber lists. Appendix A
   lists the known gaps.
3. **The verifier and the allocator read the same `insts`.** Two hand-written
   copies of instruction facts can agree on a wrong answer; one generated copy
   cannot disagree with itself, and the MIR interpreter and the native
   differential check its content independently.
4. **Costs are policy.** Nothing in `costs` can justify that a transformation
   is correct.
5. **Slices are pinned.** The imported ISA model is a named subset at a named
   upstream commit. A new slice is a reviewed change that reruns the emulator
   differential.
6. **Migration is behavior-preserving.** Moving a fact from code into a table
   must leave every emitted object byte-identical: the two-build identity on
   the six translation units of the codegen size comparison
   (`docs/archive/codegen-quality-plan.md`) and on both native corpora is the test.
   Moving `encodings` also requires that the generated instances assemble;
   moving `isa-semantics`, that the emulator agrees with the native run and
   the interpreters on the corpus; moving `patterns`, that every existing rule
   gets a verdict, where a failing row is a bug report.

## 5. What "this backend operation is correct" means

Backend correctness is not one statement. Each kind of operation has its own
claim and its own way of being established.

| Row | Operation | Claim | Established by | Kind |
| --- | --- | --- | --- | --- |
| B1 | Lowering pattern | For every input in the pattern's domain, the EvoIR operation's value equals the effect of the machine sequence on the result register, and no state outside the declared set changes | Bounded SMT per width against `isa-semantics`; Lean proof where a fixed template closes it. No pattern row has either yet (the note below) | Proved or bounded; not yet established |
| B2 | Immediate and addressing legality | The chosen encoding exists for the value | Decision functions generated from `encodings`; assembler round trip on generated instances | Oracle |
| B3 | Register allocation | The allocated function implements the virtual one: no live value overwritten, every constraint met, spills and reloads paired | The allocation verifier, **per compiled function**, reading `insts` | Translation validation |
| B4 | Frame and calls | Argument placement, callee-saved preservation and alignment follow `abi-<os>` | Generated conformance cases against GCC and Clang; verifier check of callee-saved preservation | Oracle and per-instance |
| B5 | Emission | The printed text assembles to the intended instruction | Assembler round trip | Oracle |
| B6 | Machine-level peephole | As B1 | As B1; no pattern row has a proof yet | Proved or bounded; not yet established |
| B7 | The description itself | `isa-semantics` matches the processor; `abi-<os>` matches the platform | Emulator and native differentials; reference-compiler differentials | Tested, trusted |

B3 is the important design choice: the allocator is not proved. Each of its
outputs is checked, as regalloc2's checker does;
`crates/evocc-mir/src/generic/allocverify.rs` plays that role (EvoMIR
Section 9.2). For the checked subset, the Lean checker `checkAllocation`
also validates the allocation (`spec/verification.md`).

**Pattern entries.** A pattern is a typed record. Its row in `patterns` is

```text
pattern NAME root=ROOT widths=W[,W] sequence=SEQ guard=GUARD writes=WRITES flags=FLAGS memory=MEMORY claim=CLAIM [ops=OP,...] [shifts=SHIFT,...]
```

and it stands for an entry with four parts, for example:

```text
pattern add_imm
  evoir   %r = add.bits<64> %x, const c      where 0 <= c < 4096
  minst   ADD Xd, Xn, #c                     flags untouched
  claim   exec(ADD Xd,Xn,#c)(s[Xn:=x]).X[d] = x + c mod 2^64,
          and the state is unchanged outside X[d]
```

Four artifacts are generated from an entry:

```text
  gen_rust   -> selection table entry and its guard (isa/aarch64/patterns.rs)
  gen_docs   -> a row in the pattern table of EvoMIR Section 5.6
  gen_tests  -> bounded SMT query per width; assembler round-trip instance
  gen_lean   -> the theorem statement (proof by fixed template, or UNKNOWN)
```

Patterns are data. An agent can therefore propose one without being trusted:
the proposal is typed data, and the verdict comes from the generated
obligations, as with the EvoIR registry `g0.ub.v1`.

*Informative.* `tools/generate-target.py` writes `gen_rust` and `gen_docs`.
`tools/isa_model_smt.py` qualifies the SMT translation of the pinned model;
the pinned decoder does not yet translate, so no pattern row has a B1 or B6
proof, and the selection variants that use the rows stay optional (EvoMIR
Section 5.5).

## 6. Non-goals

- Writing an ISA specification by hand, or modeling instructions that EvoCC
  does not emit (system registers, SIMD beyond what is lowered, privileged
  state).
- Proving the allocator, the frame builder or the emitter as algorithms.
- A generic opcode space, a lowering DSL or a TableGen-like system.
- Deriving the ABI from the ISA model. They are different documents with
  different oracles.
- Tables for another ISA. The layout is per ISA, so that a new ISA adds a
  directory, not a redesign.

## Appendix A. Where the facts live

*Informative.* Paths are relative to `crates/evocc-mir/src/`. Function and
constant names are the stable anchors; line numbers drift.

**Facts generated from the tables.**

| Table | Generated file | Main consumers |
| --- | --- | --- |
| `regfile` | `isa/aarch64/regfile.rs` (`CALLER_SAVED`, `CALLEE_SAVED`, `GPR_ALLOCATABLE`, `GPR_CALL_CLOBBERS`, `SIMD_CALLER_SAVED`, `SIMD_ACROSS_CALL`, `SIMD_LOW64_PRESERVED`, `RESERVED`, `FIXED_ASM`, `SCRATCH`, `SCRATCH_UNIT`, `SIMD_SCRATCH`, `FRAME_POINTER`, `LINK_REGISTER`, `PLATFORM_REGISTER`, the views); `crates/evocc-evoir/src/assembly_registers.rs` | `isa/aarch64/allocation.rs` (`candidates`), `linscan.rs` (`physical`, re-exports), `emit.rs` |
| `insts` | `isa/aarch64/insts.rs` (the `MachInst` and `MachTerm` implementations: `uses`, `defs`, `operand_slots`, `writes_flags`, `discardable`, `edges`; `assembly_uses`) | `generic/ssa.rs`, `generic/allocation.rs`, `generic/allocverify.rs`; `lib.rs` (`inst_defs`, `inst_uses`, `term_uses`, `term_edges`, `writes_flags`, `AsmBody::read` are wrappers) |
| `encodings` | `isa/aarch64/encodings.rs` (`ADD_SUB_MAX`, `ADD_SUB_DOUBLE_MAX`, `MEMORY_SCALED_MAX`, `PAIR_MAX`, `memory_offset`, `logical_mask`, `has_immediate_form`); the encoding table of EvoMIR Appendix A.1 | `lower.rs` (`memory_address`, `lower_compare`), `validate.rs`, `emit.rs` (`IMM12`, `IMM12_SHIFTED`, `SCALED_LIMIT`, `emit_add_immediate`), `immediate.rs` (re-export), `asm_ast.rs` |
| `abi-darwin`, `abi-linux`, `profile-kernel` | `isa/aarch64/abi_tables.rs` (`DARWIN`, `LINUX`, `KERNEL` and their digests) | `isa/aarch64/abi.rs` (`Aarch64Abi`, `Target::abi`, `Profile::admits`), `generic/abi.rs` (`AbiFacts`, `ArgumentCursor`), `generic/frame.rs` (`layout`), `generic/allocation.rs` (`pin_boundaries`, `pin_call`), `generic/allocverify.rs` |
| `patterns` | `isa/aarch64/patterns.rs` (`CONSTANT_`, `LOCAL_`, `EXTENSION_`, `CONTROL_PATTERNS`); the pattern table of EvoMIR Section 5.6 | `isa/aarch64/constants.rs`, `local.rs`, `extensions.rs`, `controls.rs` |
| `costs` | `isa/aarch64/costs.rs` | the same selection modules |

**Facts still written in code.**

| Fact | Lives in | Notes for extraction |
| --- | --- | --- |
| Pins and constraints | `generic/allocation.rs`: `pin_boundaries`, `pin_call`, `pin_assembly`, `asm_interferences`, `tied_pairs`, `early_clobbers`; `isa/aarch64/allocation.rs`: constraint views and banks | Generic algorithms over `insts` and `regfile` facts |
| Flags adjacency | `validate.rs` (single adjacent consumer) | `writes_flags` and the flags tag are generated |
| ABI classification | `lower.rs`: `record_plan`, `Passing`, `call_arguments`, `call_record_result`, `named_argument_extent`, `lower_va_start` | The one AArch64 aggregate, HFA, HVA and variadic classifier; it reads `Aarch64Abi` facts |
| Prologue and epilogue | `emit.rs`: `frame_of_for`, `emit_prologue`, `emit_epilogue`, `emit_frame_access` | Layout comes from `generic/frame.rs`; instruction sequences are hand-written |
| Codegen options | `emit.rs`: `CodegenOptions`, `StackGuard`, `RelocationModel`, `FrameVariants`; `native.rs` | |
| OS syntax | `emit.rs`: `impl Target` (prefixes, sections, page and GOT operands) and the remaining `Target` matches (visibility, weak symbols, initializer sections, TLS) | No table yet; EvoMIR Section 8.1 lists the differences |
| Baseline selection | `lower.rs`: `lower_inst`, `lower_block`, `lower_compared_selection`, `lower_checked_result`, `take_checked_flags`, `memory_address`, `Lowerer::shift`, `sign_extend`, `zero_extend` | Rules enter `patterns` only when they are touched |

**Known gaps against the rules.**

- Emitter expansions use `x16`, `x17`, `v30` and `v31` as scratch without a
  per-instruction clobber in `insts`. Correctness rests on those registers
  being in `RESERVED`, outside every candidate set.
- `asm_ast.rs` derives `SCALED_LIMIT` with a literal access size, and its
  `IMM12` duplicates the one in `emit.rs` (Rule 1). The boundary test does
  not check immediate literals in hand-written ISA code. It does check copies
  of the named constants from `costs.rs`, `encodings.rs` and `patterns.rs`;
  generated owners are identified by their path, not exempted from that check
  by a first-line comment.
- The AAPCS64 variadic save areas and the even-register rounding of a
  `bits<128>` argument apply under both ABI profiles without a table fact;
  EvoMIR Sections 6.1 and 6.5 state the resulting Darwin limits.

## Revision

**Revision 1.1.0, 2026-10-16.** A terminator's `forwarding` fact can name
`body.fixed`: it prevents forwarding only when its assembly body has a fixed
register binding. The body must exist. Generated selectors retain indirect
dispatch's unconditional exclusion. EvoMIR Section 9.2.1 checks the actual
SSA transport before allocation.

**Revision 1.0.2, 2026-10-16.** The boundary tests cover named constants from
the cost, encoding and pattern tables. The older migrated-declaration scan
recognizes both generated-header forms. The remaining literal-range limit is
unchanged; no target fact or code-generation rule changes.

**Revision 1.0.1, 2026-10-14.** Corrections that make the text state what
the code and the tables do; no component, rule or obligation changes. Section
3 states the actual first line of each table (`abi.v1`, `profile.v1`), Rule 2
of Section 4 states what `target_boundary.rs` checks, and the rows B1 and B6
of Section 5 state that no pattern row has a proof yet. The errata note is
`docs/archive/notebook/spec-errata-20261014.md`. Revision 1.0.0 of 2026-10-06
was the pinned ground truth of the target description contract: it kept the
components, form, rules and obligations of revision 2 of 2026-10-03, removed
the status preamble and the migration schedule, stated the table header and
digest form, and updated the extraction map to the generated tables and the
`generic/` and `isa/aarch64/` modules. The complete history is in git:
`git log -- docs/target-spec.md spec/target.md`.

**Change procedure.** A change to this document must follow these steps:

1. Edit the normative text. A change of a table format also changes
   `tools/generate-target.py` and its tests.
2. Bump the revision in the header: the patch number for a correction that
   changes no requirement, the minor number for a compatible addition, the
   major number for an incompatible change. Replace the paragraph above.
3. Update the digest of this file in `spec/pins.json` with
   `python3 tools/check-spec-pins.py --update`, and commit both files together.
4. Run the gate. Every command must pass: `python3 tools/check-spec-pins.py`;
   `python3 tools/generate-target.py --check`;
   `python3 -m unittest tools.tests.test_generate_target`.

**What revisions 1 and 2 added** (informative):

| Revision | Content |
| --- | --- |
| 1 | The contract: components, layout and form, rules, obligations B1 to B7, pattern entries, migration order, non-goals and the extraction map of the hand-written backend. |
| 2 | Generated `regfile`, `insts`, `abi-<os>` and `profile-kernel` with shared consumers; the first `patterns` and `costs`; `encodings` with its provenance. |
