# Roadmap benchmark evaluation

Latest refresh: [2026-09-15 measurements and current issue inventory](benchmark-evaluation-20260915.md).
The September 7 measurements below are historical; they are not the current
benchmark or kernel acceptance status.

Evaluation started 2026-09-07, run `20260907T141450.311159Z`, subject
`ad6176f32bf5b122830106390019898c99a9efe3`. Compiler binary SHA-256:
`b13d4a08539dfb6056e61f47631768357ced2e9f404247d7893593643c9ca6e6`.
This change adds an evaluation interface; it changes no compiler semantics.

```sh
python3 tools/bench.py all --jobs 4 --programs 500 --budget 20 --allow-qemu-drift
```

The [latest receipt](../../../tests/contracts/bench-receipt.json) records each
command, duration, raw outcome counts, previous receipt and evidence location.
The [status page](../../status.md) is generated from those receipts. This review
records interpretation, not a second acceptance authority.

The musl correction is measured by a second invocation,
`20260907T153648.272156Z`, checkout `388c399`, with the same compiler digest:

```sh
python3 tools/bench.py musl --no-build --jobs 4 --budget 20
```

It retains the first invocation's other lane readings with their original
measurement identities. Both invocations are complete; no benchmark remains
running. The original all-run exits 2 (runner/final-receipt failures); the musl
rerun exits 1 (measured incomplete coverage), not 0.

## Measurement boundaries

- E0 and Csmith count **programs**. Whole-program lanes and the kernel count
  **translation units in a tested mixed build**, not independently passing tests.
- The existing lanes run serially, with four emission/Csmith workers. Their
  reference-build settings and per-program watchdogs are unchanged. Wall times
  include compilation, native execution and container overhead; they are not
  compiler speed comparisons.
- Csmith uses both campaigns, 500 original seeds each, and the shared runtime
  repairs already recorded in its receipt. An unavailable or disagreeing
  reference is not an agreement and remains in the 1,000-program denominator.
- Torture remains a mixed integer/FP corpus. The runner does not consume its
  `dg-options`, target requirements or timeout factors. In particular, a test
  requiring `-fwrapv` is not evidence that default checked arithmetic is wrong.
  A complete, frozen **non-floating-point eligible** denominator is still missing.
- zlib/zstd/TinyCC/toybox/musl retain the existing lane's documented source,
  sysroot and build adjustments. These results do not prove unmodified upstream
  sources compile. Tests passing relative to a failing reference baseline are
  reported separately from a fully green suite.
- cJSON's current `make test` compiles the reference C sources directly instead
  of consuming substituted objects. Its result is `VALIDATION_GAP`, not subject
  execution evidence, even if that reference command passes.
- The first musl attempt exposed a broken measurement boundary: relinking also
  rebuilt libc-test's `runtest.exe` against the tested libc. Its children spun
  before exec while the parent waited, bypassing its own five-second timeout.
  That task-owned attempt was stopped and remains `ERROR` in the initial run,
  not a pass. The harness now retains the reference supervisor and keeps test
  programs linked to the substituted libc; the original five-second limit stays.
- The kernel adapter is mixed-link boot plus substitution verification. It is
  neither a fully EvoCC-built kernel nor a kselftest run. A boot cannot override
  a failed substitution check.
- This is the roadmap's real-program correctness/coverage reading (Q1), not
  a new replay of all architectural gates or a GCC/LLVM optimization-speed
  comparison (Q2/Q3). Historical architectural corpus passes remain historical.
- Mbed TLS, PolyBench, llvm-test-suite, jq, Lua, SQLite, the full-kernel build and
  kselftest have no adapters yet. The entry point reports `NOT_IMPLEMENTED`;
  the existing architectural corpus counts do not stand in for these milestones.

## Observed blocking constructs

The following are measurements and source inspections, not all established
compiler root causes. Interpreter refusal of an external library call is a
classification limitation, not an explanation of a native wrong answer.

