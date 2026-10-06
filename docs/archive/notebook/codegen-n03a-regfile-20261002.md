# N03a step 1: generated register facts

Implementation: `42a003d2`. [Plan](../../tests/contracts/codegen-n03a-20261002.json);
[receipt](../../tests/contracts/codegen-n03a-regfile-20261002-receipt.json).
The canonical `targets/aarch64/regfile` now supplies register banks, views,
ordered pools, reserved roles and assembly register primitives. Allocation
policy, verifier bank admission, ABI/frame layout and interpreter semantics
are unchanged. The existing changed/full gate checks deterministic generation.

| Check | Observed result |
| --- | --- |
| RED | Missing description/interface and generator; existing gate failed to reject stale generation |
| Focused Rust / Python | 43/43 in seven Rust suites; 24/24 Python |
| Clippy / format / regeneration | PASS on the final implementation |
| Six original TUs, A checked/unchecked | 12/12 complete objects identical to the original baseline |
| Six original TUs, B checked/unchecked | 12/12 complete objects identical to the original baseline |
| Darwin original corpus | 384/384 executed and agreed; all object twins and original-byte comparisons identical |
| Sealed Linux original corpus | 384/384 executed and agreed; all object twins and original-byte comparisons identical |
| Retained g0 IR control | 12/12 object twins and original-byte comparisons identical; 10/10 standalone certificate replays |
| Full workspace | Pending final N03a gate; the preparation Clippy failure remains recorded, not relabeled PASS |

All measured target-object size deltas are **zero**. A/B executions retain the
existing reference dependencies and are not full-application acceptance.
The g0 check is compatibility replay of original triples with the current
checker, not re-admission of historical chains under a substituted identity.
One build invocation named the wrong package for `evo-ir-asm`; the corrected
`evocc-mir` build and actual capture succeeded, with both logs retained.

**Claim ceiling:** register-fact extraction only. Instruction descriptors,
generic machine algorithms and synthetic ISA, ABI extraction, and final full
workspace acceptance remain open. No P0/g0 or performance gain is claimed.

**Disk:** candidate objects, assembly and native scratch were removed; original
baseline objects remain necessary for steps 2–3. Accounted shared-target growth
since the preparation checkpoint is +283,303,936 bytes; compact captures add
857,245 bytes (accounted total +284,161,181 bytes, excluding small source/log
changes). Free space remains about 149 GiB.
