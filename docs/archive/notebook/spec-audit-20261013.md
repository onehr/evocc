# Specification consistency audit (T03), 2026-10-13

This note records the T03 audit of `spec/`. It maps normative clauses to the
code, the tests and the cited evidence, and it proposes child tasks. It
changes no specification, code or task file.

## Base and method

- Commit: `546fe46a`. Every code location cites this commit. Other work
  changed some source files in the working tree during the audit, so a line
  in the working tree can differ.
- Compiler: `.build/frozen/366fd796/evocc-darwin`, which prints
  `evocc 0.1.0 (366fd796613e 2026-10-13)`. All probes ran on the macOS arm64
  host. The host Apple clang is the platform reference for the Darwin ABI.
- Documents, equal to `spec/pins.json` when the audit started:

| Document | Revision | SHA-256 |
| --- | --- | --- |
| `spec/evoir.md` | 2.1.0 | `7f36e99609536d61b2b840e795ab7405029a941f7ee81bb04c383c565529908b` |
| `spec/evomir.md` | 1.0.1 | `e87d23a14f55fcf587c45a5a80faf8752971aac86872a36fa94b71fba6026cb8` |
| `spec/target.md` | 1.0.0 | `13e6cc6fe6d3d1080bf7e34a162b82096197053e6be78ede8bbda270c3456482` |
| `spec/verification.md` | 1.0.0 | `c6e43be04e60b9ad644f3e948764e270ddb23a7eb48ec31dc020b50200bd0899` |
| `spec/diagnostics.md` | 1.3.0 | `eb63f6366ae68948228dd0b3147a8579a9abc1884799318cff17444b784ba29c` |

- The audit was read only. It ran no Cargo, no Lake and no built binary other
  than the frozen compiler. TESTED means that a test asserts the clause. The
  tests were read, not run.
- Seven slices ran in parallel: `verification.md` with EvoMIR §9.3 and A.2;
  EvoIR §8.5 to §11; EvoMIR §1 to §10 with `target.md`; EvoIR §1, §2, §4 and
  §10.2 to §10.4; EvoIR §3, §5, §6, §7 and §9; EvoIR §10.1, §10.5 and
  `diagnostics.md`; and a lead pass. The lead pass covered E4006 and E4008,
  operand traversal, metadata mutation, the EvoIR §8 preamble and
  `spec/README.md`. The lead re-ran every S1 probe and re-read the code of
  most S2 rows. S3 and S4 rows rest on the evidence of their slice.
- Another task edited these parts during the audit, so this note gives them
  one line each: EvoIR §3.13.1 and §6.3 (D01), the EVP epoch marker in EvoIR
  §11.3 (D02), the text-format limits of EvoMIR Appendix A (D03), the
  documentation of fortified-call builtins, and `spec/diagnostics.md`.

## Status and severity

| Status | Meaning |
| --- | --- |
| TESTED | A test asserts the clause. A refusal clause also has a negative test. |
| UNTESTED | The code can follow the clause, but no test asserts it. |
| CONTRADICTED | The code, a probe or another normative sentence disagrees. |
| UNFALSIFIABLE | No test can decide the clause as it is written. |

| Severity | Meaning |
| --- | --- |
| S1 | Silent wrong code or a broken fail-closed rule, shown by a probe. |
| S2 | Other normative behaviour that the code contradicts, or a safety rule with no test. |
| S3 | An untested or unfalsifiable rule, or an erratum where the code follows another normative sentence. |
| S4 | Stale text with no effect on behaviour. |

## Starting point: E4006 and E4008

Decision: both rows were errata, not new authority. The code was right
before the text. T03a corrected the text in `ef011218` (2026-09-23). The
edit added no feature.

- **E4006** (`not-lowered: indirect-branch`, EvoMIR §10 l.1422). Commit
  `ebca8425` (2026-09-13) implemented computed branches before the text
  changed. Lowering is at `crates/evocc-mir/src/lower.rs:2909-2921`, and
  emission at `crates/evocc-mir/src/emit.rs:3305`. No source file emits
  E4006. Positive tests:
  `crates/evocc-mir/tests/section9_native_differential.rs` (computed counter,
  copy cycle and two spill-pressure variants, run natively on a Darwin host)
  and `crates/evocc-mir/tests/section7_out_of_ssa.rs:276`. Negative tests
  exist at the EvoIR level: `crates/evocc-evoir/tests/indirect_edges.rs:38`
  (seven malformed bindings), `:59` (a null target gives E3048) and `:105`
  (a successor named only by a relocation). No EvoMIR-level negative test
  exists (A38).
- **E4008** (`not-lowered: pointer-difference`, l.1424). EvoIR §10.1.3
  (l.4221-4227) defines the form: `ptr.to_int` of each operand,
  `sub.bits<64>`, then `sdiv.bits<64>` by `sizeof(T)`. "Exact" is a property
  of defined executions, not an opcode. Each step is an existing EvoMIR §5.1
  row. The front end has used this form since `ffce18e2` (2026-09-06). Probe
  P16 shows it. Positive test:
  `crates/evocc-lower/tests/relocations_and_addresses.rs:512`. Gap: a
  zero-size element gives a division by zero with no guard (A08).
- A test still accepts every withdrawn code as a Section 10 code
  (`crates/evocc-randgate/tests/randomized_semantic_gate.rs:1076-1093`, A48).
  `tests/contracts/semantic-gate-contract.json:694-696` keeps the old
  meanings. That file is a sealed contract and stays as it is.

## What holds

- `verification.md`: every cited Lean name and path exists. Ten copied
  statements match the Lean source, from `Refines` to `AbiSourceEntry`.
  `Pipeline.refusal` (`lean/EvoIR/Pipeline/Check.lean:110-147`) runs the
  twelve checks in the stated order. Lean `Eval.stages` and the Rust `STAGES`
  have the same rows. The rule counts 27, 45, 51, 55 and 7 hold. The record
  field order in the code matches §7.4.
- The five registry digests, recomputed from the specification blocks, equal
  the constants in `crates/evocc-proofcheck/src/lib.rs:23-36` and `:198` and
  in `crates/evocc-eqopt/src/evp1.rs:28-47`.
- The EvoIR §4.2 effect table matches `inst_effect_summary`
  (`crates/evocc-evoir/src/lib.rs:1034-1156`) on all 34 rows.
- Operand traversal of `br.indirect` (EvoIR §9.3, l.3759-3763) visits the
  target first, then the edge arguments in order. Three places do this:
  `inst_uses` (`crates/evocc-evoir/src/lib.rs:635-637`), the checker
  (`crates/evocc-proofcheck/src/reassemble.rs:465-467`) and the producer
  (`crates/evocc-eqopt/src/evp1.rs:2510-2512`). Lowering restores the edge
  order (`crates/evocc-lower/src/lib.rs:4776`, `:16284`). Tests:
  `crates/evocc-evoir/tests/section9_completeness.rs:492` and
  `crates/evocc-evoir/tests/indirect_edges.rs`.
- Metadata mutation: the receipts record 41 rehashed semantic forgeries
  that exit 3 (`crates/evocc-proofcheck/tests/copy_is_opaque.rs`;
  `tests/fixtures/proof-semantic-metadata.out` and
  `tests/fixtures/section9-contract-audit.out`). The sentinel instantiates
  59 of 59 instruction variants and 143 of 143 fields
  (`crates/evocc-evoir/tests/section9_completeness.rs:15`, `:1663`). Weak
  linkage, `align`, `stack_protect`, the alias target and the RMW opcode
  have no direct mutation. The byte comparison of the canonical text covers
  them.
