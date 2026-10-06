# N07 E3: negative SMT qualification

Implementation `98542c3f`; plan `81374b99`; RED `20671add`.
[Plan](../../../tests/contracts/codegen-n07-smt-20261003.json),
[receipt](../../../tests/contracts/codegen-n07-e3-20261003-receipt.json),
[rule inventory](../../../tests/contracts/codegen-n07-e3-verdicts-20261003.json).
**Closed negative: no SMT proof admitted.** Neither compiler lowering nor the
verbatim vendor model changed. All optimization defaults remain OFF.

| Frozen item | Result |
| --- | --- |
| Intended bodies | 81 integer op/width shapes plus eight constants |
| First body, `add/64` | Generator exit 1; no SMT query or solver answer |
| Normal constant-folding retry | Same unsupported builtin; exit 1 in 2.11 s |
| Other 88 bodies / negative solver controls | NOT RUN after qualification failure |
| Proof count / counterexamples | 0 / 0; a generator failure is neither UNSAT nor SAT |
| Inventory | 59 Inst families, 44 Section 5 rows, nine patterns, 28 specialized paths; explicit missing-model/blocked verdicts |
| Focused Python | 55 PASS / 0 FAIL |
| Full workspace | 394 suites; 3,032 PASS / 0 FAIL / 5 existing ignored; Clippy, format and regeneration PASS |

The pinned Sail 0.20.3 SMT backend reaches `integer.sail:733`:
`imm : bits('datasize) = undefined`, then reports
`No generator z#z#undefined_bitvector`. Its decoder dependency closure includes
that immediate-instruction function even for literal register-add opcode
`8b010000`. The ordinary `--Oconstant-fold` option does not remove the failure.
No undefined value was replaced, decoder rewritten or unsafe option enabled.
Qualifying another translator/model treatment is deferred, not silently added
to this slice's trust boundary. **N07-E3-D1:** pinned generator unsupported;
no compiler counterexample obtained. All wider/memory/effect/ABI and opt-in
selection domains remain NOT_MODELLED, not proved by E2 association.

The project generator avoids a separate observed overload-inference explosion
by using explicitly typed Boolean bindings for the same preservation condition.
A manual verbose diagnostic produced a 4,266,282,358-byte log before termination,
violating the 500 MiB capture limit. It was stopped, hashed and removed; the
incident receipt and causal excerpt remain. Subsequent commands use the existing
4 MiB-output/500 MiB-retention runner. This incident is **not PASS**.

E2's 768 native and 24 TU byte-identity results remain historical evidence.
No production source or release binary changed; fresh Linux/TU campaigns were
not run after failed qualification and are not credited to E3. No size benefit.
**Claim ceiling:** a reproducible tool limitation and explicit coverage inventory,
not B1/whole-backend correctness, solver qualification or N07 proof acceptance.

**Disk:** scoped retained increment **503,808 bytes**; shared frontend cache
unchanged at 24,640,536 KiB, about 64 GiB free. Excludes Git/tracked files and
unrelated volume activity. Retired 88 unexecuted property files / 468,449 bytes.
Keep the
minimal failure, compact logs and existing current E2 baselines/tool cache;
retire regenerable unused property files after recording hashes. No new VM disk,
target directory or Lean-cache change. Continue N03c spill quality, then N03d.