| Area | Observation | Example / evidence |
|---|---|---|
| Object aliases | All three torture link failures reference undefined `b` in an object `alias` declaration. | `alias-2`, `alias-3`, `alias-4`; E0 run `20260907T141514Z/results.jsonl` |
| Macro state | c-testsuite `00206` exits zero with wrong stdout; its source uses `push_macro/pop_macro`. Torture `pushpop_macro` aborts. | E0 runs `20260907T141450Z` and `20260907T141514Z` |
| Switch / labels | Accepted syntax still encounters lowering restrictions: non-compound switch body and unstructured labels. | c-testsuite `00051`, E3101; one refusal of each construct in that run |
| Function-pointer declarations | Four c-testsuite programs are refused with `fnptr requires a prototype`. | E0 run `20260907T141450Z/results.jsonl` |
| General refusal bucket | Torture has 108 `E1001 expected-declaration` refusals. They need source-form classification; they are not 108 instances of one proven root cause. | E0 run `20260907T141514Z/results.jsonl`, example `20000717-3` |
| Integer vectors / places | All 23 `excess-initializer` sources and 17 of the 18 `invalid-projected-lvalue` sources contain a vector attribute spelling. The inspected examples use integer SIMD: non-floating-point does not mean scalar-only. This lexical inventory is not an eligibility classifier. | `20050316-1`, `pr105613`; E0 run `20260907T141514Z` |
| Variable-length objects | Torture has 19 VLA refusals, a known profile boundary rather than a silent miscompile. | `20010209-1`; same E0 run |
| Native wrong answers / traps | Torture has 35 execution failures; bit-field cases and `mode`-attributed memory tests need oracle-guided reductions. Do not weaken arithmetic checks to turn these green. | `bitfld-3`, `bitfld-5`, `memclr`, `misalign`; same E0 run |
| Header type admission | zstd stops at GCC's builtin vector types in `arm_neon.h` (31 units), `_Float32` declarations in `stdlib.h` (7), and `__PRETTY_FUNCTION__` (4). cJSON stops at vector typedefs in `math-vector.h` (2). Admitting unused declarations and executing SIMD/FP are separate requirements. | zstd run `20260907T144316Z`, cJSON run `20260907T144430Z`, `units.jsonl` |
| Mixed-link exclusions | TinyCC emits 4 objects but only 2 remain in the passing build; `arm64-link.o` and `tcc.o` cause relink failure. | TinyCC run `20260907T144438Z`, `bisect.json` |
| Return validation | 16 toybox units hit `return expression disagrees with the function result type`; another hits an unavailable edge environment value. These need a shared-path reduction, not 17 unit-specific patches. | `blkid.o` / function `escape`, `cut.o`; toybox run `20260907T144525Z` |
| Kernel asm inputs | 2,745 units are refused with `unsupported assembly input constraint Kr or operand type`. This is the dominant current frontier, not a generic asm or `Ir` count. The other two refusals are frame address and computed goto, one unit each. | `.vmlinux.export.o`, `atomic_ll_sc.h:278:1`; K1 run `measure-20260907T145629Z/survey/units.jsonl` |
| Substitution identity | The mixed kernel boots, but six units fail byte comparison; this leaves K1 failed and the native final receipt unpublished. | `measure-20260907T145629Z/proof.json`, details below |
| Explicit register bindings | Emission inspection finds no assignment to `x8` before `svc 0` in musl's `__syscall6`, while its input source binds the syscall number to `register long x8 asm("x8")`. This is a concrete lowering candidate, not yet a reduced explanation of the full libc-test failure set. | `.build/ladder/musl-asm/obj__src__misc__syscall.lo.s`, `.build/ladder/musl/arch/aarch64/syscall_arch.h` |

Csmith's raw 1,000 unique `(campaign, seed)` rows independently recompute to
932 agreements and 68 unavailable references. For all 68, EvoCC, GCC O0/O2 and
Clang O0 time out; Clang O2 has 48 timeouts, 17 nonzero exits and 3 completed
runs. This is not 68 proven compiler bugs, nor 68 successes. Evidence:
`.evidence/ladder/csmith/20260907T141825Z/results.jsonl`.

Toybox's denominator grew from 201 to 203: the fresh build also enumerated
`factor.o` and `fallocate.o`, and both assembled. On the common 201-unit set,
assembled coverage is 172 before versus 175 now; total tested mixed-build
coverage is 177/203 with the same `chattr immutable` reference failure. The
test universe is the adapter's recorded 56 named test outcomes, not every
possible upstream configuration.

## Measured milestone readings

These are coverage counts, not a percentage of compiler architecture completed.
The previous column is the last available record, not the compiler before this
harness change: this turn changes no compiler semantics. Musl's supervisor and
bisection configuration changed, so its before/after counts are not a controlled
compiler comparison; toybox's changed denominator is reconciled above.

