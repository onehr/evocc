# Specification errata T03w, T03x and T03y, 2026-10-14

This note records the text errata of TODO T03w, T03x and T03y. They come
from the audit `docs/archive/notebook/spec-audit-20261013.md`. For each
finding it gives what the text said, what the code does, what changed and
what is left for the Rust or the Lean lane.

## Base and method

- Base: commit `9e9fccaf`. The crates that the cited code lives in had no
  working-tree change, except `crates/evocc/src/main.rs`; its lines are those
  of `9e9fccaf`.
- The principle: the code is the reference for what EvoCC does today. A
  passage that describes intent with no check is marked "Informative". A
  place where the code looks wrong is listed below as a defect, and the text
  does not bless it.
- No Cargo, no Lake and no built binary ran. The Python checks below ran.
- The audit's spec line numbers refer to the texts before batch 2
  (`c9752fcf`). Every passage was located again.

| Document | Before | After |
| --- | --- | --- |
| `spec/evoir.md` | 2.4.0 | 2.5.0 |
| `spec/evomir.md` | 1.2.0 | 1.2.1 |
| `spec/target.md` | 1.0.0 | 1.0.1 |
| `spec/verification.md` | 1.1.0 | 1.1.1 |
| `spec/README.md` | not pinned | five documents |

EvoIR takes a minor revision, because Section 11.7 adds trusted-base rows and
Section 4.4 states that no transformation reorders. The other documents take
a patch revision.

## T03w: EvoIR

**A26: a bound rolls the transaction back.**
Text: the Section 8 preamble said that reaching a bound "rolls the whole
transaction back"; Section 8.9 listed budget exhaustion among the rollbacks.
Sections 8.11 and 11.3 said the opposite.
Code: the region builder keeps the admitted regions and stops at the bound
(`crates/evocc-eqopt/src/evp1.rs:268-286`); the sweep does the same
(`crates/evocc-eqopt/src/scalar_eq.rs:660-672`); the persistent transaction
commits them (`crates/evocc-eqopt/src/persistent.rs:564-623`).
Change: Sections 8, 8.7, 8.9 and 8.10 state the successful stop. They also
separate a failed region, which is declined, from a failed transaction, which
returns the original module and state (`persistent.rs:625-646`).
Found while fixing this: the old preamble said that a failed transaction
"continues compilation". The driver stops instead: a chain failure gives
"verified optimization failed" and exit 1 (`crates/evocc/src/main.rs:1927`,
`crates/evocc-eqopt/src/chain.rs:610-616`). The text now states that. If the
owner wants a fallback to the module without rewrite, that is a Rust change.

**A27: epochs, nodes, `no-new-best`, and the named phase functions.**
Text: one epoch traversed the rules over canonical e-classes; the node bound
counted the persistent graph; `saturate_scalar` and `extract_module` were
named phases.
Code: one forward sweep per block proposes one rewrite per candidate
(`scalar_eq.rs:604-606`). The region builder counts certificate nodes
(`evp1.rs:206`, `:268-286`), caps the region count with `max_epochs`
(`:273`), derives each epoch marker from the merges that a rewrite matches on
(`:821-841`), and writes only `stop=saturated` or `stop=policy`
(`:530-534`). No function named `saturate_scalar` or `extract_module`
exists. The checker verifies only the declared box
(`crates/evocc-proofcheck/src/lib.rs:146-178`).
Change: Section 8.11 states the three bounds, what the certificate records,
the tier order and the stop reasons, and describes the current producer in
an informative paragraph. Section 8.9 describes the transaction without the
missing functions. Section 11.3 aligns the `saturated` meaning and the
budget example. The exit pattern
`rg -n "rolls the whole transaction back|saturate_scalar|extract_module|signed_add_associative" spec/evoir.md`
finds nothing (exit 1).

