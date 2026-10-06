# Remaining-work evidence packet and batch execution contract

Prepared 2026-09-27 at source HEAD `73d5ba3ff2f4e1fac9f0b04bd17831b8f22c892f`.
This is **evidence preparation**, not feature implementation or a new full run.
All 39 unchecked TODO headings are indexed below; parent/child headings overlap
and are not 39 independent compiler defects. Six are later roadmap gates.
No TODO item is closed by this packet. No new large VM disk, clone or worktree
was created, and no full kernel/application campaign was started.

## Frozen subject and contents

| Artifact | Identity |
| --- | --- |
| EvoIR | 1.9.114, SHA-256 `5ece3b8f273a19e39871ed2f7e01761d8abc0e1319e4f12f41c26a82dd8cd5ec` |
| EvoMIR | 0.50, SHA-256 `6a0ffe0005b26d6eba71ffc78b609cda727a4da778c3a7e6fb5223332b387c4d` |
| Current tested compiler | `.build/floating-targeted/strict-20260927/evocc-asm-ssa`, SHA-256 `e850aacaffba056b0edac81416c604d725f3c77d367e21e5273ffb5d231deab5` |
| Target | AArch64 Linux, LP64, little-endian, GNU11 for new probes; original programs keep their recorded dialect/argv |
| Sealed image | `evo-flight-lab:sealed`, descriptor `sha256:2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2` |
| References | GCC 14.2.0-19, Clang 19.1.7; separate O0/O2 results |
| Runtime | Apple container 1.2.2, arm64 Linux guest, no network; not a kernel kselftest VM |

Raw commands, stdout, stderr, exit codes, source hashes and target versions are
in `.build/floating-targeted/closure-20260927/`. The bounded `capture.py` recipe
uses the existing `tools/ladder/mixed.py` container helper and existing sysroot.
It is not a new acceptance adapter. A portable allowlisted archive is produced
there as `evocc-remaining-evidence-20260927.zip`; it includes current specs,
selected semantic implementation files, tiny fixtures, raw JSON, original failing
record identities and the focused research assignment. It excludes compiler
binaries, sysroots, kernel trees, VM disks and full historical evidence trees.
The archive's `SHA256SUMS` covers its payload; its own hash is reported separately.

The [measured fixture record](../../tests/fixtures/remaining-boundaries-20260927.out)
is tracked in Git. Raw-output inventory/hash data remains in the small archive.
Archived original records are **historical replays**, not newly executed originals.
Their source hashes are rechecked when assembling the packet. The musl rows
include original compiler argv alongside the projected EvoCC argv and omissions.
Historical LLVM `native=PASS` on a refused unit can describe a reference-resident
mixed image: `programOutcome` and disposition govern the compiler result.

## Newly measured small matrix

37 cases × two optimization levels × C-HIR/EvoIR/assembly = **222 EvoCC commands**.
37 × GCC/Clang × O0/O2 = **148 reference compile-and-run records**. All reference
compiles succeed. Of the executions, **92 have explicit finite/defined-value
assertions and pass**; **56 are observations**, not automatically PASS because
they exit zero. An additional eight tiny-vector reference link/run controls
give four self-call passes and four crossed-call failures. Four emitted EvoCC
assemblies link/run: two widened-integer controls pass; two strlen masks are 1.

| Family | New evidence | What it does not establish |
| --- | --- | --- |
| Floating complex, f32/f64/f128 | 24 finite assertions pass in references; 36 special-value bit/status captures. Finite division is 5+i; large equal operands divide to 1+0i. Zero-divisor flag masks are 1 for f32, 3 for f64/f128 in both references | An approved complex arithmetic/fenv algorithm, NaN payload policy, crossed complex ABI, or EvoCC support |
| Half / bfloat | 24 reference assertions pass. `_Float16`/`__fp16` input 1.5 bits `3e00`, `__bf16` bits `3fc0`; result 2 bits `4000`. Unary expression size is 4 for `__fp16`, 2 for the others | Arbitrary rounding, NaN/subnormal behavior, mixed ABI or native hardware feature assumptions |
| Floating vectors | 16 reference assertions pass for float4/double2 arithmetic/masks, shuffle and float-to-int conversion | Full lane exception policy, ABI pressure, all NEON headers/builtins/intrinsics |
| Atomics | 24 reference assertions pass: C11 update, RMW/exchange, scalar/pointer CAS failure writeback, 8-byte generic load/store and 16-byte pointer-bearing record CAS | Concurrent ordering proof, arbitrary padding/unaligned access support or lock-free guarantees |
| strlen ICE | GCC mask 3/7 at O0/O2; Clang 7/7; EvoCC 1/1 in both interpreters and native | Authority to execute ordinary strlen during ICE evaluation |
| Integer-complex division | GCC 4+3i; Clang 5+i at both optimization levels | A complete overflow/truncation algorithm selected by one example |
| Pointer-VLA typeof | GCC side-effect counters 0; Clang 1; array controls both 1 | Authority to change all VLA evaluation/lifetime semantics |
| Ancestor return address | Framed reference probe: GCC exit 1, Clang 0. EvoCC rejects `-fno-omit-frame-pointer`; language-only control reaches E2913 | Native ancestor support, frame/PAC safety or an interpreter frame model |
| pr23135 | Defined widened product control passes all references, both interpreters and EvoCC native | A new original-program UB waiver; prior original/scalar UBSan evidence remains separately qualified |