- The 7 cited `.evidence/` paths, all cited file paths and all 16 cited long
  identifiers resolve.
- No finding breaks the `core-v1` claim of `verification.md` §1. No finding
  makes the EvoIR certificate checker accept an invalid certificate.

## Findings, most severe first

Spec lines are those of the pinned revisions above. Code lines are those of
`546fe46a`. The probes P1 to P16 are listed at the end.

| ID | Clause | Code (`546fe46a`) | Tests | Status | Sev | Kind | Task |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A01 | evomir §6.7 l.830-835: `x19`-`x28` are callee-saved. target §5 B4 l.142: the verifier checks preservation. | `crates/evocc-mir/src/generic/allocation.rs:240-246` builds the saved set from the assignment only. `crates/evocc-mir/src/emit.rs:454` saves that set. An `asm` that clobbers `d8` gets a save; one that clobbers `x19` or `x20` does not. The verifier never reads the set. | none | CONTRADICTED | S1 | implementation defect | T03g |
| A02 | evomir §6.2 l.759-768: a signed narrow argument gets `sext` to `bits<32>`; the callee zero-extends a narrow result. | `crates/evocc-mir/src/lower.rs:3579-3586` zero-extends every narrow argument at the boundary ("Recorded gap"). `lower.rs:3031-3039` zero-extends every narrow result. Apple arm64 extends both by signedness. | `crates/evocc-mir/tests/section9_calling_convention_probe.rs:47` (unsigned shapes only) | CONTRADICTED (argument); the result rule is wrong for Darwin | S1 | implementation defect and erratum | T03h |
| A03 | evomir §6.1 l.733, §6.6 l.824-826: Darwin refuses stack arguments (E4010). | `crates/evocc-mir/src/lower.rs:532` counts general-register positions only. A SIMD overflow goes to Linux-style 8-byte slots. | `section9_calling_convention_probe.rs:629` (general registers only) | CONTRADICTED | S1 | implementation defect | T03h |
| A04 | evomir §6 l.713-714: a restriction is "refused, never approximated". §6.5 l.805-810 notes the Darwin pair. TODO D04 says that the C driver emits only Linux. | `crates/evocc-mir/src/lower.rs:6160` rounds a `bits<128>` argument to an even pair on Darwin. Since EvoMIR 1.0.1 the driver emits Darwin, and Darwin is the default target on macOS. | none on Darwin | CONTRADICTED (known as D04; its scope sentence is stale) | S1 | implementation defect | T03h |
| A05 | evoir §9.1 l.3447-3452: one target authority. target §5 B7 l.145: `abi-<os>` matches the platform. No section states the Darwin C data layout. | `crates/evocc-types/src/lib.rs:2393-2400` gives `long double` 16 bytes at alignment 16 on every target. `crates/evocc/src/aarch64-apple-darwin.h:95-96` defines `__SIZEOF_LONG_DOUBLE__` as 8. | none | CONTRADICTED | S1 | implementation defect and spec gap | T03i |
| A06 | evoir §10.1.2 l.4075: `_Atomic` is typed unsupported. §10.1.3 l.4268: `_Atomic` objects are outside the profile (E2702). | `crates/evocc-parser/src/lib.rs:10088`, `:10118`, `:10134` report E2702 for assignment and update only. A read lowers to a plain `load` and `ldr`. | negative tests cover writes only: `crates/evocc-parser/tests/day5_g2_lvalues.rs:944`, `crates/evocc-parser/tests/aggregate_copies.rs:158`, `crates/evocc/tests/language_semantics.rs:736` | CONTRADICTED | S1 | implementation defect | T03j |
| A07 | evoir §10.1.12 l.4906-4908: a nonconstant range-designator initializer is E2917. | `crates/evocc-parser/src/lib.rs:4393-4408` expands `[a ... b]` into one target per index. The initializer then runs once per element. GCC documents one evaluation. | none for this case | CONTRADICTED | S1 | implementation defect | T03j |
| A08 | evoir §6.2 l.2427: division lowers as `divisor_nonzero`, `ub.assume`, then `sdiv`. evomir §4.3 l.415 and §5.1 l.528: the guard "has been asserted before". | `crates/evocc-lower/src/lib.rs:11364-11368` emits `sub` and `sdiv` by `sizeof(T)` with no guard. `sizeof(T)` is 0 for `struct e {}` and for `T[0]`. The EvoIR interpreter traps with E3002. The C-HIR interpreter panics on `byte_delta % scale` (`crates/evocc-chir/src/interp.rs:9167`, exit 101). The checked native build returns 0. | `crates/evocc-lower/tests/expression_value_flow.rs:139` covers a zero runtime stride (E3011); none covers a constant zero size | CONTRADICTED | S2 | implementation defect | T03k |
| A09 | evoir §11.4 l.6786-6787: the checker exits only with 0, 2 or 3. | `crates/evocc-proof/src/evp1.rs:354`, `:370` allocate `Vec::with_capacity(count)` from an unbounded count. `crates/evocc-proofcheck/src/replay.rs:305`, `:522`, `:576` call `find`, which indexes `parent[class]` with no bound (`:670-677`). `crates/evocc-proofcheck/src/eligibility.rs:182-207` recurses with no bound. The driver still fails closed (`crates/evocc-proof/src/checker_process.rs:21-28`). | none | CONTRADICTED (code reading, not run) | S2 | implementation defect | T03l |
| A10 | verification §7.7 l.483-485: on exits 3 to 6, `output.s` and `record.json` must not exist. | `crates/evocc/src/proof_required.rs:770-771` creates `output.s`, then `record.json`. If the second write fails, the first file stays, and `:405` prints "no output was written" with exit 4. | none | CONTRADICTED (stub-checker probe of the slice) | S2 | implementation defect | T03n |
| A11 | verification §2.1 l.37, §7.2 step 9, §7.6 l.474: `--emit=asm` is required, and the text goes to standard output. | `crates/evocc/src/main.rs:1047` admits every action that maps to `EmitAsm`. The cc actions `-S -o` and `-c -o` reach it, and `proof_required.rs:428` writes the output file instead. | none | CONTRADICTED (stub-checker probe of the slice) | S2 | erratum or defect; the owner decides | T03n |
| A12 | evomir §1.1 l.95-97, §10 l.1411-1413: a refusal is reported at the EvoIR instruction. | `crates/evocc-mir/src/native.rs:216-221` keeps only the codes. `crates/evocc/src/main.rs:2201-2204` reports each code at `<command-line>` with the text `evomir-lowering-refused`. The `Refusal` record (`crates/evocc-mir/src/lower.rs:34-41`) holds the function, block, index and detail. | library: `crates/evocc-mir/tests/section10_refusals.rs:132-137`; driver tests check the code text only (`crates/evocc/tests/integer_vectors.rs:119`, `crates/evocc/tests/inline_assembly_lowering.rs:116`) | CONTRADICTED | S2 | implementation defect | T03o |
| A13 | evomir §9.2 l.1364-1374, §7.5 l.1143-1148; target §5 B3 l.141. Three rules: an entry value is in its own ABI register; an `asm` clears its clobbers; spills and reloads pair. | `crates/evocc-mir/src/generic/allocverify.rs:227-262` accepts any argument register below the parameter count. `:437-449` clears clobbers for a terminator `asm` only, and `apply` (`:453-467`) clears call clobbers only. The dataflow state (`:179`) holds registers, not spill slots. | none for these three rules | CONTRADICTED | S2 | implementation defect | T03g, T03p |
| A14 | evomir §4.7 l.507-513: a `tst` immediate is an encodable mask; raw text cannot bypass the domains. | `crates/evocc-mir/src/validate.rs:544-547` checks the classes only. `crates/evocc-mir/src/text_parse.rs:1273-1283` reads any immediate. | none | CONTRADICTED | S2 | implementation defect | T03q |
| A15 | evomir §6.14 l.1032-1034: `-fpack-struct`, `-mabi`, `-Ofast` and other values are refused. | `crates/evocc/src/cc.rs:839-846` ignores them with a warning, and silently under `-w` or `--cap-lints=allow`. | `crates/evocc/tests/compilation_options.rs:157-165` covers the `--emit=asm` path only | CONTRADICTED | S2 | implementation defect | T03r |
| A16 | evoir §10.1.3 l.4221-4227 and §3.5 l.636: the C-HIR interpreter carries the same-object rule and reports E3013. | `crates/evocc-chir/src/interp.rs:9216` reports an unsupported expression, which `:334` maps to E3007. `crates/evocc/src/main.rs:522` runs `--check` on EvoIR, which cannot see the rule, and prints "No undefined behavior was found on the executed path". | none | CONTRADICTED | S2 | implementation defect or erratum | T03s |
| A17 | evoir §10.1.15 l.5265-5266: a `const` pointee of `__builtin_*_overflow` is E2913. | `crates/evocc-parser/src/lib.rs:14416-14428` checks for a pointer, an integer and `_Bool`, but not for qualifiers. Clang refuses the program. | `crates/evocc-parser/tests/checked_arithmetic.rs:60-81` has no `const` case | CONTRADICTED | S2 | implementation defect | T03t |
| A18 | evoir §10.5 l.6266-6270: `__has_builtin(name)` is 1 for every name the front end resolves. | `crates/evocc-cpp/src/lib.rs:2036-2037` answers from `crates/evocc-cpp/predefined/has-builtin.txt`, a list of 9 names. | `crates/evocc-cpp/tests/target_profile.rs:352-358` covers the listed names only | CONTRADICTED | S2 | implementation defect | T03t |
| A19 | evoir §2.1 l.330-332: static relocations may not target TLS, also through an alias. | `crates/evocc-evoir/src/lib.rs:1972-1975` and `:2143-2148` check only names in the TLS table, so a non-TLS alias of a TLS object passes. The backend emits `.set a, t` and a GOT access. The two interpreters disagree. | `tests/fixtures/tls-alias.c` through `crates/evocc/tests/elf_lifecycles.rs:273` covers a TLS alias only | CONTRADICTED | S2 | implementation defect | T03u |
| A20 | evoir §3.2 l.479-483, §9.5 l.3912-3913: an out-of-range `const` literal is malformed, and a parser never truncates it. | `crates/evocc-evoir/src/text_parse.rs:945-947` reads a `u64` with no width check, and `lib.rs:2885` checks the type only. The interpreter masks the value. The Lean reader refuses it (`lean/EvoIR/Core/IRReader.lean:254`). A `bits<128>` literal above `2^64 - 1` cannot be written. | none | CONTRADICTED | S2 | implementation defect | T03u |
| A21 | evoir §3.7 l.713-718, §6.1 l.2411: an invalid `lifetime.end` reports E3018. | `crates/evocc-memory/src/lib.rs:1007-1008` maps it to E3007, an unsupported-operation code, not undefined behaviour. | the unit tests at `crates/evocc-memory/src/lib.rs:3685` and `:3748` assert E3007 | CONTRADICTED | S2 | implementation defect | T03u |
| A22 | evoir §3.7 l.756-766, §10.4 rules 9 and 10 (l.6078-6090): each access consumes a matching guard; each `ptr.offset` result goes at once to `pointer_valid`. | Implemented at `crates/evocc-evoir/src/lib.rs:1617-1690`. Pointer and function-pointer accesses skip the layout match (`:1699-1704`, `:1730`). | none: no test omits, retargets or resizes a guard | UNTESTED; CONTRADICTED for pointer layouts | S2 | missing test and implementation defect | T03u |
| A23 | evoir §4.5 l.1883-1891: an e-class never mixes result types. | `crates/evocc-egraph/src/lib.rs:117-122` keys a node by operator and children. `crates/evocc-eqopt/src/persistent.rs:295-298` and `crates/evocc-eqopt/src/scalar_eq.rs:1384-1386` give `zext`, `sext` and `trunc` one operator for every width. So `zext` to 32 and to 64 bits of one value share a class. The slice found no effect on output. | none | CONTRADICTED (latent) | S2 | implementation defect | T03v |
| A24 | evoir §9.1 l.3447-3452: no other target may use this format until the grammar has a target identity. §3.12 l.1427: a list object is 32 bytes. | `--emit=evoir --target=aarch64-apple-darwin` writes the same format, with an 8-byte `va_list`. | none | CONTRADICTED | S2 | spec gap and implementation defect | T03i |
| A25 | evoir §11.5 l.6791-6792: "enforced by a test that reads the dependency closure". | Only `tests/acceptance/day8-red.sh:25-32` exists. It checks `evocc-evoir` against `evocc-egraph`, and nothing for `evocc-proofcheck`. Neither `tools/gate-changed.sh` nor CI runs it. The boundary holds today (`crates/evocc-proofcheck/Cargo.toml`). | none for the checker | CONTRADICTED | S2 | missing test | T03m |
| A26 | evoir §8 l.2808-2809, §8.9 l.3209-3212: reaching a bound rolls the transaction back. | §8.11 l.3355-3358 and §11.3 l.6724 say the opposite. The code keeps the admitted regions (`crates/evocc-eqopt/src/evp1.rs:273-285`, `crates/evocc-eqopt/src/scalar_eq.rs:662-671`). | `crates/evocc-eqopt/tests/verified_chain.rs:513`, `crates/evocc-randgate/tests/certificate_budget.rs` | CONTRADICTED (inside the spec) | S3 | erratum | T03w |
| A27 | evoir §8 l.2797-2810, §8.11 l.3332-3357: an epoch traverses the rules over classes, in tiers; the node bound counts the whole graph; `no-new-best`; `saturate_scalar` and `extract_module`. | `crates/evocc-eqopt/src/evp1.rs:206` counts certificate nodes. `:273` caps the region count with `max_epochs`. `:321` records the region count as epochs. `:530-534` writes only `saturated` or `policy`. `:841` derives markers from depth. Neither named function exists. | none; the checker cannot see these quantities | CONTRADICTED and UNFALSIFIABLE | S3 | erratum | T03w |
| A28 | evoir §11.3 l.6600-6661 and l.6743-6748, §9.5 l.3950-3952: status 2 for malformed and status 3 for invalid input, rule by rule. | Implemented in `crates/evocc-proofcheck/src/replay.rs:260-264`, `:345-349`, `:474-510`, `:521-591`, `:634-646` and `crates/evocc-proofcheck/src/lib.rs:224-233`. An `after.evo` that fails validation exits 2 (`lib.rs:69-75`), because validation runs inside the parse. `epoch 0` is accepted (D02, in flux). | format tests cover the version, counts, positions and binding order. No negative test covers union endpoints, congruence roots, unused derived nodes, the extraction closure, region order, the digest form, attribute order, a missing `endregion`, `evp2` without a budget line or `evp1` with a marker. | UNTESTED (13 rules); CONTRADICTED (status of an invalid module) | S3 | missing test | T03l |
| A29 | evoir §11.6 l.6819-6822; §11.7 A1 and A4, l.7172 and l.7175: eligibility is not shared, only the parser is. | Both sides use `evocc_evoir::inst_results` (`crates/evocc-eqopt/src/eligibility.rs:21`, `crates/evocc-proofcheck/src/eligibility.rs:8`). The checker's `all_values_mut` (`crates/evocc-proofcheck/src/reassemble.rs:463`) copies the producer's (`crates/evocc-eqopt/src/evp1.rs:2508`). | an independent traversal test covers 10 of 59 variants (`crates/evocc-evoir/tests/section9_completeness.rs:492-526`) | UNTESTED | S3 | erratum and missing test | T03w, T03z |
| A30 | evoir §11.7 l.7164-7178: the trusted base of a certificate. | Ordinary `-Oeq-scalar` compiles the module in memory (`crates/evocc/src/main.rs:2028-2054`). The checker sees its rendering (`crates/evocc-eqopt/src/chain.rs:550`, `:619`). The table does not list the renderer, which this path trusts. | the field sentinel in `section9_completeness.rs` is evidence, not a proof | UNTESTED (unlisted assumption) | S3 | erratum | T03w |
| A31 | evoir §11.6 l.7135-7142: a mutation campaign bounds the risk of the shared format. | `crates/evocc-proofcheck/tests/day8_proof_contract.rs:382` has five hand-written output or certificate mutants and one disabled rewrite. No mutant changes the optimizer or the shared reader `crates/evocc-proof/src/evp1.rs`. | partial | UNFALSIFIABLE as written | S3 | erratum or missing test | T03w |
| A32 | evoir §9.3.1 l.3767-3771; the machine-read tables of `spec/README.md` and EvoMIR; target §3 l.75-78. | `tools/check-textual-grammar.py`, `python3 -m unittest tools.tests.test_generate_target` and `tests/acceptance/day8-red.sh` run in neither `tools/gate-changed.sh` nor `.github/workflows/ci.yml`. `tools/generate-target.py --check` runs in the gate but not in CI. `tools/tests/test_generate_target.py` has no refusal case for CR, tab, spacing or a missing final LF. | not in any gate | UNTESTED | S3 | missing test | T03m |
| A33 | verification §5.4 l.378-380, §10 l.550-552; evoir §11.7 l.7167-7168: the receipt binds the spec and lists the trusted base by number. | `tests/contracts/lean-p0-receipt.json` binds revision 2.0.1 and `9d0f3c3c…`, but the spec is 2.1.0, `7f36e996…`. Its `trust` list has eight unnumbered items. `tools/lean/audit.py:2099-2100` writes the binding; nothing compares it. | none | CONTRADICTED (stale) | S3 | missing check | T03y |
| A34 | evomir §5.1 l.549 and l.556: the guard of a `ptr.offset` "has been asserted before"; the front end lowered the memory guards "into `ub.assume`". | The front end emits `pointer_valid` and `access_in_bounds` with no `ub.assume` (P15). EvoIR §6.1 l.2390-2395 keeps memory rules out of `ub.assume` on purpose. `crates/evocc-mir/src/lower.rs:3425-3431` drops the guards, so native code checks no E3009 to E3013 fault. The formation guard follows the offset (EvoIR §10.4 rule 10). | none | CONTRADICTED | S3 | erratum | T03x |
| A35 | verification §2.1 l.35-42, §7.6 l.475: an admission refusal gives exit 1; `-O0` is required. | `crates/evocc-chir/src/core_profile.rs:76-86`, `:391-393` and `crates/evocc/src/proof_required.rs:451-457` refuse the Darwin target, `-fwrapv`, `-frounding-math` and the stack protector with exit 6, after compilation. Zero-initialization is refused per object. `crates/evocc/src/main.rs:1049` admits the default level without `-O0`. | library test `wrong_target_and_nonbaseline_source_options_refuse` only | CONTRADICTED | S3 | erratum | T03n |
| A36 | verification §2.2, §4.1, §4.5, §4.6, §7.1 to §7.6, §8 item 5, §9. | Implemented in `crates/evocc/src/proof_required.rs:138-196`, `:260-325`, `:501-502`, `:532-569`, `:822-829` and in Lean `Rewrite/`, `Alloc/Check.lean:293-296`, `Asm/Check.lean`. The compiler digest becomes an empty string when the driver cannot read itself (`proof_required.rs:715-716`). | No test covers: 7 Lean certificate refusals; the timeout range and default; duplicate options; variant options; a missing `-fno-builtin`; the `EVOCC_PROOF_CHECKER` fallback; a chain gap; `-Oeq-scalar` with no step; an unwritable directory; a snapshot refusal; a checker signal; an undecodable proposal; a Rust certificate refusal; `STAGES` against `--list-stages`; the record key order; 5 `core-v1` refusal rows; the bridge-block, register-set, entry-structure and stack-bound mutations; Lean and Rust Core agreement outside receipts | UNTESTED | S3 | missing test | T03z, T03n |
| A37 | evomir §1.2 l.110, §4.7 l.512-513: validation is a pipeline step. | `crates/evocc-mir/src/regalloc.rs:18` uses the result only to switch forwarding off. The native path does not refuse invalid lowered EvoMIR. The allocation verifier stays the gate. | `crates/evocc-randgate/src/differential.rs:605` (randgate only) | CONTRADICTED | S3 | implementation defect | T03q |
| A38 | evomir §10 l.1422: invalid targets and malformed bindings keep their own checks. | The EvoMIR interpreter reports E3048 (`crates/evocc-mir/src/interp.rs:436-460`). The validator refuses empty or unsorted successors (`crates/evocc-mir/src/validate.rs:636-645`). | none at the EvoMIR level | UNTESTED | S3 | missing test | T03q |
| A39 | evomir §8.6 l.1291, §10 l.1425: Darwin placement, sections, TLS and patchable entries are E4009. | `crates/evocc-mir/src/native.rs:147-176` stops with `NativeStop::Assembly`, which the driver reports as E0004. | `crates/evocc/tests/n03a_abi_placement.rs:396-402` pins E0004 | CONTRADICTED | S3 | implementation defect or erratum | T03o |
| A40 | evoir §10.1.1 l.4065-4066, §10.1.3 l.4158-4170 and l.4216, §10.1.6 l.4430-4433, §10.1.11 l.4856, §10.1.19 l.5934-5938, §10.5 l.6300-6302 and l.6335-6336. | A `static int memset` after `__builtin_memset` gives the E3101 wrapper. An ambiguous anonymous member gives E2102, not E2906. A goto into a statement expression gives E1003, not E2918. `pack` inside a record gives E1001, not E0107. A noninteger bound gives E2203 at file scope. A cast between record tags passes the front end and fails in lowering (E3101). `-E` drops the whitespace spelling. Address integers are fixed at `bits<64>` (`crates/evocc-lower/src/lib.rs:4715` and other sites). | mostly none | CONTRADICTED; one UNFALSIFIABLE | S3 | implementation defect and erratum | T03t |
| A41 | evoir §1.7 l.225, §2.1 l.371-377, §3.7 l.692, l.700-701 and l.713-715, §3.8 l.942-966, §5.2 l.2091-2100, §9.5 l.3940-3948, §10.4 module rules. | Nested tokens in `fnptr` types pass (`crates/evocc-evoir/src/lib.rs:3448-3476`). The effect-participant scan skips `asm`, strict FP and terminators (`:2099-2131`). `lifetime.end` of a global passes (`:3054-3063`). `global_addr` admits any address space (`:2979-2981`). The access size has no bound (`:3254-3260`). Distinct origins, generated spellings, the TLS table, lifecycle records and E3062 have code but no negative test. | none | CONTRADICTED and UNTESTED | S3 | implementation defect and missing test | T03u |
| A42 | evoir §4.4 l.1872-1881, §6.4 l.2628-2642, §6.5 l.2646-2655, §7.2 l.2676-2687, §8.1 to §8.4 l.2847-2938, §10.2 l.6033-6043, §10.3 l.6045-6053. | No transformation reads `ub.assume` facts or branch, range and provenance facts. No registry has `signed_add_associative`. No code uses the reordering rules. EvoMIR §1 says that EvoMIR never enters e-classes, so §10.2 and §10.3 bind nothing. One sentence (l.2842-2845) makes §8.1 to §8.4 informative; their text has no "Informative" marker. | none | UNFALSIFIABLE | S3 | erratum | T03w |
| A43 | evoir §8.5 l.2998-3006 and l.3024-3025: taint and exclusion cross every edge. | `crates/evocc-proofcheck/src/eligibility.rs:364-387` handles four edge kinds and ends in a wildcard arm. | `br` only: the unit test at `eligibility.rs:677` and `crates/evocc-randgate/tests/regions_outside_the_entry_block.rs:377` | UNTESTED | S3 | missing test | T03z |
| A44 | target §4 rules 1 and 2 l.105-113, §5 B1 and B6 l.139 and l.144. | `crates/evocc-mir/src/asm_ast.rs:179`, `:183` duplicate target facts. The boundary scan (`crates/evocc-mir/tests/target_boundary.rs:208`) skips `costs.rs`, `encodings.rs` and `patterns.rs`. No pattern has a generated SMT or Lean proof; the note at l.182-186 says so. | none | CONTRADICTED and UNTESTED | S3 | implementation defect and missing test | T03z, T03x |
| A45 | evomir §10 E4004 l.1420; §5.2 l.584; §9.2 bullet 5 l.1372. | E4004 at `crates/evocc-mir/src/lower.rs:356`. No truth from aliases at `lower.rs:2034-2041`. Early-clobber and symbolic-operand refusals at `crates/evocc-mir/src/generic/allocverify.rs:43-47`, `:133-141`. | none negative | UNTESTED | S3 | missing test | T03p, T03z |
| A46 | verification §6 l.401-402: readers refuse noncanonical forms. evomir §9.3 l.1378: the checkers validate each compilation. | The Lean JSON readers use `Json.parse` and accept any whitespace (`lean/EvoIR/MachineReader.lean:161-164`, `lean/EvoIR/Alloc/Reader.lean:199-208`, `lean/EvoIR/Asm/Reader.lean:179-183`). The checkers run only with `--proof-required`. | none | UNFALSIFIABLE; CONTRADICTED | S3 | erratum | T03y, T03x |
| A47 | evoir §8.9 l.3229-3232; the comment of `crates/evocc-eqopt/tests/registry_digests.rs:6-9`. | The history replays with certificate rules plus a producer self-check (`crates/evocc-eqopt/src/persistent.rs:207-229`), not an independent registry. The digest test leaves out `g0.ub.v1`. | partial | UNFALSIFIABLE (overstated); weak test | S4 | erratum and missing test | T03w, T03m |
| A48 | evomir §10: withdrawn codes keep their names only. | `crates/evocc-randgate/tests/randomized_semantic_gate.rs:1076-1093` accepts E4001, E4005 to E4008, E4011 and E4012. `crates/evocc-mir/src/lower.rs:693-694` cites the withdrawn E4001. `crates/evocc-eqopt/src/scalar_eq.rs:111`, `:117` reuse E4001 and E4002 for other errors, and `crates/evocc-eqopt/tests/scalar_eq_bridge.rs:120` pins that. | none | CONTRADICTED | S4 | stale test and code | T03x |
| A49 | `spec/README.md` l.3-15 and l.64: four pinned documents. | `spec/pins.json` pins five. The table omits `diagnostics.md`. | none | CONTRADICTED | S4 | erratum | T03y |
| A50 | EvoMIR and `target.md` errata. | §8.4 l.1233 and §8.5 l.1262 disagree on the GOT for defined default-visible symbols (`crates/evocc-mir/src/emit.rs:839-849`). §9.1 l.1353 calls every §4 form executable, but `crates/evocc-mir/src/interp.rs:1948-1951` refuses `spill.*`. The Status table (l.27) cites `Machine.lean` for semantics in `MachineExecution.lean:676-694`. §10 l.1412 says "no assembly for that function", but the whole unit gets none (P10). target §3 l.69-70 versus the actual `version abi.v1` and `version profile.v1` lines. | none | CONTRADICTED | S4 | erratum | T03x |
| A51 | EvoIR errata. | §3.10 and §3.12 omit `f128` for `va_arg` (`crates/evocc-evoir/src/variadic.rs:103-113`). The §6.1 reason table omits E3022 and E3063 (`lib.rs:132-138`). §6.1 says "1:1" locations, but the driver maps sites to C spans (`crates/evocc/src/main.rs:2264-2293`). The §9.8 example summary is not the top summary. Pointer equality compares addresses (`lib.rs:5556-5576`), not provenance. §3.11 calls `asm` effects maximal, but `atomic` and `may_unwind` are false. §4.2 says E3007 is "emitted for nothing", but it is a catch-all. The §10.5 lists differ from the data files. | none | CONTRADICTED | S4 | erratum | T03w |
| A52 | verification errata. | §4.6 l.257-260 omits the `zeroMask` exception and the entry-structure check (`lean/EvoIR/Asm/Check.lean:15-17`, `:63-64`). §2.1 l.42 says "one optional rewrite option", but `crates/evocc/src/main.rs:1054` admits every EqSat option with `-Oeq-scalar`. `all_stages_checked` is true when `rewrite` is NOT_CHECKED. | none | CONTRADICTED | S4 | erratum | T03y |
| A53 | evomir §10 l.1430 (E4020 "must never reach a user"); §9.2 l.1363-1364 (the verifier "never reads" allocator state); target §4 rule 6 l.123-130. | No test can observe these. The verifier signature supports the second. | none | UNFALSIFIABLE | S4 | design statement | none |