**A29 (text): only the parser is shared.**
Code: both sides call `evocc_evoir::inst_results`
(`crates/evocc-eqopt/src/eligibility.rs:21`,
`crates/evocc-proofcheck/src/eligibility.rs:8`). The checker's
`all_values_mut` (`crates/evocc-proofcheck/src/reassemble.rs:463`) is
byte-identical to the producer's (`evp1.rs:2508`, 197 lines, compared with
`diff`). The checker also uses the shared `render_module`
(`crates/evocc-proofcheck/src/lib.rs:129`), `validate_module`
(`reassemble.rs:104`) and `parse_evp1`.
Change: Section 11.6 adds the paragraph "Shared, and not fail-safe". Section
11.7 rows A1, A3 and A4 name the shared and copied code.
Left: the traversal test covers 10 of the 59 instruction variants
(`crates/evocc-evoir/tests/section9_completeness.rs:492-526`); T03z.

**A30: the renderer is not in the trusted base.**
Code: ordinary `-Oeq-scalar` compiles the in-memory module
(`main.rs:2036-2062`); the checker sees its rendering (`chain.rs:546`,
`:550`, `:619`).
Change: Section 11.7 adds row A8; Section 11.6 states why a dropped field is
dangerous on this path.

**A31: the mutation campaign.**
Code: `crates/evocc-proofcheck/tests/day8_proof_contract.rs:382-497` runs the
real producer once and edits its output: five corruptions exit 3, one
certificate with no region exits 0. No test mutates the optimizer, the
shared certificate codec (`crates/evocc-proof/src/evp1.rs`) or the shared
parts of `evocc-evoir`.
Change: the residual-risk paragraph of Section 11.6 states the property, that
no test measures it, the campaign that exists and what it does not cover.

**A33 (EvoIR part): "the receipt above lists them by number".**
Code: the `trust` list of `tests/contracts/lean-p0-receipt.json` has eight
unnumbered items (`tools/lean/audit.py:1593-1611`).
Change: Section 11.7 says that the receipt keeps its own list. Section 11.6
names the new binding check (T03y below).

**A42: design text with no code.**
Code: no transformation reads `ub.assume`, branch, range or provenance
facts; no registry has `signed_add_associative` (no hit in `crates/`, `lean/`
or `tools/`); no pass reorders EvoIR (the only module pass is
`IdentityPass`, `crates/evocc-passes/src/lib.rs:393`); EvoMIR never enters
e-classes (EvoMIR Section 1).
Change: Section 4.4 states that no transformation reorders and keeps the
conditions as informative design. Sections 6.4 and 6.5 state that nothing
reads `ub.assume` facts; the reassociation example is prose, and the rule
block is gone. Section 7.2 states that `Γ` holds only the Section 8.10 known
bits. The Status section lists Sections 8.1 to 8.4, 10.2 and 10.3 and the
fact table of Section 7.2 as informative, and each of these sections says so.

**A47 (text): the history replays against the trusted registry.**
Code: `verify_history` replays the e-graph certificate under its own rules
and re-derives each entry with the producer's rule code
(`persistent.rs:207-230`).
Change: Section 8.9 "Justification" calls it a producer self-check and names
the checker as the independent check.
Left: the comment of `crates/evocc-eqopt/tests/registry_digests.rs:6-9` and
the missing `g0.ub.v1` digest; T03m.

**A51: smaller EvoIR errata.**

| Item | Code | Change |
| --- | --- | --- |
| `f128` anonymous arguments | `crates/evocc-evoir/src/variadic.rs:103-113` admits `f128` and integer vectors | Sections 3.10 and 3.12 list them |
| Reason table | `crates/evocc-evoir/src/lib.rs:122-145` maps `bit_builtin_zero_operand` to `E3022` and `floating_to_integer` to `E3063` | Section 6.1 adds both rows |
| Fault location | `main.rs:2257-2320` maps the fault site to the recorded C span, else to the function's declaration, else to no position | Section 6.1 states that |
| Section 9.8 summary | a function that calls an external function has the top summary (Section 4.6) | the example shows the top summary |
| Pointer equality | both interpreters compare `pointer_address` (`lib.rs:5568-5590`, `crates/evocc-chir/src/interp.rs:8913-8960`, `crates/evocc-memory/src/lib.rs:1221-1243`) | Sections 3.5 and 5.3 state address equality |
| `asm` effects | `lib.rs:1070-1077`: `atomic` and `may_unwind` are false | Sections 3.11 and 10.1.16 refer to the Section 4.2 summary |
| `__has_attribute` answers | `crates/evocc-cpp/predefined/has-attribute.tsv` has 46 rows | Section 10.5 adds `patchable_function_entry` (1), `ection` and `x` (0); a script compared all 46 rows |
| `E3007` "emitted for nothing" | the trap reason is emitted for nothing, but the interpreters report `E3007` as a catch-all (`lib.rs:3949-3971`, `crates/evocc-mir/src/interp.rs:139`) | no text change; see the defects below |

