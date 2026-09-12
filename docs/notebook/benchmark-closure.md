# Benchmark issue closure

Baseline: `e6c7938`; all-lane measurement `20260907T141450.311159Z`,
musl rerun `20260907T153648.272156Z`, compiler SHA-256
`b13d4a08539dfb6056e61f47631768357ced2e9f404247d7893593643c9ca6e6`.
The [evaluation](benchmark-evaluation.md) and its linked raw records are the
inventory authority. Counts below overlap where a row is a subcategory.

## Kernel boot blocker tracker (updated 2026-09-13)

This is the current kernel-specific index into the existing issue inventory,
not a second tracker or a new acceptance framework. The latest compilation
measurement is `survey-20260912T092730.260925Z`; the baseline failed
mixed-link attempt is `measure-20260910T160033Z`. Neither measured the later
`fb05c80` goto repair. The targeted replays below measure subsequent fixes;
they do not replace that full-survey denominator. Earlier reports remain history.

**Scope:** the pinned ARM64 configuration in `contracts/kernel-target.json`,
currently 2,799 target C compile records. K6 requires all required target C
objects from EvoCC, a complete link without reference-compiler C fallback,
and the real boot predicate. Existing assembly sources, allowed assembler and
linker tools, host build tools, generated files, and initramfs provenance must
be accounted for separately; 2,799 C records do not prove those paths. K7 adds
the selected runtime selftests and is not a prerequisite for reporting the
first K6 boot. Floating-point execution remains deferred, not an assumed
remaining dependency of this general-register-only kernel configuration.

