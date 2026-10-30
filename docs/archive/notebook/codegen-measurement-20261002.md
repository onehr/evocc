# N05: bounded measurement automation accepted

Implementation `449d589e`; [plan](../../../tests/contracts/codegen-n05-20261002.json)
frozen in `626f29f8`, [live plan](../../../tests/contracts/codegen-n05-live-20261002.json)
frozen in `449d589e`. [Compact receipt](../../../tests/contracts/codegen-n05-20261002-receipt.json).
Full workspace: 377 suites, 2,915 PASS / 0 FAIL / 4 ignored; Clippy/fmt PASS.
Shared Python: 135 PASS. No Lean or registry change.

| TU | Checked bytes | Unchecked bytes | Unchecked / GCC14-O2 | Compile median ms, checked / unchecked |
| --- | ---: | ---: | ---: | ---: |
| integer-mix | 128 | 128 | 1.333 | 20.57 / 18.94 |
| deflate | 30,184 | 28,668 | 1.986 | 635.83 / 595.61 |
| inflate | 28,264 | 26,520 | 2.668 | 642.37 / 622.39 |
| zstd compress | 122,972 | 120,592 | 2.473 | 2788.55 / 2776.47 |
| zstd decompress | 33,300 | 33,052 | 2.281 | 1098.42 / 1088.38 |
| cJSON | 29,496 | 29,384 | 2.041 | 542.36 / 526.00 |

Fresh original C compiled 36 times under the release compiler, not old IR.
All 12 object pairs are byte-identical; independent bundle replay has zero
completed rejections and zero bundle failures. Six bounded mixed-link modes,
with retained reference-built dependencies, passed in both profiles (12 runs).
The 257 unchanged EvoIR measurements total 52,825 steps before/after: 70 are
TV-admitted, 187 opaque. Integer-mix still records **MISMATCH_NOT_VALIDATED**
(1,001 versus 1,002 volatile events); its 11,017 steps are measurement-only.
This is the first live trend observation, not a cross-revision compile-cost
bound or improvement. Five library TUs still miss the 1.4 target.

The actual Git/ELF injection produced 8 -> 24 -> 24 bytes with identical twins;
receipt-first bisect identified `ff5b17a0`, without switching the checkout.
It is a detector/localizer test, not an EvoCC application result. The local
pre-push hook is now configured. Its real entry point accepted simulated update
stdin for `449d589e` with `OBSERVED`, no missing commits and no remote push.
It measures missing clean HEAD; missing historical implementation receipts
block as NOT_RUN rather than silently checking out another revision.

Raw capture: `.evidence/codegen-trend/measure-6lgm77ye/receipt.json`;
append-only numbers/hashes: `.evidence/codegen-trend/history.jsonl`.
Use `python3 tools/render-status.py --codegen-trend` with that history path.
Object/assembly/executable/proof intermediates were removed after measurement
and replay. **Disk net:** live retained capture +395,008 bytes (448 KiB allocated),
plus 80 KiB allocated injection capture and small source/record files; shared
Cargo gate growth +4.12 GiB is recorded once in N01. Host free-space changes
are not attributed to this slice. Capture cap remains 500 MiB.

**Claim ceiling:** current-source selected-TU measurement, bounded reference-
dependency replay, regression localization and local hook operation. Not full
applications, whole-module TV for opaque inputs, latency, improved codegen,
verified rewrite gain, or a claim that a remote push was performed.