## T03x: EvoMIR and the target description

**A34: the memory guards "have been asserted before".**
Code: `pointer_valid` lowers to nothing and `access_in_bounds` only records
an alignment (`crates/evocc-mir/src/lower.rs:3481-3488`). The front end emits
no `ub.assume` for them (EvoIR Section 6.1). The formation guard follows the
offset (EvoIR Section 10.4, rule 10).
Change: the `ptr.offset` and guard rows of EvoMIR Section 5.1 state that
native code relies on the C rule and checks no memory fault.

**A50: EvoMIR and target errata.**
- GOT addresses. Code: `crates/evocc-mir/src/emit.rs:839-850` uses the GOT
  when a symbol is weak, or when the code is position independent and the
  symbol has default visibility; on Darwin only for undefined symbols. Tests:
  `crates/evocc-mir/tests/section8_linkage.rs:125`,
  `crates/evocc/tests/external_symbol_lowering.rs:286`. Change: Section 8.4
  states that one rule, and Section 8.5 refers to it.
- `spill.*` and `arg.*`. Code: the interpreter refuses them
  (`crates/evocc-mir/src/interp.rs:1948-1972`). Change: Sections 4.6 and 9.1
  exclude them.
- The Status table now names `lean/EvoIR/MachineExecution.lean`, where `step`
  is defined (`:676`).
- One refusal fails the unit (`crates/evocc-mir/src/native.rs:216-221`).
  Change: Sections 1.1 and 10 say "translation unit".
- Target Section 3: the first lines are `version abi.v1` and
  `version profile.v1` (`tools/generate-target.py:913`, `:1050`). Change: the
  form rule lists each `<kind>`.

**A44 (text).** Code: `crates/evocc-mir/tests/target_boundary.rs` scans
`generic/` for ISA names (`:159-192`) and checks a fixed list of migrated
declarations (`:194-241`). No pattern row has an SMT or Lean proof. Change:
target Rule 2 states what the test checks; rows B1 and B6 say "not yet
established".
Left: `asm_ast.rs:179` and `:183` derive limits by hand (Rule 1), and the
generated-file filter misses three tables; T03z.

**A46 (EvoMIR part).** The Lean checkers run only in proof-required mode.
Change: EvoMIR Section 9.3 says so.

Left for the Rust lane (T03x code):
- `crates/evocc-randgate/tests/randomized_semantic_gate.rs:1083-1090`
  accepts the withdrawn `E4005` to `E4008`, `E4011` and `E4012` (A48).
- `crates/evocc-eqopt/src/scalar_eq.rs:111` and `:117` reuse `E4001` and
  `E4002`, and `crates/evocc-eqopt/tests/scalar_eq_bridge.rs:120` pins it
  (A48).

## T03y: verification, receipt binding and README

**A33: the receipt binding is not compared.**
Code: the receipt binds revision 2.3.0 and `41cefe2f…`; nothing compared it.
Change: `tools/render-status.py --check` now fails when
`binding.spec_sha256` or the revision in `binding.spec_revision` differs from
the `spec/evoir.md` pin. A missing receipt, binding or pin also fails.
`tools/tests/test_render_status_binding.py` tests the check. Verification
Section 5.4, EvoIR Section 11.6 and `spec/README.md` name the check.