The new EvoCC first-error map is intentionally retained rather than normalized
away: complex component-store forms report E1003, `__builtin_complex` E2001;
half/bfloat function signatures expose E3101; other half/vector paths report
E2202/E1003/E3007/E3050. Additional atomic builtins remain E2001 and `_Atomic`
E2702. L07/L08/L09 retain E2948/E2904/E2913. These are **reproduced boundaries**,
not evidence that every diagnostic denotes a distinct root cause. Fixing admission
will expose later stages; those stages require the acceptance matrix below.

## One decision packet, not a question after every patch

Already approved: strict non-trapping scalar FP/world/fenv, real binary128,
scalar f32/f64 `w` assembly, GNU inline, bounded source bytes, local SSA, object
provenance and existing scalar atomic accesses. Do not reopen these principles.

| TODO IDs | Decision still needed | Evidence / recommended review scope |
| --- | --- | --- |
| K07 | Bounded nonliteral strlen ICE classification | Static const arrays/pointers, offsets, termination, qualifiers, cycles, unevaluated contexts; ordinary function effects stay intact |
| L03 | Separate `_Float16`, `__fp16`, `__bf16` executable contracts | Use the measured type/promotion differences; reuse scalar engine/ABI infrastructure, audit unsupported encodings and feature requirements |
| L04 | Complex arithmetic, exceptional values, fenv and helper policy | Primary focused deep-research task below; 68 musl refusals make it the immediate coverage blocker |
| L05 | Lane arithmetic/conversion/status and intrinsic scope | Reuse admitted scalar rules; distinguish GNU FP vectors from target-header/NEON families |
| L06 | Tiny-vector public ABI | Clang-compatible mode is a candidate, not a claim of GCC interoperability; explicitly record four crossed failures |
| L07 | Integer-complex division algorithm | Recommend a complete bounded profile after auditing actual Clang/GCC source; no single-answer patch |
| L08 | Pointer-to-VLA typeof evaluation | Choose expression-category rules and keep captured bounds/object lifetimes explicit |
| L09 | Actual ancestor-address support | Specify frame, depth, PAC, optimization and reached-only unsupported cases; never manufacture a pointer |
| L10 | Evidence threshold for source UB | This is factual source analysis plus acceptance-policy review, **not a vote on whether UB exists**; preserve original results |
| L11a | General `_Atomic` types/access/update | Object/type semantics and default order, not merely another accepted spelling |
| L11b | Explicit RMW/CAS/exchange orders and width domain | Expected writeback, weak/strong, invalid/dynamic orders, pointer semantics, text/proof integrity |
| L11c | Generic aggregate atomic representation/runtime | Padding, initializedness, pointer provenance, alignment and helper ABI; deserves design audit, not a scalar-to-record cast |

The packet's proposals are not new normative approvals. Review these together;
continue independent implementation/diagnostics while they are reviewed. Seek
further input only for a genuine new conflict, missing authority or unavailable
external prerequisite—not for an ordinary engineering step or a passing fix.

## Complete open-task index and next executable gate

The table includes parents, subcases, final acceptance jobs and later milestones.
It does not flatten all of them into language bugs or promise every system-level
failure can be resolved by one compiler patch.

