# N03a step 2: generic machine algorithms

Implementation: `cb65735d` (operand facts), `a1717dc1` (SSA destruction),
`16cc9b2b` (allocation and verifier). [Plan](../../../tests/contracts/codegen-n03a-20261002.json);
[receipt](../../../tests/contracts/codegen-n03a-machine-20261002-receipt.json).
The canonical `targets/aarch64/insts` now generates one operand, edge, call,
flags, spill and DCE descriptor read by the shared out-of-SSA pass, linear scan,
spill placement, slot compaction and the independent allocation verifier. The
verifier keeps its own dataflow and refusal checks; selection, ABI/frame layout
and interpreter semantics are unchanged.

| Check | Observed result |
| --- | --- |
| Focused Rust / Python | 75/75 in eight Rust suites; 33/33 Python; focused Clippy, format and regeneration PASS |
| Synthetic ISA | SSA and ten allocation cases on a test-local instruction type, no AArch64 import |
| Six original TUs, A checked/unchecked | 12/12 object twins and original-byte comparisons identical |
| Six original TUs, B checked/unchecked | 12/12 object twins and original-byte comparisons identical |
| Darwin original corpus | 384/384 executed and agreed; 384 object twins and original-byte comparisons identical |
| Sealed Linux original corpus | 384/384 executed and agreed; 384 object twins and original-byte comparisons identical |
| Retained g0 IR control | 12/12 object twins and original-byte comparisons identical; 10/10 standalone certificate replays |
| Full workspace | Pending final N03a gate |

All measured target-object size deltas are **zero**; no proof rejection or
bundle failure occurred. The g0 control replays the original triples with the
current checker (`3670b9ec…`); it does not re-admit historical chains.

**Claim ceiling:** instruction facts and generic machine algorithms only. ABI
and frame extraction, the final boundary test and full workspace acceptance
remain open. No P0/g0 or performance gain is claimed.

**Disk:** candidate objects, assembly and native scratch were removed; original
baselines stay for step 3. Retained compact captures add 857,279 bytes.