The [15 compilation families](#full-kernel-compilation-survey-2026-09-12)
partition 161 first-error refusals, **not all defects**. Later constructs can
be hidden by the first error, and successful assembly says nothing about
runtime correctness. An allowed refusal in an intermediate compiler profile
still blocks K6 if the frozen kernel requires that unit; it cannot be waived
by shrinking the configuration or substituting a reference object.

Use stable IDs, an observed failure/reproducer, affected original units and
run, owning component, next tailored test, and closure evidence. `OPEN` means
work or diagnosis remains; `LOCAL_VERIFIED` means regression tests passed but
original affected programs have not all been replayed; `CLOSED` requires those
replays and the appropriate gate. An `UNMEASURED` milestone is not a known
compiler bug or a PASS. A newly observed failure gets its own ID or joins an
existing ID only after its source-level cause is established.

### Link, execution and integration work outside the compile-count table

The original link/boot failures now have passing isolated, five-object combined,
and eleven-object nVHE replays below. The full required-C
combination remains unmeasured after the repairs. The acknowledged QEMU digest drift remains in their receipts;
it is neither a waived failure nor evidence that the compiler caused a timeout.

| ID / priority | Observed blocker or unmeasured gate | Owner / next tailored check | Closure |
|---|---|---|---|
| K-R01 / P0 | Only `arch/arm64/kernel/cpuinfo.o` substituted formerly caused an Oops in `kfree` via `kobject_set_name_vargs` and `cpuid_cpu_online`. [Original capture](../../.evidence/k1/measure-20260910T160033Z/bisection-1/round-014/boot/boot.json). | C-HIR linkage: adjacent strings in `section(".data..percpu" "")` were decoded as a section name containing quotes/spaces. Fixed by `9d889fa`, RED `a7af1a6`. | Original single-unit relink **0**, boot **PASS**, installed object preserved: [replay](../../.evidence/k1/attributes-cpuinfo-20260912/boot/boot.json). Final combination still OPEN. |
| K-R02 / P0 | Only `arch/arm64/kernel/hw_breakpoint.o` substituted formerly timed out without useful serial output. [Original capture](../../.evidence/k1/measure-20260910T160033Z/bisection-2/round-014/boot/boot.json). | Same malformed per-CPU section as K-R01; shared string-fragment decoding fixed by `9d889fa`. No timeout change. | Original single-unit relink **0**, boot **PASS**, installed object preserved: [replay](../../.evidence/k1/attributes-hw-breakpoint-20260912/boot/boot.json). Final combination still OPEN. |
| K-L01 / P0 | `arch/arm64/kernel/irq.o` formerly failed linking with undefined `irq_shadow_call_stack_ptr`. [Original log](../../.evidence/k1/measure-20260910T160033Z/bisection-3/round-013/substitution/relink.log). | A no-argument internal inline configuration function returned false, but the unreachable reference survived. RED `bddc65f`; `a4009fd` exposes only proved constant leaf returns, retaining return conversions and effectful calls. | Original single-unit link **0**, boot **PASS**: [replay](../../.evidence/k1/constant-inline-irq-20260912/boot/boot.json); affected gate **1,425 passed / 0 failed / 3 existing ignored**, Clippy and fmt PASS. Full combination still OPEN. |
| K-L02 / original replay PASS | Large combination failed building nVHE `hyp-reloc.S`: `Unexpected RELA type 311`. [Link log](../../.evidence/k1/measure-20260910T160033Z/attempt-04/substitution/relink.log). | Kbuild replay dropped `-fno-PIE`; RED `144a83e`, repair `0719ce5` preserves explicit address modes and macros, weak GOT and default PIE. | HYPREL and nVHE sub-link passed in `nvhe-addresses-20260912`; the subsequent distinct K-L04 failure is below. Final eleven-object relink **0**, boot **PASS** in [the repaired replay](../../.evidence/k1/nvhe-configuration-20260912/boot/boot.json). Mode gate: **648/0**, Clippy/fmt; K1 Python **100/100**, Ruff. |
| K-L03 / original replay PASS | Earlier September 9 reductions isolated link failures in `debug-monitors.o` and `entry-common.o`. [Prior report](benchmark-evaluation.md#native-replay-on-33bc3d4). | Their old undefined references are absent with the current compiler; both original objects were included in the five-object combined check below. No duplicate compiler repair was needed. | Combined relink **0**, boot **PASS**, all five installed objects retained: [replay](../../.evidence/k1/known-failures-20260912/boot/boot.json). Full required-C combination still OPEN. |
| K-L04 / original replay PASS | After K-L02, nVHE linked with undefined `__kvm_nvhe_kvm_vcpu_apply_reg_masks`: an unreachable call survived `vcpu_has_nv(vcpu)`, which returns false in this build. [Original log](../../.evidence/k1/nvhe-addresses-20260912/relink.log). | RED `550ff80`, repair `29704aa`: use a proved internal inline return only after evaluating the real call and arguments; preserve effectful/unproved calls and source constant-expression rules. | Eleven original objects assembled, retained, relink **0**, boot **PASS**; affected gate **1,431 passed / 0 failed / 3 existing ignored**, Clippy/fmt. Final full composition remains OPEN. |
| K-B01 / original group link and boot PASS | Kbuild replaced `alternative.o`, `signal32.o`, `vdso.o` after regenerating `vdso-offsets.h`. | RED `39e48d7`; prepare, recompile the selected objects against the actual build headers, install, verify content stability, then link. [Five-object replay](../../.evidence/k1/prepared-headers-20260912-v2/substitution.json): no overwritten object; the three consumers are byte-proven through vmlinux (allowing relocations). | [Boot PASS](../../.evidence/k1/prepared-headers-20260912-v2/boot/boot.json), Image `af053a63...`; this closes the reproduced three-object overwrite, not the unmeasured full-composition lineage audit. |
| K-B02 / P1 | Current survey projects `.o.cmd` arguments and records omitted flags; it is not a complete `CC=evocc` Kbuild invocation. | Driver/build integration: audit omitted options by semantics, ABI, instrumentation and safe no-op; cover generated/header probes and assembler-with-cpp. Record HOSTCC/init ownership explicitly instead of inferring it from target C coverage. | Every build-required option/path is implemented or justified under the frozen target, with no silent semantic omission or target-C fallback. |
| K-G01 / K6 | `kernel-full` adapter is `NOT_IMPLEMENTED`; complete EvoCC composition/link/boot is UNMEASURED. | Extend the existing bench/K1 path, not a parallel runner: require all target C inputs, exact object lineage, complete link and the contract's nonce-bound ordered boot/shutdown predicate. | Same measured compiler/configuration, all required C objects, linked Image and real passing boot; no bisection exclusions. |
| K-G02 / K7 | `kernel-selftest` adapter is `NOT_IMPLEMENTED`. | After K6, declare the selected KUnit/kselftest cases/configuration and reuse the boot lane. Report reference failures/unavailable cases separately. | Execute the selected tests in the measured kernel and publish their actual results, not only a boot marker. |

### Execution order and update rules

Targeted combined replay `known-failures-20260912` uses the same release
SHA-256 `cd12aa10d326d3838fd5f18aee40d641678053d56fb4e09c6e6392553dc2a54b`
for `cpuinfo.o`, `hw_breakpoint.o`, `irq.o`, `debug-monitors.o`, and
`entry-common.o`. All five substituted objects survived Kbuild unchanged,
the link exited zero, and the unchanged boot predicate passed for Image
`23d5eb829ed4b336e198eea82d82fc65f9d8844628b567535aed5ddd39c9db28`.
This verifies the interaction of those five fixes/rechecks, not a 2,799-object
boot or K6: every other target C object in this trial remains reference-built.

The separate eleven-object nVHE replay `nvhe-configuration-20260912` uses
`29704aa`'s release SHA-256
`fbc920dd292a9e6567320f301b6391ff2ad615244fc79225e822051175f62cbf`.
Seven unchanged assembly outputs reuse digest-checked objects; all eleven
installed objects are retained through the successful link and nonce-bound
boot of Image `81ddd63fba5c012aadd181217a07ff3686c663b00055df633022544e23640e4c`.
No timeout was changed. This is an independent targeted composition, not a
claim that all previous substitutions or all required C objects were combined.

1. Keep the original 78-unit `K-C01` compile/assembly replay separate from
   runtime closure. Diagnose the remaining P0 link failures above before claiming
   that closing compile refusals is sufficient. Small independent static-address
   fixes (`K-C02/03`) can proceed without another full survey.
2. RED then GREEN per established source-level cause; use the existing test
   targets and `tools/gate-changed.sh`. Address the remaining `K-C*` families
   and `K-B*` integration work; update the relevant row with commit and measured
   test/replay, not just a prose progress entry.
3. After the known repair batch passes tailored checks, use `tools/bench.py`
   for the full compilation/link/boot measurement and required regression
   benchmarks. Cache exact unchanged objects, never a changed composition's
   boot result. Every new failure reopens or adds an issue; do not claim the
   known list guarantees boot success.
4. `docs/status.md` remains receipt-rendered measurement only. This notebook
   owns issue status; `docs/roadmap.md` owns milestone definitions and links
   here. Other benchmark issues remain in the existing general inventory:
   VLA, integer vectors, hosted-library coverage or missing adapters are not
   automatically kernel boot prerequisites without a source/build dependency.

## Execution and closure rules

### Fast kernel survey first (2026-09-12)

The latest request authorizes one complete kernel compilation survey before
resuming the tailored-first compiler repairs below. It does not authorize a
new all-benchmark campaign or turn cached execution into a fresh boot.

1. Preserve the user's completed `20260910T160033Z` run: 2,638 assembled and
   161 refused out of 2,799; its mixed link still failed. Reuse its current
   release binary, not an unnecessary compiler rebuild.
2. Add a survey-only option to `tools/bench.py kernel`, using the existing
   per-unit runner, denominator and watchdog. Publish compilation outcomes
   separately from linking/boot, which remain NOT_RUN in this mode.
3. RED-first, reuse an assembled object only for identical assembly, assembler
   arguments and immutable toolchain identity, with the object's digest checked.
   Assembly containing external includes/incbin stays fresh. Preserve unchanged
   installed objects; do not build a new runner or a boot-verdict cache.
4. Invalidate the incomplete source-only cache: include the source/reference
   input snapshot, including headers, generated files and predefined-date
   inputs, in frontend reuse.
   Record fresh/cache counts and time per step. Use six compiler workers on
   the otherwise idle 12-core/64-GiB host, and only one heavyweight run.
5. Run the complete survey, seal its compact per-unit report and rank first
   errors by source construct. Resume compiler RED/GREEN on the largest shared
   in-profile defect, then the remaining VLA/ABI/control-flow/type inventory.
   Keep all original failures and floating-point deferrals visible.

The existing dirty bench receipt and rendered status came from the user's
run and must be preserved as historical measurements, not reverted. The
19-GiB free-space check rules out making another kernel tree or archiving
duplicate assembly for every trial. Full link/boot is a later explicit step;
unchanged individual objects never justify skipping a changed combination.

Except for the explicitly requested survey above, the 2026-09-10 instruction
supersedes per-fix whole-lane measurements. Each
semantic family gets a minimal failing regression, a shared-path fix and
tailored validation of its affected consumers. Full survey/benchmark runs
wait until every currently identified in-profile defect is repaired and its
tailored tests pass. A focused PASS is recorded as locally verified, not as
an original-program or milestone closure.
Keep unavailable references, unsupported programs and baseline failures visible.
No compiler changes are justified merely by reducing a diagnostic count.
Do not rerun whole lanes after every patch. After tailored validation, run
the complete gate, isolated replay and a serial campaign through
`python3 tools/bench.py`; classify every new failure and repeat the same
repair loop until the reviewed completion conditions hold. Floating-point
execution remains deferred by the user's
standing instruction; FP-dependent milestones cannot be called complete.

Order is by correctness first, then shared-path reach. Small independent
wrong-result fixes precede changes that require new machine constraint plumbing.
Only one heavyweight benchmark invocation runs at once.

Current tailored builds use `CARGO_TARGET_DIR=target/tailored`, with unchanged
debug/test profiles and `--locked --offline -j 1`. On 2026-09-10 the previous
`target/frontend/debug/deps` contained 1,097,395 files; a one-second rustc
sample showed dependency-directory enumeration/sorting, not a hung interpreter.
The smaller five-test build took 15m27s there, while the fresh 75-test build
completed in 28.61s. The old cache is untouched; no runner or timeout change
was made, and these different commands are not a normalized speed benchmark.

The user's 2026-09-10 follow-up authorizes implementing the PM-decided rules
from the sealed probes without waiting for the missing body revisions.
Record differences in Appendix D only; do not rewrite specification-body
sections. The previously body-pending items below are now implementation
work, not blockers. Probe disagreement still requires an explicit refused
shape, not a guessed result or a removed runtime check.

On 2026-09-12 the user separately authorized the standard byte-aligned static
data relocation rule explained for K-C12. Revision 1.9.91 corrects that
specific Section 2.1 body restriction; this is not general authority to
rewrite unrelated rules or weaken executed access checks.

### Full kernel compilation survey (2026-09-12)

Command: `python3 tools/bench.py kernel --survey-only --evocc
/Users/onehrxn/dev/30days/c30/target/frontend/release/evocc --jobs 6`.
Run `survey-20260912T092730.260925Z`; [capture summary](../../.evidence/k1/survey-20260912T092730.260925Z/substitution.json),
[per-unit results](../../.evidence/k1/survey-20260912T092730.260925Z/units.jsonl),
and [bench receipt](../../tests/contracts/bench-receipt.json).
The measured checkout was `8f6cf84` with pre-existing receipt/status edits;
the frozen driver SHA-256 was
`73de74e803e3f0b2e5a22d624b25b06e7254a4e7e9c44855ffe403a7f357ef00`.
The later compiler repair below is deliberately not included in these numbers.

All 2,799 original identities were checked: **2,638 assembled, 161 refused**,
zero timeouts, zero crashes and zero assembler failures. All refusals exited
with code 1 and a typed diagnostic. Defined-symbol comparison reported no
missing/extra symbol sets. Recounting `units.jsonl` reproduced the summary,
and its SHA-256 is
`ce46e04aa698251cd23c01aa1032a1d242e6a8f3dfda9dc19481626d342528d5`.
Comparison with `measure-20260910T160033Z/survey` found the same driver and
zero per-unit outcome changes, not merely equal totals.

Cold-run elapsed time was **1,800.786 seconds (30m01s)**: input snapshot
29.776 s, compilation 1,657.428 s, assembly 51.233 s and symbol comparison
42.325 s; the total also includes the final input/binary verification.
Per-unit compiler wall time: median 2.929 s, maximum 38.349 s
(`arch/arm64/kvm/inject_fault.o`), with six workers and the unchanged
180-second limit. A single-job tailored compiler build and affected-crate
tests overlapped part of this run; this is not an isolated throughput benchmark.
The earlier 7,481.7-second measurement also linked and bisected failures, so
dividing those totals would not establish a compiler speedup.

Frontend/object cache hits were **0/0**: the old incomplete input key was
invalidated and the object cache was populated for the first time. A subsequent
single-object check of `lib/bitrev.o` reused the real object in 0.005 s with
unchanged digest and timestamp; the 114 Python tests also exercise invalidation.
The date-input guard landed after this survey process started; this fresh run
crossed no UTC date boundary, and the stronger key conservatively misses its
frontend cache on the next invocation. This capture adds approximately 4.4 MiB
of compact records; emitted assembly and objects remain in the existing cache,
not copied into another evidence tree.

No objects were installed, no kernel was linked or booted, and no KUnit or
kselftest ran. The common runner therefore correctly returns `INCOMPLETE`
(exit 1) with `testVerdict=NOT_RUN`; the survey subprocess itself exited 0.
No old successful mixed-kernel receipt was promoted to this composition.

First-error families below partition the 161 refused units. Function/block
names in validator messages are grouped by the violated rule; counts are
units, not diagnostic occurrences or predictions of the next run.

| ID / state | Units | Code | First-error family | Example unit | Next tailored check / owner |
|---|---:|---|---|---|---|
| K-C01 / original compile replay PASS | 78 | E3101 | Goto destination discarded; `fb05c80` | `arch/arm64/kernel/signal.o` | All 78 affected originals compile/assemble in `initializer-and-goto-20260912`; runtime/full composition remains separate. |
| K-C02 / original compile replay PASS | 29 | E3026 | Static address/relocation | `drivers/base/dd.o` | `d387e65` passed 27 originals; `29704aa` passes the final `drivers/tty/sysrq.o` and `net/core/neighbour.o` in [the two-object replay](../../.evidence/k1/absolute-place-addresses-20260912/substitution.json), with no access provenance invented. Combined runtime remains separate. |
| K-C03 / original compile replay PASS | 21 | E3101 | Integer-valued static initializer not folded | `drivers/acpi/acpi_apd.o` | `d387e65`: all 21 affected originals compile/assemble; both interpreters and native fixtures pass, with static exposure and a non-exposure negative case. |
| K-C04 / original compile replay PASS | 6 | E3021 | Asm `i` operand | `net/ipv4/arp.o` | RED `f312954`: the netfilter `__builtin_constant_p` guard was false, but lowering still visited its short-circuited right operand. Evaluate the left operand and retain its obligations, omit only a skipped right operand with no incoming label edge. All six originals assemble and match symbols in [the replay](../../.evidence/k1/guarded-immediates-20260912/substitution.json). No immediate constraint is weakened. |
| K-C05 / original compile replay PASS | 5 | E3101 | Aggregate value in scalar lowering | `arch/arm64/kvm/hyp/nvhe/ffa.nvhe.o` | RED `fabe30a`: register operands transfer the complete four-/eight-byte object representation using ordinary guarded loads/stores and existing assembly effects. Both oracle and subject native fixtures pass; all five originals assemble and match symbols in `aggregate-and-case-20260912`. |
| K-C06 / original compile replay PASS | 5 | E3101 | Zero pointer-offset scale | `fs/super.o` | RED `7f152a4`, fix `7e79813`: both interpreters preserve evaluated indices, pointer identity and E3001. Both oracles O0/O2 and subject native fixtures agree; gate **1,433/0**, Clippy/fmt PASS. All five original units assemble and match symbols in [the replay](../../.evidence/k1/zero-sized-pointer-offsets-20260912/substitution.json). |
| K-C07 / OPEN | 4 | E2939 | Wide bit-field promotion | `drivers/irqchip/irq-gic-v2m.o` | Parser/C-HIR: probe the exact promotion/type-query contexts; retain documented conflict boundaries rather than waiving units. |
| K-C08 / original compile replay PASS | 2 | E2202 | EFI wide-character initializer type | `drivers/firmware/efi/libstub/file.o` | Replay omitted supported `-fshort-wchar`; RED `6c11a5c`, fix `54d265a`. Both original EFI units assemble and match symbols in [the replay](../../.evidence/k1/efi-characters-20260912/substitution.json); K1 Python **101/101**, Ruff PASS. No parser acceptance rule changed. |
| K-C09 / original compile replay PASS | 2 | E3021 | `+r@x30` register binding | `arch/arm64/kernel/process.o` | RED `67ad4cf`: pin x30 only at explicit asm operands; ordinary allocation still excludes it and every prologue/epilogue saves/restores the return address. Four sealed O0/O2 runs and native read-write/tied/asm-goto/call tests pass. Both original units assemble and match symbols in [the replay](../../.evidence/k1/link-register-20260912/substitution.json). |
| K-C10 / original compile replay PASS | 2 | E3030 | Aggregate varargs | `net/9p/client.o` | RED `2d375a0`; ordinary anonymous operands carry byval shape and aggregate va_arg copies a list entry into a destination object. Both interpreters pass; eight GCC/Clang O0/O2 mixed-call directions pass. Both originals assemble with symbol parity in [the replay](../../.evidence/k1/aggregate-variadic-20260912/substitution.json), compiler `ac5eaf9f...`; no install/boot claim. |
| K-C11 / original compile replay PASS | 2 | E3034 | Function-pointer arithmetic | `arch/arm64/net/bpf_jit_comp.o` | Byte arithmetic uses the existing exposed-address conversions; reached calls still check target/signature. Both BPF originals compile/assemble with symbol parity in `function-addresses-20260913`. |
| K-C12 / original compile replay PASS | 2 | E3101 | Packed pointer relocations at byte offsets 2 and 12 | `drivers/acpi/acpica/exdump.o` | RED `3d8f6c1`; user-authorized Section 2.1 correction in revision 1.9.91. Validator and shared memory separate pointer width from access alignment. Both interpreters, sealed oracles and subject native fixtures pass; both original ACPICA units assemble and match symbols in [the replay](../../.evidence/k1/packed-relocations-20260912/substitution.json). Bounds, nonoverlap and provenance remain checked. |
| K-C13 / original compile replay PASS | 1 | E3029 → E3034 → assembled | Computed goto | `kernel/bpf/core.o` | Explicit storage/effect edges pass C-HIR, EvoIR, MIR, proof replay and native tests. After the shared function-address fix, the original compiles/assembles with symbol parity in `function-addresses-20260913`. |
| K-C14 / original compile replay PASS | 1 | E3101 | Numeric pointer round trip in a case constant | `kernel/signal.o` | RED `fabe30a`: permit numeric pointer casts in the shared iterative static evaluator, preserving its fuel and forbidden-expression checks. Both interpreters/oracles/native fixtures pass; the original assembles and matches symbols in `aggregate-and-case-20260912`. |
| K-C15 / OPEN | 1 | E3101 | Volatile union copy | `kernel/rcu/update.o` | C-HIR/lowering: reduce whole-union access, preserve copied bits and the required volatile event order. |

### Targeted compile repairs after relocations (2026-09-12)

The affected-crate Cargo gate passed **1851 tests, 0 failed, 4 existing long
campaign tests ignored**. Command: `CARGO_TARGET_DIR=target/tailored CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 3977afc --locked --offline -j2`.
The first run found the old short-circuit CFG golden and blanket aggregate
varargs rejection; their replacements retain value assertions and keep the
floating-record refusal. The final Clippy-only adjustment uses a borrowed
one-element slice: `cargo clippy --workspace --all-targets --locked --offline -j2 -- -D warnings`
then passed; the memory **31/31** and variadic **7/7** tests were rerun.

Final independent replay of committed `6df3eac` used
`git clone --no-local --no-hardlinks` into a new checkout with a fresh Cargo
target. `cargo test --workspace --no-fail-fast --locked --offline -j2`
passed **2,216 tests, 0 failed, 4 existing ignored**, across 292 test targets
including doctests. Full-workspace/all-target Clippy with `-D warnings`, fmt,
Ruff, diff-check and **104/104** K1 Python tests passed there. Both existing
`day5-layout-conformance.sh --subject` runs rebuilt their probe independently:
base **128/128** and attributed **22/22**, both `ORACLE_CONSENSUS`, no mismatches.
The original broad log is a temporary replay artifact (SHA-256
`1b80b415c891e6a097441c0c19aa5cffd802ca4f5ef98749027f170bee8d7eaa`),
not another copied kernel evidence tree. These results validate this committed
repair batch; no later full-kernel survey or boot composition is inferred.

Build ordering was separately verified on two vDSO producers plus the three
dependent consumers. `prepared-headers-20260912-v2` prepares with the installed
composition, freshly recompiles all five with its generated headers, reaches
a content fixed point and links successfully. All five object hashes survive
the link; the vmlinux proof confirms its three direct consumers, while the
two vDSO objects belong to their separate sub-link, not direct vmlinux symbols.
The resulting Image boots and shuts down with the new nonce; QEMU digest drift
was explicitly acknowledged. The first attempt stopped because Kbuild rewrites
identical Makefile bytes with a new timestamp; the revised check ignores only
metadata at the preparation fixed point, while normal survey cache keys retain
their metadata sensitivity. **104/104** K1 Python tests and Ruff pass.

`aggregate-variadic-20260912` freshly compiles both original 9p failures with
symbol parity: **2/2**, no cache/install/relink. The fixture checks 4/16/24-byte
records, copied lists, direct/indirect calls and register exhaustion; both
interpreters and all eight GCC/Clang O0/O2 mixed-call directions pass. EvoCC
assembly uses the configured GNU assembler; attempting Clang's assembler on
its explicit `:pg_hi21:` spelling failed and is retained as a toolchain-format
observation, not counted as a successful assembler test. **8** of the original
161 compile refusals remain; this is targeted repair accounting, not a fresh
2799-unit survey.

The reviewer resolved the computed-goto rule: indirect entries carry explicit
local-storage object addresses and current mem/world, not ordinary C values.
EvoIR 1.9.92 and EvoMIR 0.29 record complete indirect-edge binding and machine
transport. This supersedes the incomplete effect-only proposal; no validator
exception, implicit token or implicit frame-address namespace was introduced.

### Explicit indirect bindings (2026-09-13)

Tests-only `67c0eac` recorded **6 failed / 9 passed / zero build errors**:
unknown `br.indirect` syntax and the existing E3029 lowering refusal. The
implementation retains ordinary local SSA, full successor sets including
static label relocations, positional address/effect binding, and distinct
runtime E3048 target errors. The existing GNU signed-shift profile is unchanged;
two stale specification clauses were corrected, and explicit legacy E3006
obligations still execute.

Targeted checks pass **41 tests** across IR, lowering, MIR and independent proof
replay, plus **20** array-bound tests. Native cases include the reviewer's
counter, swapped object addresses, forty-address actual spills, a copy cycle
that overwrites the original target register, and both `asm.goto` entrance
paths. Declaration-owned labels, suspended expression operands and captured
array strides have dedicated value checks. Storage addresses are transported
without reading their contents; saved operands reload only on expression
resumption, and scope allocation/cleanup is routed onto the chosen path.

The scope-entry probe disagrees: GCC-14 accepts a computed bypass of a VLA
typedef; Clang-19 rejects it against the conservative successor set. Appendix D
records the existing refusal-direction protocol and E2945, including the case
where the operand names an otherwise safe target. This is not oracle consensus.

The final release replay, `computed-edges-final-20260913`, freshly processed
only `kernel/bpf/core.o` with no cache or installation. It was **refused E3034**
at `kernel/bpf/core.c:251:20`, after the old first error E3029 (5.611 seconds
for the unit). That is not an assembled object, and it does not replace the
2,799-record survey or establish K6/K7. The same release executes the expanded
`computed-edges.c` fixture through both interpreters and Linux native with
result zero; GCC-14 and Clang-19 agree at O0/O2, recorded in its `.out` file.
The changed gate passes **1,834 / 0 failed / 4 existing ignored**, across
251 test targets; Clippy and Rust formatting pass. Both layout subject corpora
pass, **128/128** and **22/22**, with no mismatches. The measurement record is
`tests/contracts/day9-acceptance-receipt.json`. An isolated clone was cancelled
by the user before any tests ran; it is **NOT RUN**, not a passing replay.

### Execution priority and disk use (2026-09-13)

The immediate target is the full-kernel compile/link/boot path, not another
workspace clone. Reuse `target/tailored`, run focused tests and affected original
units while fixing known blockers, then perform the full kernel compile, link
and boot checks. Do not start repeated workspace replays between these fixes.
The next observed blocker is K-C11: `kernel/bpf/core.o` still reports E3034.

Historical layout runs retained 344 Cargo output directories, totalling 8.76
GiB. These were removed without deleting probes, observations or summaries.
The layout harness now removes its run-local Cargo output on exit, after
recording the subject digest and measurements. The cancelled clone and this
turn's two temporary layout build trees were also removed. Kernel source,
reference/pristine builds, installed objects and the assembly cache are retained
for the actual link/boot work; those are not duplicate evidence to discard.

### Kbuild option audit (2026-09-12)

K-B02 is a separate full-configuration blocker, not another member of the
161 first-error refusals. Re-enumerating the same 2,799 pristine records with
`tools/k1/substitute.py::target_c_records` and
`tools/k1/cmdrecord.py::evocc_argv` confirms these omitted options. Counts for
mutually exclusive modes below use the **last** option in each original argv,
not the number of occurrences; the rows overlap.

| Required recorded behavior | Units | Current result / next implementation |
|---|---:|---|
| `-ftrivial-auto-var-init=zero` | 2,799 | Driver rejects the option; projection drops it. Implement explicit automatic initialization while retaining the default poison semantics without the option. |
| `-fstack-protector-strong` | 2,732 | Driver rejects the option; projection drops it. Implement the recorded `sp_el0` guard at offset 1240 and retain the 67 final `-fno-stack-protector` overrides. |
| `-mbranch-protection=pac-ret` | 2,792 | Driver rejects the option; projection drops it. Implement return-address protection and retain the seven final `none` overrides. |
| `-mstrict-align` | 1 | `arch/arm64/kernel/pi/map_range.o`; inspect generated accesses and implement the option before claiming its required early-boot mode. |
| `-ffreestanding` | 41 | Audit predefined macros and builtin consumption instead of treating it as an optimization-only omission. |

The three driver refusals above were reproduced on compiler `ac5eaf9f...`
using `tests/fixtures/signed-wrapping.c`; each exits 1 with `unsupported option`.
Other omitted flags still need classification into already-equivalent target
defaults, diagnostics/debug data, assembler/dependency plumbing, or required
code generation. For example, x18 is already reserved, but that does not prove
every omitted machine option equivalent. Neither a successful five-object boot
nor a future 2,799-object assembly count closes this audit. No flag was silently
accepted, no frozen configuration changed, and no additional full survey ran
for this read-only audit.

### Earlier targeted replay checkpoints

The following short-circuit repair passed the **45/45** assembly CLI tests
and **112/112** switch, expression-value, side-effect and statement-expression
tests; the affected `--lib` gate, all-target Clippy and fmt also passed.
`guarded-immediates-20260912` uses compiler `1c60ac7b...`: all six originals
assemble with symbol parity, no cache hits and no installation. The O2 sealed
probe agrees on exit zero; Clang O0 refuses the unreachable immediate, recorded
explicitly rather than claimed as consensus at all optimization levels.
Together with the preceding x30 replay, **10** original compile refusals remain.

The next targeted baseline, `remaining-compile-20260912`, rechecked all 18
then-open identities against compiler `0b9f8f22...`: 18 refused, no new
failure classes. After the explicit link-register fix, `link-register-20260912`
assembles both affected originals with compiler `97ce544d...`, no symbol
drift or cache hits, and no install/relink. Validation: the assembly CLI suite
passed **44/44**; `tools/gate-changed.sh 3977afc --locked --offline --lib -q -j 2`
passed **37/37** library tests plus all-target Clippy and fmt. This is a
targeted gate, not a new full-workspace or full-kernel result; **16** of the
original 161 compile refusals still need work after this repair.

### Byte-aligned static relocations (2026-09-12)

The standard ABS64 data relocation patches eight bytes at a byte offset;
it does not require a naturally aligned pointer slot. The correction removes
only the validator's offset-modulo-eight restriction, installs relocations
with byte alignment, and lets the shared pointer memory API accept actual
power-of-two access alignment independently of the target pointer width.
The existing byte/provenance representation and assembler emitter are reused.

`3d8f6c1` records five new failing tests with no build errors. The focused
GREEN command passed **100/100**:
`CARGO_TARGET_DIR=target/tailored cargo test -p evocc-memory --lib -p evocc-evoir --test relocations_and_addresses -p evocc-lower --test relocations_and_addresses --locked --offline --no-fail-fast -j 2`.
Tests cover offsets 0..7, the exact 2/12 packed layout, object/function/exposed
addresses, byte copies and field updates, and retained overflow, range,
overlap, symbol, initializedness, E3011/E3012/E3013 and both byte-order checks.

The two fixtures `packed-pointer-relocations.c` and
`packed-relocation-accesses.c` pass sealed GCC-14 and Clang-19 at O0/O2
(**8 zero exits**); EvoCC assembly linked with sealed GCC-14 also executes
both successfully (**2 zero exits**). `readelf -rW` confirms ABS64 at offsets
`0x2` and `0xc` in the first subject object, with no padding inserted.
The small oracle observations accompany the fixtures; native temporary
assembly stays under `.build/k1/packed-relocation-probes`.

Original replay command:
`python3 tools/k1/substitute.py --allowlist .build/k1/packed-relocation-units.txt --evocc /Users/onehrxn/dev/30days/c30/target/tailored/release/evocc --no-install --run-id packed-relocations-20260912 --jobs 2 --timeout 180`.
Both `drivers/acpi/acpica/exdump.o` and `drivers/acpi/acpica/rsdumpinfo.o`
compile, assemble and match reference defined symbols, with **0 refusals**
and **0 cache hits**. Subject SHA-256:
`0b9f8f22c5bc583b212ec6cbcfae2396f6cdae7a5da72edba0dd31307c7fe8bb`;
per-unit record SHA-256:
`3644d115c169f2a8ffac6b153eb4feadc1552522f10c56c5797be636b1289a86`.
No objects were installed and no full-kernel link, boot or selftest was run.

Layout regression commands: `C30_JQ=/usr/bin/jq C30_TIMEOUT=/opt/homebrew/bin/gtimeout C30_DAY5_LAYOUT_EVIDENCE_ROOT=/tmp/evocc-packed-layout-20260912 bash tests/acceptance/day5-layout-conformance.sh --subject`, and the same command with
`C30_LAYOUT_CASE_SET=attributed C30_LAYOUT_MANIFEST=tests/contracts/layout-attributed-cases.json`.
They measured **128/128** and **22/22**, zero mismatches and oracle consensus
(runs `20260912T125559Z-67250` and `20260912T125753Z-84275`). The first
attributed invocation omitted its case-set selector and was rejected before
execution (`manifest caseCount differs`); it is not counted as a test pass.
Temporary build trees remain outside the repository, under the stated `/tmp`
root. `ruff check tools tests/acceptance`, formatting, diff-check and the
status renderer check pass; the full-survey receipt remains unchanged.

The completed affected-crate gate passed **1,845 tests / 0 failures / 4
pre-existing ignored**, Clippy `-D warnings` and workspace formatting:
`CARGO_TARGET_DIR=target/tailored CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true tools/gate-changed.sh 855686e --locked --offline -q -j1 -- --test-threads=4`.
It covers the eleven affected/reverse-dependent crates, not a new full
workspace or isolated-clone milestone replay; those remain part of the
final campaign after the remaining known defects are repaired.

### Expression goto destinations (2026-09-12)

The later `aggregate-and-case-20260912` replay verifies five ticket-lock union
assembly users and `kernel/signal.o`: **6/6 assembled**, no installation/link,
release SHA-256 `05fbe3ae873e0424691a5f39c0e0b272503ef1347399c508cf89150e95098657`,
per-unit record SHA-256 `0a6a5380ae0f5691e50aaf8b6c8203bb390d549ae40e21f908c6ab158f316e23`.
Together with the later two-unit packed relocation replay, the disjoint
targeted runs have replayed **143 of the original 161 refused identities
successfully**, across their recorded repair binaries; this is not a fresh
assembled count for all 2,799 units. Eighteen original identities remain in
the open families, and the full no-fallback link, boot and selected selftests
remain OPEN.

The first pointer-case patch failed the existing static-initializer fuel
regression: a recursive folding shortcut bypassed the explicit budget. That
shortcut was removed rather than weakening the test; the final implementation
admits the numeric casts in the existing iterative evaluator. The failed gate
is not counted as a pass.
The corrected affected gate then passed **1,435 tests / 0 failures / 3 existing
ignored**, Clippy `-D warnings`, and workspace formatting; K1 Python tests
passed **101/101** and Ruff/diff-check were clean. Command:
`CARGO_TARGET_DIR=target/tailored CARGO_BUILD_JOBS=1 tools/gate-changed.sh fabe30a --locked --offline -q -j1 -- --test-threads=4`.
This is the affected-crate gate, not the still-pending full workspace/offline
replay and full-composition milestone acceptance.

The later targeted replay `initializer-and-goto-20260912` examines exactly
the 128 originals from K-C01/02/03 using release SHA-256
`f45460ee5dda440f872a49223e8e5f45499f7dc3ade6354ef4207c0a2166556d`:
**126 assembled, 2 refused (E3026), zero timeout/panic/assembler failures**.
Breakdown: goto **78/78**, integer relocations **21/21**, numeric addresses
**27/29**. It installs nothing and performs no relink; its records are in
[the targeted capture](../../.evidence/k1/initializer-and-goto-20260912/substitution.json),
with per-unit SHA-256 `cf7413ac7a33b4d48e43c480bdf844e5d13d7458d02c6441145abbf920c4ca0a`.
`d387e65` passes the affected gate (**1,801 passed, 0 failed, 4 existing
ignored**, Clippy/fmt) and the final updated Section 9 sentinel separately.
Both sealed reference compilers run the three static-address fixtures at
O0/O2 (**12 zero exits**), and both link drivers run their subject assembly
(**6 zero exits**). No full 2,799-unit count is inferred from this subset.

The completed September 10 survey had 78 first-error refusals saying that a
goto target was not lowered. Kernel `scoped_guard` places its goto inside the
for-step statement expression and the destination inside `if (0)`. The
statement-only collector missed that reference and allowed branch selection
to discard the destination. The repair reuses the existing expression-aware
function-reference walk for both target checks and lowering; no validator or
runtime obligation was removed, and no new traversal framework was added.

RED `3b7fd97` reproduces the loop step, a backward expression jump, and an
initializer jump in `tests/fixtures/expression-goto-targets.c`. C-HIR and both
sealed compilers returned zero while lowering reported the missing target.
GREEN `fb05c80` passes the same fixture through both interpreters and
Linux arm64 native assembly/link/execution. An additional unevaluated-sizeof
case ensures a jump that is not executed cannot retain an unsupported body.

Tailored release SHA-256:
`8c63d6026820dfcb60d19c205cb61515c1c5294321e0ba2ad9c6e8aa3e5ba3dc`.
Six original units, one per represented top-level source directory, were
replayed through `cmdrecord.evocc_argv(parse_record(record), driver, src,
pristine)` with the original `--emit=asm --survey` arguments and 180-second
limit. Their output was discarded after checking compilation; this is not
an assembly/link/boot measurement.

| Original unit | Before | Tailored replay |
|---|---|---|
| `arch/arm64/kernel/signal.o` | missing goto target | assembly emitted |
| `drivers/base/core.o` | missing goto target | E3026 static relocation |
| `fs/dcache.o` | missing goto target | assembly emitted |
| `io_uring/cmd_net.o` | missing goto target | assembly emitted |
| `kernel/events/core.o` | missing goto target | E3026 static relocation |
| `lib/bug.o` | missing goto target | assembly emitted |

This sample establishes four new successful emissions and two moved frontiers,
not 78 new passes or a new full-survey count. The frozen full survey continues
to use the earlier release binary. Next: reduce the static relocation and
integer-valued address initializer families, then zero-size pointer arithmetic
and the remaining asm/ABI/control-flow inventory. Keep the broader tailored
plan below; floating execution is still deferred.

The affected gate exposed two pre-existing test migrations: the scalar golden
renderer had no `DynamicAlloca` arm, and an older CLI test still required
E3026 for absolute integer pointer initializers admitted on September 10.
The former now classifies the instruction with its allocation family; the
latter asserts exact target bytes, no invented relocations, and E3013 from
both interpreters on attempted dereference. No compiler acceptance rule was
changed to satisfy these stale tests.

Validation: `CARGO_TARGET_DIR=target/tailored CARGO_BUILD_JOBS=1
tools/gate-changed.sh 8f6cf84 --locked --offline -q -j1 -- --test-threads=4`
passed: 729 tests, zero failures, three pre-existing ignored long campaigns;
Clippy `-D warnings` and workspace formatting passed. The selected crates were
`evocc`, `evocc-eqopt`, `evocc-lower`, `evocc-policy`, `evocc-proofcheck`, and
`evocc-randgate`, not the entire workspace. The two Python commands
`python3 -m unittest discover -s tools/k1/tests -q` and
`python3 -m unittest discover -s tools/ladder/tests -p test_bench.py -q`
passed 99 and 15 tests respectively. Ruff on the changed tools and
`git diff --check` passed. Full mixed-link/boot, all-benchmark campaigns and
an isolated workspace replay were not run for this tailored repair.

### Next repair batch from this measurement

1. **Static address initializers.** Two minimal forms have now been replayed:
   `static void *p = (void *)0x300 + 0x1000UL;` (E3026) and
   `static int x; static unsigned long p = (unsigned long)&x;` (E3101).
   The candidate driver still refuses them; sealed GCC-14 and Clang-19 both
   compile and execute checks of `0x1300` and `&x` respectively with exit 0.
   Sources are in `.build/targeted/20260912-goto/{absolute-pointer-offset,
   symbol-in-integer}.c`. These reproduce the `TIMER_ENTRY_STATIC` and
   `APD_ADDR` shapes; they do not prove all 50 units share only these defects.
   Add tests to the existing static-initializer target, use the shared address
   evaluator/relocation path, and assert bytes/addends plus fail-closed invalid
   dereferences. Do not fabricate provenance or relax relocation validation.
2. **Zero-size pointer offsets and remaining asm/value boundaries.** Reduce
   the five zero-scale and five aggregate-in-scalar cases before changing the
   relevant shared path. Keep immediate/register constraints, aggregate ABI,
   and volatile accesses as separate semantic tests.
3. **Finish the existing non-floating inventory.** Follow the tailored-first
   plan below for the remaining declaration/VLA, varargs, computed-control-flow,
   vector/atomic and benchmark-adapter work. Every original failure needs a
   repair or a reviewed source-bound disposition, not a guessed exclusion.
4. **Only then run the next complete integration campaign.** Recompile with
   the new driver, reuse only exact unchanged assembly objects, and run the
   original benchmarks. Any changed kernel composition still gets a real
   link/boot. Neither this survey nor the six-unit repair sample closes K6/K7.

### Tailored-first execution plan (2026-09-10)

Starting checkout: `21f522d`. The last torture capture remains
`20260909T215349Z`: 1,268 PASS, 268 compiler failures, 11 FP-deferred,
145 evidenced not-applicable and zero oracle-unusable, out of 1,692 original
identities. The 268 unresolved programs are not 268 independent defects:
266 are refusals, one a runtime failure and one a link failure. No planned
improvement below is a new measurement.

| Order | Remaining work | Tailored acceptance before any campaign |
|---|---|---|
| 1 | Source-width constant arithmetic and wide-field regressions | Existing C04 RED plus boundary values and nonconstant-overflow rejection; minimize the three lost wide-field passes, test consensus and conflicting contexts separately. Implement the authorized probe-backed rules and record body differences only in Appendix D. |
| 2 | Remaining declaration, call and initialization defects within the existing profile | One reproducer per original first-error construct; parser type assertions, C-HIR/EvoIR execution, negative compatibility tests and native execution where relevant. Include prototype parameter scope and builtin/local-function binding. |
| 3 | Dynamic automatic objects, zero-byte objects, static addresses and integer aggregate varargs | One-time bounds, lifetime exits, invalid entry, zero-size identity/access, relocations and ABI register/stack boundaries. Reuse existing oracle probes; do not infer new IR semantics from direction-only approval. |
| 4 | Computed branches, remaining asm operands, frame chains and kernel symbol metadata | Successor/edge validation, spills and ties, same/different function label cases, section/visibility symbols, and small linked programs. Preserve all independent validators. |
| 5 | Integer vectors and remaining execution/type coverage | Separate header admission from execution; lane values, masks, subscript places, ABI, volatile field events, integer complex values, atomic operations and wide division boundary cases. Floating execution stays deferred. |
| 6 | Complete original-program classification and benchmark prerequisites | Source-bound dispositions for every remaining broad refusal; retain all original options and identities, all 68 unavailable Csmith seeds and all 18 reference-baseline failures. Finish relevant adapter/workaround checks without running full campaigns. |
| 7 | Integration and closure | Once orders 1–6 are locally verified or have evidenced permitted dispositions, run the full gate, layout corpora, isolated offline replay, then the real survey and serial benchmark campaign. Any new compiler failure reopens its family; no clean result is predicted. |

Use existing test targets and fixtures, not another runner or receipt layer.
For a shared type/IR change, build all affected construction sites and test
parse/render/validation/execution together; for local parser defects, begin
with parser and lowering tests only. Tailored native tests may use a handful
of exact reproducers, but do not run the 1,692-program torture campaign,
1,000-seed Csmith campaign or full-configuration kernel survey during the
repair batches. Keep benchmark receipts at their measured values until the
actual campaign runs. Tests-only RED commits must compile and fail for the
declared behavior; implementation commits record the exact tailored command
and result. No failure is hidden by disabling a test or changing an oracle.

## Completion checklist

The 2026-09-09 reviewed disposition supersedes the earlier "only FP may be
deferred" acceptance wording below. Keep every original identity in exactly
one of five categories: PASS, compiler failure, FP deferred, evidenced not
applicable, or oracle-unusable. Closure requires zero compiler failures and
evidence for every non-PASS; an unsupported implementation is not by itself
evidence of inapplicability. Default execution is gnu11 on aarch64-linux;
required test options and target selectors must be consumed explicitly.
The approved review schedules address operands, GNU signed shifts, stack and
function alignment, and 128-bit predicates first, followed by the reviewed
memory/ABI and type coverage work. The final campaign remains sequential.

The unit of work is the complete issue inventory below, not the next individual
repair. The user's 2026-09-08 instruction makes floating-point execution the
only deferred implementation category. Non-floating-point vectors, variable
length objects, unsupported syntax, linking failures and validation gaps remain
work, not reasons to label the non-floating-point objective complete. Existing
specification-body restrictions remain visible until an authorized rule changes
them; an implementation must not silently contradict the body.

The user's follow-up authorization on 2026-09-08 covers the four previously
blocked body changes: real variable-length objects, integer vectors, symbolic
assembly immediates and PIE-safe strong external-data addressing. These are now
implementation work, not pending questions. Each change must still specify and
test its semantics across the consumers it affects; floating-point execution
alone stays deferred. Closing this inventory does not close unmeasured roadmap
milestones or turn unusable oracle programs into passes.

The numbered rows remain the inventory authority; this checklist schedules
their remaining closure work without creating a second set of measurements.

- [x] Restore nested macro push/pop state; replay the original failures (row 1).
- [ ] Finish native wrong-result classification and repairs; preserve runtime
  obligations and consume required test options explicitly (rows 4, 17).
- [x] Repair goto-entered automatic storage, initialization and lifetime;
  replay toybox cut/httpd/sed and control-flow regressions (rows 8, 12).
- [ ] Finish aliases/weak symbols and register-binding original-program
  validation, separating FP-dependent libc failures (rows 2, 3).
- [x] Correct strong external-data and function addressing for the original
  TinyCC PIE links; all seven assembled objects now pass together (row 11).
- [x] Replay assembly register alternatives and kernel substitution with the
  exact measured binary; close each substitution mismatch (rows 5, 6).
- [ ] Resolve the remaining kernel assembly/symbol/control-flow constructs,
  retaining explicit specification conflicts rather than bypassing them (row 7).
- [x] Classify remaining toybox function-value and object-subvalue forms:
  all eight measured sites require deferred FP execution (row 9), not PASS.
- [ ] Admit unused system-header types independently of FP execution (row 10).
- [ ] Classify and implement admitted declaration/call forms, not a blanket
  diagnostic-code exception (row 13).
- [ ] Implement the required integer-vector behavior under a complete contract
  and replay the original initializer/place failures (row 14).
- [ ] Close the variable-length-object requirement under an authorized contract;
  a current profile refusal is not a non-FP pass (row 15).
  Prior-parameter visibility, prototype `[*]`, inner/outer array-parameter
  bounds, captured sizeof and fixed-storage local pointer views are locally
  repaired, including block-static pointer views. Definitions capture sizes on
  entry; prototypes do not evaluate them. Actual variable-length
  storage/reclamation, variably modified typeof and legacy variable-bound definitions remain
  unfinished; pointer adjustment is not full VLA support.
- [ ] Reduce and repair every remaining non-FP diagnostic family, including
  builtins, ABI, initializers and the remaining TinyCC compile refusals
  (rows 11, 16).
- [ ] Account for every original Csmith seed, including all 68 unavailable
  references; never replace a seed or count a timeout as agreement (row 18).
- [x] Make cJSON tests execute the subject objects, not rebuilt reference C
  sources (row 19).
- [ ] Resolve or independently establish reference-baseline/environment failures;
  retain them in the reported universe (row 20).
- [ ] Implement missing non-FP milestone adapters when their prerequisites are
  ready; FP-dependent milestones remain explicitly deferred (row 21).
- [ ] Run one final full benchmark campaign after incremental closures, with
  unchanged program identities and an explicit per-program FP classification.
- [ ] Reconcile every outcome, update receipts and render status; report the
  non-FP objective complete only with no unresolved non-FP compiler failures,
  coverage refusals, regressions or unverified subject runs.

Invalid or nonterminating reference programs are not compiler successes. Their
disposition needs individual evidence and remains visible beside the original
denominator. An unclassified or unavailable reference blocks an "ALL passed"
claim. Completion means closure of this measured inventory and final campaign,
not a claim that no unknown compiler bug can exist.

### Approved arithmetic and alignment work

All five rules below were granted in `dff089b`, with the two reviewed
alignment corrections in `177378e`. They are implementation and original-run
validation work, not pending authority. The separate constant-expression and
memory/ABI rules still awaiting body revisions have measured probes ready.

| Approved rule | Current implementation and measured boundary |
|---|---|
| AArch64 `%a` address-operand modifier | Implemented; the sealed 90/2,798 lowering result exposes a different tied pointer/integer input. That representation fix is also implemented, with original full-kernel remeasurement pending. |
| GNU signed-left-shift bit patterns | Implemented without removing count or other arithmetic obligations. Eleven original torture runtime failures and the ctest runtime failure now pass; `insn.o` remains in the 90-object combination that boots on the first attempt. |
| Real over-aligned stack frames | Implemented through reserved `x16`; recursive aligned-256/spill fixture passes both interpreters and Darwin/Linux native execution. |
| Explicit function alignment metadata | Implemented and merged; original `align-3` passes both interpreters, native Linux and full replay `20260909T062923Z`; changed gate 3,348 passed / 0 failed, eight existing ignored. |
| Machine lowering of 128-bit signed UB predicates | Implemented; 27 focused tests and 21,074 native boundary checks pass. All five E4003 originals also PASS in full replay `20260909T062923Z`; changed gate 599 passed / 0 failed, four existing ignored. |

The large-layout reduction for original `991014-1` also has an independently
measured oracle split: GCC-14 accepts both byte-layout assertions at O0/O2,
while Clang-19 rejects the array sizes. The secondary failed assertions are
not Clang layout values. The current refusal remains visible; the source
and outcomes are in `.evidence/builtin-probes/20260909-large-layout/`.

## Issue inventory

| Order | Issue family | Baseline and closure requirement | Incremental bench | State |
|---|---|---|---|---|
| 1 | Macro push/pop state | `00206` and `pushpop_macro` now pass; definitions, undefined and nested states restored before subsequent uses | `ctest torture` | CLOSED: `20260908T005741.383578Z`, zero regressions |
| 2 | Object aliases and weak symbols | Three original alias link failures now pass; musl symbol exclusions 167 to 0; replay `20260909T132842Z` substitutes all 1,010 assembled units together out of 1,339, with no additional baseline test failure | `torture musl` | PARTIAL: symbol repair benched; 17 reference-baseline failures and remaining refusals stay visible |
| 3 | Explicit register operands | Binding reaches IR, allocation and all output edges; native spills and binding validation pass; corrected musl recorder preserves 1,339 identities | `musl` | PARTIAL: 1,010-unit combination measured in `20260909T132842Z`; further failures remain |
| 4 | Remaining native failures | `20260909T215349Z`: torture retains five raw runtime failures and one alloca link failure; four runtime failures have reviewed not-applicable evidence, while constant-overflow `920730-1` remains OPEN | `ctest torture` | PARTIAL: no lost PASS in this replay; three older lost wide-field passes remain OPEN under the narrowed conflict-only rule |
| 5 | Kernel substitution mismatches | Fresh `measure-20260908T102907Z` boots with 76 substituted units, 64 linked; corrected NOBITS memory-image replay proves 61 and records 3 no-symbol units, zero mismatches, same linked-image digest | `kernel` | CLOSED for the identified cache/extent/zero-fill defects; original failed proof retained beside `proof-zero-fill.json` |
| 6 | Assembly register alternatives | All twelve authorized spellings select `r` after one evaluation; native operand/branch tests pass, and the original kernel replay has no unresolved alternative or register-alias first error | `kernel` | CLOSED for the identified selection/binding defects in `20260908T123730.141577Z`; later assembly failures remain in row 7 |
| 7 | Kernel compilation, link and boot | `survey-20260912T092730.260925Z`: 2,638 assembled / 161 refused out of 2,799, zero timeouts/crashes/assembler failures. Link/boot NOT_RUN; September 10 link/boot failures remain unresolved | `kernel` | OPEN: [kernel tracker](#kernel-boot-blocker-tracker-updated-2026-09-12) separates K-C01–15 compilation, K-L/R link/runtime, K-B integration and K-G milestone gaps; goto fix is LOCAL_VERIFIED, not a new full-kernel pass |
| 8 | Return and edge typing | toybox now 195/203, zero regressions; cut/httpd/sed all assemble and enter the passing mixed build, preserving all 203 original identities | `toybox` | CLOSED: return and goto-storage fixes benched in `20260908T080325.213428Z` |
| 9 | Other toybox lowering refusals | Eight retained refusals: args/xwrap/sort call strtod, hash calls sin, seq calls fabs, printf calls strtold, od loads a long double, uptime computes FP load averages | `toybox` | FP-DEFERRED: every measured first site classified; none counted as PASS |
| 10 | Unused system-header types | zstd now has six passing substituted units; all five pretty-function first sites clear, leaving 37 integer-NEON declarations and one E3101 | `zstd cjson` | PARTIAL: C function-name choice and storage-only declarations replayed; integer vector declarations remain OPEN |
| 11 | TinyCC compiler build | 8/11 assemble and pass together; all original PIE link failures are repaired; three compile refusals remain | `tinycc` | PARTIAL: function-name fix replayed in `20260909T075341Z`; remaining source forms and adapter workarounds OPEN |
| 12 | Switch, labels, function prototypes | Nested cases, promoted dispatch, discarded abrupt exits, unprototyped calls, enum/integer pointer compatibility and jump-entered storage pass original replay | `ctest torture toybox` | CLOSED for these identified constructs; remaining declaration forms stay in row 13 |
| 13 | Torture declaration coverage | Six raw E1001 refusals in `20260909T215349Z`; bracket attributes now parse through the shared attribute implementation, and the 66 function-declaration placement errors remain zero | `torture` | PARTIAL: the identified declaration and bracket-attribute defects are replayed; remaining syntax forms and per-program applicability OPEN |
| 14 | Integer vectors | `20260909T215349Z`: 22 excess initializers and 17 projected-place failures mention vectors; non-FP does not imply scalar; keep vector execution distinct from header admission | `torture` | OPEN |
| 15 | Variable-length objects | `20260909T215349Z`: 20 torture E2203 refusals plus other lanes; explicit profile limitation, not a solved test or silent miscompile | `torture musl` | OPEN |
| 16 | Remaining diagnostic classes | Torture 328 raw refusals in `20260909T215349Z`; five-way classifier records 268 compiler failures overall, including unclassified refusals | `torture musl` | OPEN: not every refusal has an evidenced disposition yet |
| 17 | Torture applicability/options | `20260909T215349Z`: 1,268 PASS / 268 compiler failure / 11 FP deferred / 145 not applicable / 0 oracle-unusable; 1,692 unchanged identities | `torture` | PARTIAL: five additional measured FP classifications, no new original PASS; additional unclassified programs may still require FP execution |
| 18 | Unavailable Csmith references | Fresh `20260909T193124Z`, subject `691b654`: all 1,000 emit; 932 agree, 68 oracle-unusable, zero mismatch; every identity, runtime checksum/verdict, generator option and runtime digest unchanged | `csmith` | OPEN: four repeating source paths identified, 64 undetermined; every unavailable identity is listed in the applicability notebook, and none is a PASS |
| 19 | cJSON subject test linkage | Native mutations of either installed object fail; original replay relinks both objects into the public test and 54 public-API assertions without recompiling their implementations | `cjson` | CLOSED: mechanism replayed in `20260908T121307Z`; 0/2 subject objects means the milestone is still INCOMPLETE |
| 20 | Reference baseline failures | musl 17 and toybox 1; separate environmental/applicability faults from subject regressions | `musl toybox` | OPEN |
| 21 | Missing milestone adapters | Mbed TLS, PolyBench, llvm-test-suite, jq, Lua, SQLite, full-kernel and kselftest: eight NOT_IMPLEMENTED entries; prerequisites and FP deferral remain visible, not silently waived | named adapters | OPEN |

## Progress

### Variable record members and original-program classification

Tests-only `571c7f7` compiled with 17 passes and one failure: a typedef hid
a variable array member from the parser's written-bracket check, so an
invalid record was silently accepted. The canonical member check now covers
arrays, unions and pointer layers, preserving E2203. A tailored regression
also prevents over-refusal of function-pointer members whose own prototype
parameters have array bounds; those bounds belong to the prototype scope,
not to the member object.

`cargo test --locked --offline -j 1 -p evocc-parser
--test array_bound_validation --test gnu_typeof --test parser_matrix
--test parameter_and_pointer_sign_conversions -p evocc-lower
--test expression_value_flow --test static_storage_and_discard --no-fail-fast`
passes 146 tests (19 + 17 + 2 + 14 + 75 + 19), zero failed or ignored.
Affected Clippy, fmt, Ruff, diff-check and the 44/44 shape audit pass.
The callback fixture passes four reference and two subject native runs;
the six variable-member reductions retain the measured GCC/Clang conflict.

Targeted syntax probes of twelve unchanged E2203 originals identify eight
variable-record extensions and four excluded nested definitions: GCC accepts
all twelve and Clang rejects the named constructs. Their source-bound
dispositions are recorded for the next campaign, not applied retroactively.
The existing disposition unit test command (`python3 -m unittest discover
-s tools/ladder/tests -p test_program_dispositions.py`) passes three tests;
direct `classify_program` validation checks all twelve source digests and
evidence paths. The applicability notebook lists every identity.
Of the original twenty E2203 rows, two have tailored native passes, twelve
now have evidenced prospective exclusions, and six remain dynamic-object
cases to implement/classify. No full campaign ran; its 268 compiler-failure
count is unchanged, and ordinary VLA support remains OPEN.

### Block-static variable-array pointers

Tests-only `eb380c4` compiled with 73 passes and one E2203 failure.
The shared fixed-storage declaration path now captures block-static pointer
bounds on every reached declaration while leaving static initialization and
identity unchanged. Inline copies use an explicit address cast to view the
one lifted object with invocation-local dimensions. Ordinary memory places
then handle reads, assignments and updates without weakened validation.

`cargo test --locked --offline -j 1 -p evocc-lower
--test expression_value_flow --test front_end_resolved_forms -p evocc-parser
--test array_bound_validation --test gnu_typeof
--test parameter_and_pointer_sign_conversions -p evocc-chir --lib
--no-fail-fast` passes 215 tests; `cargo test --locked --offline -j 1
-p evocc-lower --test static_storage_and_discard` adds 19: 234 distinct
passes, zero failures or ignored. Both use the tailored target. Affected
Clippy, fmt, Ruff, diff-check and the 44/44 instruction-shape audit pass.
Four sealed reference runs and two subject native executions pass
(`static-array-pointers.out`), including shared inline statics, fixed arrays
of pointers, compound/prefix updates and address-of. Actual VLA allocation
and scope reclamation remain OPEN; no campaign count changed.

### Local variable-array pointers over fixed storage

Tests-only `9c0cb03` compiled with 72 passes and one E2203 failure.
Automatic pointer declarations now reuse bound capture before initialization,
including uninitialized pointers, fixed arrays of pointers, register objects
and for initializers. An inline regression also exposed stale capture IDs in
written array layers; those now use the same renaming as canonical types,
with allocation identity validation unchanged. Unevaluated operands do not
execute captures, noninteger bounds remain E2923, and goto/switch bypasses
remain E2945.

`cargo test --locked --offline -j 1 -p evocc-lower
--test expression_value_flow --test front_end_resolved_forms -p evocc-parser
--test array_bound_validation --test gnu_typeof
--test parameter_and_pointer_sign_conversions -p evocc-types
--test target_data_layout --test type_interner --no-fail-fast` with the
tailored target passes 163 tests (73 + 24 + 16 + 17 + 14 + 12 + 7).
`cargo test --locked --offline -j 1 -p evocc-chir --lib allocation
--no-fail-fast` adds 16 distinct tests: 179 passes, zero failures or ignored.
Affected Clippy, fmt, Ruff, diff-check and the 44/44 instruction-shape audit
pass. Four sealed reference runs and two native subject runs pass
(`local-array-pointers.out`).

Unchanged originals `20040411-1` and `pr22061-2` also pass four reference and
two native subject runs each on `962d85b` (`captured-array-originals.out`).
The latter does not execute its unused variable-array function, so it is not
evidence for actual VLA storage. No full campaign ran and frozen benchmark
receipts remain unchanged; dynamic storage and scope reclamation remain OPEN.

### Multidimensional parameter strides

Tests-only `7480eb7` compiled with 97 passes and three E2203 failures:
definition-entry strides, inner prototype stars and integer-bound validation.
Prototype-style definitions now capture every variable dimension once at
entry using the existing size-object path; their outer array layer still
adjusts to a pointer. Prototype-only declarations, including nested callback
prototypes, do not execute their bounds. Later parameters can query a prior
parameter's captured size, and mutations of the original size parameter do
not change indexing or sizeof.

Array-bound composition is shared by canonical compatibility, parser
redeclarations and allocation-manifest checking. Known constant dimensions
take precedence, differing known constants remain incompatible, and element
type/qualifier mismatches still report E2002. Inline parameter copies obtain
fresh capture identities before their types are copied; the captures'
ordinary entry initializers still run only after all arguments are bound.

`cargo test --locked --offline -j 1 -p evocc-lower
--test expression_value_flow --test front_end_resolved_forms -p evocc-parser
--test array_bound_validation --test gnu_typeof
--test parameter_and_pointer_sign_conversions -p evocc-types
--test target_data_layout --test type_interner --no-fail-fast` with the
tailored target passes 161 tests (72 + 24 + 15 + 17 + 14 + 12 + 7).
`cargo test --locked --offline -j 1 -p evocc-chir --lib allocation::tests::
--no-fail-fast` adds 15 manifest tests, including forged metadata refusals:
176 distinct passes, zero failures or ignored tests. Affected Clippy, fmt,
Ruff, diff-check and the 44/44 shape audit pass. Four sealed reference runs
and two native subject runs pass (`parameter-array-strides.out`).

An additional K&R experiment is **not** a passing value fixture:
`legacy-array-bound-probe.{c,out}` measures GCC storing 2 and Clang storing 3
after the same bound increment and first local initialization (both O0/O2).
It remains separate from the original RED fixture; legacy variable-bound
definitions are still OPEN, not a blanket evidenced-not-applicable category.
Actual VLA object storage and scope-aware reclamation also remain OPEN.
No full campaign ran; all benchmark receipt numbers remain unchanged.

### Captured array strides through pointer operations

Tests-only `0eafa69` compiled with 68 passes and one E2102
`invalid-update-operands` failure. A shared layout factorization now gives
subscripts, arithmetic, updates, pointer differences and sizeof the same
captured bounds, without inventing a static size. Bound values travel along
SSA edges even when only a type uses them; inline substitution renames the
captures in types, facts and places as well as ordinary expressions.
The existing semantic validators remain enforced.

Tailored edge cases exposed two additional false-green paths: multiplying a
wide index could wrap an invalid offset back to zero, and an overflowing
captured size could become a usable small stride. Full-width checked offsets
now preserve E3011 in both interpreters, including zero/overflowing strides
before pointer-difference division. Native pointer arithmetic consumes the
low address bits of the existing wide pair; no machine register class changes.

`cargo test --locked --offline -j 1 -p evocc-lower
--test expression_value_flow --test front_end_resolved_forms -p evocc-parser
--test array_bound_validation --test gnu_typeof -p evocc-types
--test target_data_layout -p evocc-mir --test section5_wide_integers
--no-fail-fast` with `CARGO_TARGET_DIR=target/tailored` passes 157 tests
(71 + 24 + 13 + 17 + 12 + 20), zero failed or ignored.
The same selected tests and affected libraries pass Clippy `-D warnings`;
fmt, Ruff, diff-check and the 44/44 instruction-shape audit pass.
The expanded fixture passes four sealed reference runs and both Linux
toolchains' executions of subject assembly (`captured-array-strides.out`).
Inner variable parameter dimensions were subsequently addressed above for
prototype-style definitions. Actual VLA object storage and scope-aware stack
reclamation remain OPEN; no full campaign ran or changed its measured counts.

### Literal string comparisons and embedded NUL

Tests-only `b9c34e4` compiled with seven passes and two semantic failures:
literal comparisons reached E2927 because of a same-named static declaration,
while the measured nonliteral binding conflict was silently accepted.
A further RED pinned `strlen("a\0tail")` folding to the wrong length (E2911
in its assertion). Both literal operations now use the shared string decoder
and stop at the first NUL, without changing array extents; strcmp yields
the measured int -1/0/1. Nonliteral static-binding ambiguity is E2946.

`cargo test -p evocc-parser --test builtin_strlen --test library_builtin_typing
-p evocc-lower --test compiler_hints --test front_end_builtin_integrity
--test front_end_resolved_forms --no-fail-fast` with the target/offline flags
above passes 52 tests (7 + 11 + 9 + 1 + 24), zero failed or ignored.
The same library/test selections pass Clippy `-D warnings`; fmt, Ruff,
diff-check and the 44/44 instruction-shape audit are clean.
Both the fixture and unchanged original `921007-1.c` pass sealed GCC-14 and
Clang-19 at O0/O2 and subject-generated Linux assembly linked by each
toolchain (Clang uses `-fno-integrated-as`, as in earlier tailored checks).
Literal operations are byte-identical to spelled-out constants in EvoIR;
no full campaign was run and benchmark receipts remain unchanged.

### Captured array sizes and forbidden scope entry

Tests-only `077e6c2` compiled with 64 existing passes and three E2203
failures. Block-scope variable array typedefs now retain bound identities,
initialize compiler-owned size objects once per reached declaration, and
elaborate `sizeof` into reads of those objects. Distinct captured types do
not merge their sizes; compatibility still follows element types when a
bound is variable. Typedef chains, sequential declarators, repeated loops,
backward jumps, inline substitution and unevaluated operands are covered.

Two further tailored RED observations were repaired before committing:
jumping over a variable typedef had been accepted, and comparing its type
with a fixed-bound array incorrectly folded false. The existing jump-region
walk now rejects direct/assembly-goto or switch entry with E2945; it still
enforces statement-expression entry restrictions and permits exits/reentry
through the declaration. The shared type comparison handles variable bounds.

`cargo test -p evocc-lower --test expression_value_flow -p evocc-parser
--test array_bound_validation --test parser_matrix --test gnu_typeof
--test statement_expressions -p evocc-types --test target_data_layout
--no-fail-fast` plus the final added array-refusal guard passes 119 distinct
tests (68 + 13 + 2 + 17 + 7 + 12), zero failed or ignored, using the target
and offline flags above. Affected library/test/example Clippy, fmt, Ruff,
diff-check and the 44/44 shape audit pass. Four sealed reference executions
and two subject Linux executions exit zero; both references reject all five
negative cases (`captured-array-bounds.out`).
This checkpoint implemented size capture, not whole VLA closure: runtime
strides and inline remapping were subsequently addressed above; actual
dynamic objects and scope-aware stack reclamation remain OPEN.
The existing E2203 storage and
E2904 variably modified typeof boundaries are retained rather than replaced
by a fabricated fixed layout. No full campaign or milestone count changed.

### Ordinary stack-allocation binding

Tests-only `3a90067` compiled with two semantic failures: an implicit GNU89
`alloca` had the wrong return type (E2202), and conflicting external bindings
were silently accepted. Compatible external declarations and GNU89 implicit
calls now reuse the explicit builtin's prototype and allocation path; GNU11
still requires a declaration, static same-name functions remain ordinary,
and `-fno-builtin` does not disable `__builtin_alloca`.
The external-body and assembly-rename probe results disagree across references;
E2944 records these conflicts, including a definition following the caller.
The former external-body control now uses a static function, the agreed case.

`cargo test -p evocc-lower --test compiler_hints --test expression_value_flow
--test front_end_builtin_integrity -p evocc-parser --test library_builtin_typing
--test builtin_strlen --no-fail-fast` with the current target and offline flags
passes 88 tests (7 + 64 + 1 + 11 + 5), zero failed or ignored.
Parser/library and selected-test Clippy, fmt, Ruff, diff-check and the 44/44
shape audit pass; `cargo build -p evocc` builds the debug executable.
Unchanged original `941202-1.c`, with its required `-std=gnu89`, exits zero
under both sealed references at O0/O2 and under both Linux toolchains linking
the generated subject assembly (`dynamic-stack-names.out`). This repairs its
specific link failure in tailored execution, not a new full-campaign result.
Actual VLA declarations, captured sizes and scope reclamation remain OPEN.

### Runtime-sized function-lifetime stack storage

Tests-only `0e711af` compiled and failed on the missing dynamic allocation
text form and two E3046 lowerings. `__builtin_alloca` now has an actual
manifest site and once-evaluated byte-size operand, becomes `alloca.dynamic`
with mem/world threading, and reaches a native `stack.alloc` operation.
Both interpreters preserve objects beyond the allocating block and kill them
at function return; bounds, initializedness and alignment errors remain active.
Fixed frame slots and spills are addressed from the entry frame when dynamic
storage moves sp, and each allocation keeps the outgoing argument area disjoint.

One first rerun failed because the IR test tried to return an object pointer
through the scalar public entry API. That test now returns only effect tokens
and checks the actual object layout/lifetime in the outcome; the independent
C test still returns a pointer through an internal call and asserts E3010.
No interpreter API or validator was relaxed to make that test pass.

Using the current target and flags above, `cargo test -p evocc-evoir --test
day7_object_memory_red -p evocc-lower --test expression_value_flow --test
compiler_hints -p evocc-mir --test section9_native_differential -p
evocc-proofcheck --test pointer_memory_proof --no-fail-fast` passes 75 tests
(3 + 64 + 2 + 4 + 2). `cargo test -p evocc-evoir --test section9_completeness
-p evocc-mir --test section6_variadic_and_assembly --test section8_emission
--test agreement_with_the_evoir_interpreter --no-fail-fast` passes another
47 (1 + 6 + 12 + 28): 122 distinct tailored tests, no ignored cases.
The new native check asserts real spills at 16/64/256-byte alignment and up
to a 70KB fixed frame; the C fixture includes recursion, repeated allocation,
a twelve-argument call and variadic register/stack arguments after allocation.

Affected-library/bin and selected-test Clippy with `-D warnings`, fmt, Ruff,
diff-check and the 44/44 instruction-shape audit pass. The randomized semantic
gate target compiles (`cargo check -p evocc-randgate --test
randomized_semantic_gate`), and `cargo build -p evocc` builds the debug CLI.
Both sealed Linux toolchains link and execute the subject fixture, exit zero;
all four reference runs also exit zero (`dynamic-stack-objects.out`).
Ordinary-name recognition is verified separately above; real VLA declarations,
captured sizeof and scope reclamation remain separate work. No campaign count
or whole VLA-family closure is claimed from this result.

### Array parameter bounds at definition entry

Tests-only `e47bf43` compiled with three declared E2203 failures and three
existing parameter checks passing. The parser now retains the bound expression,
adjusts the outer array parameter once, and elaborates definition-only bound
effects as ordinary C-HIR entry statements. Recursion and inline substitution
therefore use the existing execution and symbol-remapping paths. Prototypes,
including nested callback prototypes and block-scope redeclarations, do not
execute their bounds; `sizeof` of the adjusted parameter stays pointer-sized.

A supplementary negative test first caught accidental admission of a floating
bound after adjustment. The shared integer-class check now rejects floating,
pointer and void bounds with E2923; both sealed compilers reject these cases.
One existing test still expected E2203 for a now-admitted outer parameter; that
row was moved to acceptance while its local-object and inner-dimension refusal
checks remain. Neither failure was recorded as a passing run.

With `CARGO_TARGET_DIR=target/frontend`, `--locked --offline -j 1`,
`cargo test -p evocc-parser --test array_bound_validation --test gnu_typeof
--test parameter_and_pointer_sign_conversions -p evocc-lower --test
expression_value_flow --no-fail-fast` passes 103 tests (12 + 17 + 13 + 61).
`cargo clippy -p evocc-parser --lib -- -D warnings`, fmt, Ruff, diff-check and
the 43/43 instruction-shape audit pass; `cargo build -p evocc` builds the debug
CLI. Both sealed toolchains link and execute the subject assembly for
`tests/fixtures/parameter-bound-effects.c`, exit zero; four reference executions
also return zero, with the exact commands and warnings in the adjacent `.out`.
Actual dynamic objects remain OPEN and no campaign or milestone count changes.

### Volatile bit-field execution

Tests-only `fc3288a` compiled and failed twice on the declared volatile
bit-field refusal, independently in C-HIR execution and lowering. Volatile
reads and read/modify/writes now preserve the container's neighboring bits
and carry the existing world token; packed containers are split into bounded
scalar accesses rather than reading past the object. The two interpreters
agree on the complete access trace as well as the program result.

With `CARGO_TARGET_DIR=target/frontend`, `--locked --offline -j 1`,
`cargo test -p evocc-lower --test expression_value_flow --test
checked_and_bit_lowering -p evocc-types --test target_data_layout` passes
80 tests (60 + 9 + 11), including the two original RED tests, not adding their
separate rerun again. Affected-library Clippy (`evocc-types`, `evocc-chir`,
`evocc-lower`, `--lib -- -D warnings`), fmt, Ruff, diff-check and the
instruction-shape audit pass. `cargo build -p evocc` builds the debug CLI;
both sealed toolchains link and execute its assembly for
`tests/fixtures/volatile-bit-fields.c`, exit zero.

The four reference executions agree numerically, but their small-field
store widths and packed instruction sequences differ. The adjacent `.out`
records that distinction; the implementation follows the reviewed declared
container rule, not a false claim of identical oracle access traces. No
new IR instruction or weakened validator was needed. Full campaigns and
their measured counts remain unchanged.

### Exact wide checked arithmetic

Tests-only `4198a69` compiled with ten PASS and four semantic failures:
E2913 at the old wide-builtin parser boundary and E3044 at the old exact
intermediate-width limit. The parser now uses the shared integer-class
predicate, and lowering retains its existing narrow fast path while wider
operations preserve sign, low magnitude and a separate overflow bit.
Zero multipliers cannot cause an internal division by zero; opposite-sign
cancellation, MIN/MAX boundaries, narrow destinations, conditional operands,
volatile destinations and once-only evaluation are explicitly checked.
No runtime UB obligation or validator was relaxed, and no new IR type was added.

With `CARGO_TARGET_DIR=target/frontend`, `--locked --offline -j 1`:
`cargo test -p evocc-parser --test checked_arithmetic -p evocc-lower
--test checked_and_bit_lowering --no-fail-fast` passes 14 tests;
`cargo test -p evocc --test external_symbol_lowering wide_` passes two;
`cargo test -p evocc-chir --test checked_arithmetic_semantics --test
wide_integer_boundary -p evocc-lower --test wide_integer_values --test
fail_closed_forms` passes 24. These are 40 distinct tailored test functions,
not original benchmark programs. `cargo clippy -p evocc-parser -p evocc-lower
--lib -- -D warnings`, fmt, Ruff, diff-check and the instruction-shape audit
pass. An implementation compile initially used nonexistent greater-than
comparison variants; reversing operands for the existing comparisons fixed
that development error, which was not counted as RED or PASS.

Both GCC-14 and Clang-19 link and execute the subject assembly generated from
`tests/fixtures/wide-checked-arithmetic.c`, exit zero; the four reference
executions and Clang runtime prerequisite are in its adjacent `.out` file.
The full gate, layout/replay, full survey and benchmark campaign remain
deferred under the tailored-first instruction; all campaign numbers stay
at their last measured values.

### Hosted wide division and spill adjacency

Tests-only `27d2b6e` records 38 passing tests, nine semantic failures and
zero build errors. Division/remainder now execute through the existing
width-aware integer kernels and lower to caller-clobber-aware runtime calls,
with the original zero-divisor and signed MIN/-1 obligations intact.
The live-pair C fixture then exposed a separate E4020: spill reloads/stores
broke the required adjacency of carry-producing and consuming instructions.
Tests-only `db5351e` pins that reproducer plus two deterministic spill cases.
Spilling now treats each flags chain as indivisible and saves a final
flag-producing instruction's results on each conditional outgoing edge.
Neither validator was relaxed.

With `CARGO_TARGET_DIR=target/frontend`, all commands use
`--locked --offline -j 1`. `cargo test -p evocc-chir --test wide_integer_boundary
-p evocc-evoir --test wide_integer_boundary -p evocc-lower --test
wide_integer_values --test fail_closed_forms -p evocc-mir --test
section5_wide_integers --no-fail-fast` passes 44 tests. The machine targets
`section9_wide_integers_natively`, `section7_linear_scan`,
`section7_allocation_verifier`, `section7_out_of_ssa`,
`section4_inline_assembly` and `section6_variadic_and_assembly` pass 43 more;
`cargo test -p evocc-mir --lib spill_flag_tests` passes two, and the exact CLI
test `wide_division_keeps_live_pairs_across_helper_calls` passes one.
Thus 90 distinct tailored tests pass, with repeated native runs not added
again. Affected-library Clippy with `-D warnings`, fmt, Ruff and diff-check
pass. A first test-development compile referenced a non-reexported validator;
it was corrected before the spill RED commit and was not counted as RED.

Both sealed compilers execute `tests/fixtures/wide-division.c` at O0/O2;
both also link and execute the final subject assembly, all six exiting zero.
The existing wide-form native matrix reports 146,157 checks on Darwin and
146,196 on Linux, zero failures; those counts cover all tested wide forms,
not division alone. The runtime-obligation tests now use volatile inputs
so they do not accidentally test the separately authorized constant-wrap rule.
Wide checked arithmetic was still separate work at that checkpoint and is
subsequently addressed above. All campaign counts are unchanged.

### Hidden symbols and explicit object sections

Tests-only `11a9c29` records six semantic failures and zero build errors.
Hidden visibility now reaches definitions, declarations, aliases and native
symbol directives; strong hidden addresses use direct page relocations while
weak hidden addresses retain nullable GOT access. Explicit object sections
preserve alignment and relocations, including zero-initialized PROGBITS and
the validated NOBITS boundary. EvoMIR reuses the canonical global text reader
and writer instead of losing relocations through its old duplicate path.

Tailored commands are `cargo test -p evocc-evoir --test symbol_linkage --test
function_linkage --test section9_completeness -p evocc --test
external_symbol_lowering --locked --offline --no-fail-fast -j 1` and
`cargo test -p evocc-mir --test section8_external_symbols --test section8_linkage
--test section8_emission --test section8_zero_runs --locked --offline
--no-fail-fast -j 1`, with `CARGO_TARGET_DIR=target/frontend`.
They pass 117 distinct tests, zero failed or ignored; the six initially
filtered metadata tests are included, not added again. The first regression
attempt exposed one stale sentinel header after adding hidden visibility;
the expected canonical header was corrected and the test rerun, not disabled.
Affected-library Clippy with `-D warnings`, fmt, Ruff and diff-check pass.

Both sealed compilers execute `symbol-sections.c` with its reference caller
at O0/O2, and both assemble/link and execute the subject's Linux output,
all six exiting zero. ELF inspection checks hidden undefined/defined symbols,
the alias, direct versus GOT relocations and sections of size/alignment 4/64
and 8/8. Conflicting attribute probes and their exact reference outcomes are
in `tests/fixtures/symbol-sections.out`; E2943 refuses disagreements rather
than silently picking a reference. These are object-section/default-hidden
visibility checks, not full ELF attribute support or a newly passing kernel
boot. All original campaign counts remain unchanged.

### Ancestor frame addresses

Tests-only `a852841` records 30 passing guards, four semantic failures and
zero build errors. Frame depths now pass through the unsigned-int operand
conversion into canonical EvoIR and machine metadata; native lowering follows
saved frame links, with a bounded-size loop for depths above two. The loop
uses reserved x16 without changing condition flags. Abstract interpreters
retain the reached-only E3031 boundary for physical ancestor queries.

`CARGO_TARGET_DIR=target/frontend cargo test -p evocc-lower --test
activation_addresses --test fail_closed_forms -p evocc-evoir --test
activation_addresses --test section9_completeness -p evocc-mir --test
section5_activation_addresses -p evocc-parser --test compile_time_builtins
--locked --offline --no-fail-fast -j 1` passes 42 distinct tests, zero failed
or ignored. The updated native fixture checks depths 1, 2 and 3 through an
over-aligned leaf frame; GCC-14 and Clang-19 at O0/O2 all return zero with
`-fno-omit-frame-pointer -fno-optimize-sibling-calls`.
The repaired debug compiler's Linux assembly also executes with exit zero
when assembled/linked with each sealed toolchain (Clang uses the established
`-fno-integrated-as` path). Affected-library Clippy with `-D warnings`, fmt,
Ruff and diff-check pass; the textual shape audit reports 43/43 and no
disagreement. These are tailored native executions, not a full lane replay.

Nonzero return-address queries remain E2913: the sealed runtime probe returns
zero addresses under GCC but caller link-register values under Clang. Depth
range conflicts and unsigned narrowing are separately pinned in
`tests/fixtures/ancestor-frame-operands.out`; accepting one compiler's answer
is not consensus. No campaign result changes from these tailored checks.
The broad workspace all-targets check was intentionally interrupted (exit
130) and deferred to final integration, not recorded as PASS.

### General and memory assembly operands

Tests-only `2e195f8` records three semantic failures, five passing guards and
zero build errors. The reviewed `m`/`=m`/`+m` forms now use the existing
address-based Q forms, while input `g` selects the existing register form.
Parser object checks admit writable arrays without accepting const subobjects
or non-addressable inputs. The IR's canonical constraint vocabulary and its
validator are unchanged; memory operands cause no scalar load or writeback.

`CARGO_TARGET_DIR=target/frontend cargo test -p evocc --test
inline_assembly_lowering -p evocc-parser --test gnu_asm -p evocc-evoir --test
inline_assembly --locked --offline --no-fail-fast -j 1` passes 60 tests,
zero failed or ignored, including native operand and goto regressions. The
final expanded fixture repeats the eight `memory_` tests successfully;
these are not eight additional distinct tests. `cargo clippy -p evocc-parser
-p evocc-evoir -p evocc-lower --lib --locked --offline -j 1 -- -D warnings`,
fmt, Ruff and diff-check pass. Both sealed compilers execute the final
fixture at O0/O2, and both Linux toolchains execute subject assembly, all
six exiting zero. The initial array write-only probe was corrected after
GCC O2 exposed its invalid expectation of an unwritten element; no compiler
change or subject PASS is justified by that failed reference attempt.
All full-campaign counts are unchanged.

### Union member values and static initializer dependencies

Tests-only `b589d6f` records three semantic failures and zero build errors.
Member casts now select an actually compatible member and reuse its checked
initializer, keeping an outer rvalue cast instead of inventing another IR
representation. E2942 rejects incompatible types and the measured narrow
bit-field conflict; static member values still require constant initialization.
The C-HIR static dependency closure now visits aggregate-copy expressions as
well as scalar stores, including ordinary nested compound initializers.

`CARGO_TARGET_DIR=target/frontend cargo test -p evocc-parser --test
aggregate_copies -p evocc-lower --test record_call_abi --test
static_storage_and_discard --test relocations_and_addresses --locked
--offline --no-fail-fast -j 1` passes 61 tests. `cargo test -p evocc-chir
--lib --locked --offline -j 1` passes another 41, including deep initializer
and fuel checks: 102 distinct tests, zero failed or ignored. `cargo clippy
-p evocc-parser -p evocc-chir -p evocc-lower --lib --locked --offline -j 1
-- -D warnings`, fmt, Ruff and diff-check pass.
The expanded `tests/fixtures/union-member-casts.c` executes with zero exits
under sealed GCC-14 and Clang-19 at O0/O2; fresh debug subject assembly also
links and executes with zero exits through both toolchains (Clang uses GNU
`as`). Original torture and milestone results remain at their measured
campaign values; this is local verification, not campaign closure.

### Zero-byte storage and empty record arguments

Tests-only `8254d99` records five semantic failures, zero build errors.
Object allocation now accepts zero bytes without making zero-sized memory
accesses valid; empty initializers and copies preserve lifetime checks.
The text reader accepts the renderer's empty byte payload only for size zero,
and record marks retain their alignment checks while admitting empty records.
Linux argument placement no longer inserts an alignment filler for an empty
record. The strengthened MIR test checks the incoming argument count, not
just an incidental `mov` that also appeared in the incorrect sequence.

`CARGO_TARGET_DIR=target/frontend cargo test -p evocc-memory --lib --test
empty_objects --locked --offline -j 1 --quiet` passes 31 tests. `cargo test
-p evocc-lower --test static_storage_and_discard --test
relocations_and_addresses -p evocc-evoir --test record_marks --test
day7_object_memory_red -p evocc-mir --test section6_records --locked
--offline --no-fail-fast -j 1 --quiet` passes 63: 94 distinct tailored tests
in total, zero failed or ignored. The two interpreters agree on E3011 for
reached byte accesses and E3010 for a dead zero-byte local.
`cargo clippy -p evocc-memory -p evocc-chir -p evocc-evoir -p evocc-lower
-p evocc-mir --lib --locked --offline -j 1 -- -D warnings`, fmt, Ruff and
diff-check pass. A fresh debug driver emits the shared fixture with
`--emit=asm --target=aarch64-unknown-linux-gnu`: both complete-subject native
runs and all four `-DONLY_CALLER`/`-DONLY_CALLEE` mixed links with sealed
GCC-14/Clang-19 exit zero. Clang uses GNU `as`; its existing address warnings
and the assembler's zero-length `.space` warnings are not hidden as errors
or claimed absent.
The unchanged full-campaign counts remain the benchmark authority.

### Static absolute addresses without invented provenance

Tests-only `0c9bc5f` records four semantic failures and zero build errors:
two lowering tests report E3026 and two memory tests reject static address
bytes. The reviewed S12 rule is now implemented using existing global bytes,
not fake symbols or a new IR instruction. Intermediate integer casts and
conditional arithmetic conversions are preserved. C-HIR completes static
initializer representations consistently with EvoIR, retaining actual symbolic
pointer fragments and leaving arbitrary addresses unexposed.

`CARGO_TARGET_DIR=target/frontend cargo test -p evocc-memory --lib -p
evocc-lower --test relocations_and_addresses --locked --offline --no-fail-fast
-j 1 --quiet` passes 53 tests. The follow-up `cargo test -p evocc-lower
--test relocations_and_addresses --test static_storage_and_discard -p
evocc-chir --lib --locked --offline --no-fail-fast -j 1 --quiet` passes 82;
these commands cover 111 distinct tests, not 135. The final fixture also
pins `(void *)(1 ? -1 : 1U)` to `0xffffffff`, not a sign-extended value.
The two sealed compilers execute the fixture at O0/O2 with four zero exits;
debug EvoCC assembly linked by each sealed toolchain also exits zero (two
runs). `cargo clippy -p evocc-memory -p evocc-chir -p evocc-lower --lib
--locked --offline -j 1 -- -D warnings`, fmt, Ruff and diff-check pass.
No frozen benchmark count or original-program verdict is changed by these
tailored results.

### Wide-field consensus and conflicting precision contexts

Tests-only `36288de` records three parser semantic failures and one lowering
failure, with no build error. The consensus fixture covers the signed 52/53-bit
XOR/OR and unsigned 35-bit sign checks behind `991118-1`, `bf64-1` and
`bf-sign-2`, plus unsigned stored updates, returned values and neighboring bits.
Both sealed compilers execute it at O0/O2 with zero exits.

The parser now checks the consuming operation instead of refusing every wide
field promotion. It retains precision through parentheses, comma, conditionals
and update results, and only discharges an unsigned sign comparison when both
possible precisions are unsigned. A macro-span regression prevents unrelated
refusals from being erased. The additional probe found negative comparisons
and mixed signed-long operations disagree even though positive comparisons
and unsigned-long arithmetic agree; those shapes remain E2939, with exact
outputs in `tests/fixtures/wide-bit-field-contexts.out` and Appendix D.

`CARGO_TARGET_DIR=target/frontend cargo test -p evocc-parser --test
unsupported_bit_field_semantics -p evocc-lower --test
static_storage_and_discard --locked --offline --no-fail-fast -j 1 --quiet`
passes 26 tests (nine parser, 17 lowering), no failures or ignored tests.
The fresh debug subject's assembly for the consensus fixture also links and
executes with zero exits under both sealed native toolchains.
These are local regression results; the original-program counts remain at
the last sealed campaign, and the full campaign is deliberately not rerun.

### Source-width constant arithmetic without weakening runtime checks

Tests-only commit `4675bd6` extends the existing RED with source-width
32/64/128-bit boundaries, integer promotions, surrounding assignments and
an unevaluated conditional arm. The focused CLI result before the repair
is two passing guards and one semantic failure, E3004 in both interpreters;
there is no build error. Both sealed reference compilers execute the shared
`constant-source-width.c` fixture at O0/O2 with exit zero.

The C-HIR interpreter and lowering now use the shared source-constant
knowledge and integer kernel for constant add/subtract/multiply/negation
that needs wrapping. Lowering's separate recursive integer evaluator is
removed. Executable constant knowledge preserves failing division, so a
known-branch shortcut cannot remove a reached MIN/-1 obligation. Ordinary
object reads are never propagated into constants; the inline-argument test
now covers both the approved literal result and the unchanged volatile
overflow diagnostic.

`CARGO_TARGET_DIR=target/frontend cargo test -p evocc --test
constant_source_width -p evocc-lower --test expression_value_flow --test
day6_lower_runtime_diagnostics --test day6_g1_equivalence_red --test
front_end_builtin_integrity --test wide_integer_values --test
static_storage_and_discard --locked --offline --no-fail-fast -j 1 --quiet`
passes 92 tests, zero failures or ignored tests. `cargo test -p evocc-chir
--lib --locked --offline -j 1 --quiet` passes another 41, including the
512-level static initializer and its fuel guard. Debug subject assembly
for the same fixture is piped into sealed GCC-14 and Clang-19 (the latter
with `-fno-integrated-as`); both native executables exit zero. No full
campaign or release measurement has run, and the frozen benchmark totals
remain unchanged. `cargo clippy -p evocc-chir -p evocc-lower --lib --locked
--offline -j 1 -- -D warnings`, fmt, Ruff and `git diff --check` pass.

### Variadic list subobjects use ordinary value copies

Tests-only commit `d9c90e8` pins the integer-only shape exposed by original
`va-arg-13`: the C-HIR interpreter rejects the projected list with
`function call nested outside scalar expression subset`, with no build error.
The parser's shared projected-lvalue conversion omitted `__builtin_va_list`.
It now retains the same checked place as a record subobject; varargs operand
validation recovers that place without admitting a list-valued call result.
No new value representation, interpreter operation or backend rule is needed.

`tests/fixtures/projected-variadic-lists.c` checks direct members, pointer
members, array elements, independent callee list copies, and one-time base
and index effects. Both interpreters and canonical text round-trip pass.
`CARGO_TARGET_DIR=target/frontend cargo test -p evocc-parser --test
front_end_introspection -p evocc-lower --test expression_value_flow --test
variadic_execution --test front_end_resolved_forms --locked --offline
--no-fail-fast --quiet -j 1` passes 93 tests, zero failures or ignored tests.
Parser-library Clippy with `-D warnings`, fmt and Ruff pass.

Both sealed reference compilers execute this fixture at O0/O2 with exit
zero. After `cargo build -p evocc --bin evocc --locked --offline -j 1`, the
debug subject's `--emit=asm --target=aarch64-unknown-linux-gnu` output for
both this fixture and `parameter-bound-scope.c` is piped directly into the
sealed GCC-14 and Clang-19 assembly/link commands; all four native runs exit
zero (Clang uses `-fno-integrated-as`). No host capture directory is created.
The original torture campaign remains NOT RUN for these repairs; the last
sealed counts and the pending source-width arithmetic RED remain unchanged.

### Prior parameters in declaration expressions

Tests-only commit `f4e0430` records four semantic failures and zero build
errors: missing or incorrectly shadowed prior parameters in array bounds.
The same defect silently selects the outer enumerator in
`int matrix(long width, int (*data)[sizeof(width)])`: the old release's two
interpreters return 16 for the reduced array-stride example, not 39.
The shared expression-symbol lookup now materializes the actual prior
parameter binding, including its storage and attributes, and final parameter
binding reuses that identity. A typeof operand rolls back its own declarations
without deleting parameter identities retained by builtin audit records.
Two additional targeted failures pinned lost attribute metadata (0 rather
than 1) and an inconsistent builtin record before their corresponding fixes.

`CARGO_TARGET_DIR=target/frontend cargo test -p evocc-parser --test gnu_typeof
--test day5_g2_bindings --test front_end_introspection -p evocc-lower
--test expression_value_flow --test front_end_resolved_forms --locked
--offline --no-fail-fast --quiet -j 1` passes 134 tests, zero failures or
ignored tests. After extending the shared fixture with the array-stride
case, the `expression_value_flow parameter_` filter passes all four selected
tests through both interpreters. The tracked fixture
`tests/fixtures/parameter-bound-scope.c` also exits zero with sealed GCC-14
and Clang-19, each at `-std=gnu11 -O0` and `-O2`. Attribute-query execution
was separately checked with GCC because Clang does not provide that builtin.
Parser-library Clippy with `-D warnings`, workspace fmt, Ruff and diff-check
pass. These are tailored results, not a workspace or benchmark closure;
the original-program campaigns and their existing counts remain unchanged.

### Unevaluated type queries and constant statement branches

`0d8a757` opens four semantic RED tests for `__builtin_classify_type`, all
E2001 and no build errors; `ab4b7c1` resolves the agreed expression categories
to an integer constant without evaluating the operand or adding an IR node.
The conflicting `_Bool` category is E2941, not a guessed reference answer.
Six resolved-form pairs retain byte-identical EvoIR. The selected lexer,
parser and C-HIR command passed 838 tests, and the three selected lowering
targets passed 80 tests before the following branch repair.

`fb11cf8` then records two E3101 RED failures in unselected statement arms.
`2a5b092` skips an arm only when constant selection and the existing
goto/case/addressed-label checks permit it; condition evaluation and runtime
obligations remain. The new fixture, direct goto entries, switch dispatch,
addressed labels and resolved forms pass 108 tests. The two original
`20040709` programs progress from integer `T(A)` to long-double `T(W)`;
their complete A–V integer diagnostic reductions execute correctly on the
subject and both references, without replacing either original benchmark.

The affected gate was actually run, not inferred from focused tests:
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true
tools/gate-changed.sh fb11cf8 --locked --offline --quiet -j 1` finishes with
677 passed / 1 failed / 3 existing ignored. The sole failure is the pending
C04 `constant_source_width` RED (E3004 in both interpreters). The first run
also found two stale text goldens from `e6ab6cf`'s expression-edge selection;
their exact text and negative-mutation anchors were updated, preserving the
expected values and independent SSA checks, and the complete affected set
was rerun. No ignored test was added. This is not a green workspace claim.

The gate's same six crates pass a separate all-targets Clippy command with
`--locked --offline -j 1 -- -D warnings` (12m57s). Fmt, Ruff, diff-check and
29 benchmark Python tests pass. The original layout commands with `--subject`
pass 128/128 (`20260909T215052Z-43130`) and 22/22
(`20260909T215214Z-43670`), each with zero mismatch and oracle consensus.

Release subject `5aabf60`, SHA-256
`f2d9d58cf3556b2c32f17cb13ac1a7ba3bb8e43a4d44f739511ac17a4622d9cd`,
passes both regression fixtures through both native toolchains; Clang uses
`-fno-integrated-as` for the emitted assembly syntax. The five source-bound
FP dispositions in `5aabf60` are evidence-backed classifications, not new
subject passes. Probe commands and outcomes are in
`.evidence/builtin-probes/20260910-type-classification/results.txt` and
`.evidence/builtin-probes/20260910-floating-execution/results.txt`.

`CARGO_TARGET_DIR=target/frontend python3 -u tools/bench.py torture --no-build
--jobs 2` finishes wrapper `20260909T215349.472835Z` and capture
`20260909T215349Z`. All 1,692 identities, options and raw outcomes match
`20260909T201535Z`: 1,269 raw pass / 328 refused / 89 not-applicable /
5 runtime fail / 1 link fail. Reviewed totals are 1,268 PASS / 268 compiler
failure / 145 not-applicable / 11 FP-deferred / 0 oracle-unusable. Zero
previous raw passes or validated PASS are lost. The five classification
changes above account for the entire compiler-failure reduction; no new
whole-program PASS is claimed. The benchmark remains INCOMPLETE. Csmith
and the other unselected lanes retain their earlier measurements, not new
results from this binary.

The existing independent `git clone --no-local` was fast-forwarded to
`5aabf60` and replayed offline with its own `target/replay`, without borrowing
the main build cache or copying evidence. Its 13 selected targets pass
196 tests / 0 failed / 0 ignored: C-HIR initializer schema; parser attributes,
pointer attributes, pointer conversions and compile-time builtins; EvoIR
assembly/bits and textual completeness; lower expression flow, varargs,
resolved forms, scalar goldens, switch labels and computed-goto boundaries.
Command: `CARGO_TARGET_DIR=target/replay cargo test -p evocc-chir --test
g2_initializer_schema -p evocc-parser --test gnu_attributes --test
gnu_pointer_attributes --test character_pointer_conversions --test
compile_time_builtins -p evocc-evoir --test module_assembly_and_bits --test
section9_completeness -p evocc-lower --test expression_value_flow --test
variadic_execution --test front_end_resolved_forms --test
day6_g1_equivalence_red --test switch_labels --test computed_goto_lowering
--locked --offline --no-fail-fast --quiet -j 1`. This focused independent
replay does not override the explicit C04 failure in the affected gate.

### Typed builtin termination and original-program replay

`fe20a3f` records three failing semantic tests, with zero build errors.
`be5c3f5` implements `__builtin_trap()` as the existing typed-terminator
family's `trap.builtin_trap`, reporting E3047 when reached and emitting
`brk #1000` on AArch64. It remains distinct from `__builtin_unreachable`:
neither grants a dead-edge assumption. Unevaluated and untaken occurrences,
comma expressions, inline substitution, textual round-trip, validation and
both interpreter paths are covered. No fake return value or external call
is introduced. The compact oracle/native probe record is
`.evidence/builtin-probes/20260910-builtin-trap/results.txt`.

Release subject `691b654`, SHA-256
`6e218a8583dbaf404f032ba03a393575c469ca2c84e48dec25661afeeda4a1fc`,
was measured by `CARGO_TARGET_DIR=target/frontend python3 -u tools/bench.py
csmith torture --no-build --jobs 2`, sequential wrapper
`20260909T193124.118062Z`. Csmith capture `20260909T193124Z` retains all
1,000 identities, all emit, 932 agree and 68 remain oracle-unusable;
every result row is identical to `20260909T162714Z`, including all five
execution statuses and checksums. Generator identity, options and runtime
digests are unchanged. The applicability notebook now names all 68 inputs,
separating four identified repeating source paths from 64 undetermined ones.

Torture capture `20260909T201535Z` retains all 1,692 identities and options,
with zero lost raw passes or validated PASS. `20020418-1` and `pr125291`
now pass unchanged; both also pass the four sealed GCC-14/Clang-19 O0/O2
reference runs. Raw totals are 1,269 pass / 328 refused / 89 not-applicable /
5 runtime fail / 1 link fail. Reviewed totals are 1,268 PASS / 273 compiler
failure / 145 not-applicable / 6 FP-deferred / 0 oracle-unusable.
The third trap original, `pr115033`, reaches the explicitly excluded
`__builtin_exit`; Clang rejects the unchanged source at both optimization
levels. `691b654` binds that exclusion to the source and probe rather than
claiming a third repaired program. The older `pr124358` oracle conflict
also remains separate from its raw subject success. Other lanes were not
remeasured by this two-suite invocation, which correctly exits INCOMPLETE.

The focused Rust regression command passes 158 tests across twelve target
summaries, zero failures. Whole-workspace all-targets Clippy passes with
`CARGO_TARGET_DIR=target/frontend cargo clippy --workspace --all-targets
--locked --offline -j 1 -- -D warnings`; fmt, Ruff, diff-check, and the
17 benchmark/disposition Python tests pass. These are not a fresh green
workspace test claim: the authority-pending constant-width RED is still open.
The subject layout commands pass 128/128 and 22/22, zero mismatches:
`bash tests/acceptance/day5-layout-conformance.sh --subject` (capture
`20260909T192907Z-30945`), and the same command with
`C30_LAYOUT_CASE_SET=attributed` and
`C30_LAYOUT_MANIFEST=tests/contracts/layout-attributed-cases.json`
(capture `20260909T193015Z-31682`). The first attributed invocation,
`20260909T192939Z-31199`, omitted the case-set selector and stopped at its
manifest check before executing any case; it is retained, not counted PASS.

A separate `git clone --no-local` of `691b654`, with its own empty
`target/replay` and no copied evidence, passes 138 tests across nine targets,
zero failed or ignored, with the stable toolchain and this offline command:

```sh
CARGO_TARGET_DIR=target/replay cargo test \
  -p evocc-chir --test g2_initializer_schema \
  -p evocc-parser --test gnu_attributes --test gnu_pointer_attributes \
  --test character_pointer_conversions \
  -p evocc-evoir --test module_assembly_and_bits --test section9_completeness \
  -p evocc-lower --test expression_value_flow --test variadic_execution \
  --test front_end_resolved_forms --locked --offline --no-fail-fast --quiet -j 1
```

This replays the changed semantic families, not all workspace tests. The
separate `constant_source_width` RED remains one PASS / one FAIL in its last
recorded run; the architecture review explicitly reserves its body revision.
The refreshed remote remains `33bc3d4`, so no new authority is inferred from
network recovery. The broader non-FP completion condition is still unmet.

### Bracket attributes and original-program replay

`b932d53` adds the tests-only attribute RED (five declared failures, no
build errors); `790db6d` shares GNU attribute handling while retaining
bracket namespaces and attachment to declarations or derived types.
Unknown namespaces cannot silently enable GNU layout, aliases or inlining.
Sealed layout conflicts remain refused. A further malformed-name regression
first fails on `[[u8"bad"]]`, then passes after the complete spelling is checked.
The parser/C-HIR component command recorded below passes 669 tests before that
last guard; the final seven focused targets pass 134 tests, zero failures.
Library Clippy, fmt, Ruff and the 14 benchmark-tool tests pass. This is not a
fresh full workspace gate: the separately recorded constant-width RED remains open.

Release subject `0775488`, SHA-256
`414bb8358a4bea82b793bdd0bae2aa77774ed651f44094e037ba36d1bc1217c2`,
passes `tests/fixtures/bracket-attributes.c` through both interpreters and
both sealed native assembly/link/execution paths. The unchanged full torture
replay (`python3 -u tools/bench.py torture --no-build --jobs 2`,
`CARGO_TARGET_DIR=target/frontend`) is `20260909T182954Z`, wrapper
`20260909T182954.542810Z`: 1,692 identities and options unchanged, zero lost PASS.
Raw outcomes are 1,267 pass / 330 refused / 89 not-applicable / 5 runtime fail /
1 link fail. The only raw transitions are `pr123978` and `pr124358`, both
refused to pass. R01 excludes the independently measured oracle conflict on
`pr124358`; the source-bound `stdarg-4` disposition now records reached FP.
Thus the five-way result is 1,266 PASS / 276 compiler failure / 144 not-applicable /
6 FP-deferred / 0 oracle-unusable, not 1,267 validated PASS. `pr125291` clears
bracket parsing and now exposes the still-unimplemented `__builtin_trap`.
No Csmith, kernel or library result is refreshed by this torture-only run.

### Variadic inline objects and qualified character pointers

`1ee0d18` records the integer-only inline `va_list` regression: lowering
reports E3101 for the parameter initializer, while both sealed compilers
at O0/O2 execute the fixture with return zero. `bd084c1` makes substitution
and initializer validation copy the complete 32-byte list object, as the
existing ordinary-call path already does. `ae35b6f` extends the fixture
through an inline list return passed to an ordinary callee; original C-HIR,
substituted C-HIR and EvoIR all execute it correctly. The initializer schema,
expression-value-flow and variadic-execution suites pass 59 tests, zero
failures, with `CARGO_TARGET_DIR=target/frontend cargo test -p evocc-chir
--test g2_initializer_schema -p evocc-lower --test expression_value_flow
--test variadic_execution --offline --no-fail-fast -j 1`.

`5fad412` records a second tests-only RED: assignment through `char **`
rejects an immediate qualified character pointer even though other conversion
sites accept it. `9a69f0e` applies the same character-pointer rule there,
without admitting nested qualifier removal or writes through const places.
The sealed GCC-14/Clang-19 O0/O2 fixture and 36-combination matrix all return
zero. `CARGO_TARGET_DIR=target/frontend cargo test -p evocc-parser
--test character_pointer_conversions --test parameter_and_pointer_sign_conversions
--test modifiable_lvalue_types --test day5_g2_lvalues -p evocc-lower
--test expression_value_flow --test variadic_execution --offline
--no-fail-fast -j 1` passes 97 tests, zero failures. Parser/C-HIR library
Clippy with `-D warnings`, fmt and Ruff pass. These are focused results;
they do not imply a fresh workspace or isolated-replay total.

Subject `9a69f0e`, release SHA-256
`120cce232c35d20256042511b30f0cb8ddf49102cb01ed3481fe77839c166f00`,
executes both fixtures with return zero through C-HIR, EvoIR, and the emitted
AArch64 assembly independently linked and run with GCC-14 and Clang-19:
eight checks, all successful. `python3 -u tools/bench.py ctest torture
--no-build --jobs 2`, wrapper `20260909T171506.687331Z`, retains all original
identities and every prior PASS. ctest remains 209 PASS / 4 compiler failure /
7 FP deferred. Torture changes from 1,264 to 1,265 PASS: original
`20121108-1` is repaired. Its other five-way counts are 279 compiler failure,
143 evidenced not applicable, and 5 FP deferred. The overall gate remains
INCOMPLETE, not an all-program success. Native captures are
`.evidence/ladder/e0/20260909T171506Z/` and `20260909T171533Z/`.

Five separately probed original torture programs now have source-bound
FP dispositions: `20020227-1`, `20020411-1`, `960512-1`, `pr38969`, and
`pr42248`. Each executes floating complex values in a reached call path;
all 20 sealed compiler/optimization runs exit zero. These are deferrals,
not passes. The mixed `stdarg-4` program was deliberately not classified
from its floating declarations. Original replay now clears its inline-list
initialization failure at line 19 and reaches the floating
`va_arg(ap, double)` at line 9. All four sealed original-source
GCC-14/Clang-19 O0/O2 runs return zero; the integer-only reduction's four
subject paths also pass. The original still has a compiler-failure label
in this frozen run, not a fabricated PASS; its newly established FP
frontier is recorded for the next classification.

The complete parser/C-HIR component run passes 664 tests, zero failures
and zero ignored, including doctests:
`CARGO_TARGET_DIR=target/frontend cargo test -p evocc-parser -p evocc-chir
--offline --no-fail-fast --quiet -j 1`. The separate
`cargo test -p evocc --test constant_source_width --offline --quiet -j 1`
remains one PASS / one FAIL: both interpreter modes emit E3004 for the
authority-pending constant-context rule. The broad reverse-dependency
gate was interrupted during compilation (exit 130); it is INCOMPLETE,
not a full green gate or an isolated replay. No RED was disabled.
The subsequent three-crate all-targets lint finishes successfully:
`CARGO_TARGET_DIR=target/frontend cargo clippy -p evocc-chir -p evocc-parser
-p evocc-lower --all-targets --offline -j 1 -- -D warnings` (exit 0).

### Constant edge values and original kernel object replay

`9d4d4a7` adds a tests-only wrong-value regression: the focused suite reports
17 passes and one failure, with no build errors. For
`if ((1 ? -1 : 0U) < 0)`, the C-HIR interpreter and sealed GCC-14/Clang-19
at O0/O2 return zero, but the previous lowering returns one. `b7afc89`
reuses the existing C-HIR integer type query to apply the conditional common
type before selecting an edge, including omitted-middle conditionals;
checked runtime arithmetic is unchanged.

The six focused lower suites pass 115 tests with zero failures:
`cargo test -p evocc-lower --test switch_labels --test expression_value_flow
--test computed_goto_lowering --test static_storage_and_discard
--test front_end_resolved_forms --test day6_g3_differential --offline
--no-fail-fast`. C-HIR/lower library Clippy with `-D warnings`, fmt and Ruff
pass. The preceding broad `gate-changed.sh 33bc3d4 --offline` cold build was
interrupted after the new wrong value was confirmed; it is INCOMPLETE, not
a passing gate. The separate authority-pending constant-overflow RED remains
open, and no new workspace or isolated-replay total is claimed.

Release digest `de76e7d611b6fa7660922b8f78f7a56793fba973b6abb40aeaf2235de9b3b46b`
executes both `conditional-common-type.c` and `constant-switch-linkage.c`
with return zero in C-HIR, EvoIR, and the emitted assembly linked and run
using each sealed compiler. The original Kbuild replay
`.evidence/k1/constant-edges-20260909T1621Z/` assembles all three selected
objects (`debug-monitors.o`, `entry-common.o`, `configs.o`) with exact
defined-symbol parity. It installs nothing and performs no kernel relink
or boot; these are targeted checks, not an updated K1 milestone count.

### Original-kernel lowering moves past the assembly-label frontier

The sealed `20260909T112601Z-99781` capture measures `e04fc4c`, executable
`45c9060ad238d43b49876534818c73461529a26de51a179afd2ce52ea28d9823`:
2,008 clean and 790 diagnosed out of the same 2,798 units, with no panic,
timeout or unavailable result. All 5,603 capture files were digest-checked.
Against `20260909T031003Z-16402`, 1,918 diagnosed units become clean and
all 90 previously clean units remain clean. This is a combined implementation
measurement, not 1,918 gains attributable to one patch or a native kernel result.

The first-diagnostic counts are E3101 687, E3026 86, E3021 7, E2939 4,
E3030 2, and E3029/E3031/E3034/E3046 one each. Source-level subclasses
include zero-byte globals 372, unreachable CFG blocks 225, duplicate labels
21, unsupported integer initializer evaluation 20, and array subvalues 8;
the complete breakdown is in the acceptance receipt. Zero-byte globals,
record ABI metadata and pointer scaling remain subject to the pending body
correction, not a validator relaxation.

The run used two workers and a 60-second bound. Median unit wall time was
2,434.169 ms; maximum was 18,535.361 ms for `hns3_enet.c`. Focused single-job
debug compilations occasionally ran concurrently in an isolated worktree,
so these are coarse observations, not controlled performance comparisons.

Subsequent RED/GREEN repairs use canonical composite call prototypes,
preserve dereferenced array addresses, share flexible-record initialization
and full-element storage extent, retain conditional edges to labels nested
inside expressions, and admit same-record casts without dropping volatile
reads. Original `hcd_queue.c` lowers successfully in a targeted debug replay.
Original ctest `00216` advances past initialization and record casts but still
hits the zero-byte-object boundary. Neither targeted observation changes the
sealed numbers above; original benchmark replays remain required. The disputed
wide-bit-field shift prototype is not merged.

The dependent gate on `a146cdd`, `CARGO_TARGET_DIR=target/frontend
CARGO_BUILD_JOBS=2 tools/gate-changed.sh e04fc4c --offline --quiet`, finishes
with 1,311 passed, one failed and three existing ignored tests across 196
test-result groups. The sole failure is the retained
`constant_arithmetic_preserves_the_source_integer_width` RED (C04); its body
revision is still pending. This is not a green workspace or an isolated replay.
The separate full-workspace all-targets Clippy run was interrupted to release
the shared Cargo lock; it is INCOMPLETE, not PASS. Formatting and Ruff passed.

`74c04b5` / `b438861` add and repair block-only extern storage, exclusion of
same-spelled local symbols from the external table, and expression extension
prefixes. Their focused execution checks pass 76 Rust tests, including both
interpreters and native expression evaluation; 118 ladder Python tests and
`cargo clippy -p evocc-parser -p evocc-lower --lib --offline -- -D warnings`
pass, as do fmt and Ruff. The superseded include-next rewrite helper and its
two tests are removed, replaced by direct byte-preservation checks of the
exported headers. Original benchmark replay without the source adaptations
remains required; the for-declaration oracle conflict remains refused.

### Variadic register preservation and internal-body original-program replay

`CARGO_TARGET_DIR=target/frontend python3 -u tools/bench.py musl --no-build
--budget 45 --jobs 2` measured `7f91fd2`, executable
`45c9060ad238d43b49876534818c73461529a26de51a179afd2ce52ea28d9823`, in
wrapper run `20260909T112011.563321Z` and musl capture
`.evidence/ladder/musl/20260909T112011Z/`. All 1,007 assembled units
substitute together and the first libc-test round has no new failures.
The original 17 reference failures remain; none becomes a subject PASS.
Compared with the previous receipt (`20260909T070307Z`), assembly rises
995 to 1,007 and units in a passing build rise 498 to 1,007, without
symbol-parity exclusions or exhausting the bisection budget.

This measures the vector-register save repair, incomplete external-object
addresses, array alignment and internal-function retention together. The
earlier sealed wrapper probe isolates the variadic ABI defect; the full
libc replay now verifies its integration. Retention keeps ordinary source
checks, transitive calls, addresses, aliases and assembly references, and
preserves surviving allocation origins. Static linkage is inherited from
the canonical symbol even when a later definition omits `static`.

The lane remains INCOMPLETE: 332 units are refused (219 E3101, 83 E1006,
14 E2001, eight E2203, five E2939, and one each E1005/E3021/E3030).
E3101 includes two canonical-call-arity mismatches and a VLA layout refusal;
it is not a floating-point disposition by itself. The broad changed gate
still has its declared constant-evaluation failure, not a full-green claim.
The pending wide-field narrowing is not in this measured executable.

### Full-width shift-count edge awaiting body correction

`width-edges.c` in `.evidence/builtin-probes/20260909-wide-bitfield-shapes/`
compiles and runs under GCC-14 and Clang-19 at O0/O2. For a 40-bit field
holding `0xffffffffff`, `field << 1UL` remains a conflict: GCC gives
`0xfffffffffe`, Clang gives `0x1fffffffffe`; same-width addition and
multiplication agree. The exact command and output are in `width-edges.out`.
Revision 1.9.90 currently groups left shift with usual-conversion arithmetic;
the proposed correction is to retain E2939 for a wide-field left operand
regardless of the shift count's width. Authority was requested; the body is
unchanged. This does not invalidate the consensus mallocng multiplication
or the independent vector-save and incomplete-extern repairs.

### Target-selected wide character original-program replay

`python3 -u tools/bench.py ctest --jobs 2`, subject `b54ae63`, wrapper
`20260909T085125.845803Z`: 209 PASS / 11 refused out of the original 220.
The five-way counts are 209 / 4 / 7 / 0 / 0; the run is INCOMPLETE.
Capture: `.evidence/ladder/e0/20260909T085126Z`. Comparing every original
identity with `20260909T074238Z`, only `00220` changes (refused to PASS),
with no lost passes. Remaining compiler failures are `00140` (aggregate
varargs), `00144` (update operands), `00207` (VLA), and `00216` (initializer).

`3bbb5d8` shares the target's wide-character type across decoding, typing,
layout, initialization, and driver options. `tools/gate-changed.sh c461caf
--offline --quiet` passes 1,704 tests in 254 suites, with zero failures and
four existing ignored tests; Clippy and fmt pass. The first attempt exposed
two stale hosted-wide-string fixtures; both now assert the correct complete
16-byte object, rather than retaining the old short-wchar assumption.

A targeted replay using `tools/survey-cohort.py`'s existing `inventory` and
each selected unit's unchanged `projected_argv` passes both original EFI
units: `drivers/firmware/efi/libstub/file.c` (`8c75a470ff720055`, 1.768 s)
and `secureboot.c` (`c8917c6473e8338d`, 1.674 s). Both retain
`-fshort-wchar`, exit 0, and emit no diagnostics. These are two serial
targeted checks, not a new full-configuration survey.

### Function-name and storage-only original-program replay

`python3 -u tools/bench.py ctest torture zstd tinycc --jobs 2`, subject
`c461caf`, wrapper `20260909T074238.247563Z`, exits INCOMPLETE. ctest is
208 PASS / 12 refused (up one); torture is 1,259 PASS / 338 refused /
5 runtime failures / 1 link failure / 89 original-option exclusions.
The measured five-way counts are respectively 208 / 5 / 7 / 0 / 0 and
1,259 / 293 / 0 / 140 / 0 in PASS / compiler failure / FP deferred /
not applicable / oracle-unusable order. No source or identity changed.
Captures: `.evidence/ladder/e0/20260909T074238Z` and `20260909T074333Z`.

`00178` now passes: an unused `double` object's storage and `sizeof` do not
require floating-point execution. `288c101` also shares the C function-name
resolver and repairs pointer logical negation through the existing truth
conversion. Its 72 focused tests and changed gate against `bc2916c` pass
(the successful gate listing was truncated, so no aggregate is inferred).
Floating-point evaluation still fails closed. zstd rises from 2 to 6 of 44
units in a passing mixed build (`20260909T075101Z`); TinyCC rises from 7 to
8 of 11 (`20260909T075341Z`). Both have zero symbol exclusions.

Torture loses six raw passes to the reviewed refusal classes. Original
GCC-14/Clang-19 O0/O2 replay passes all 24 executions; these are not six new
oracle conflicts. The 15:30 review requires narrowing the wide-field rule,
so `991118-1`, `bf64-1` and `bf-sign-2` remain OPEN compiler failures.
Only the three explicit storage-order attributes are new reviewed-profile
exclusions. Their new disposition entries do not rewrite this run's sealed
counts. See [applicability evidence](benchmark-applicability.md).

The updated priority is the specified vector-register varargs save area,
incomplete external-object addressing, and the overly broad field refusal;
then the remaining memory/ABI and type work. None is hidden as deferred FP.

### Aligned-function and full-width original-program replay

`python3 tools/bench.py ctest torture --jobs 4`, subject `48f8da4`, wrapper
`20260909T062900.006209Z`: ctest is 207 PASS / 13 refused; torture is 1,265
PASS / 326 refused / 11 runtime failures / 1 link failure / 89 not-applicable.
Captures are `.evidence/ladder/e0/20260909T062900Z` and
`.evidence/ladder/e0/20260909T062923Z`. All 220 and 1,692 identities remain;
every one of the previous 1,228 torture PASS rows still passes. Of the 37
additional passes, 33 are now correctly executed after resolving original
requirements, and four are compiler repairs: `align-3`, `pr122943`,
`pr54471`, `pr85582-2`. `pr61375` and `pr63302` also remain PASS.
There are zero unresolved requirements. This run predates both the new
reviewed-disposition classifier and the typed wide-field/storage-order
refusals; it cannot serve as their execution evidence.

`9ba7b87` is the compiled semantic RED for wide-field promotions and scalar
storage order; `bc2916c` implements E2939 and E2940 without changing storage
or explicit conversions. Focused tests passed 85/85; the changed gate against
`d11b20e` passed tests, Clippy and fmt (the aggregate test listing was truncated,
so no total is inferred). Replaying the six original preprocessed programs
with `target/frontend/debug/evocc --parse-only` reports E2939 for `bitfld-3`,
`bitfld-5`, `pr32244-1`, `pr34971`, and E2940 for `20230630-2` and `20230630-4`.
These are deliberate refusals with separate oracle evidence, never passes.

### Signed-shift original-program replay

`python3 tools/bench.py ctest torture --jobs 4`, subject `67c4fdd`, wrapper
`20260909T050443.696884Z`: ctest is 207 PASS / 13 refused; torture is 1,228
PASS / 296 refused / 12 runtime failures / 1 link failure / 83 not-applicable /
72 requirement-unresolved. All 220 and 1,692 original identities remain.
The lower PASS total is not a compiler regression count: 33 earlier PASS rows
now have inapplicability evidence, and 35 need unresolved requirement handling.
Those 35 were not rerun and cannot support a no-regression claim.
Among programs actually executed in both runs, no prior PASS becomes a
compiler failure; eleven previous runtime failures and two previous refusals
become PASS. Raw per-program records remain the authority; this is not full
five-way closure and no unresolved requirement is called a pass.

### Qualified prefetch addresses

`35e20cd` is a compiled single-test RED: both interpreter CLI paths report
E2913 for a volatile-qualified object address. The parser now normalizes
only the hint's address operand to its existing prototype; ordinary calls
retain qualifier validation. All 55 focused integration tests pass, including
native execution, exactly two volatile pointer loads and one update store,
no hint call, and the existing arity/non-pointer refusals. The unused external
declaration is not an executed call; the initial name-absence assertion was
corrected to check the operation and observable accesses instead.
The eight-crate all-targets changed gate against `35e20cd`, Clippy, fmt,
Ruff and diff-check pass. Part of the test listing exceeded the terminal
output limit, so no exact aggregate test count is inferred from that run.
Original replay `20260908T213244.792960Z`, subject `9f6ad28`, preserves
all 220 ctest and 1,692 torture identities and every prior PASS.
`builtin-prefetch-3` changes from refusal to PASS: torture now has 1,283
passes, 382 refusals, 26 runtime failures and one link failure; ctest stays
206/220. Captures are `.evidence/ladder/e0/20260908T213244Z` and
`20260908T213308Z`. This qualified-address refusal is closed; the overall
benchmark remains INCOMPLETE and the final all-lane replay has not run.

### Function-pointer casts and the reached-call boundary

`b5837ce` contains two compiled RED tests: both interpreters/lowering refused
the cast before its call could be reached. Casts now preserve the function
identity while changing the static interface; lowering uses the existing
pointer/integer round trip. Argument evaluation still precedes the unchanged
dynamic E3019 check. The storage/cast-back fixture and argument-side goto
pass both interpreters and native execution, while an actually reached
incompatible call still reports E3019 in both interpreters. All 54 focused
integration tests pass. The changed all-targets gate against `b5837ce`
passes 1,273 tests, zero failures and three existing ignores; Clippy, fmt,
Ruff and diff-check pass. Original replay `20260908T210905.121630Z`, subject
`c7e4e28`, retains all identities and prior passes: `call-trap-1` changes
from refusal to PASS. Torture reaches 1,282 passes, 383 refusals, 26 runtime
failures and one link failure; ctest remains 206/220. Captures are
`.evidence/ladder/e0/20260908T210905Z` and `20260908T210929Z`. This closes
the identified cast/evaluation-order refusal, not the other call or ABI work.

### Deallocation builtin and assembly spelling

`b95f568` is the two-test compiled RED: E2001 for `__builtin_free`, E1001
for file-scope `__asm`, with the other 77 integration tests passing. Three
entries in the shared contextual-spelling/prototype tables implement the
two forms without a new intrinsic, interpreter library model or assembly
path. All 79 integration tests now pass, including native execution,
byte-identical assembly/IR for the alternate spelling and the retained
external-call and invalid-prototype diagnostics. The changed all-targets
gate against `b95f568` passes 1,267 tests, zero failures and three existing
ignores; Clippy, fmt, Ruff and diff-check pass. Original replay
`20260908T202104.952070Z`, subject `15b9d74`, preserves all 220 ctest and
1,692 torture identities and every previous PASS. `pr84339` and `pr84478`
change from E2001 to PASS: torture reaches 1,280 passes, 385 refusals,
26 runtime failures and one link failure; ctest remains 206/220.
Captures are `.evidence/ladder/e0/20260908T202105Z` and `20260908T202128Z`.
The assembly spelling moves its original sites past parsing, but their
remaining unsupported constraints are not passes or closure of that family.

The next correctness repair was nested designator continuation: the valid
`{ .inner.a = 11, 22 }` writes 22 into the outer following field rather than
`inner.b`. Both interpreters return the same wrong value, while GCC-14 and
Clang-19 at O0/O2 return zero from the value-checking reduction. Aggregate
brace elision after a designator (`.m = 0` in `pr33631`) and inferred-array
bounds need the same subobject cursor. Tests-only `6c38b38` has three
compiled failures and 48 passes. The shared full-path cursor now makes all
three value fixtures pass in both interpreters and natively; the dependency
gate against that checkpoint passes 1,271 tests, zero failures and three
existing ignores, with Clippy, fmt, Ruff and diff-check green. Original
replay `20260908T204756.423848Z`, subject `2bfe5f4`, retains all identities
and prior passes: `pr33631` changes from refusal to PASS. Torture now has
1,281 passes, 384 refusals, 26 runtime failures and one link failure;
ctest remains 206/220. Captures are `.evidence/ladder/e0/20260908T204756Z`
and `20260908T204820Z`. The identified continuation and brace-elision
defects are repaired and benched; the other initializer families stay OPEN.

### Alignment specifiers and non-object queries

`572a823` and `65430a9` are the compiled RED checkpoints: missing C11
alignment syntax, the non-object query refusal, and the incorrectly accepted
weak alignment on an incomplete array. The implementation uses one target
alignment query across the parser, C-HIR constant/interpreter paths and
lowering; neither void nor a function type becomes allocatable. `_Alignas`
reuses object/member attributes while rejecting invalid placements and weak
requests, preserving ignored zero requests without exposing them as a written
`aligned` attribute. The 47-test integration target passes both interpreters
and native fixtures. The 12-crate all-targets changed gate against `572a823`
passes 1,664 tests, zero failures and four pre-existing ignores; Clippy,
fmt, Ruff and diff-check pass. Fresh subject layout runs
`20260908T195100Z-76026` and `20260908T195228Z-76488` pass 128/128 and 22/22,
both with oracle consensus and zero mismatches. The intervening attributed
invocation `20260908T195159Z-76384` omitted its case-set selector and failed
manifest validation before measurement; it is not a passing run.
Original replay `20260908T195446.660965Z`, subject `0a25483`, executable
SHA-256 `609f5a0cd382a051960b8f2a12b919b8f05907be6402c6c70787ec86d1981fed`,
retains every identity and prior PASS: torture gains `20000223-1` and
`pr68532`, reaching 1,278 passes, 387 refusals, 26 runtime failures and one
link failure out of 1,692; ctest remains 206/220. Captures are
`.evidence/ladder/e0/20260908T195446Z` and `20260908T195511Z`.
The previously refused `align-3` now executes and fails: its function
declaration's explicit alignment was lost across redeclaration, so the
unattributed query default becomes visible. Its real code address also lacks
the requested alignment; returning 256 from the query alone would not fix it.
Function-header alignment metadata and actual emission have been submitted
for authority. This remains OPEN beside the over-aligned native-frame defect;
neither is counted as repaired by the two original-program gains.

### Obsolete initializer designators

`f6338a5` is the compiled tests-only RED: the field-colon fixture reports
E2001 before implementation. The parser now normalizes the obsolete field,
single-index and range spellings into the existing designation representation;
subobject resolution and initializer execution are unchanged. Both
interpreters and native Darwin pass the fixture, and out-of-object
designators retain E2906. Both sealed compilers pass all four O0/O2 runs
and reject the three malformed/multi-component omissions of `=`.
The all-targets changed gate against `f6338a5` passes 1,261 tests, zero
failures and three pre-existing standing-run ignores across eight crates;
Clippy, fmt, Ruff and diff-check pass. Original-program replay
`20260908T191239.871250Z` (subject `e829ba8`, executable SHA-256
`d2e1b1d72812e83057331d1b1a0cf2c37eec2acf755580f205efb49b195f74a9`)
retains every identity and prior PASS. The three original non-FP designator
programs change from REFUSED to PASS: torture is 1,276 passes, 390 refusals,
25 runtime failures and one link failure out of 1,692; ctest stays 206/220.
Captures: `.evidence/ladder/e0/20260908T191240Z` and `20260908T191304Z`.
This spelling family is closed; the other initializer/alignment work is not.
Evidence: `.evidence/builtin-probes/20260909-obsolete-designators/results.txt`.

### Block-local implicit calls, remaining bit counts and source dependencies

`0c6e054` contains compiled RED regressions for the block-only external
declaration failure and the `clrsb`/`parity` families. The block's C-HIR now
contains its implicit function declarations; symbol visibility and the
manifest's scope validation are unchanged. Five execution shapes include
nested blocks, a conditional return and an unevaluated `typeof` operand.
The six integer builtins use the shared integer kernel and existing EvoIR
bit operations, not new external calls or instructions. The 42-test CLI
integration target passes, including both interpreters and native execution;
the eight-crate all-targets changed gate against `0c6e054`, Clippy and fmt
pass. The terminal truncated part of the test listing, so no reconstructed
workspace test count is claimed for that invocation.

The original E0105 family is also a preparation defect: all seven programs
using `memcpy-ax.h` or `gcc_tmpnam.h` emit when given only their original
include directory. Five more include C files outside the sparse subtree.
`266d612` is the two-test RED. Preparation now exposes the pinned original
directory to both emission and diagnosis and retrieves only those five
referenced blobs from the same commit. Sources are not rewritten and no
program is removed. The seven preparation tests and thirteen wrapper tests
pass; the original-source directory and commit are recorded in each suite's
receipt. Original-program replay remains required for both repairs.

Original replay `20260908T184344.076686Z`, subject `93e8afa`, executable
SHA-256 `70b3f63ec3963f039162aeb379fcdb2753e26f62cd9a4227c3d105ea5568cfe2`,
retains all identities and every previous PASS. Torture adds 63 passes:
1,273 passes, 393 refusals, 25 runtime failures and one link failure out of
1,692. The 66 manifest scope errors and all 12 E0105 errors are zero;
`builtin-bitops-1` is PASS. Three previously refused originals now expose
further failures: `920730-1` and `loop-3b` report E3004; `941202-1` links
against a missing `alloca`. They are failures, not successes. Ctest remains
206 passes, 13 refusals and one failure out of 220. Captures:
`.evidence/ladder/e0/20260908T184344Z` and `20260908T184409Z`.
The declaration-placement and measured bit-count families are closed on this
replay; other GNU89, syntax, arithmetic and dynamic-storage work remains open.

The two new E3004 failures are not one rule. Both sealed compilers run both
originals successfully at O0/O2. GCC UBSan also accepts `920730-1`, whose
`INT_MAX + 2` is a compile-time fold, but diagnoses the original runtime
`i * 4` in `loop-3b` at line 18. Clang's sanitizer runtime is missing from
the sealed image, so that diagnostic check is NOT RUN, not agreement.
Section 10.1.19 currently keeps runtime obligations even for a literal
expression; the constant-folding difference must not be hidden by removing
the check. Evidence: `.evidence/builtin-probes/20260909-new-overflow-failures/results.txt`.

The alignment investigation exposes a separate existing native error:
`aligned(256)` stack objects are emitted at a merely 16-byte-aligned `sp`
offset. A recursive address-and-live-byte probe returns zero for both sealed
compilers at O0/O2, but the subject assembly returns one. This remains OPEN
under rows 4/16; admitting `_Alignas` syntax alone cannot close stack
alignment. The necessary exception to EvoMIR Section 6's fixed-offset frame
rule has been submitted for authority rather than silently changing the body.
Evidence: `.evidence/builtin-probes/20260909-alignment-specifiers/results.txt`.

### Explicit legacy integer declarations

`46228fa` is the tests-only RED for explicit GNU89 omitted integer types,
identifier-list parameters, undeclared calls, mode macros and exact option
propagation into emission and diagnosis. The default GNU11 behavior remains
strict. The implementation creates ordinary linked function declarations,
retains parsed qualifiers and rejects non-callable objects with the measured
E2937 rule; no missing function is replaced by an interpreter model.
Both sealed compilers agree on the positive and negative probes in
`.evidence/builtin-probes/20260908-legacy-declarations/results.txt`.
Both interpreters and native Darwin pass the execution fixtures. The focused
run passes 44 Rust tests and 18 Python tests. The changed all-targets gate
against `46228fa` passes 1,257 tests across eight crates, zero failures and
three pre-existing standing-run ignores; Clippy, fmt, Ruff and diff-check pass.
The first gate found two call-target regressions: missing-parenthesis syntax
must retain its diagnostic priority, and a precedence fixture had incorrectly
declared a called function as an integer object. Both are corrected and the
gate rerun is green. Original replay `20260908T181121.039466Z`, subject
`87ebd71`, executable SHA-256
`0b500b9f358caf8b7ae3c320fa88e3443e8b215032b203cabfcd96030a2231ff`
retains all original identities and outcomes: torture 1,210/1,692 and ctest
206/220 pass. Seventy E1001 first errors move, but 66 programs expose the same
misplaced block-scope implicit function declaration in the allocation manifest.
Captures: `.evidence/ladder/e0/20260908T181121Z` and `20260908T181148Z`.
The representation defect needs another RED and repair; neither GNU89
original-program closure nor complete mode conformance is claimed.

### Flexible character-array initializer storage

`4ca0c26` is the two-test RED for direct file-scope flexible members initialized
by narrow, wide or empty strings. The declared array type remains incomplete;
the shared string extent calculation supplies storage, including the terminating
zero, without changing `sizeof` of the record. Fixed arrays retain their exact
bound, including omitted terminators and zero-filled padding. Both interpreters
and native Darwin now execute the fixture successfully.
The separate RED `0131fa1` prevents accidentally admitting nonempty flexible
initializers nested in another record or union: GCC and Clang disagree on the
string form, so it remains E2202. Empty nested lists are accepted by both and
remain accepted; automatic flexible initialization retains E2919.
Sealed GCC-14/Clang-19 O0/O2 results and the negative admissions are recorded in
`.evidence/builtin-probes/20260909-flexible-strings/results.txt`.
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true
tools/gate-changed.sh 4ca0c26 --all-targets --locked --offline --quiet`
passes 1,986 tests across all 17 workspace crates, zero failures and four
pre-existing standing-run ignores; Clippy and fmt pass. The shared merge changed
Cargo.lock, so this is a full-workspace rather than five-crate gate.
The repair is `b0da087`; wrapper `20260908T172546.081411Z` replays the exact
original corpus using executable SHA-256
`1c4c9532fa7d152b134e09a5a9b4e503e8c074a178d5187a2adc6f7927d35b25`.
All identities are unchanged: torture `20010924-1` changes from REFUSED to
PASS, while `pr28865` stays REFUSED and now reports the measured nested-form
E2202 instead of E3101. No outcome regresses. Torture is 1,210 passes,
459 refusals and 23 failures out of 1,692; ctest stays 206 passes, 13 refusals
and one failure out of 220. Captures: `.evidence/ladder/e0/20260908T172546Z`
and `20260908T172644Z`. The direct storage defect is closed; the retained
oracle conflict is not counted as a success.

### Compatible function declarations and actual definition arity

`0f0a6a5` is the tests-only RED for compatible unspecified declarations
following prototypes, including typedef-based declarations and an empty
definition following a zero-parameter prototype. Lowering now reuses the
type interner's parameter-compatibility rule for declarations without
weakening a definition's bindings or arity. The accompanying negative
regression exposed an existing parser bug: an empty definition after a
one-parameter prototype was accepted. It now retains E2002, as do conflicting
return types and promotion-incompatible prototypes.
Both interpreters and native Darwin execute the fixture successfully;
GCC-14 and Clang-19 at O0/O2 agree, and both reject all three negative probes
(`.evidence/builtin-probes/20260908-function-redeclarations/`).
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true
tools/gate-changed.sh 0f0a6a5 --all-targets --locked --offline --quiet`
passes 1,630 tests across 12 affected crates, zero failures and four existing
standing-run ignores; Clippy, fmt, Ruff and diff-check pass. The repair is
`4f7bee9`; original replay `20260908T163813.683179Z` uses executable SHA-256
`23f7bd31fb89bfbc21b26589870f3ec155ad8978e2918b9fae9ecba7ec6361bc`.
All original identities are unchanged: torture `const-addr-expr-1` changes
from REFUSED to PASS, while `comp-goto-1` remains refused and reaches the
existing E3029 computed-goto boundary instead of E3101. No outcome regresses.
Torture is now 1,209 passes, 460 refusals and 23 failures out of 1,692;
ctest remains 206 passes, 13 refusals and one failure out of 220.
Captures: `.evidence/ladder/e0/20260908T163814Z` and `20260908T163937Z`.
This closes the identified composite-declaration defect, not all of row 13.

### Variadic record return placement

`f065b94` is the tests-only RED for the original `920726-1` call mismatch.
The IR return destination is now placed before anonymous arguments, while
the machine's indirect return address is placed last in its call operand
list so it goes to x8. Fixing only the first exposed a native write to
address 30, the final anonymous argument, rather than closing the case.
Both interpreters and native Darwin now pass the direct/indirect small and
large record regression; both Linux cross-link directions pass with GCC-14
and Clang-19 at O0/O2 (eight executions). The changed all-targets gate against
`f065b94` passes 764 tests, zero failures and four pre-existing standing-run
ignores; Clippy, fmt, Ruff and diff-check pass. The repair is `7f55a01`.
Evidence: `.evidence/builtin-probes/20260908-variadic-record/results.txt`.
The subject assembly uses the specified GNU assembler; the initial Clang
integrated-assembler refusal of `:pg_hi21:` is recorded, not called a pass.
Original replay `20260908T160746.894867Z`, executable SHA-256
`4bbe84757e3e6adbdb70a53cbd5140a3d13cfe1f8108e0d8162f47b6f73dd2e0`,
changes exactly `920726-1` from REFUSED to PASS, with no regressions.
Torture now has 1,208 passes, 461 refusals and 23 failures out of the same
1,692 programs; ctest is unchanged at 206/220. Captures:
`.evidence/ladder/e0/20260908T160747Z` and `20260908T160812Z`.

### Kernel replay and linked object definitions

`python3 tools/bench.py kernel --jobs 4 --allow-qemu-drift`, wrapper
`20260908T141252.550489Z`, captures subject `be2349c` and executable
SHA-256 `a29b0d83db55706c5e14041b6e4271330e3422ed4103c3d04af8e3dc75b588e8`.
All 2,799 original identities remain: 85 assemble, 2,714 refuse. Six formerly
refused units assemble; none moves in the other direction. All 2,712 E3021
first errors now name `%a0` at processor.h:390, not `i` or `+rm`.
The 85-object link fails on kaslr's missing `__kaslr_is_enabled`; removing
only kaslr exposes a boot timeout, independently reduced to `insn.o`.
The retained 83-object combination boots, with 69 linked objects: 15 proven,
51 proven modulo relocations, three no-symbol and 14 not-linked, zero proof
mismatches. These exclusions are failures, not successful substitutions.
Capture: `.evidence/k1/measure-20260908T141252Z`; passing Image digest
`90ee50eec552e947e3b2cb3dfe93ac5e21f365488b49cecd2092d9bd66983d5e`.

`ce08737` is the tests-only global-storage RED, `3f629ec` the repair:
declarations bound in several scopes share one storage identity, irrespective
of the first declaration's scope. Both interpreters and native execution now
pass the int, bool, incomplete-array and static-address cases. The affected
all-targets gate against `ce08737`, Clippy, fmt, Ruff and diff-check pass.
A direct replay of the original kaslr command using that debug subject emits
one exported `__kaslr_is_enabled` and its `strb`; the kernel measurement above
predates the repair and is not its mixed-link closure.

The exact pre-repair `insn.o` and reference object agree on basic immediate
encoding, but the first negative branch-offset decoding traps in EvoCC
(exit 133), versus reference result -16. The reduction is `(imm << 6) >> 4`.
Both sealed compilers return `-2 -2147483648 -16` for the standalone signed
shift probe at O0/O2; Clang's LLVM `shl` has no `nsw` flag. GCC explicitly
documents this signed-shift extension even though UBSan diagnoses it:
<https://gcc.gnu.org/onlinedocs/gcc/Integers-implementation.html>.
Evidence: `.evidence/builtin-probes/20260908-instruction-encoding/`.
The current body's strict shift rule conflicts with this GNU behavior;
an amendment has been requested, not bypassed. The `%a` contract conflict
is separately measured in `20260908-assembly-address/address.c` at both
optimization levels and with both `p`/`r` inputs. Neither is an FP issue.

### Integer absolute values and runtime-domain classification

`d598bfd` implements the three explicit integer absolute builtins and the
ordinary standard-name call selection, with options and same-unit assembler
identities preserved. The original tests-only checkpoint is `a3884e3`.
Both interpreter/native focused tests pass; the affected all-targets gate
against that checkpoint passes 1,032 tests, zero failures or ignored tests,
plus Clippy, fmt and Ruff. Original-program replay now closes the three
identified absolute-value cases, not the remaining native failures.
`afabd27` separately records the missing benchmark builtin-option projection
as a tests-only RED; `b01821d` implements it after the frozen kernel run
finishes, with all 42 targeted Python tests passing.

`python3 tools/bench.py ctest torture --jobs 4`, wrapper
`20260908T151818.777128Z`, measures subject `d160f3f`, executable SHA-256
`1ad01a326c9969a433b49e3beaf66f6c72afee0d934ce698f8309ffc449744a3`.
ctest remains 206 passed, 13 refused and one failed out of 220; torture is
1,207 passed, 462 refused and 23 failed out of the same 1,692 identities.
Exactly `20021127-1` moves FAIL to PASS and `pr34130` / `pr42614` move
REFUSED to PASS, with no regressions and no assembly/link failures.
Captures: `.evidence/ladder/e0/20260908T151818Z` and
`.evidence/ladder/e0/20260908T151844Z`. This executable predates the
abrupt-expression repair below; that repair has no benchmark closure yet.

The original `arith-rand` and `arith-rand-ll` each pass ordinary sealed GCC-14
and Clang-19 at O0/O2, but **both independent UBSan instrumentations** report
signed-minimum negation at source line 86. Twelve other retained torture
failures and ctest `00200` produce signed-left-shift diagnostics in both
instrumentations. `pr7284-1`, with its required C89 mode, is a sanitizer
disagreement: GCC exits zero, Clang reports signed shift overflow. These are
not new compiler passes. In particular the signed-shift reports alone do not
establish that these GNU programs are out of scope: GCC's documented extension
above is diagnosed by UBSan too. Keep that contract issue open independently
of ordinary C signed-overflow diagnostics.
Evidence and exact instrumentation/link settings are
`.evidence/builtin-probes/20260908-runtime-domains/results.txt`; Clang uses
the installed GCC sanitizer runtime because its own standalone archive is
absent. Every original program and headline outcome remains unchanged.

### Newly reduced control-flow case

`12f9eb5` and `d80c546` pin abrupt exits in value expressions, including
comma operands and conditionals with no normal continuation. Lowering now
retains a predecessorless insertion point, not a fabricated value or an edge
from the abrupt exit. The C-HIR task machine discards the comma's left operand
without requiring a scalar result. Fifteen focused lowering tests and the
native CLI regression pass; the affected all-targets gate against `12f9eb5`,
Clippy, fmt, Ruff and diff-check pass. The repair is `33bd17c`.
The reduced `int f(void) { int first=({ goto done; 0; }); int k=5;
done:return k; }` now lowers and both interpreters report E3001 for the
reached uninitialized read. It must never be folded into a constant return.
The original replay `20260908T154642.149536Z`, executable SHA-256
`c52a9d530825b65b1f288cd0daa4d8b3d93df8d6b27bfeac35a69c01c15f34f6`,
preserves every prior outcome: ctest 206/220, torture 1,207/1,692,
zero regressions. The new regression fixes a diagnosed semantic hole;
it is not an additional original-program pass. Captures:
`.evidence/ladder/e0/20260908T154642Z` and `20260908T154708Z`.

### Latest completed incremental replay

`python3 tools/bench.py ctest torture zstd cjson --jobs 4`, wrapper
`20260908T135846.118517Z`, captures clean `02d66b1` and executable
SHA-256 `4bc8805812e92f47ef470b9532a57aad85598a750c1c71a1899640514c856f36`.
The production includes wrapping and target-header types; its assembly RED
tests are deliberately still red, and no later assembly fix enters this run.
ctest stays 206/220, with 13 refused and one runtime failure. Torture keeps
all 1,692 identities: 1,204 pass, 464 refuse, 24 fail, zero link/assembly
failures. Exactly `20040409-1w`, `20040409-2w`, `20040409-3w`, `920612-1`,
`930529-1`, `950704-1`, `pr22493-1`, `pr23047` and `pr57124` move FAIL to
PASS; no other program changes outcome. The two header-dependent E1001
refusals now report unsupported value execution, not a new pass.
Native captures are `20260908T135846Z` and `20260908T135911Z`.

zstd remains 2/44 in its passing mixed build (`20260908T140139Z`); 37 units
now reach `arm_neon.h:40`, `typedef __Int8x8_t int8x8_t`, and five reach
`__PRETTY_FUNCTION__`. cJSON remains 0/2 (`20260908T140339Z`): `cJSON.c`
reaches `__builtin_nanf`, and `cJSON_Utils.c` reaches a floating-point
function type. Its public tests still execute the installed reference
objects; this is not an EvoCC cJSON pass.
The header implementation passed the changed all-targets gate against
`57bc8c2`: 1,889 passed, zero failed, four pre-existing ignored; Clippy,
fmt, Ruff lint and diff-check passed. This is not the final offline replay.

`python3 tools/bench.py kernel --jobs 4 --allow-qemu-drift`, wrapper
`20260908T123730.141577Z`, captures clean subject `bc7cc9b` and executable
SHA-256 `fc07b29798f1bc914a0064a3a7f90c2cb43cd253f6e29f0e830e9a64aa892a69`.
All 2,799 original unit identities remain: 79 assemble and 2,720 refuse.
Only `drivers/acpi/apei/ghes_helpers.o` changes assembly outcome (refused to
assembled); the 2,260 prior register-alias errors now reach later assembly
constraints. The exact new frontier is 2,716 `asm:i` at jump_label.h:54,
one at jump_label.h:38, one `asm:+rm` at vdso/math64.h:13, one computed goto
and one frame-address refusal. No count is inferred from a probe.
All 79 assembled objects substitute and relink, including both formerly
failing EFI objects; the mixed image boots. The substitution proof has
13 proven, 49 proven modulo relocations, three no-symbol and 14 not-linked
objects, zero mismatches: 65 linked objects, not a full EvoCC kernel.
Capture is `.evidence/k1/measure-20260908T123730Z`, image digest
`18aba753678cc7273f5e6edf4e1e4ecca6f1558ac66a474913c1a5027cfa5bc4`.
The wrapper correctly exits INCOMPLETE. This run predates signed-wrapping
and target-header-type changes and is not evidence for those changes.

Signed-wrapping options now pass the CLI/native regression and
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true
tools/gate-changed.sh dbd89ee --all-targets --quiet -- --test-threads=1`:
1,018 tests, zero failures or ignored tests in the five selected crates,
Clippy and fmt green. The three option/preparation Python tests and thirteen
benchmark-entry tests pass, as do Ruff lint and diff-check. The default and
explicitly disabled modes still report E3004; shift, division and pointer
checks remain tested. The original torture replay is recorded above; its
measurement retained every source and did not change the earlier kernel
run's binary or results.

The digit-builtin, register-alias and expression-scope static fixes pass
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true
tools/gate-changed.sh afc4c63 --all-targets --quiet -- --test-threads=1`:
1,015 tests, zero failures or ignored tests in the five selected crates,
Clippy and fmt green. Ruff lint and diff-check pass. This is an incremental
all-targets gate, not the final whole-workspace/doctest replay. Their original
kernel replay is recorded above, separately from the preceding measurements.

`python3 tools/bench.py ctest torture tinycc cjson --jobs 4`, wrapper
`20260908T120936.530123Z`, measures executable SHA-256
`ba3d6f93ef7b053ff642c9f98c43a5a9032e2352c482a450d31052e56a839135`.
The captured subject is `a81c243` with only a new C test fixture pending;
the measured production includes `5b3c87a`, not the later digit/register fixes.
Every original ctest and torture identity is retained. ctest remains
206 pass / 13 refused / 1 failed out of 220. Torture becomes
1,195 pass / 464 refused / 33 failed out of 1,692: `memclr` and `misalign`
change from FAIL to PASS, while `20050107-1`, `20050119-1` and `20050119-2`
change from PASS with ignored enum attributes to E2903 under the measured
enum-mode conflict. These five are the only outcome changes; this is not a
zero-regression claim or closure of the non-FP objective.
TinyCC remains 7/11 assembled and all seven pass together; `tccpp` advances
past the repaired ordinary-call directive case to an FP call refusal.
cJSON relinks and executes its actual installed objects successfully, but
both are reference objects because its two subject units still hit header
declarations. This validates the adapter repair, not an EvoCC cJSON PASS.
Native runs are `20260908T120936Z`, `20260908T121000Z`,
`20260908T121237Z` and `20260908T121307Z`, respectively.

Shared symbol addressing and link-time assembly operands `7ac11d3` are
measured by `python3 tools/bench.py tinycc kernel --jobs 4 --allow-qemu-drift`,
wrapper `20260908T102835.690828Z`, frozen executable SHA-256
`b3b730c0afa50abb5c4821db6c5997966f42a2b08b980ef064c7a00665b7a95b`.
Its completed TinyCC lane `20260908T102835Z` rises from 6/11 to 7/11 in
the passing mixed build: `tccrun.o` joins the original six, all eleven
identities and assembly outcomes are unchanged, and no isolated relink
failure remains. Four compilation refusals remain; this is not TinyCC PASS.
The kernel lane completed with 78/2,799 assembled, of which 76 enter a booting
mixed link after excluding two EFI units referencing `__efistub_isdigit`.
The initial proof falsely reports two NOBITS objects as mismatches; the
wrapper's original ERROR is retained. After `0e44fde`, replay on exactly the
same linked image gives 13 proven, 48 proven modulo relocations, 3 no-symbol
and 12 not-linked units, zero mismatches (`proof-zero-fill.json` and
`reconciled-receipt.json` in `measure-20260908T102907Z`). NOBITS checks exact
symbol extent and zero-filled memory, not nonexistent file bytes; malformed
or missing extents still fail. This is 64 linked units, not a full kernel.
The binary was not rebuilt while the lane consumed it; later C-preprocessor
and test-linkage changes are not attributed to this measurement.

Integer `mode` and packed-subobject execution landed in `5b3c87a`; the
original memory failures are now replayed above, with the enum conflicts
kept separate from the successful repairs.
The exact memory fixture passes both sealed compilers at O0/O2 and the two
interpreters plus native execution; the changed gate found an old wide-access
test that required natural alignment even for packed storage. Its replacement
pins both reduced power-of-two alignment and invalid alignment refusals.
The successful serialized rerun is `CARGO_TARGET_DIR=target/frontend
CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh f747dc5
--quiet -- --test-threads=1`: 1,541 pass, zero fail, four pre-existing ignored,
Clippy and fmt green; Ruff lint and diff-check also pass. `cargo test` with
the same ten-package set and `-- --list` lists 1,545 including those four.
The preceding 1,540-pass/one-failure attempt remains a failed gate.
The enum-mode oracle conflict remains visible: the three original programs
named above previously passed with ignored attributes, but the oracles disagree on
the resulting type compatibility. This is not a new PASS or a reason to drop
the original program; `_Bool`, enum and abstract-type-name mode conflicts
are explicitly refused under the existing conflict protocol.

The final serialized gate against `c489ffa` passes 1,533 tests, zero
failures, four pre-existing ignored tests, plus Clippy, fmt, Ruff lint and
diff-check. An earlier invocation passed its native tests but failed a
doctest after a concurrent Cargo rebuild replaced a dependency; that attempt
is not green. The final replay uses one Cargo invocation through completion.
Both sealed compilers link and run the final symbolic-immediate fixture and
the undefined-symbol-only-in-a-comment control. The latter exposed and fixed
spurious runtime address loads; pure asm-literal definitions need no runtime
reference, while ordinary uses retain their allocation and address materialization.

External-data GOT addressing `9065012` is measured in
`20260908T093536.969536Z` (`python3 tools/bench.py tinycc --jobs 4`), native
run `20260908T093537Z`: TinyCC rises from 4/11 to 6/11 units in a passing
mixed build. All eleven identities and all seven assembly outcomes remain;
`arm64-link.o` and `tcc.o` join the previously passing four. The remaining
`tccrun.o` link now reports a direct relocation against `longjmp`, a function
address rather than the earlier `environ` object. It remains OPEN, not hidden
behind the passing data-address probe. Capture `9065012` is clean, binary
SHA-256 `5e03b5a43164641ff0ff5107ee2e09c9a35a4b8416668c394e624483801e67fc`.
The changed gate against `3b0065a` passes 518 tests, zero failures, with four
pre-existing ignored tests; Clippy, Rust fmt, Ruff lint and diff-check pass.
The sealed GCC-14/Clang-19 static/PIE source and subject probe has eight
build/run successes, and the Darwin native regression passes. Ruff formatting
of the untouched Python tree was also inspected: 43 existing files do not
match `ruff format`; no unrelated formatter rewrite was made or claimed green.

The prototype-array repair `858b8c2` is measured in
`20260908T090630.550681Z` (`python3 tools/bench.py ctest torture --jobs 4`):
c-testsuite rises to 206/220 and torture remains 1,196/1,692. Original
`00162` is the only changed outcome, refused to native PASS; both identity
sets and all earlier passes are unchanged. The source checkout `91158d1`
is clean at capture, with executable SHA-256
`384941528b53ad45a9c9ff1ae2879476bf01e017db673e28d31ffb02892e5f31`.
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true
tools/gate-changed.sh f1c2453 --quiet` passes 990 tests, zero failed or
ignored, plus Clippy, fmt, Ruff and diff-check. This closes prototype `[*]`,
not real VLA execution or the entire declaration-coverage family.

The attributed-type repair `1cd7701` is measured in
`20260908T085411.464607Z` (`python3 tools/bench.py ctest torture --jobs 4`):
c-testsuite rises to 205/220 and torture remains 1,196/1,692. Original
`00210` is the only changed outcome, refused to native PASS, with all
identities and earlier passes preserved. The five-crate changed gate passes
985 tests, zero failed/ignored, plus Clippy, fmt, Ruff and diff-check.
Executable SHA-256 is
`d9d3077f8221e8679fcf9610f050799d2e9b5c7330fdb5a19f779a0329748e50`;
the checkout is `5aeea8c`, with only the next tests being written while its
release build finished. Prototype-array production changes were made later
and are not included in this replay.

Toybox's eight remaining first sites were individually replayed with the
same adapter transformations (including re-escaping regenerated flags.h).
They require FP execution: `lib_args`, `lib_xwrap` and `sort` reach strtod;
`lib_hash` reaches sin; `seq` reaches fabs; `printf` reaches strtold;
`od` has a long-double subvalue; `uptime` forms FP load averages.
`CFG_TOYBOX_FLOAT` is one in this measured configuration. All eight remain
refusals in the 203-unit universe, not invented non-FP passes, and this
does not waive other outstanding toybox input-adaptation/baseline issues.

The line-control repair `2d063ef` is replayed in
`20260908T084118.829770Z` (`python3 tools/bench.py ctest torture --jobs 4`):
c-testsuite rises to 204/220 and torture remains 1,196/1,692. The only
changed program is original `00152`, refused to native PASS; no identities
or earlier passes are lost. Its changed gate runs 1,090 tests with no
failures or ignored tests, Clippy, fmt, Ruff and diff-check green.
The source identity is `0a914b7`, executable SHA-256
`213abc7bd72465d157a2346d1bdff87f76b39b4043fb635028ce119dd26f4a92`;
the dirty flag records pending test/receipt work, not the subsequent
type-name implementation. Earlier attempt `20260908T084012.993329Z`
could not start the sealed execution environment and remains NOT RUN,
not evidence of a compiler regression or a passing replay.

`python3 tools/bench.py toybox ctest torture csmith --campaign all
--programs 500 --jobs 4 --budget 40` completed run
`20260908T080325.213428Z`: toybox 195/203, c-testsuite 203/220,
torture 1,196/1,692 and Csmith 932/1,000 agreeing. All original identities
remain present, with no outcome regressions; toybox cut/httpd/sed are the
only changed outcomes, refused to assembled, and join the 195-unit
no-regression mixed build. Its one reference-baseline failure remains visible.
Csmith still has 68 oracle-unusable results and zero mismatches.

The run correctly records dirty checkout `82e2cff`: the already-staged
production patch was committed as `2f268ab` after the wrapper captured its
checkout identity. The measured executable SHA-256 is
`040927dddb910127976bff07817b36faaeb9c2d95d381688273bd57ccd3a8884`;
it was not rebuilt during the campaign. This is not a clean-commit replay.
The goto repair passed 981 tests, zero failed/ignored, through
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true
tools/gate-changed.sh 82e2cff --quiet`, plus Clippy, fmt, Ruff and diff-check.
The following line-control change is not attributed to this executable.

`python3 tools/bench.py ctest torture csmith tinycc toybox --campaign all
--programs 500 --jobs 4 --budget 40` completed run
`20260908T061410.809670Z`, measuring the frozen `a1393e9` executable
(`2d6241716a6f42339c5a38a3903d20f3ce572f5706fc9637c3368de7688d661b`).
The wrapper returns INCOMPLETE, not PASS: c-testsuite 203/220, torture
1,195/1,692, Csmith 932/1,000 agreeing, TinyCC 4/11 units in a passing
mixed build, toybox 192/203. Csmith retains all original seeds and outcomes,
including 68 oracle-unusable and zero mismatches. TinyCC's original
`tccdbg.o` and `tccelf.o` change from refusal to assembled and join the
passing four-unit combination; `arm64-link.o`, `tcc.o`, and `tccrun.o`
remain isolated relink failures. No earlier PASS was lost in these lanes.
The subsequent enum repair is not attributed to this older executable.

`python3 tools/bench.py ctest torture --jobs 4` then measures `7b436bc`
in run `20260908T070638.695281Z`: 203/220 c-testsuite passes and
1,196/1,692 torture passes. Both original identity sets are unchanged;
the only outcome change is torture `enum-3`, refused to native PASS,
with no previously passing program lost. Torture retains 461 refusals
and 35 runtime failures, with zero emission timeouts or assembly/link
failures. Unselected lanes keep their earlier compiler identities; this
is not a new all-milestone PASS.

### Original-program replay and capture correction

Native arithmetic classification was extended without changing any original
program or benchmark outcome. Both instrumented compilers at O0/O2 report
overflow/invalid shifts on twenty programs when their per-program options
are omitted. Seven are **not** invalid-test exclusions: `20040409-1w`,
`20040409-2w`, `20040409-3w`, `920612-1`, `930529-1`, `950704-1`
require `-fwrapv`, and `pr57124` requires `-fno-strict-overflow`; all 28
correct-option instrumented executions pass. The adapter and execution
profile must implement those requested semantics explicitly. Clang uses the
system UBSan runtime because its standalone runtime is absent. `pr7284-1`
requests C89 and has a GCC/Clang sanitizer-dialect disagreement, retained
separately. Per-program first faults and both command forms are in
`.evidence/builtin-probes/20260908-torture-options/results.txt`.

`memclr` and `misalign` also expose ignored integer `mode` attributes,
not merely inline-assembly interpreter refusals: `mode(QI)` remains a
four-byte unsigned int instead of the one-byte type both oracles use.
The reduced sizeof assertion is RED E2911, and the sealed mode probe
measures ranks, signedness, member layout and truncation. GCC/Clang disagree
on applying `mode(QI)` in an abstract type name (sizeof 1 versus 4), so that
placement must not silently choose a width. This shared type repair remains
OPEN and precedes less consequential coverage work.

The frozen `8a9f19d` executable was measured through
`python3 tools/bench.py torture musl --jobs 4 --budget 40`.
Torture `20260908T034747Z` retains the identical 1,692 program identities:
1,189 pass, 469 refused, 34 runtime failures, zero emission timeouts or link
failures. All formerly passing programs still pass; only `memclr` changed,
from emission timeout to runtime failure, so no extra PASS is claimed.

Musl `20260908T035026Z` is invalid as a full-denominator comparison: a
parallel recorder merged `sysconf.lo` and `crypt.lo`, silently reducing
1,339 units to 1,337. Its partial bisection identified `fprintf.lo`, and
the substituted test report includes failed floating-point formatting;
Section 6's GPR-only varargs ABI does not preserve FP arguments. The run
was interrupted after 25 probes, not reported as a passing mixed build.
`a86454c` holds one lock across the complete command record; 95 Python
tests and a 16-writer sealed stress test preserving 512/512 identities pass.
The fix requires a fresh original-unit replay, not relabeling this run.

The earlier musl run `20260908T015031Z` also lacks `substituted-REPORT`:
the adapter treated an absent report as an empty failure set. Its claimed
987-unit passing build is withdrawn in the derived receipt, with the
historical number retained under `invalidatedUnitsInPassingBuild`; it is
not replaced by a made-up zero. `6cdc159` removes that fallback, with a
missing-versus-empty regression and 96/96 Python tests. Fresh FP failures
therefore cannot yet be called regressions from a proven old passing build.

The two `arith-rand` originals also contain signed `-INT_MIN` at line 86:
both sealed compilers' O0/O2 UBSan instrumentation refuses that operation.
Clang's bundled runtime is absent, so its successful diagnostic runs use
the explicitly named system `libubsan`; the failed initial links remain
recorded in `.evidence/builtin-probes/20260908-arithmetic-torture/results.txt`.
This diagnoses an applicability issue without changing either original or
their native FAIL outcomes into PASS.

### Positional parameters without source names

Tests-only `a316754` fails semantically with C-HIR E3007 and lowering
E3101, with no build errors. `a26bb02` retains all ABI slots and argument
effects while making only the C identifier binding optional. Both
interpreters and native execution pass the fixture, including an unnamed
aggregate parameter; the original `pr103209` now returns zero in both
interpreters. The changed-crate gate against `a316754` passes 950 tests
with zero failures or ignored tests, plus Clippy/fmt. Original native
torture run `20260908T042040Z` passes `pr103209`: 1,190/1,692 pass,
468 refused, 34 runtime failures, zero emission timeouts or link failures.
Against `20260908T034747Z`, the identity set is unchanged and this is the
only changed outcome; all 1,189 previous passes are retained.

The same frozen executable (source `2a50ced`, SHA-256
`70b313d9ff4b1c5798b43ba6ec36775c7f0c8e4f7d1498ac52090a84a4728710`)
completed musl run `20260908T042300Z`. Capture now contains all 1,339
units; 987 assemble with zero symbol-parity exclusions. The 40-probe
budget is exhausted, with a 494-unit no-new-failures combination verified
in round 2, relative to 17 reference failures. Single-unit failures are
`fprintf.lo` and `snprintf.lo`; further failures remain after excluding
both. The final report exists (48,100 bytes), but belongs to the final
failing probe, not the best passing subset; `bisect.json` records which
combination each probe actually measured. This is INCOMPLETE, not a
restoration of the withdrawn 987-unit passing claim.

### Pointer interfaces and switch entries

`274ede9` compares compatible function/void pointers through their existing
address representation. `631f03e` routes switch segments through the same
label-aware sequence lowering as ordinary compounds, retaining goto targets
after a terminated path. `d32f676` preserves unprototyped function addresses
in storage, promotes indirect-call arguments, and constructs the actual call
interface without weakening dynamic signature checks. A long argument or
missing argument to an integer-parameter target remains E3019 in both
interpreters. This does not claim support for nested case labels or jumps
past unallocated automatic objects.

`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true
tools/gate-changed.sh abfa496 --quiet` passed 956 tests, zero failures and
zero ignored, plus Clippy and fmt. The focused tests include native execution;
original c-testsuite and torture replay remains the benchmark acceptance.

Replay `20260908T050555.942587Z` (compiler source `5a8d94b`) now measures
200/220 c-testsuite passes and 1,194/1,692 torture passes. The original
identity sets are unchanged, and no prior pass is lost. c-testsuite
`00087`, `00088`, `00089`, `00209`, and torture `20041212-1`, `20111208-1`,
`pr17377`, `pr67037` change from refusal to native PASS. Torture `eeprof-1`
changes from refusal to runtime FAIL: it requires `-finstrument-functions`,
which the adapter does not consume; its expected instrumentation callbacks
therefore never run. It remains a recorded failure, not a compiler PASS.

### Nested switch dispatch

Tests-only `3fabe4b` exposes non-compound switch refusal and wrong C-HIR
values for cases inside conditionals and loops. `fa6f895` additionally pins
integer promotion of a narrow selector and ownership of nested cases and
continue targets. The sealed GCC-14/Clang-19 O0/O2 probe executes all shapes
with exit zero (`.evidence/builtin-probes/20260908-nested-switch/results.txt`).

The C-HIR entry point previously selected a different recursive statement
executor for a call-free, memory-free function. That executor retained the
same top-level-only case scan. The repair removes it and runs every function
through the existing iterative machine; switch entry shares goto's scope and
loop continuations. Lowering retains the compact flat-switch representation,
while nested labels use the existing label/edge machinery and case matcher,
not flattened loop bodies. A narrow selector's promoted value must be carried
instead of being replaced by its original narrow local on later comparisons.
`19aa801` passes the changed gate against `3fabe4b`: 965 tests, zero
failures or ignored, Clippy and fmt clean. The test inventory was also
recounted with the same five crates' `cargo test -- --list` (965).
Entry past automatic memory-object
declarations remains a separate open storage/lifetime issue, not waived here.

Original replay `20260908T054623.296899Z` measures source `19aa801`:
c-testsuite `00051` and `00143` change from refusal to PASS, giving 202/220.
Torture retains all 1,194 prior passes; `medce-1` changes from refusal to
link failure at its deliberately unresolved `link_error` symbol, while
EvoIR execution returns zero. It is not a benchmark PASS: constant branch
emission still leaves the unreachable call in the object. The 1,692 original
identities are unchanged. Toybox now has 192/203 assembled substitutions
in its passing combination, relative to its one baseline failure; TinyCC
has 5 assembled, 2/11 in a passing build, with three isolated link failures
(`arm64-link.o`, `tcc.o`, `tccrun.o`). These remain incomplete milestones.

### Discarded expressions with abrupt control flow

Tests-only `c6a4138` records three semantic failures, all `statement
expression has no normal path to its value`, with no build errors. A
discarded expression can return, break, continue, or jump rather than
produce a normal continuation. The tracked fixture
`tests/fixtures/discarded-expression-exits.c` compiles and exits zero with
both pinned compilers at O0 and O2 in the sealed network-disabled image.
The repair reuses statement `Flow` for discarded expressions and merges
only actual fallthrough states; value-required contexts remain explicitly
checked. `aa08860` passes all 329 tests selected by the changed gate against
`c6a4138`, plus Clippy/fmt. Original run `20260908T055627.176403Z` moves
only c-testsuite `00213` from refusal to PASS (203/220); torture's original
1,692 identities and every outcome remain unchanged (1,194 PASS).

### Macro inhibition survives argument substitution

Two TinyCC parser failures have one preprocessing cause: `#define field
p->field` inside a prescanned argument becomes `p->p->field` on a later
rescan. In the originals this is `s1->s1->eh_frame_section` at
`tccdbg.c:815` and `s1->s1->symtab_section` at `tccelf.c:1978`, not a new
kind of projected lvalue. Tests-only `e9abc66` records two wrong-token
failures plus a passing function-name boundary control, with no build errors.
Both pinned preprocessors give the same nine lines for tracked fixture
`tests/fixtures/macro-rescan-inhibition.c` under `-std=gnu11 -E -P` in the
network-disabled sealed image. Preserve inhibition on the token occurrence,
not on its spelling globally: later source occurrences and newly pasted
tokens must remain eligible. All 105 C-preprocessor tests pass. `a1393e9`
and the constant-branch repair `b6cf13f` pass the combined changed gate
against `d22b5fc`: 1,243 tests, zero failed or ignored, Clippy `-D warnings`
and fmt clean. Original-program replay remains the closure requirement.

TinyCC's `tccrun.o` link failure independently names
`R_AARCH64_ADR_PREL_PG_HI21` against libc `environ` under PIE linking.
`arm64-link.o` also addresses `stderr`; `tcc.o` addresses `stderr`/`stdout`,
but their isolated linker diagnostics have not yet been recaptured. The
existing authorized GOT exception applies to undefined weak references,
not all external symbols. Keep this address-recipe boundary open; disabling
PIE in the benchmark is not an implementation fix.
The tracked reduction `tests/fixtures/external-object-pie.c` compiles and
exits zero with GCC-14 and Clang-19 using explicit `-fPIE -pie`; the
subject's `a1393e9` assembly fails GCC linking with the same relocation
error. Clang's default integrated assembler rejects the prescribed
`:pg_hi21:` spelling before linking, so that attempt is not reported as
the same linker diagnosis; use of GNU `as` must be explicit when isolating
the linkage failure. Explicitly rerunning Clang-19 with
`-fno-integrated-as -fPIE -pie` reaches the same `environ` relocation failure
(link exit 1), isolating the address recipe from the assembler syntax.

### Compatible enumeration pointer types

The original torture `enum-3` combines pointers to a negative enumeration
and its compatible `int` representation. The producer and place consumer
did not share the same compatibility check, and the interner itself did
not recognize an enum's existing underlying rank. Tests-only `71949c6`
records three semantic failures with zero build errors: false rank
compatibility, refused pointer-expression execution, and E2911 on the
positive compile-time query. The Appendix D entry records the pinned
O0/O2 positive executions and a separate GCC/Clang qualifier disagreement;
the existing nested-qualifier rule is retained, not loosened to follow GCC.
The extended fixture also exposed E2002 at a compatible redeclaration,
then allocation-manifest and interpreter-return-type identity checks;
these consumers now use the same target rank and type compatibility,
without changing the independent SSA validator or nominal tag identity.
Implementation `7b436bc` passes
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true
tools/gate-changed.sh 71949c6 --quiet`: 1,560 passed, zero failed, four
pre-existing standing-campaign tests ignored, Clippy `-D warnings` and fmt
clean for the twelve affected/reverse-dependent crates. Ruff, diff-check,
and the 12 benchmark-entry Python tests pass. This is a changed-crate gate,
not a newly claimed full-workspace or isolated-clone replay. Original
torture run `20260908T070702Z` closes the reduced enum family by native PASS.

The full original-seed Csmith replay `20260908T051022Z`, through
`python3 tools/bench.py csmith --campaign all --programs 500 --jobs 4`,
retains all 1,000 identities and every prior outcome: base 469 agree/31
oracle-unusable, bitfields 463 agree/37 oracle-unusable, zero mismatch and
zero subject refusal. Every unusable case times out in both GCC O0 and O2;
this does not establish that every such source is nonterminating. The native
campaign measured source `3fabe4b`, before the nested-switch repair, and is
not relabeled as verification of a later commit.

### Explicit local-register operand binding

`.evidence/builtin-probes/20260908-local-register-operands/{probe.c,results.txt}`
pins a local `register long wanted asm("x8")` passed as an `r` input.
GCC-14/Clang-19 at O0/O2 each exit zero; subject `25ab85d` emits assembly
that links successfully but exits one: it reads the old x8 value rather
than the argument. No original program or seed was replaced by this reduction.

LOCAL-REGISTER-OPERAND-REPRESENTATION: Section 10.1.18 says a local
register binding matters at an asm operand, but Section 3.11/9's operand
records and admitted constraint vocabulary have no field or spelling for it.
The binding must reach allocation and output edges, including tied/early-clobber
and asm.goto operands; inserting moves outside that boundary is not an adequate
substitute. This is separate from the immediate/register alternatives already
authorized by revision 1.9.78. The user authorized carrying the binding through
the IR and allocation boundary on 2026-09-08.
Tests-only checkpoint `ce22a80` records both semantic failures without a build
error. Binding now survives both textual IR forms, uses short-lived pinned
operands, and transfers outputs over every asm.goto edge. The independent
allocation verifier rejects a mutated binding; native tests cover both goto
edges under spilling and a callee-saved x19 binding. A new negative test also
ensures the source cannot directly admit the lowered `@xN` spelling.

`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true tools/gate-changed.sh ce22a80 --quiet`:
843 passed, zero failed, four existing ignored; Clippy/fmt PASS. The earlier
842-test gate was superseded by the additional source-vocabulary negative.
K1 Python tests: 88/88; bench tests: 12/12; Ruff/diff-check PASS; the grammar
audit has 42/42 instruction shapes with zero disagreement (not a full grammar
proof). Original musl and torture replay is still required for family closure.

### Immediate-register input alternatives

Tests-only checkpoint `9c46be8` has three E3021 semantic failures (`Jr`/`Kr`)
and zero build errors. Lowering now selects the existing `r` alternative for
exactly `I`, `J`, `K`, `L`, `i`, `n` paired with `r` in either order, after
evaluating the operand once. No backend constraint or instruction is added.
Two old rejection rows are migrated to still-invalid `Jrr`/`rII`; the newly
admitted `Jr`/`rI` are covered by the positive table, not silently dropped.

The eleven focused tests pass, including native assembly/link/execution of
all twelve spellings, 32/64-bit values, single evaluation and both asm.goto
output edges. The six logical/add/sub values match the sealed `kr.c` probe.
Outputs and 128-bit register inputs retain E3021.
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 9c46be8 --quiet`:
303 passed, zero failed or ignored, Clippy and fmt PASS; Ruff and diff-check
PASS. Original kernel replay is still required before this family closes.

### Symbol linkage

The immutable-linkage reuse fix (`eecdb45`, RED `4069f80`) passes the
reverse-dependency changed gate, Clippy and fmt. The original torture
`memclr.c`, previously exceeding the unchanged 60-second emission limit,
now emits assembly in **1.397 seconds**, exit 0, empty stderr, using release
SHA-256 `e7dc4f01d59d217bb75784686b8f7145e582cf2d039e3dcb55f04a671d6a7069`.
Command: `target/frontend/release/evocc --target=aarch64-unknown-linux-gnu --emit=asm .build/ladder/run-torture/src/memclr.c`
under `subprocess.run(timeout=60)`, stdout discarded. This closes the
emission timeout, not its earlier runtime failure; the original program still
needs the full execution lane. Profile samples had identified repeated
whole-unit `SymbolLinkage::derive` inside each scalar initializer.

Kernel replay `measure-20260908T030624Z` reused the valid 2,799-unit survey
`measure-20260908T022219Z/survey`; every reduction retained release `25ab85d`
SHA-256 `d59b26ba3656f811592b782e36a2ad0a1cf0a8fb61a1c9bf5efad783b3b71d09`.
78 assembled, 2,721 refused; `efi/libstub/string.o` and `vsprintf.o` were
individually reduced link failures (`__efistub_isdigit`). The other 76 objects
relinked and booted, but `proof.json` reports 52 proven, 2 mismatched,
7 unverifiable, 12 not linked, 3 without symbols. The benchmark correctly
remains ERROR, not a validated kernel pass: inspect the zero-filled object
slices (`xen_features`, `guid_null`, `uuid_null`) and the missing extents for
`ctype`/`fdt` objects before claiming substitution closure.

Tests-only checkpoint `0f2a71d` has nine semantic failures and zero build
errors. Module records now preserve weak marks and flattened object/function
aliases; aliases share allocation, call identity, effects and relocation
targets in both interpreters, without forwarding bodies or copied storage.
Alias references keep internal targets, including otherwise removable
always-inline definitions. Invalid weak/internal combinations and alias
targets report E3060/E3061; the shared-type constructor changes are mechanical.

`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 0f2a71d --quiet`:
1,471 passed, zero failed, four pre-existing ignored tests; Clippy and fmt
PASS. `ruff check tools tests/acceptance`, `python3 tools/check-textual-grammar.py`
(42/42 instruction shapes, not a full syntax claim) and diff-check PASS.
The first gate caught a malformed-C-HIR diagnostic-order regression; the
shared call-type check was corrected, the original negative assertion retained,
and the entire changed gate rerun. Focused linkage/address tests are 34/34.

Native AArch64 static-link probes pass object-alias writes, equal loaded
addresses, function aliases, weak definitions alone and strong-over-weak
replacement under both GCC-14 and Clang-19 link drivers. This does **not**
close weak external references: the explicit counterexample is
`.evidence/builtin-probes/20260908-weak-external-address/{probe.c,subject.s,results.txt}`.
Both sealed oracles at O0/O2 exit zero; the emitted subject links statically
but exits one. Default PIE linking additionally refuses its direct relocations.

WEAK-EXTERNAL-ADDRESS: EvoMIR Section 8's external-symbol
rule prescribes direct `adrp`/`add`, while revision 0.22 requires undefined weak
references to resolve to address zero; the measured linked address is 0x400000.
The user authorized GOT-based address materialization only for weak
extern/declare records on 2026-09-08, with no new EvoIR instruction. The
implementation preserves the direct address recipe for strong references and
definitions; its nine focused machine-linkage tests pass. Linux GCC-14 and
Clang-19 static/PIE links execute the original counterexample successfully;
the additional absent/present/call/read checks also pass in all four modes.
Evidence: `.evidence/builtin-probes/20260908-weak-external-address/got-results.txt`.

Darwin's default linker rejects unresolved weak references for both system
Clang and the subject. With the same explicit `-U` symbol list, both execute
the counterexample with exit zero; the native regression asserts both sides.
This is a measured linker-policy requirement, not a lowered-language change.
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=1 CARGO_NET_OFFLINE=true tools/gate-changed.sh d2d8295 --quiet`:
501 passed, zero failed, four pre-existing ignored; Clippy and fmt PASS.
Ruff and diff-check PASS. Alias/weak remains OPEN for the static-initializer
performance regression below, not for an unimplemented GOT exception.

The original `memclr` source again times out at 60 seconds. A two-second
system stack sample has 178/178 samples below static integer interpretation's
construction of a fresh linkage map; every initializer repeats a whole-unit
declaration scan. This diagnostic run overlapped the tail of the kernel
survey, so it is a hot-path observation, not an unloaded timing comparison.
The immutable linkage result must be reused without changing error checks;
the final original-program replay retains the 60-second limit.

### Kernel reduction compiler mismatch

Wrapper `20260908T022219.374623Z` finished its 2,799-unit survey at
`measure-20260908T022219Z/survey`: 78 assembled, 2,721 refused, no timeout or
panic. The initial mixed link reports missing `__efistub_isdigit`. Subsequent
reductions used a different executable because `bisect.py` ignored `K1_EVOCC`:
survey digest `d59b26ba...71d09`, reduction digest `4c39a20f...5f01`. Those
reduction PASS outcomes are invalid and the owned subprocesses were terminated;
the wrapper records ERROR, not completion. The valid per-unit survey is retained.
Tests-only `f7fe61b` and fix `e5f71eb` pin the override through reduction and
expose exact-binary survey reuse through `tools/bench.py`. K1 tests 87/87,
benchmark tests 12/12, Ruff and diff-check PASS. A correct mixed-link replay
is still required; no kernel closure is inferred from the broken reduction.

`python3 tools/bench.py torture musl --no-build --jobs 4 --budget 40` finishes
INCOMPLETE in wrapper `20260908T014701.174683Z`, subject `b504ee9`, compiler
`eea68d679e70193687b8ecd61b6e92e2fff1aeae4fd7dbe92f3088d9fc24fff8`.
The release executable was unchanged throughout both lanes.

- Torture `20260908T014701Z` retains all 1,692 original identities and all
  1,186 prior passes. `alias-2`, `alias-3` and `alias-4` change from link
  failure to PASS: 1,189 pass, 469 refused, 33 runtime failures, zero link
  failures and one emission timeout. `memclr` changed from runtime failure
  to the 60-second emission timeout; this remains a measured failure, not
  a cleared issue or a reason to increase the watchdog.
- Musl `20260908T015031Z` retains all 1,339 original units and every prior
  assembled unit. `intscan.lo` and `sigaction.lo` change from refusal to
  assembled: 987 assembled, 352 refused. Symbol-parity exclusions fall from
  167 to zero. This run originally claimed all 987 substituted units passed
  together against the 17-failure reference baseline; that claim is withdrawn
  because its substituted test report is absent. The valid fresh replay above
  establishes a 494-unit passing combination, not 987, and does not establish
  complete musl coverage.

An additional RED checkpoint `f1ee3cf` pins an alias whose target definition
was first declared by a block-scope extern. Fix `66a1f03` identifies actual
top-level definition nodes rather than the first symbol scope. Its changed
gate (`tools/gate-changed.sh f1ee3cf --quiet`, same offline/build settings)
passes 939 tests, zero failures or ignored tests, Clippy and fmt PASS.
This follow-up is not included in the benchmark compiler identified above.

### Macro definition restoration

Revision 1.9.77 is implemented through per-name stacks that preserve the whole
definition and its feature-query implementation, or its undefined state.
The tests-only checkpoint `a6c302d` has five semantic failures, zero build
errors; restoring `_Pragma` effects during rescanning also fixes their ordering
relative to subsequent macro uses. Header spelling, pack independence, invalid
argument classification and fresh-translation-unit stacks are pinned.
The measured probe has one empty pop; malformed push/pop arguments are counted
separately as ignored pragmas by name, and successful stack operations are not
reported as ignored-safe.

`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh a6c302d --quiet`:
1,037 passed, zero failed or ignored; Clippy and fmt PASS. Ruff and diff-check
PASS. Implementation `dba5385` is measured by
`python3 tools/bench.py ctest torture --jobs 4`, wrapper run
`20260908T005741.383578Z`, compiler
`0154d28e92cf57b72cbcd16bd542b8780e3d8e3586a08152ce1a44f0a317876b`.
Native runs `20260908T005741Z` and `20260908T005804Z` retain every original
220/1,692 identity and every prior pass: the only outcome changes are
`00206` and `pushpop_macro`, both failure to pass. c-testsuite rises 195 to
196/220; torture rises 1,185 to 1,186/1,692. The macro family is closed;
the benchmark suites remain INCOMPLETE, with 1/34 runtime failures respectively.

### Void return effects

Tests-only checkpoint `427951e` produced three declared semantic failures and
zero build errors. The shared void-type query, interpreter task dispatch and
effect-only lowering preserve direct/indirect calls, casts, comma expressions,
conditional arms and the final statement-expression value without inventing a
scalar result. Negative return-type checks remain refused.

- `cargo test -p evocc-lower --test static_storage_and_discard --offline -j 4`
  with `CARGO_TARGET_DIR=target/frontend`: 11 passed, zero failed.
- `CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true tools/gate-changed.sh e6c7938`:
  tests, Clippy and fmt green for evocc/chir/lower/parser/proofcheck.
- `ruff check tools tests/acceptance` and `git diff --check`: PASS.
- `python3 tools/bench.py toybox --jobs 4 --budget 20`:
  run `20260907T165020.113321Z`, native run `20260907T165020Z`,
  compiler `92084dcb125b537b46d5b8e1c554b510a5edc6fd452961a9dfdde39aebbd0792`.
  The same 203 units now have 191 assembled and linked together, 12 refused,
  no symbol exclusions and no runtime regression relative to the unchanged
  one-failure reference baseline. All 177 previously assembled units remain so;
  14 improve. The benchmark is still INCOMPLETE, not a closed toybox milestone.

The remaining first errors are edge-value availability (cut/httpd/sed), one
prototype (dd), seven function-value-type refusals and one object-subvalue
refusal. Both httpd and sed advanced beyond their original return refusal.

### Independent diagnosis before subsequent edits

The six K1 byte mismatches all first differ after the actual emitted function
ends: the probe was reading GCC's size into the next section/function because
EvoCC omitted ELF symbol sizes. This is a measurement-boundary defect, not
evidence for changing instruction semantics; exact subject symbol extents and
section-bound checks are required, not a wider relocation mask.

The original 2,799-unit K1 replay `measure-20260907T170640Z`, invoked through
`python3 tools/bench.py kernel --jobs 4 --allow-qemu-drift` (wrapper run
`20260907T170640.283040Z`), measures compiler
`171ab6aaeca625acc11a3d8127137e0edcf2f7d022665498fec2558246b7c52a`.
All 52 formerly assembled units remain assembled, with 2,747 refusals unchanged.
The mixed kernel boots and the substitution proof now has 9 exact matches,
30 matches modulo the existing relocation masks, zero mismatches, 2 no-symbol
units and 11 units outside the vmlinux link. Each of the six former mismatches
now verifies. The kernel adapter remains INCOMPLETE: 41 linked units, of which
39 carry verified bytes, are not 2,799 compiled kernel units or a kselftest pass.
This run precedes the function-address and static-code-relocation fixes below;
its compiler identity is not attributed to those later changes.

Four original wide-bit-field torture tests (`bitfld-3`, `bitfld-5`, `pr32244-1`,
`pr34971`) each exit zero with sealed GCC-14 and 134 with Clang-19 at `-O0`.
Their GCC-specific width arithmetic cannot be treated as a dual-oracle
positive requirement. Original failure counts remain unchanged.

Original `20230630-2` and `20230630-4` are another measured disagreement:
both GCC-14 O0/O2 runs pass, while both Clang-19 O0/O2 runs abort with 134
after warning that `scalar_storage_order` is unknown and ignored. They
assert GCC's reversed object byte order, not ordinary bit-field layout.
Evidence is `.evidence/builtin-probes/20260909-storage-order/results.txt`.
The two subject failures remain in the denominator; matching Clang's
unimplemented attribute is not a passing GCC torture result.

Torture `pr19687` is already covered by Appendix D's inactive-union-tail
decision: GCC and Clang agree at O0 but disagree at O2. Its E3001 does not
authorize initializing bytes outside the profile's selected union member.
`pr22493-1` and `pr23047` explicitly request `-fwrapv`; their native traps
cannot justify removing default signed-overflow obligations.

Two additional actual defects have tests-only checkpoints and focused fixes:

- `9e4f3a2`: function addresses were routed through object storage, and
  `&*get()` through an object-pointer operation. The lowerer now uses the
  resolved function call interface, while both consumers retain a function
  designator's pointer without loading memory. Fifteen address/relocation
  regressions pass, including exactly-once calls and `&*null` cancellation.
  Original c-testsuite `00095` and torture `941014-1` now pass natively.
- `4b50b19`: torture `20071220-2` emits `.xword bar^b1`, which the assembler
  reads as an invalid expression. Static relocations must name the same
  platform-local labels as `block_addr`, including blocks not named by an
  `adr`. Four relocation tests and eight block-address tests now pass;
  original torture `20071220-2` now assembles, links and executes successfully.

`python3 tools/bench.py ctest torture --jobs 4` finishes as INCOMPLETE, not
PASS, in wrapper run `20260907T174641.388158Z`, using compiler
`4746bc92624a640dd1236d6bb12a93ccd79b05c835c19f928e5736ffd6d79c17`.
Native runs `20260907T174641Z` and `20260907T174705Z` preserve the original
220 and 1,692 program identity sets respectively:

- c-testsuite: 194 to 195 pass, 24 to 23 refused, 2 failures unchanged;
  `00095` is the only changed outcome.
- Torture: 1,175 to 1,185 pass, 478 to 469 refused, 35 runtime failures and
  3 link failures unchanged, assembly failures 1 to 0. Newly passing originals
  are `20000314-3`, `20000914-1`, `20071220-2`, `941014-1`, `memchr-1`,
  `pr41239`, `pr46309`, `return-addr`, `strlen-2`, and `strlen-3`.
- Both suites retain every formerly passing program; zero regressions.
  These gains cover the void-return fix as well as the function-address and
  static-label fixes, so they are not attributed entirely to the last patch.

The final working-tree gate is
`CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 4b50b19 --quiet`:
1,314 passed, zero failed, 4 previously ignored tests remain ignored, Clippy
and fmt pass. This is the selected reverse-dependency crate set, not a new
whole-workspace or isolated-clone claim. Python discovery in `tools/k1/tests`
and `tools/ladder/tests` passes 86 and 93 tests; Ruff and diff-check pass.

A goto entering a scope past an array declaration is reduced to
`.evidence/builtin-probes/20260908-goto-storage/probe.c`; both sealed oracles
return zero and C-HIR agrees, while lowering requests an unavailable edge
address. The missing value is a real allocation, not a scalar phi: substituting
zero or cloning an ordinary allocation origin would violate the object model.
This was an implementation issue, not a permission to weaken validation;
`2f268ab` and replay `20260908T080325.213428Z` close it as recorded above.

The first unavailable Csmith input, base seed 11, has a concrete nonterminating
induction cycle, not an EvoCC checksum disagreement. `func_25` returns `&g_37`,
so `func_23` takes the arm containing
`for (g_37 = 0; g_37 >= -28; g_37 = safe_sub_func_uint8_t_u_u(..., g_37, ..., 6))`.
That macro explicitly returns `uint8_t`; each update is in 0..255 and the body
does not change the counter. The loop cannot reach its negative exit bound.
The original sealed run already has four timeouts and a Clang O2 exit 1 for
this seed. A further GCC O1 UBSan check times out; Clang's trapping UBSan build
exits 139 (the non-trapping sanitizer runtime is unavailable in the image).
None of those outcomes is a PASS or authority to rewrite the original seed.
Base and bitfields seed 307 have now also been inspected. Their preprocessed
bodies are byte-identical (SHA-256
`c0cc0b2e72bfc407c6352251c0a548b2e9e334c063693dcf9b9497c8a6b068a4`).
`func_22` reaches `lbl_213` with `p_24=65535` and `l_207=6`; each pass resets
`g_49` to zero and a nonzero 16-bit sum takes the goto before its update.
In a bounded diagnostic of the unchanged source, GCC-14 O2 times out with
or without `-fno-finite-loops`; Clang-19 O2 prints the old checksum by default
but times out with that option. The option also removes its emitted
`llvm.loop.mustprogress` metadata. This is an optimization-dependent reference
domain, not a checksum that every oracle supplies; both original verdicts
remain oracle-unusable. The compact commands, output and source reasoning are
in `.evidence/builtin-probes/20260909-csmith-loops/results.txt`.

Bitfields seed 457 has the same measured optimization dependence: Clang-19
O2 prints `91a167a00`, but adding `-fno-finite-loops` to the separate diagnostic
run times out at the unchanged ten-second limit. The source reaches
`func_48`, where `lbl_267` resets `p_51`, its two-iteration loop leaves it
two, and `if(p_51)` returns to the label without a possible return path.
All three checksum-producing unavailable originals now have an identified
repeating source path and bounded optimization probe; that is not three
passing differential cases. The other unavailable originals still require
individual classification.
The other 64 unavailable inputs still need independent source classification;
none of the 68 unavailable originals is counted as agreement.

Final confirmation command (only after the applicable incremental checks):

```sh
python3 tools/bench.py all --jobs 4 --programs 500 --budget 40 --allow-qemu-drift
```

An incomplete lane, unresolved issue, or deferred prerequisite is reported as
such; it is never changed to PASS merely to reach an all-green summary.

### 2026-09-09: bit patterns and real stack addresses

Signed left shift now uses the measured GNU truncated bit pattern at 32, 64
and 128 bits; invalid counts and the other arithmetic obligations remain.
`cargo test -p evocc --test external_symbol_lowering --test static_integer_initializers -p evocc-chir --test
g3_integer_semantics --test wide_integer_boundary -p evocc-lower --test
day6_g3_differential --test day6_lower_runtime_diagnostics` was covered by
the focused run (81 passing tests across six targets, including static
initializers). The existing 320-source scalar corpus remains byte-identical:
the new rule changes four expected observations from E3006 to integer
results, yielding 260 returns and 60 typed diagnostics, with zero mismatches.
The changed-crate gate against `ce81980` passed 858 tests with zero failures
and three pre-existing ignored tests. Original torture and boot replay is
still pending; focused checks do not close those rows.

The recursive over-alignment fixture now returns zero in both interpreters
and native Darwin execution instead of native exit 1. It tests 32 separate
256-byte-aligned frame objects, live bytes and real allocator spills. Both
target emitters use the reserved x16 scratch sequence and restore sp through
x29. The emitted Linux assembly runs successfully through GCC-14 and through
Clang-19 with `-fno-integrated-as` in the sealed image. Clang's integrated
assembler rejects the pre-existing `:pg_hi21:` global-address spelling; that
is a separate recorded toolchain incompatibility, not a successful lane or
a reason to alter this fixture. Evidence: `.evidence/builtin-probes/20260909-stack-realignment/`.
The command `CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2
tools/gate-changed.sh a53b3c0 --offline` completed with exit 0 (tests, Clippy
and fmt) for evocc, evocc-mir, evocc-policy, evocc-proofcheck and evocc-randgate;
the worktree run used the same target directory by its absolute path.

### 2026-09-09: aligned functions and full-width predicates

`5438c94` implements canonical function `align=N`, maximum alignment across
redeclarations, emitted `.p2align`, and alignment-aware function addresses
in both interpreters. Original `align-3` returns zero through both
interpreters and through GCC-14 / Clang-19 with GNU `as` on Linux.
The changed gate against `4dc50da` passed 3,348 tests, zero failed and eight
pre-existing ignored; Clippy and fmt passed. The focused set passed 66 tests.
Evidence: `.evidence/builtin-probes/20260909-function-alignment/`.

`e71ad8d` records the semantic RED for full-width predicates;
`d11b20e` implements the carry chains, four-limb magnitude product and
full-width divisor/count checks without relaxing the validator or admitting
128-bit division. The focused command `cargo test -p evocc-mir --test
wide_predicates --test section5_wide_integers --offline --no-fail-fast`
passed 27 tests; native execution also checked 21,074 independent bounds.
`tools/gate-changed.sh ed38a72 --offline` passed 599 tests, zero failed,
four pre-existing ignored, Clippy and fmt (same shared frontend target).
The five original programs `pr122943`, `pr54471`, `pr61375`, `pr63302`,
and `pr85582-2` emitted and executed successfully through both Linux
toolchains. Of their 20 reference O0/O2 executions, GCC-14 O2 `pr122943`
aborted; the other 19 passed. That reference failure remains distinct from
the ten passing subject executions. Evidence:
`.evidence/builtin-probes/20260909-wide-predicates/results.txt`.
Neither focused measurement is a new full-benchmark result.

`f9e4696` resolves the original target/board requirements for all 1,692
torture identities: zero unresolved, 89 inapplicable by original options or
selectors. These are preparation counts, not executed passes. The 114
ladder tests and Ruff pass; expected-failure annotations never change a
native verdict, and only the original `dg-timeout-factor` changes a
program's default limits. Target evidence is
`.evidence/builtin-probes/20260909-torture-requirements/results.txt`.

### 2026-09-09: mixed-link replay after shifts and alignment

`python3 tools/bench.py kernel --jobs 4 --allow-qemu-drift` finished run
`20260909T053908.023962Z`, capture
`.evidence/k1/measure-20260909T053908Z/`, measuring `ed38a72` and executable
`49f4833501b006b2dcbea979c2cfd2e361466264bfd3d3ce0f6c675a703ecefd`.
The executable was not rebuilt during the run. All 90 assembled objects
were substituted; relink returned zero and the first boot passed, without
removing `insn.o`. Substitution proves 73 units present in vmlinux and 76
in the booting kernel in total (previously 75), not all 2,799 target units.
Image `7cc2381683dc6238f62c8519e35de993cf2acbed0b51bf91e981222cf76b6bb3`
and serial `0904ec05e343f425cfcbfe660ec0a8082be8191676140cc2cabbb11fc5c5cba3`
are recorded by the existing lane. Its INCOMPLETE exit is correct.
The 2,709 refusals are 2,419 E3021 numeric asm-label references, 286 E3101,
two E3026, one E3029 and one E3031. Of E3101, 238 say external object layout,
47 say incomplete-array layout and one says no live address binding.
These are new measured frontiers, not a prediction of the next pass count.