| Benchmark | Previous | Fresh reading | Meaning / native run |
|---|---:|---:|---|
| c-testsuite | 187/220 | 194/220 programs pass | 24 refused, 2 fail; `20260907T141450Z` |
| GCC torture | 1,175/1,692 | 1,175/1,692 programs pass | 478 refused, 35 runtime failures, 1 assembly and 3 link failures; `20260907T141514Z` |
| Csmith | 932/1,000 | 932/1,000 programs agree | 68 unavailable references, 0 mismatches; `20260907T141825Z` |
| zlib | 19/19 | 19/19 units | PASS for the recorded static/non-PIE build; `20260907T144305Z` |
| zstd | 2/44 | 2/44 units | Partial mixed build; `20260907T144316Z` |
| cJSON | 0/2 | 0/2 units | Two front-end refusals and subject-validation gap; `20260907T144430Z` |
| TinyCC | 1/11 | 2/11 units | Four assembled, two excluded by relink; `20260907T144438Z` |
| toybox | 172/201 | 177/203 units | 175/201 on common set; one reference failure; `20260907T144525Z` |
| musl | 178/1,339 (older harness) | 817/1,339 units | No new failures relative to 17 baseline failures; `20260907T153648Z` |
| kernel | 42/2,799 objects | 52/2,799 objects | Mixed boot PASS, substitution gate FAIL; `measure-20260907T145629Z` |

The kernel run enumerates 2,799 unique units, assembles 52 and refuses 2,747.
It substitutes all 52; 41 appear in the linked image. Their checks are 11 proven,
22 proven modulo relocations, 2 without symbols, and 6 mismatches; another
11 units are not linked. The six mismatching symbols are:

- `screen_info_pixel_format` in `drivers/video/screen_info_generic.o`;
- `fdt_nop_node` in `lib/fdt_wip.o`;
- `HUF_readStats_wksp` in `lib/zstd/common/entropy_common.o`;
- `FSE_decompress_wksp_bmi2` in `lib/zstd/common/fse_decompress.o`;
- `ZSTD_getErrorString` in `lib/zstd/common/zstd_common.o`;
- `ZSTD_decompressBlock` in `lib/zstd/decompress/zstd_decompress_block.o`.

Raw results are in `.evidence/k1/measure-20260907T145629Z/`:
`survey/units.jsonl`, `survey/substitution.json`, `attempt-01/boot/boot.json`,
and `proof.json`. The native script exits 1 at the proof check, so the wrapper
reports `ERROR` (no fresh final receipt); it does not reuse the older K1 receipt
as new evidence. The boot is observed; successful substitution is not established.

Musl's bisection independently identifies `obj/src/signal/sigsetjmp_tail.lo`:
round 15 substitutes that single object and fails `setjmp-static.exe` and
`setjmp.exe`; round 14's sibling `sigset.lo` passes relative to the baseline.
Its `__syscall4` has the same missing-`x8` emission pattern noted above.
Spec section 10.1.18 already states that a local register binding matters when
used as an asm operand. Silently treating that binding as irrelevant is not
authorized by the rule that ordinary local values behave like automatic objects.
After excluding that single unit, round 16 retests the entire remaining
817-unit set and finds no regression. The 20-probe budget is not exhausted.
The full population is 817 in that tested set + 1 runtime-failing unit +
167 symbol-parity exclusions + 354 compile refusals = 1,339 unique units.
The exclusions and baseline failures keep the milestone `INCOMPLETE`.
The largest refusal messages are non-integer/non-object-pointer function
value types (213) and invalid floating literals (83); these include known
FP boundaries and must not all be labeled non-FP compiler defects.

## Next work, ordered by correctness and reach

1. Reduce native wrong-answer candidates before enlarging the pass denominator:
   macro push/pop state, object aliases, bit-field and memory operations. Run
   `python3 tools/bench.py ctest torture` after each semantic fix. Account for
   torture's required options before labeling a reference-extension case a bug.
2. Inspect the six K1 substitution mismatches without weakening comparison,
   and implement the measured `Kr` asm-input frontier. Recheck with
   `python3 tools/bench.py kernel --allow-qemu-drift`; boot alone is insufficient.
3. Reduce musl runtime failures and toybox return/edge validation; then admit
   unused header types separately from executing SIMD/FP. Recheck the selected
   `musl toybox zstd cjson tinycc` lanes. Repair cJSON's subject test linkage
   before using its tests as evidence.
4. Freeze the non-FP torture eligibility/options and investigate the 68 Csmith
   unavailable references. Keep original seeds and report unavailable outcomes;
   do not obtain an all-pass label by voting, skipping, or changing denominators.
5. Add the missing Mbed TLS adapter next in roadmap order. The other seven
   absent adapters stay explicitly unimplemented; FP execution remains deferred.

## What this implementation improves