Totals: 7 S1, 18 S2, 21 S3 and 7 S4 rows. Several rows group related
clauses.

## Proposed child tasks

T03a to T03f exist, so the new tasks start at T03g. Each exit names a test or
a check.

- [ ] **T03g — Save the callee-saved registers that `asm` clobbers** (A01;
  the `asm` rule of A13). P1 shows a host caller that loses `x19`. Next
  action: add P1 as a native regression for both targets. Then add the
  general callee-saved clobbers to the saved set, and make the allocation
  verifier clear the clobbers of every `asm`. Exit: the regression returns
  42 on the macOS host and in the sealed Linux lane. A forged allocation that
  keeps a value in an `asm`-clobbered register across the `asm` gets E4020.

- [ ] **T03h — Darwin calls across the compiler boundary** (A02, A03, A04).
  P2 to P5 compute wrong values against Apple clang. Next action: add host
  tests for P2 to P5, fail closed with a Section 10 code for each shape that
  the backend cannot place, correct the result rule of EvoMIR §6.2 for
  Darwin, and correct the scope sentence of D04. Exit: on a macOS host, each
  of P2 to P5 gives the clang value (9, 9, 10, and the pair in `x1:x2`), or
  the driver refuses it with E4010 or E4009. No probe compiles to another
  value.

