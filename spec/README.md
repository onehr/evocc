# Specifications

This directory holds the ground truth of EvoCC: five pinned documents that state what the
intermediate representations, the target description, the checked compilation path and the
presentation of diagnostics must be.
Code, tests and proofs implement these documents. When code and a document disagree, one of them has a
defect; a fix changes the code, or it changes the document with a new revision.

## Documents

| Document | Governs |
|---|---|
| [`evoir.md`](evoir.md) | EvoIR: types, block-argument SSA, instructions, effects, memory, undefined behavior and diagnostics, equality saturation, text syntax, cross-IR contracts, the proof-checking interface and the reviewed rule registries |
| [`evomir.md`](evomir.md) | EvoMIR: registers and classes, sub-width values, instructions, lowering from EvoIR, frame and calling convention, SSA destruction and allocation, emission, verification and diagnostics |
| [`target.md`](target.md) | The internal target description: the components in `targets/aarch64/`, their form and rules, and what a correct backend operation means |
| [`verification.md`](verification.md) | The checked compilation path: the claim, the subset `core-v1`, the semantics and refinement, the stage checkers and theorems, the pipeline theorem, the exports, proof-required mode and the trusted base |
| [`diagnostics.md`](diagnostics.md) | The presentation of diagnostics: code ranges, spans and source bytes, text and color, message style, JSON records and source repairs, exit status; the diagnostic authority table stays in `evoir.md` Section 6.3 |

The grammar script and the target generator's Python unit tests are manual
change checks. Do not infer that the workspace gate or CI runs them. The
workspace gate runs the specification-pin and target-generator `--check`
commands; its Cargo tests also check the textual grammar contract.

## Pins

`spec/pins.json` (format `evocc-spec-pins-v1`) records the revision and the SHA-256 of each pinned
document. Each `spec/*.md` file except this README is pinned. `tools/check-spec-pins.py` compares the
pins with the files and exits 1 on a mismatch, with the message
`spec/<file> changed: bump its revision and run tools/check-spec-pins.py --update`.
`tools/gate-changed.sh` runs the check in both of its modes, before its Cargo steps. The revision of a
document is the first line that contains "Revision" and a version number.

## Changing a document

1. Edit the document.
2. Increase its revision line, and record the change where the document keeps its history.
3. Run `python3 tools/check-spec-pins.py --update`. It rewrites `spec/pins.json` and warns if the
   content changed but the revision line did not.
4. Run `python3 tools/check-spec-pins.py` and the readers of each machine-read block that you
   changed (see below).
5. Commit the document and `spec/pins.json` in the same commit.

A generator that writes into a document also changes its digest. After
`python3 tools/generate-target.py` rewrites the generated tables in `evomir.md`, bump the revision of
`evomir.md` and update the pins.

## What does not belong here

A specification states what must hold. It does not state what is done, planned or measured. These
belong elsewhere:

- the design of verified optimization: `docs/verified-optimization-design.md`;
- status, evidence and receipts: `docs/status.md`, `docs/evidence.md`, `tests/contracts/`;
- open work and task codes: `TODO.md`;
- superseded plans, dated notes and reviews (history, not maintained): `docs/archive/`.

A new document belongs in `spec/` only if it is a contract that code, tests or proofs must obey, and
it gets a pin at the next `--update`.

## Machine-read blocks

Some blocks are test or generator inputs. They must keep their shape, and an edit of such a block
changes a generated file or a test result, so run its reader before you commit. `evoir.md` and
`evomir.md` list their own machine-read parts in their "Status and scope" sections.

| Block | Document | Reader |
|---|---|---|
| Revision line | All five | `tools/check-spec-pins.py`; for `evoir.md` and `evomir.md` also `tools/render-status.py` (dashboard versions); `tools/lean/audit.py` binds the `evoir.md` line and SHA-256 into `tests/contracts/lean-p0-receipt.json`, and `tools/render-status.py --check` fails when that binding differs from the `evoir.md` pin |
| Section 6.3 Diagnostic Authority table | `evoir.md` | `crates/evocc/build.rs` (the diagnostic table of the driver), `crates/evocc/tests/check_product.rs` |
| Sections 9.3, 9.3.1 and 9.3.2: grammar productions and shape blocks | `evoir.md` | `crates/evocc-evoir/tests/spec_grammar_contract.rs`, `tools/check-textual-grammar.py` |
| Section 11.6 reviewed registry blocks and digests | `evoir.md` | `tools/lean/registry_to_lean.py` (generates `lean/EvoIR/Registry.lean` and `GuardRegistry.lean`; step 1 of `tools/gate-lean.sh`), `tools/lean/audit.py`, `crates/evocc-eqopt/tests/registry_digests.rs` |
| The `stack.alloc` and `stack.release` rows (Section 4.6), the `alloca.dynamic` and `lifetime.end` rows (Section 5.1), the `= stack.alloc` production (Appendix A) | `evomir.md` | `crates/evocc-mir/tests/section5_lowering.rs` |
| Generated pattern table (`aarch64-patterns` markers, Section 5.6) and encoding table (`aarch64-encodings` markers, Appendix A.1) | `evomir.md` | Written by `tools/generate-target.py` from `targets/aarch64/patterns` and `targets/aarch64/encodings`; `--check` runs in `tools/gate-changed.sh`; `tools/tests/test_generate_target.py` |
| Target tables | `target.md` | No tool reads `target.md`. Its tables describe `targets/aarch64/`, which `tools/generate-target.py` reads |
| Stage table, record fields, exit codes | `verification.md` | No tool reads `verification.md`. `crates/evocc/tests/proof_required.rs` and the Lean test modules check the same contract |

The EqSat ladder lanes (`tools/ladder/eqsat_*.py`) record `evoir.md` and `evomir.md` as campaign
inputs, so an edit of either file also changes those campaign inputs.