One selector now owns benchmark dispatch, a single measured release binary,
sequential heavyweight lanes, bounded worker settings, fresh-receipt checking,
truthful nonzero exit statuses, and automatic status rendering. Individual runs
retain other milestones' previous readings with their own provenance. Existing
lane captures are reused; the wrapper writes a small summary and bounded error
tail, not another copy of every log.

The evaluation itself found and fixed the musl supervisor self-dependency.
It also removed stale README claims and made cJSON's validation gap and the
unimplemented milestones visible. None of these changes claims to have repaired
the compiler failures enumerated above.

## Verification of the benchmark tooling

| Command | Observed result |
|---|---|
| `python3 -m unittest discover -s tools/ladder/tests` | 93 passed |
| `python3 -m unittest discover -s tools/k1/tests` | 80 passed |
| `ruff check tools tests/acceptance` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `bash tools/gate-changed.sh 13ca033` | No crate changed; no Rust tests selected |
| `git diff --check` | PASS |

The 173 Python tests also pass in an isolated `git clone --no-local` checkout,
with local-only fetch and no network or native benchmark prerequisites. This
checks the tooling's replayability, not a second execution of every native lane.
No new full-workspace Rust, Clippy or layout-corpus measurement is claimed for
this harness-only change. Raw outcome sums and unique program/unit identities
were checked independently before writing this review.

## Address-operand measurement

Subject `8ad4ea3`, release SHA-256
`256036c08ef66f493dff435d4504eea6ec3ea988745a237c7b5cea36c3b9fded`:

- `python3 tools/survey-cohort.py --command-record-root .build/reference/gcc-lane-1 --action lower --evocc target/frontend/release/evocc --jobs 4 --timeout 60`:
  run `20260909T031003Z-16402`, 90 clean, 2,708 diagnosed, zero unavailable,
  timeout or panic, out of the unchanged 2,798 units. The 2,706 E3021 units
  share a pointer input tied to a 64-bit integer output in `RELOC_HIDE`.
- `python3 tools/bench.py kernel --no-build --jobs 4 --allow-qemu-drift`:
  run `20260909T035847.646343Z`, 90 assembled and 2,709 refused out of 2,799.
  The first 90-object mixed image times out; bisection isolates `insn.o`.
  Restoring that reference object gives a booting 89-object substitution,
  with 75 units proven included by the lane's accounting. Status remains
  **INCOMPLETE**, not a whole-kernel pass.

These measurements precede the signed-shift, stack-realignment and tied-input
fixes. They are not closure evidence for those later commits. Raw lowering
and mixed-link unit denominators are distinct and are never added together.

## Native replay on `2f15059`

The release driver digest is
`8cda0bfd926552d1d118487d9c7e9afca71c1e38706e87e9b3b468c464238e84`.
`python3 tools/bench.py musl tinycc toybox ctest --no-build --budget 45 --jobs 2`
completed as `INCOMPLETE` in wrapper run `20260909T132841.817643Z`:
musl 1,010/1,339 assembled and included in the passing mixed build, TinyCC 8/11,
Toybox 195/203, and c-testsuite 209/220 native passes with 11 refusals.
Musl gained three original units (`__syscall_cp`, `pthread_cancel`, `strftime`),
with no other per-unit outcome changes; its 17 reference-test failures remain
visible. The earlier sandbox invocation failed to start the container and is
not a compiler regression or a passing run. Torture and Csmith were not rerun.

`python3 tools/bench.py kernel --no-build --jobs 2 --allow-qemu-drift` finished
unsuccessfully in wrapper run `20260909T133504.882775Z`, capture
`.evidence/k1/measure-20260909T133504Z/`: 2,193 assembled, 605 refused, one
assembler-refused, zero timeouts, out of 2,799. The original tools excluded
118 units for reported symbol collisions and attempted 2,075 substitutions;
the mixed link failed. Three reductions isolated `cpuinfo.o` (boot timeout),
`debug-monitors.o` and `entry-common.o` (link failures), and the fourth attempt
with 2,072 substitutions still failed. The previous passing K1 receipt was
not replaced; the unified wrapper correctly reports `ERROR`, not a fresh pass.

The single assembler failure is a missing search path for the unchanged
`.incbin "kernel/config_data.gz"`, not missing compiler output. Collision
classification also conflates weak definitions with strong ones and misses
the nVHE/vDSO link namespaces; corrections do not retroactively change this
run. Link logs additionally expose references to `__bad_size_call_parameter`
and `uprobe_brk_handler`, and forbidden vDSO GOT relocations for hidden
external objects. These are newly measured work items, not closed milestones.