- [ ] **T03i — State and test the Darwin target profile of EvoIR** (A05,
  A24). Next action: decide the Darwin `long double` layout and the target
  identity of EvoIR text. Exit: under `--target=aarch64-apple-darwin`,
  `sizeof(long double)`, `__SIZEOF_LONG_DOUBLE__` and
  `offsetof(struct {char c; long double d; int x;}, x)` equal Apple clang
  (8, 8, 16), or the driver refuses `long double` objects with a stated code.
  Darwin EvoIR text carries a target mark, or the EvoIR reader and
  `-Oeq-scalar` refuse it. EvoIR §9.1 and §10.5 state the rule.

- [ ] **T03j — Fail closed for `_Atomic` reads and nonconstant range
  initializers** (A06, A07). Exit: parser tests show
  `_Atomic int g; int ld(void){return g;}` gives E2702 at `g` (or EvoIR
  `atomic.load seq_cst` and `ldar`), and `int b[4]={[0 ... 2]=f()};` gives
  E2917 at `f()`.

- [ ] **T03k — Keep the division rule of a zero-size pointer difference**
  (A08). Exit: for P9, EvoIR has `divisor_nonzero` and
  `ub.assume.division_by_zero` before the `sdiv`, or the front end refuses
  with a stated code. The checked native build traps with `brk #3002`.
  `--interpret-c-hir` reports a diagnostic instead of a panic. A lowering
  test covers `struct e {}` and `int (*)[0]`.