| IDs | Existing minimal evidence / next work | Closing gate |
| --- | --- | --- |
| K01, K01b | `instruction-publish-alias.c`, `instruction-publication-observer.out`, `qemu-instruction-publication.out`, static-key phase/module reductions. Generalize/prove the runtime invalidation diagnosis; link it causally to the original fault | Unchanged paired sched_ext sequence and declared production lane; private four-byte-store intervention alone is insufficient |
| K02, K03 | `kvm-demand-fault-profile.out`, `kvm-memslot-fault-profile.out`, `kvm-hotpath-original-replay.out`, transport and scalar-shape controls. Profile remaining repeated hot paths; avoid random optimizer edits | Original all-mode programs within existing 120-second budgets on final Images |
| K04 | `ublk-blocked-observer.out`, existing worker-failure reduction. Capture the intermittent blocked path; no timeout increase or success-retry attribution | Original stress and orderly shutdown with cause-backed repair/disposition |
| K06 | Existing paired IOPF resource/config controls; uniform rebuilt Image still pending | New subject's unchanged iommufd population; matching config/modules |
| K07 | `fortify-nonliteral-strlen-ice.c` plus new remeasurement | Approved bounded rule, positive/negative ICE and ordinary-call tests, complete original FORTIFY suite |
| K10 | Existing BPF bounded/original timeout records; collect the failing subtest/hot path | Full unchanged test_progs, not selected subtests or a larger watchdog |
| K11, K11f | Existing case-level ledger and `pidfd-recycle-progress.out`; reconcile outstanding/known shared outcomes | Every installed program accounted, pidfd original on final Image; classification does not turn FAIL into PASS |
| K12 | Existing collection guard/module-splice and bitmap 390447-check controls | Matching built modules/payloads and affected original collections after T04 |
| K13 | Final acceptance, no substitute small case | Entire frozen 970-program pair; all programs, inner verdicts, hashes, timeouts and shutdowns |
| K14 | Final acceptance, preserve duplicate identities/XFAIL | 1504 built-in cases and real loading of 31 modules/765 cases per matching kernel |
| B08, B08b | `floating-standard-evaluation.out`, `driver-symbol-sections.out`, `no-strict-aliasing.out`, current omission list | Complete option classification/equivalence tests and original commands; complex closure still separate |
| B08d | Current 17-case reference ledger: API, static/DSO, strptime, strtold, wordexp, powf, daemon, atfork | Case reductions plus a genuine shared-loader lane; no static DSO “PASS” |
| B10 | Final acceptance, existing common entry point | One frozen compiler, all 15 lanes serially, unchanged populations and five-way accounting |
| L03, L04, L05 | New half/complex/vector fixtures; selected original rows and source hashes | Approved semantics → both interpreters → native/mixed ABI/proof controls → unchanged affected originals |
| L06, L07, L08, L09 | Existing small ABI/division/typeof/ancestor cases freshly measured | Whole bounded rule and negatives, not self-consistency or one output value |
| L10 | `llvm-vector-overflow-reduction.out`: original GCC report, Clang instrumentation limit, both scalar reports, exact overflowing product | Approved source-bound disposition or compiler fix; keep full original and raw failure in B10 |
| L11a, L11b, L11c | `atomic-scope-boundaries.c`, new scalar/pointer/record CAS probes | Width/order/type/representation negatives, crossed native and concurrency controls, scoped interpreter claims |
| T02 | Existing KUnit adapter/verdict fault injections | Real K14 through the common entry point |
| T03 | Existing metadata mutations/errata; extend clause-to-test map for this batch | Parser/validator/effect/proof/cache audit including well-typed one-field tampering |
| T04, T04d | Bitmap and late-generator Make/C reductions already green | One uniform 7002-object composition against actual generated inputs; no late GCC replacements, matching signed modules and Image |
| T05 | Existing health gate, rerun after final semantic batch | Workspace/Clippy/fmt/Python/Ruff/layout/grammar/offline proof replay; explicit historical ignores |
| R01 | Later whole-kernel EqSat milestone, not a numerical fix | G2 reproducibility and certificates |
| R02 | Later performance milestone; current KVM slowdowns remain evidence | Fair locked GCC -O2 comparison, not only smaller assembly |
| R03 | Later measured self-improvement milestone | Q3 measurements under its roadmap contract |
| R04 | Later second backend milestone | Real x86-64 code generation and ABI, not AArch64 cross tests |
| R05 | Later release milestone | Reproducible bootstrap, release and proof-claim audit |
| R06 | Later Doom milestone | Pinned source/data/shim/input and deterministic output; not a generic “Doom works” claim |

## Next-round execution policy

```text
one frozen inventory + grouped semantic review
    |
    +-- all minimal REDs (reuse old ones; add newly exposed layers)
    +-- shared fixes + focused tests + bounded commits
    +-- affected-crate gates at coherent integration points
    +-- inexpensive affected-original object/program replays
    |
    `-- freeze final compiler/config/payload identities
          +-- uniform kernel build + module provenance
          +-- one complete paired kselftest + expanded KUnit
          +-- one serial application campaign + final health gate
                failure -> small reproducer -> repair -> affected revalidation
```

“All at once” means a sustained repair batch with no unnecessary approval or
whole-campaign barrier after each small fix. It does not mean one giant untested
commit, skipping crossed/native checks, or pretending the later roadmap is done.
Independent tasks continue while an external decision or long final run is pending.
Final acceptance subjects cannot inherit results from binaries built before a fix.
If a late repair changes a tested component, invalidate/rerun its affected final
gate; do not reuse stale results to preserve a nominal one-run limit.

## Focused research assignment

Use [the packet prompt](remaining-complex-research-prompt-20260927.md). Its main
task is L04; the other semantic questions are a single bounded decision annex.
Strict scalar FP, binary128 and scalar `w` assembly are implemented current
prerequisites, not invitations to restart their old research projects.