**A46 (verification part): readers refuse noncanonical forms.**
Code: the JSON readers use `Json.parse`
(`lean/EvoIR/MachineReader.lean:161-165`, `lean/EvoIR/Alloc/Reader.lean:199-208`,
`lean/EvoIR/Asm/Reader.lean:179-183`, `lean/EvoIR/Core/Source.lean:316-320`,
`lean/EvoIR/Core/CutpointInput.lean:65-67`, `:96-98`). In the pinned Lean
v4.32.2, `Json.parse` skips space, tab, CR and LF around each token and
decodes string escapes (`Lean/Data/Json/Parser.lean`). The EvoIR reader trims
each line and skips empty lines (`lean/EvoIR/Core/IRReader.lean:365-366`).
The certificate reader splits at LF and space only
(`lean/EvoIR/Rewrite/Certificate.lean:226-228`).
Change: verification Section 6 states this and relies on the digest chain.

**A49.** `spec/README.md` lists five documents with their revisions.

**A52.**
- Section 4.6 adds the `zeroMask` exception
  (`lean/EvoIR/Asm/Check.lean:63-64`, `:85`), the save slot of each
  written callee-saved register (`:295-305`) and the entry structure
  (`:314-323`).
- Section 2.1 lists every EqSat option that the driver admits with
  `-Oeq-scalar` (`main.rs:719-764`, `:1046-1059`, `:1106-1113`).
- Section 7.5 states the rule as `crates/evocc/src/proof_required.rs:707`
  computes it: `all_stages_checked` is true when every row is integrated, so
  it stays true when the row `rewrite` is `NOT_CHECKED` (`:629`).

Left for the Rust lane (T03n): Section 2.1 still requires `-O0`, but
`main.rs:1049` admits the default level, and the admission refusals give exit
6 after compilation (A35). The text stays until T03n decides.

## Defects for a later batch

- `E3007` is withdrawn (EvoIR Section 6.3: "no new condition may reuse it"),
  but the EvoIR, C-HIR and EvoMIR interpreters and the driver report it as a
  catch-all for an unsupported or invalid operation. Either a new code or a
  stated rule is needed (Rust, with an EvoIR row).
- The Lean readers accept noncanonical JSON whitespace, escapes and EvoIR
  line whitespace. The digest chain keeps this safe. A byte-exact reader would
  be a Lean change; this note proposes none.
- The pointer-equality text now follows the code. A future alias analysis
  must keep provenance and address equality apart (EvoIR Section 5.3).

## Checks run

- `python3 tools/check-spec-pins.py --update`: 5 pins written. The pin of
  `spec/diagnostics.md` moved to 1.4.0 and `d035dc15…`, the working-tree
  text of another agent.
- `python3 tools/check-spec-pins.py`: 5 documents match, exit 0.
- `python3 -m unittest tools.tests.test_render_status_binding`: 6 tests OK.
- `python3 tools/render-status.py --check`: exit 1, as expected.
  `docs/status.md` still names EvoIR 2.4.0 and EvoMIR 1.2.0, and the receipt
  binds 2.3.0 and `41cefe2f…` against the 2.5.0 pin. Both clear after
  `tools/gate-lean.sh` and `python3 tools/render-status.py`.
- `python3 tools/generate-target.py --check`: exit 0.
- `python3 -m unittest tools.tests.test_generate_target`: 44 tests OK.
- `python3 tools/check-textual-grammar.py`: 0 disagreements, exit 0.
- `python3 -m unittest tools.lean.tests.test_registry_to_lean`: 16 tests OK.
- `python3 -m unittest tools.ladder.tests.test_eqsat_pilot_status
  tools.ladder.tests.test_todo_schedule tools.ladder.tests.test_codegen_trend`:
  55 tests OK.

Not run: the Cargo readers of the spec (`spec_grammar_contract`,
`registry_digests`, `section5_lowering`, the `explain_pages` table) and
`tools/gate-lean.sh`. The edits leave Section 6.3, Section 9.3, the
registry blocks and the EvoMIR rows that `section5_lowering.rs` reads
unchanged.