- [ ] **T03l — Closed checker statuses and the certificate rule matrix**
  (A09, A28). Exit: a new test in `crates/evocc-proofcheck/tests` submits six
  hostile certificates: an out-of-range `class:` reference, extraction class
  and root class; a witness count and a seed count of `2^64 - 1`; and a
  50,000-instruction chain. Each exits 2 or 3; the chain exits 0 or 3. One
  rehashed mutant for each status-2 and status-3 rule of §11.3 exits with
  its stated status. An `after.evo` that fails validation gives the status
  that §9.5 and §11.4 both state.

- [ ] **T03m — Enforce the checks that the specs name** (A25, A32, A47).
  Exit: a Cargo test reads the normal-dependency closure. It fails when
  `evocc-eqopt` or `evocc-egraph` enters the closure of `evocc-proofcheck`,
  or `evocc-egraph` enters that of `evocc-evoir`. `tools/gate-changed.sh`
  (both modes) and CI run `tools/check-textual-grammar.py`,
  `python3 -m unittest tools.tests.test_generate_target` and
  `tools/generate-target.py --check`, or each sentence that claims this
  enforcement is removed. `registry_digests.rs` covers all five registries.

- [ ] **T03n — Proof-required publication and admission** (A10, A11, A35,
  the compiler digest of A36). Exit: a `crates/evocc/tests/proof_required.rs`
  test with a checker that blocks `record.json` asserts exit 4, no
  `output.s` and no `record.json`. `-S -o` and `-c -o` with
  `--proof-required` give the exit and the files that §7 states. A driver
  test asserts the stated exit for `-fwrapv`, `-frounding-math`,
  `-fstack-protector-strong` and a zero-initialized object. The record holds
  a 64-hex compiler digest, or the run exits 4.

- [ ] **T03o — Report EvoMIR refusals where §10 says** (A12, A39). Exit:
  the driver reports P10 at the refused function with its block, index and
  Section 10 name. A Darwin function with a section, TLS or a patchable
  entry gives E4009. UI snapshots pin both. The owner of the n03a receipts
  updates them.

- [ ] **T03p — Make the allocation verifier match §9.2** (A13, A45). Exit:
  in `crates/evocc-mir/tests/section7_allocation_verifier.rs`, these forged
  allocations each get E4020: swapped entry parameters, a reload from the
  wrong slot, two live slots merged, an early-clobber violation, and a
  symbolic operand outside `i`.

- [ ] **T03q — Make EvoMIR validation a gate** (A14, A37, A38). Exit:
  `validate_module` refuses `tst.x %v0, #5` and `tst.x %v0, #0`. The native
  entry refuses lowered EvoMIR that fails validation. EvoMIR tests cover an
  invalid indirect target (E3048 in the EvoMIR interpreter) and empty or
  unsorted indirect successors.

- [ ] **T03r — Refuse ABI-changing options on the cc path** (A15). Exit:
  `evocc -S -fpack-struct` and `evocc -S -mabi=ilp32` exit 1 with a
  diagnostic at the option, also under `-w`. A cc-path test pins both.

- [ ] **T03s — State and test the clean-check scope for pointer relations**
  (A16). Exit: `--check` reports `&x[3]-&y`, `&a<&b` and a non-exact
  difference as the spec states, or the clean-check text and EvoIR §10.1.3
  name these unchecked cases. The C-HIR code equals the code that the spec
  names. A negative test pins each case.

- [ ] **T03t — Front-end refusals that EvoIR §10.1 and §10.5 name** (A17,
  A18, A40). Exit: parser or driver tests show: a `const` pointee gives
  E2913 at the third operand; `__has_builtin` is 1 for `__builtin_memset`,
  `__builtin_clz`, `__builtin_trap` and `__builtin_offsetof`; the
  `static int memset` example exits 0 or gets a dedicated code, never E3101;
  an ambiguous `v.x` gives E2906; a goto into a statement expression gives
  E2918, or the code is withdrawn; `pack` inside a record gives E0107; a cast
  between record tags gets a dedicated code; the E2203 and E2923 precedence
  is stated and tested.

- [ ] **T03u — Make the EvoIR validator match its rules** (A19 to A22,
  A41). Exit: `validate_module` or `parse_section9_module` rejects each of:
  the P14 module; `const.bits<8> 256`; `global_addr` with `ptr<stack>`;
  `lifetime.end` of a global; a pointer load with a `[size=4]` or
  `[align=16]` guard; a load with no guard; a `ptr.offset` with no
  `pointer_valid`; a token type nested in a `fnptr`; and reordered effect
  interfaces with `asm`, strict FP and `asm.goto`. Each negative test lives in
  `crates/evocc-evoir/tests`. `InvalidLifetimeEnd` reports E3018, and the two
  unit tests assert it. The E3062 tests assert the code.

- [ ] **T03v — Key persistent e-nodes by result sort** (A23). Exit: a test
  ingests `zext.bits<32> %x` and `zext.bits<64> %x` and asserts two classes.
  Pointer, `mem` and `world` values stay outside the persistent P0 classes.
  `day8_persistent_state_contract` and `verified_chain` stay green.

- [ ] **T03w — EvoIR errata for §4.4, §6.4, §6.5, §7.2, §8, §10.2, §10.3 and
  §11** (A26, A27, the text of A29, A30, A31, A42, A47, A51). Exit: one
  revision with updated pins.
  `rg -n "rolls the whole transaction back|saturate_scalar|extract_module|signed_add_associative" spec/evoir.md`
  finds nothing. §11.7 lists the shared `inst_results` and the renderer. The
  residual-risk paragraph states the campaign that exists.

- [ ] **T03x — EvoMIR and `target.md` errata** (A34, A48, A50, the text of
  A44 and A46). Exit: revisions with updated pins. The §5.1 guard rows say
  that native code relies on the C rule and checks no memory fault.
  `randomized_semantic_gate.rs` accepts only active Section 10 codes, and the
  EqSat bridge errors stop reusing E4001 and E4002.

- [ ] **T03y — `verification.md`, receipt binding and `spec/README.md`**
  (A33, A46, A49, A52). Exit: a check (in `tools/render-status.py --check` or
  a test) fails when `binding.spec_sha256` differs from the `spec/evoir.md`
  pin. The receipt is refreshed after the in-flux edits land. The README
  lists five documents. §2.1, §4.6 and §6 match the driver and Lean.

- [ ] **T03z — Missing negative-test matrices** (A29, A36, A43, A44, A45).
  Exit: these tests exist and pass. Seven refusal fixtures in
  `lean/EvoIR/Rewrite/Tests.lean`. Table-driven failure cases in
  `crates/evocc/tests/proof_required.rs`. `evoir-sem --list-stages` equals
  `STAGES`. Edge taint for `cond_br`, `br.indirect` and `asm.goto` in the
  checker. One traversal test over all 59 instruction variants for every
  hand-written traversal. The boundary scan covers `costs.rs`,
  `encodings.rs` and `patterns.rs`. E4004 and the alias-truth rule each have
  a test.

## Coverage map

TESTED counts clauses that a test asserts. In EvoIR §10.1 and §10.5 the
first count is "consistent": the code agrees, and the slice found tests for
most of these clauses but did not confirm each one.

### `spec/evoir.md`

| Section | Checked | TESTED | UNTESTED | CONTRADICTED | UNFALSIFIABLE |
| --- | --- | --- | --- | --- | --- |
| Status and scope, machine-read parts | 4 | 4 | 0 | 0 | 0 |
| §1 | 10 | 6 | 1 | 3 | 0 |
| §2 | 17 | 10 | 3 | 3 | 1 |
| §3 (not §3.13.1) | 68 | 50 | 9 | 9 | 0 |
| §4 | 18 | 13 | 0 | 3 | 2 |
| §5 | 22 | 18 | 2 | 2 | 0 |
| §6.1, §6.2, §6.4, §6.5 | 13 | 5 | 1 | 5 | 2 |
| §7 | 5 | 2 | 0 | 0 | 3 |
| §8 preamble and §8.1 to §8.4 | 7 | 3 | 0 | 2 | 2 |
| §8.5 to §8.12 | 41 | 31 | 4 | 3 | 3 |
| §9 | 40 | 31 | 3 | 6 | 0 |
| §10.1 (sampled) | 150 | 137 (consistent) | 3 | 9 | 1 |
| §10.2 to §10.4 | 22 | 13 | 5 | 1 | 3 |
| §10.5 | 24 | 20 (consistent) | 0 | 4 | 0 |
| §11 | 70 | 46 | 18 | 4 | 2 |
| Appendix A (rows) | 52 | 51 | 0 | 1 | 0 |
| Appendix B | 1 | 1 | 0 | 0 | 0 |
| **Total** | **564** | **441** | **49** | **55** | **19** |

Not checked: §3.13.1 and §6.3 (in flux), §10.1.8 (an example only) and
§10.1.20.

### `spec/evomir.md`

| Section | Checked | TESTED | UNTESTED | CONTRADICTED | UNFALSIFIABLE |
| --- | --- | --- | --- | --- | --- |
| Status table | 1 | 0 | 0 | 1 | 0 |
| §1 | 17 | 16 | 0 | 1 | 0 |
| §2 | 14 | 14 | 0 | 0 | 0 |
| §3 | 8 | 8 | 0 | 0 | 0 |
| §4 | 16 | 14 | 0 | 2 | 0 |
| §5 | 17 | 14 | 1 | 2 | 0 |
| §6 | 20 | 14 | 0 | 6 | 0 |
| §7 | 10 | 9 | 0 | 1 | 0 |
| §8 | 14 | 11 | 1 | 2 | 0 |
| §9.1 | 3 | 2 | 0 | 1 | 0 |
| §9.2 | 9 | 4 | 2 | 2 | 1 |
| §9.3 | 8 | 6 | 1 | 1 | 0 |
| §9.4 | 4 | 2 | 0 | 1 | 1 |
| §10 | 18 | 12 | 2 | 3 | 1 |
| Appendix A.2 | 3 | 3 | 0 | 0 | 0 |
| **Total** | **162** | **129** | **7** | **23** | **3** |

Not checked: the text-format limits of Appendix A (in flux). About 12 of the
40 rows of the §5.1 table were sampled. §2.6 (strict floating point) and
§6.12 (the Apple variadic rules, open as D05) were checked lightly.

### `spec/target.md`

| Section | Checked | TESTED | UNTESTED | CONTRADICTED | UNFALSIFIABLE |
| --- | --- | --- | --- | --- | --- |
| §2 | 8 | 6 | 2 | 0 | 0 |
| §3 | 7 | 5 | 1 | 1 | 0 |
| §4 | 6 | 3 | 0 | 2 | 1 |
| §5 | 9 | 3 | 3 | 3 | 0 |
| Appendix A | 2 | 2 | 0 | 0 | 0 |
| **Total** | **32** | **19** | **6** | **6** | **1** |

Not checked: §1 and §6 (non-goals).

### `spec/verification.md`

| Section | Checked | TESTED | UNTESTED | CONTRADICTED | UNFALSIFIABLE |
| --- | --- | --- | --- | --- | --- |
| §1 | 3 | 3 | 0 | 0 | 0 |
| §2.1 to §2.3 | 19 | 11 | 5 | 3 | 0 |
| §3.1 to §3.3 | 12 | 11 | 0 | 0 | 1 |
| §4.1 to §4.6 | 29 | 24 | 4 | 1 | 0 |
| §5.1 to §5.4 | 18 | 16 | 2 | 0 | 0 |
| §6 | 4 | 3 | 0 | 0 | 1 |
| §7.1 to §7.7 | 36 | 22 | 10 | 4 | 0 |
| §8 | 8 | 7 | 1 | 0 | 0 |
| §9 | 18 | 12 | 6 | 0 | 0 |
| §10 | 5 | 4 | 0 | 0 | 1 |
| **Total** | **152** | **113** | **28** | **8** | **3** |

§11 is a change procedure and was not counted. The §5.4 receipt binding is
stale (A33) although the slice counted the §5.4 clauses as tested.

### `spec/diagnostics.md` (in flux) and `spec/README.md`

| Document | Checked | TESTED | UNTESTED | CONTRADICTED | UNFALSIFIABLE |
| --- | --- | --- | --- | --- | --- |
| `spec/diagnostics.md` | 20 | 17 | 1 | 1 | 1 |
| `spec/README.md` | 6 | 3 | 2 | 1 | 0 |

One line each for the parts in flux:

- D01: at `546fe46a`, E2911 still names two diagnostics
  (`crates/evocc-parser/src/floating.rs:145`, `:166`).
- D02: at `546fe46a`, `epoch 0` still passes
  (`crates/evocc-proof/src/evp1.rs:407-409`,
  `crates/evocc-proofcheck/src/lib.rs:168`).
- D03: at `546fe46a`, `crates/evocc-mir/src/text_parse.rs` still has no
  production for the `spill.*` and `arg.*` forms.
- `diagnostics.md` §2: an EvoMIR refusal sits on `<command-line>` (A12), and
  a command-line diagnostic covers the whole invocation instead of the
  affected argument (`crates/evocc/src/main.rs:1021`).
- `diagnostics.md` §1: E0001, E0003 and E0004 have no §6.3 row and no
  explain page.
- EvoIR §6.3: the E2932 and E2943 messages carry an `evoir-lowering-failed:`
  prefix, and E3101 wraps the E3041 validation failure.

### All files

936 clauses checked: 722 TESTED or consistent, 93 UNTESTED, 94
CONTRADICTED and 27 UNFALSIFIABLE.

## Not covered

- No test was run, because Cargo and Lake were out of scope. TESTED means
  that a test exists that asserts the clause.
- No Linux binary ran. Linux claims rest on the code and on the emitted
  assembly. A01 was run natively on Darwin only; the Linux assembly shows the
  same missing save.
- No GCC binary ran. The GCC behaviour in A07 comes from the GCC manual
  ("Designated Inits").
- The checker binary did not run, so A09 rests on code reading.
- EvoIR §10.1 was sampled: 150 clauses in about 2,400 lines. Not covered:
  GNU inline roles, optional inlining, `transparent_union`, `cleanup`,
  volatile bit-field traces and `evo fix --apply` rollback.
- EvoIR §3.13 was sampled. NaN payloads, rounding and binary128 were not
  compared with the code.
- `verification.md`: not every `checkWellFormed` rule or JSON reader field;
  the numbers in the `tools/proof-native/` receipts were not recomputed.
- The isa-semantics emulator lane and the kernel-profile features beyond the
  Darwin refusals were not exercised.

## Probes

Each probe used the frozen compiler as `evocc`. The sources are short, so
they are given in full.

- **P1** (A01). `p1.c`: `void clobber(void){__asm__ volatile("mov x19, #5":::"x19");}`.
  `p1main.c` holds 42 in `x19` with `register long v __asm__("x19")`, calls
  `clobber()` and returns `v`. With `evocc -c p1.c` and `clang -O1`, the
  program exits 5. With a clang-compiled `p1.c` it exits 42. The assembly for
  both targets has no save of `x19`. A clobber of `d8` gets
  `str d8`/`ldr d8`, but a clobber of `x20` gets no save.
- **P2** (A02). `int takesc(signed char c);` and
  `int callsc(int x){return takesc((signed char)x);}` compiled by `evocc`
  emit `uxtb w0, w0` before `bl _takesc`. With the clang callee
  `return c + 10;`, `callsc(-1)` returns 265. A clang-only build returns 9.
- **P3** (A02). `signed char m1(void){return -1;}` compiled by `evocc` ends
  with `uxtb w0, w0`. A clang caller computes `m1() + 10` as 265, not 9.
- **P4** (A03). `evocc` compiles a call of
  `float tenth(float a, …, float j)` with arguments 1 to 10 as
  `str s16, [sp, #0]` and `str s17, [sp, #8]`. The clang callee returns
  `1.4013e-45`, not 10. No E4010 is reported.
- **P5** (A04). For `long ext(long a, __int128 b); ext(1, 2)`, `evocc` puts
  `b` in `x2:x3` and clears `x1`. Apple clang puts it in `x1:x2`.
- **P6** (A05). A program that prints `sizeof(long double)`,
  `__SIZEOF_LONG_DOUBLE__`, `sizeof(struct s)` and `offsetof(struct s, x)`
  for `struct s {char c; long double d; int x;}` prints 16, 8, 48, 32 under
  `evocc` and 8, 8, 24, 16 under clang.
- **P7** (A06). `_Atomic int g = 5; int ld(void){return g;}` gives EvoIR
  `load ... : bits<32>` with `atomic=false` and assembly `ldr w0, [x0, #0]`.
  The assignment `g = 6;` gives E2702.
- **P8** (A07). `int n; int f(void){return ++n;}` and
  `int main(void){int b[4]={[0 ... 2]=f()}; return n*10+b[2];}` exit 33, so
  `f` ran three times. Clang refuses the range designator.
- **P9** (A08). `struct e {}; long f(struct e *a, struct e *b){return a-b;}`
  with `main` passing `&x[1]` and `&x[0]` of `struct e x[2]`. The EvoIR is
  `sdiv.bits<64> %v7, %v4` with `%v4 = const.bits<64> 0` and no guard.
  `--check` and `--interpret-evoir` report E3002. `--interpret-c-hir`
  panics with "attempt to calculate the remainder with a divisor of zero"
  and exits 101. The native Darwin build exits 0.
- **P10** (A12). `typedef unsigned char v4u8 __attribute__((vector_size(4)));`
  with one function that takes and returns `v4u8` and one function that only
  uses it locally. `-S` prints `error[E4002]: evomir-lowering-refused` at
  `<command-line>:1:1` and writes no file.
- **P11** (A15). `struct s {char c; long x;}` with `-S -fpack-struct` and
  `--target=aarch64-unknown-linux-gnu` warns "is not supported and has no
  effect", exits 0 and keeps `sizeof` 16. `-mabi=ilp32` behaves the same.
- **P12** (A16). `long diff(int *a, int *b){return a-b;}` with
  `diff(&x[3], &y[0])` for two arrays. `--check` exits 0 and prints "No
  undefined behavior was found on the executed path". `--interpret-c-hir`
  reports E3007 "pointer subtraction across distinct objects".
- **P13** (A17). `const int r = 0; int o = __builtin_add_overflow(1, 2, &r);`
  passes `-fsyntax-only`, and `--check` returns 30 with no finding. Clang
  refuses the call.
- **P14** (A19). `__thread int t = 7; extern int a __attribute__((alias("t")));`
  with `int *p = &a;` and a reader of `*p + a`. The EvoIR has
  `relocs=[(0, @a, 0)]`, `alias @a = @t` and `global_addr @a`. The assembly
  has `.type a, %object`, `.set a, t` and `adrp x1, :got:a`.
- **P15** (A34). `int f(int *p, int i){return p[i];}` gives `pointer_valid`
  and `access_in_bounds` with no `ub.assume`, and the assembly has no check.
- **P16** (E4006, E4008). `long diff(int *a, int *b){return a-b;}` gives
  `ptr.to_int`, `sub.bits<64>` and `sdiv.bits<64>` by 4. A computed goto
  over `static void *t[] = {&&l1, &&l2}` gives `br.indirect` with both edges
  and the assembly `br x0`.
