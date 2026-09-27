# Roadmap benchmark refresh — 2026-09-15

Status: **fifteen application lanes measured; overall INCOMPLETE**.
The latest complete campaign includes PolyBench. The dated checkpoints below
retain their original readings; they are not the current scoreboard.

September 27's final musl replay **`20260927T110110Z`** attempts all **1,339**
units, assembles **1,271** with exact symbol parity, and retains all 1,271 in
two complete libc-test runs (482 runtime-result slots each). Both have exactly
the reference's 17 failing cases, zero additional failures and zero replaced
objects; the mandatory final replay exits normally. The original pthread
regressions (B08c) and finite strict-FP profile (L01) now close. The application
lane remains **INCOMPLETE**: 68 floating-complex E3049 refusals (L04), 17 shared
failures (B08d), a static-only loader scope and explicit option omissions (B08b).
This is a mixed-library result, not all-EvoCC musl or all-tests PASS.
[Repair and exact candidate identity](../../tests/fixtures/assembly-exclusive-window.out).

September 27's [approved scalar FP assembly extension](../../tests/fixtures/floating-assembly.out)
closes B08a in EvoIR 1.9.114 / EvoMIR 0.50. It reuses the SIMD operand and
allocation path for exact f32/f64 transport, without inventing an interpreter
for opaque asm. Native tied/early-clobber/fixed-register/goto/spill and crossed
call cases pass. Eight native fenv builds pass 58 checks each; all eight original
musl math units now assemble. The four original ceil/floor/rint/rintf objects
pass 56 checks with each of four reference callers, as do reference controls.
This supersedes the four scalar-asm refusals in the older checkpoints below,
not the full musl/libc-test receipt or its remaining option accounting.

September 27's [per-symbol section regression](../../tests/fixtures/driver-symbol-sections.out)
closes a further musl option-projection gap: real function/data sections now
preserve ELF storage/TLS types, relocations, alias identity, lifecycle roots and
patch-entry link order. Twelve reference and twelve EvoCC objects pass all 48
ordinary/GC native executions. Eight unchanged musl commands retain both section
options and standard excess precision: four assemble, four retain the unapproved
scalar-FP asm refusal. This is a targeted command replay, not a musl/libc-test
whole-program result; its historical full receipt remains historical.

September 27 B06b repairs the shared toybox test environment: upstream `test.sh`
creates its test directory under PWD, so tests on the host FUSE mount could not
use file-attribute ioctls. A small mutation probe separates that from the missing
immutable capability on local ext4. Runtime-only staging plus the single
`CAP_LINUX_IMMUTABLE` capability changes the original 15-case chattr subset from
**0 PASS / 5 FAIL / 10 SKIP** to **13 PASS / 0 FAIL / 2 SKIP** on both unchanged,
previously audited binaries. j/P conditional skips remain; full toybox and its
two FP objects remain open. No fresh full-suite numerator is implied.
[Source, commands, identities and 253-test gate](../../tests/fixtures/toybox-filesystem-mutation.out).

Current round (2026-09-20): the post-HFA-repair frozen release completed all
15 application lanes serially. Torture has **zero compiler failures** in its
five-way account; Csmith has **zero mismatches**, with 68 oracle-unusable cases
retained. SQLite passed **329,803 assertions**, Mbed TLS **148/148 CTest**, and
zstd **44/44 objects**. LLVM, TinyCC, musl and deferred language boundaries
still prevent overall closure; their small reductions are the next repair batch.

September 23 reference-limit closure (B04): the separate-TU
[`hva-varargs-reference.c`](../../tests/fixtures/hva-varargs-reference.c) isolates
GCC 14 O2's omitted HVA register-save-area reads while stack arguments work.
Eight usable EvoCC/reference ABI pairs and the unchanged original varargs
fixture pass; the GCC O2 self-control and callee pairs remain oracle-unusable.
The three original torture reference refusals also reproduce with their exact
source hashes. [Measurements and limits](../../tests/fixtures/hva-varargs-reference.out)
close their accounting only; no full-campaign result or PASS numerator changes.

September 23 toybox acceptance correction (B06a/T05c): the historical 201/203
objects were compiled, but both test logs contain only **57 occurrences
(56 PASS / 1 FAIL)** and stop at `chattr immutable`. The previous dictionary
also collapsed two identically named base64 tests. A further RED proves that
plain `make` can retain an old executable because generated objects are absent
from its prerequisites. The common scoreboard is now **VALIDATION_GAP**, with
zero validated full-suite units; original captures and compilation counts remain.
The adapter replays the recorded object-link argv and upstream strip step,
preserving source/config/generated inputs, then audits installed object hashes.
It uses upstream `VERBOSE=all`, checks normal completion and exit status,
preserves occurrence identities and force-replays the final bisection selection.
Forcing the upstream build recipe was tested and rejected: its append-only
`generated/build.sh` cache caused the installed subject object to be rebuilt.

The [small filesystem probe](../../tests/fixtures/toybox-filesystem-flags.c)
records GETFLAGS failing with ENOTTY on the FUSE benchmark mount but succeeding
on container-local ext4, identically for GCC14/Clang19 O0/O2 and EvoCC.
The unchanged complete **chattr subset**, not the full suite, now runs on actual
reference/current-EvoCC applet relinks: both **0 PASS / 5 FAIL / 10 SKIP**;
the subject object survives and its binary hash differs from the reference.
This is an evidenced filesystem boundary, not a waived PASS. Two FP objects,
complete toybox execution and any newly exposed applet failures remain B06/B10.
[Measured identities and acceptance limits](../../tests/fixtures/toybox-filesystem-flags.out).

Kernel testing continues independently. The new EL2/GICv3 KVM reference run
starts all 37 original programs: **24 PASS / 4 SKIP / 2 FAIL / 6 TIMEOUT /
1 INCOMPLETE**, with source-bound configuration, memory and timing diagnoses
in `kernel-selftest-gaps-20260914.md`. It is not a subject result. Subsequent
fixed-CPU controls pass the original SVE, AArch64-only ID-register, nested-EL2
and demand-paging programs; unsuccessful timing controls remain failures.
The section-repaired run `20260921T065917Z` is now **PASS**: all
**6,765/6,765 target-C objects** compile and remain EvoCC-built after the
generated-input fixed point and complete link; all **745 modules** link with
BTF against the new `vmlinux`. Modpost warnings fall from **2,515 to zero**.
The exact new Image boots on HVF-host, prints the bound nonce and shuts down
normally. Installed QEMU drift is explicitly authorized and recorded. This
closes this configuration's section/link/boot blocker, not expanded KUnit,
module loading or the new full kselftest pair. The receipt is
`tests/contracts/kernel-bpf-full-receipt.json`; all application readings above
retain their original compiler identity and are not remeasured by this run.

Historical repair sequence:

The separate EvoCC kernel run `20260920T103126Z` verified all
**6,765/6,765 objects**, with no extra or missing defined symbols, and reached
the actual-generated-input fixed point. On September 21 (local time), final
link failed: two early-boot CALL26 overflows and six retained build-assertion
references. Module link/BTF and boot are NOT_RUN; the KUnit queue correctly
did not launch. The source reductions and targeted repairs are tracked in
`kernel-selftest-gaps-20260914.md`. Neither the compile result nor earlier
runtime controls close the pending paired full kselftest and expanded KUnit.
The six assertion references now disappear in all three original-unit replays
on `4fbbf3a8…901eb2e`; their assembly and defined-symbol audits pass. The
48-test reduction/control suite and native dual-oracle reduction pass. All
workspace tests finish with 2,593 PASS, zero failures and four historical
ignored; Clippy and fmt subsequently passed. This is a targeted repair,
not a new full kernel link or application-campaign result.

The approved function-section repair is now implemented through declaration
merging, EvoIR metadata/proof preservation and ELF emission (1.9.104 / 0.34).
Final release `73eff5c4…e4a46b` passes both interpreters and both sealed assemblers'
native runs; six original kernel objects assemble with matching symbols.
The two CALL26 targets now occupy `.init.text`, and the assertion repairs
remain green. The all-workspace changed gate passes: 2,598 tests, zero failures,
four historical ignored, Clippy `-D warnings` and fmt; 150 frozen layout cases
and 229 K1/bench-entry Python tests pass. Fresh complete composition is pending;
the historical warning count and failed link have not been relabeled PASS.
That full attempt was interrupted upon finding two pointer-declarator object
section omissions. The shared parser correction also covers pointer-returning
functions; fixed ARM64 controls and the eight linkage tests pass. The new
snapshot `dc6f68ee…875763d` now passes its own full gate (2,598 tests,
Clippy and fmt), 150 layout cases, 229 K1/bench tool tests, Ruff, eight
interpreter executions and eight dual-assembler native executions. Seven
original kernel objects pass assembly and symbol/section checks. Its fresh
complete composition, module link/BTF and actual boot/shutdown now pass under
`20260921T065917Z`; the verdict is not inherited from an older run. Additional
isolated locked/offline producer, checker and client replay passes 13/13 tests.
The portable [remaining-semantics research brief](remaining-semantics-research-brief-20260921.md)
separates strict fenv, binary128 and the measured dialect/oracle disagreements
from this independently implemented section fix.

### Completed frozen replay — 2026-09-20

`tools/bench.py ctest torture polybench csmith llvm-test-suite tinycc toybox musl
zlib cjson jq lua sqlite mbedtls zstd --jobs 2 --programs 500 --campaign all
--budget 45` finished serially, with the existing case-sensitive sysroot.
Compiler: `.build/floating-targeted/evocc-20260920-closure`, SHA-256
`ff5f51e540abf40d9f660ade9193a5930ce2f9a274e18fe43d77b4f74c691882`.
Capture: `.build/floating-targeted/applications-20260920-closure.txt`.
HEAD remains `9f571cac0e6be6d27915aa82b4c8866ae84eaf9a`; the substantial
working-tree changes are included, not represented by HEAD alone.

For each sorted, unique tracked/untracked nonignored regular path, hash its
path bytes, NUL, and raw SHA-256 content digest; concatenate those records and
SHA-256 the result. Compiler paths `Cargo.toml`, `Cargo.lock`, `.cargo`, `crates`,
`vendor`: **508 files**, `3ee32b2bf56f06f0b85d7620e8238f0a98e82208843ba00e455b95fe33ca053f`.
Harness paths `tools` plus `tests/contracts/benchmark-program-dispositions.json`:
**67 files**, `c99b6cb6bb883c951f09c13ecd8e92022e0edafd5f8d2ebc5360b56cc387b266`.
EvoIR spec SHA-256: `6b7a49b0f651a74c02ef6415e4b3fe608107b3a97e2db9ace9f512584d7a15b9`;
EvoMIR spec: `719ef2c33180e50852df180c4ad3799f44ebf7a3389d687152c34b6f6de09bb0`.
No new workspace, source clone or target tree was created. Parent run:
`20260919T222250.259791Z`, former PID 97399 (finished). Completed children:

| Lane | Fresh result | Remaining ledger |
|---|---:|---|
| c-testsuite (`20260919T222250Z`) | 219/220 PASS; 0 compiler failures | 1 binary128 deferral |
| torture (`20260919T222317Z`) | 1,504/1,692 PASS; 0 compiler failures | 39 FP-deferred, 146 not applicable, 3 oracle-unusable |
| PolyBench (`20260919T222638Z`) | 30/30 original kernels PASS | MINI_DATASET correctness; upstream allocation utility remains reference-built |
| Csmith (`20260919T222655Z`) | 1,000/1,000 emitted; 932 agreements; 0 mismatches | 68 oracle-unusable, unchanged seeds and time limits |
| LLVM (`20260919T230110Z`) | 1,014/1,735 PASS | 44 compiler failures, 17 FP-deferred, 659 not applicable, 1 oracle-unusable; classification review pending |
| TinyCC (`20260919T231249Z`) | 8/11 objects in passing mixed build | 2 deferred FP first errors and 1 real E3101 static initializer |
| toybox (`20260919T231320Z`) | 201/203 compiled; acceptance invalidated | September 23 audit: truncated test prefix and unverified relink; 2 FP objects remain |
| musl (`20260919T231421Z`) | 0/1,339 objects; subject tests NOT RUN | Original `-frounding-math` now faithfully forwarded and explicitly refused; not 1,339 independent compiler defects |
| zlib (`20260919T231524Z`) | 19/19 objects, original tests PASS | None in this configuration |
| cJSON (`20260919T231532Z`) | 2/2 objects, original tests PASS | None in this configuration |
| jq (`20260919T231539Z`) | 68/69 objects in passing mixed build | `src/builtin.c`, binary128 call interface |
| Lua (`20260919T231634Z`) | 34/34 objects, original tests PASS | None in this configuration |
| SQLite (`20260919T231713Z`) | 86/86 objects, original tests PASS | 329,803 assertions, zero errors |
| Mbed TLS (`20260919T232127Z`) | 382/382 objects, selftest and 148/148 CTest PASS | No rebuilt installed objects; default configuration, not every upstream configuration |
| zstd (`20260919T233745Z`) | 44/44 objects, original tests PASS | None in this configuration |

These close the prior missing-libm/HFA failures and PolyBench contraction-profile
differences through original-program execution. PolyBench retains exact array
dump comparison, not a new tolerance. Exclusions are not PASS. Csmith base is
469 agreements / 31 oracle-unusable; bitfields is 463 / 37. No child of this
campaign remains running; kernel acceptance remains separate. LLVM's full
mixed lit pass count includes reference-built programs and is not the EvoCC
numerator. Its large-data timeouts do not contradict MINI_DATASET correctness.

Three new tailored REDs, independently accepted and executed by both sealed
oracles at O0/O2, are `floating-comparison-builtins.c`,
`polynomial-scalar-place.c`, and `static-narrow-member-offset.c` under
`tests/fixtures/`; `benchmark-small-frontier.out` binds them to original first
errors. Compiler and harness remained unchanged throughout that campaign.
Four automated REDs (these three plus redundant FPCR admission checks) then
failed as declared, with zero Rust compilation errors. Their shared repairs
pass all four targeted tests, the 60-test language suite and native ARM64
reductions on release `3d4dac62…c3b5e3`. Original compare-fp-1, compare-fp-4
and pr50310 now link/run; libtcc.c emits. compare-fp-3 still fails linking on
impossible-path references. The full changed gate completed **2,580 PASS,
zero failures, four historical ignored**, plus Clippy `-D warnings` and fmt.
TinyCC's fresh `20260920T000937Z` run on this repaired compiler rises to
**9/11 objects**, with original baseline and subject tests both passing;
only E3049 and E3050 remain, one deferred FP object each. Its former E3101 is
closed by actual object installation and original tests, not emission alone.
The musl flag frontier is real: older coverage measurements omitted a semantic
option and cannot substitute for this run. Strict dynamic fenv is still deferred;
dropping `-frounding-math` to recover the old numerator is not an accepted repair.

During this frozen replay, the original LLVM `20090113-1` obtains positive
dynamic initializedness evidence without changing its source: independently
GCC- and Clang-UBSan-instrumented O0 objects each report nine Memcheck errors
at line 43, traced to `main`'s uninitialized stack array. Plain O0 Memcheck
controls report none; the instrumentation exposes the unknown arithmetic input
to a checked decision. Exact commands, diagnostic-tool package hashes, matching
detached libc symbols and this limitation are in
`tests/fixtures/llvm-uninitialized-original.out`. The active run's disposition
ledger was not changed mid-run. `pr23135` still has only GCC's positive vector
overflow diagnostic; Clang instrumentation does not report it.

After the frozen run finished, 29 old FP reviews were narrowed to explicit
binary16/binary128/complex/vector execution. The repaired polynomial place in
the original NEON test now reaches a genuine float-vector `vcreate_f32` cast,
separately recorded rather than excusing the former E2101. Four unchanged
originals have new dual-oracle GNU11 implicit-declaration probes and F01
dispositions; 20090113-1 has the positive dynamic evidence described above.
These source/command-bound ledger changes do not rewrite the historical
1,014/1,735 measurement or turn excluded originals into PASS.

### Subsequent targeted LLVM repairs — 2026-09-20

**2026-09-23 cJSON full private-helper closure:** corrected flight
`20260923T073554Z`, through `tools/bench.py cjson`, installs **23/23** EvoCC
objects: the two libraries and all 21 original Unity test translation units.
The latter preserve `common.h`'s embedded `cJSON.c`, so private helpers are
actually compiled by EvoCC rather than bypassed by reference test compilation.
The original public Makefile test and five public-API controls remain as separate
consumers of the installed library objects. Unity alone is reference-built and
listed in `unmeasuredBuildUnits`. There are zero refusals, exclusions or replaced
installed objects. Mutation tests independently corrupt each library and one
private-test object; each corruption fails the suite after relinking without
recompilation.

All **21 upstream programs** complete; their Unity totals are **161 PASS,
0 FAIL, 1 IGNORE**. The extra five public-API controls add **54 PASS**.
The ignored `print_number_should_print_non_number` has an unconditional
`TEST_IGNORE()` in original `tests/print_number.c:105` (its C89 FIXME), not an
EvoCC-added skip. Four `disabled: true` fixtures in the pinned JSON-patch inputs
produce eight SKIPPED messages across apply/generate paths; these are not added
to the PASS count. Sources/assertions and input files are unchanged. The adapter
uses the complete pinned CMake test selection beside the release Makefile with
explicit `-std=c89 -O2`; optional sanitizer/Valgrind builds are not claimed.

Compiler SHA-256 `ef62a7d2fae2d979e76d2ad30d5bc94c6ee60ef387a679b35e54d423bdee7875`;
test log `.evidence/ladder/cjson/20260923T073554Z/test-attempt-1.log`, SHA-256
`73bcd3e51d16cd2de7c9d295f58280b1468a9f86e277d0fef4c6368c518e1f08`.
The source/object/run facts are in `tests/contracts/ladder-cjson-receipt.json`;
the common bench receipt and generated status now show 23/23. This closes B09's
stated suite-coverage task, not the complete final application campaign.

**2026-09-23 benchmark verdict correction:** inspection of cJSON flight
`20260923T073305Z` found an empty test log despite a reported PASS. An adapter's
trailing newline detached the shared wrapper's redirection, so the recorded
zero was the redirection's status rather than the suite's. Six minimal RED
assertions reproduce this in build, relink and test wrappers. Explicit enclosing
subshells now bind the complete command, output and exit status; configuration
failure handling also preserves `set -e`. That flight is **invalid acceptance
evidence**, not a claimed compiler failure or PASS; the subsequent cJSON replay
below uses the corrected path. Other current Subject adapters' build/relink/test
strings do not have this trailing-newline shape; this inspection is not a new
runtime result for them. Frozen receipts are retained, and B10 still requires
the final campaign.

**2026-09-23 follow-up:** `lu` and `ludcmp` now pass their unchanged LARGE
inputs and original output checks in **58.9642 / 58.7030 seconds**, under the
original 60-second whole-test limit. Block-local reuse removes duplicate pure
integer/address computations while retaining every load, effect and guard;
the small RED goes from four row/element address computations to two, with
both volatile reads and all six access/provenance checks intact. The changed
gate passes **1,084 tests, zero failures, three historical ignored**, Clippy
`-D warnings` and fmt. Both interpreters and sealed native GCC14/Clang19/EvoCC
controls pass. `floyd-warshall` remains a 60-second timeout. These are two
targeted closures, not a new full 1,735-program count; their timing margin is
small. `tests/fixtures/block-local-pure-reuse.out` binds the compiler and
assembly hashes and records the corrected upstream exit-status/hash contract.
Earlier manually wrapped hash comparisons that omitted that status line are
not acceptance evidence; earlier observed child timeouts remain failures.

The follow-up straight-line block merge retains explicit bindings and every
instruction/effect, while keeping all source/indirect label identities.
Its small loop RED, permutation/token/lifetime controls, both interpreters and
GCC14/Clang19/EvoCC native checks pass; the changed gate passes **1,089 tests**,
zero failures, three historical ignored, Clippy and fmt. Original serial
replay: `lu` **59.2533 s PASS**, `ludcmp` **48.8197 s PASS**; `floyd-warshall`
**TIMEOUT at 60 s**, despite its core function shrinking from 194 to 167
instructions. `tests/fixtures/linear-block-binding.out` records identities and
the unchanged output contract. The replayed scheduler object's text shrinks
to 625,158 bytes with no new symbol drift against the installed EvoCC object;
it is not installed and supplies no new kernel-runtime claim.

The next reduced address-preparation case removes repeated MIR index extensions
and scaling within a block, with typed/signed keys and no cross-block reuse.
Its C-HIR/EvoIR/MIR and sealed native controls pass, as does the changed gate
(956 PASS, zero failures, four historical ignored, Clippy and fmt). The original
LARGE `floyd-warshall` still reaches its unchanged 60-second limit; 167 → 166
core instructions is not enough. [Fixture and bounded result](../../tests/fixtures/repeated-pointer-index.out)
retain identities; TODO B02 stays OPEN and no full-run numerator changes.

The checked-add/sub result reduction then reuses the value already computed
by the overflow predicate, passing it explicitly across the existing guard.
Both interpreters, MIR/native controls and the affected gate (964 PASS, zero
failures, four historical ignored, Clippy/fmt) pass. The unchanged LARGE
floyd-warshall still times out at 60 seconds; its core is now 160 instructions.
[Targeted record](../../tests/fixtures/checked-arithmetic-result.out) separates
this verified code-generation improvement from the unresolved performance
acceptance. No kernel Image or full-campaign result is updated by this change.

The subsequent comparison-selection reduction (TODO B02b) fixes an independent
C-HIR integer-dereference arm-type refusal and avoids re-evaluating stable
integer operands already read by a comparison. It uses a pure bit-vector mux,
not speculative arm execution; all original checks and observable effects
remain. Six tailored tests and six fixed ARM64 native controls pass. The original
LARGE program still times out at 60 seconds (160 → 108 core instructions), so
B02 remains OPEN. [Test and acceptance record](../../tests/fixtures/compared-value-selection.out)
distinguishes the targeted compiler improvement from the unresolved workload.

The 2026-09-24 checked-flag selection (TODO B02d) removes only a single-use
adjacent CSET before its checked branch. Source-width predicates, traps,
volatile observations and continuation parameters remain. The affected gate
passes **989 tests, zero failures, four historical ignored**, Clippy and fmt;
six native positive controls and ten expected-trap negatives pass. Original
LARGE still times out at 60 seconds (102 → 98 core instructions). The
[source-bound record](../../tests/fixtures/checked-flag-branch.out) retains
that negative result; B02 and the full campaign stay OPEN. No kernel Image
inherits this compiler-only improvement.

**2026-09-24 B02 closure:** the [minimal loop transport case](../../tests/fixtures/loop-value-transport.c)
goes RED → GREEN by eliminating only trivial phi copies at SSA destruction.
Distinct incoming values, counters, swapping cycles and computed-dispatch
receiving slots retain their bindings; no loads, checks or effects are hoisted.
Both interpreters, six fixed-target native controls, five data register classes
with real spills and the independent allocation checker pass. The affected gate
is **995 PASS, zero failures, four historical ignored**, Clippy/fmt green.
The unchanged LARGE `floyd-warshall` now passes its original timeit, appended
exit-status hash and comparison in **52.0360 s**, within the original **60 s**
limit. Core size 98 → 57 is supporting evidence, not the PASS predicate.
The [record](../../tests/fixtures/loop-value-transport.out) retains the first
wrapper's incorrect expected-file path as exit 2 and the subsequent complete
PASS separately. B02 is closed; B10, K02/K03 and full-campaign numerators are
unchanged. No kernel Image or large runtime artifact was rebuilt.

**2026-09-23 case reconciliation (TODO B01):** every one of the frozen run's
44 compiler-failure identities is now joined to its later evidence: **7 targeted
original PASS, 30 named FP deferrals, 5 evidenced not-applicable, 2 OPEN**.
The deferrals are 11 binary128, 1 binary16, 9 floating-complex and 9 floating-vector;
they are remaining implementation work, not passes. The open non-FP cases are
the LARGE `floyd-warshall` timeout (B02) and signed integer-vector `pr23135` (L10).
All 35 reviewed rows validate through the existing adapter against the unchanged
source hash, original argv and evidence files. Eight obsolete scalar-FP reviews
are removed; a regression test reproduces all eight stale entries before GREEN.
The [44-case map](../../tests/fixtures/llvm-failure-reconciliation.out) preserves
every original identity and distinguishes later compiler versions. This is an
accounting closure, **not** a new 1,735-program campaign or increased PASS count.

The September 24 B02 replay updates that 44-case reconciliation to **8 targeted
original PASS, 30 named FP deferrals, 5 not-applicable, 1 OPEN (`pr23135`)**.
The original full receipt's counts still remain historical and unchanged.

The unchanged `compare-fp-3` now links and runs after bounded four-way
LT/EQ/GT/unordered reasoning; operands and their poison/FPCR obligations are
still evaluated. The unchanged `20020720-1` also links/runs after exact
standard `fabs` symbol/prototype recognition and post-evaluation sign facts.
Static/asm-renamed/function-pointer/incompatible calls and disabled standard
builtins remain outside that recognition; NaN does not imply `>= 0`.

Executable, unqualified, non-addressed f32/f64 locals now use the existing
SSA environment instead of the obsolete storage-only fallback. Real objects,
volatile, zero initialization, uninitialized reads and deferred-format storage
remain covered. The final two reductions pass both interpreters and native
execution, and GCC-14/Clang-19 O0/O2; the language suite is **63/63**, the
external-symbol suite **91/91**. Release SHA-256:
`b5428a5cb93999e324d3ea5956c8db643995f7ea88cce2f9b00fbd4959b1fc32`.
`tests/fixtures/benchmark-small-frontier.out` records source-bound results.
Targeted Clippy is green. The three unchanged LARGE_DATASET programs
`lu`, `ludcmp` and `floyd-warshall` all still time out at **60 seconds**
(child exit 137); their output comparisons fail. The wrapper's own zero exit
does not count as PASS. These are unresolved performance failures, distinct
from the passing MINI_DATASET corpus. A fresh full LLVM count remains pending;
no whole-campaign numerator has been predicted.

The subsequent allocator audit finds a separate implementation mismatch with
EvoMIR section 7: spill selection used the final interval endpoint instead
of the next read. The declared MIR RED selected victim 0 instead of 1, with
zero build errors. Next-read selection and future-fixed-register exclusion
now pass both MIR controls, the unsigned C reduction in both interpreters and
native ARM64, and the changed workspace gate: **2,590 PASS / 0 FAIL / 4
historical ignored**, Clippy `-D warnings` and fmt PASS. This does not change
the lowest-free/no-coalescing policy. The three original LARGE_DATASET
programs were recompiled and rerun with their original output checks and
60-second limit; **all three still time out**, so their failures stay open.
`tests/fixtures/register-next-use-pressure.out` records the small controls,
the corrected original replay, and the excluded initial harness mistake.
The new debug compiler is `8960370e…11c849`; the live kernel composition
continues using its unchanged frozen `07a8e66e…42c237` release.

The subsequent kernel-entry repair gate completes on 2026-09-20 with
**2,587 PASS / 0 FAIL / 4 historical ignored**, Clippy `-D warnings` and fmt
PASS. Python harness suites pass **186 ladder + 162 K1** tests; Ruff and
the textual grammar comparison (**53/53**, zero differences) also pass.
The independently receipted 6,765-record BPF/tracing kernel composition is
now running with release `77faa6b3…f27d8a`; its results will not overwrite
the accepted 2,799-record K6 receipt. These gates do not close the three
LLVM performance failures or the remaining K7 baseline/runtime gaps.

While that kernel compilation remains active, runtime repairs install the
real external-module header/sample tree, keyctl and fsverity. Reference
livepatch trampoline and both PKCS7 signature subtests pass; missing
dynamic-debug configuration, a stack-map discrepancy and the full sched_ext
timeout remain explicit failures/limits. Full-suite `/tmp` is now isolated
disk-backed ext4 with verity; the selected K7 tmpfs contract is unchanged.
The direct full-composition selftest reader has negative object/boot audit
tests. Current Python checks: **173 K1 + 32 benchmark-entry PASS**, Ruff and
diff checks PASS. Details and exact runtime identities are in
`kernel-selftest-gaps-20260914.md`; none is a fresh full K7 or LLVM campaign.
The original livepatch sample's three EvoCC C inputs also link with real BTF
finalization and pass **5/5** load/patch/restore/unload checks in the frozen
reference kernel, matching its GCC-module control. The 745-module and complete
EvoCC kernel claims still await the active full composition.

The old KUnit NEON gaps now have native data evidence: all twenty original C
inputs of the three XOR/RAID6 modules compile/link, and both preserved kernels
pass **156/156** explicit NEON/data-recovery checks and **8/8** load/unload
checks. This is not a fresh full module inventory. The active BPF survey has
also exposed a real inline-label collision in `trace_fprobe.o`; its minimal
early-return regression is repaired in both interpreters and native execution;
the unchanged original unit assembles and passes symbol/DWARF checks. The
changed gate now passes **2,588 tests / 0 failures / 4 historical ignored**
in 310 test groups across all 17 workspace crates, followed by Clippy
`-D warnings` and fmt. Capture:
`.build/floating-targeted/gate-inline-label-20260920.txt`. The survey retains
its frozen binary; this gate does not retroactively replace its failed row.
Fresh layout observations also match both frozen consensus byte-for-byte:
**128/128 base and 22/22 attributed**. The existing target builds
`day5_layout_probe` offline; no fresh target/clone is made. Both sealed oracle
corpora are re-executed with zero drift in
`.evidence/day5-layout/20260920T100134Z-80000` and
`20260920T100229Z-80295`. The obsolete Cellar `gtimeout` path is overridden
with the installed binary; the attributed corpus uses its existing
`C30_LAYOUT_CASE_SET=attributed` selector. Neither a missing host-tool attempt
nor a wrong-corpus-selector rejection is counted as a passing check.

The pending supplemental-kernel path now recompiles both export metadata and
module descriptors after actual modpost, then invokes the audited module/BTF
links before accepting a complete composition. The selftest reader checks the
linked module inventory independently. Targeted REDs are GREEN; **176 K1 harness
tests**, Ruff and diff checks pass. These changes await the next frozen run,
not a rewritten result for the active survey. The shared dead-cgroup failure
now has a stock-drain reproduction on both kernels and matching upstream
source diagnosis; its original FAIL remains recorded in the K7 notebook.

The frozen BPF/tracing survey `20260920T023330Z` has now **finished**:
**6,764 assembled / 1 refused / 6,765 total**, zero symbol differences,
including all 745 generated module descriptors. The sole refusal is the
already reduced/repaired `kernel/trace/trace_fprobe.o` inline-label collision.
The unchanged `77faa6b3…f27d8a` binary's record remains INCOMPLETE: **no object
installation, link or boot**. The next compiler build and complete-composition
run must obtain their own result; no arithmetic promotion to 6,765 PASS.
Capture: `.evidence/k1/20260920T023330Z/substitution.json`.

Historical full-composition launch, `20260920T103126Z`, compiler
`07a8e66e3da4f282a4e5108547940d020ae4409e91201012c48480fc6442c237`:
`SOURCE_DATE_EPOCH=1786299935`, four workers, existing low-disk 64-object
batches, unchanged timeouts. Parent/worker PIDs at launch: **87424/87427**;
capture `.build/floating-targeted/kernel-bpf-repaired-20260920.txt`.
It reuses the existing source/output trees; no new clone or Cargo target.
The new release's reduced native test exits zero and its original tracing
unit reproduces the verified assembly in 4.634 seconds. All remaining
full-composition, module-installation and K7 claims awaited actual results at
this checkpoint. This run subsequently ended INCOMPLETE at final link, as
recorded in the current summary above; these are former, not active, PIDs.

The subsequent runtime-installation gate passes **180 K1 + 186 ladder tests**,
Ruff and diff checks. The original modinst/sign/depmod recipes install all
**745 reference modules** without changing their linked ELF bodies or kernel
inputs; the runtime disk is bound to its actual Image and module inventory.
Fresh reference-only replays pass BPF livepatch/PKCS7 **4 subtests**, CPU-cgroup
**9/9**, and sched_ext **23/23**. The latter two have explicitly recorded
120-second whole-program budgets; sched_ext runs on the fixed TCG CPU because
its unchanged hotplug test hits a kernel BUG under HVF. Their prior failures
remain recorded. Capture:
`.evidence/kselftest-full/reference-modules-budget-20260920/summary.json`.
This is not subject execution or a new 970-program full-suite result. The
EvoCC module installation and paired replay still depend on the active full
composition. Details and source-based shared-failure diagnoses remain in the
[kernel runtime ledger](kernel-selftest-gaps-20260914.md).

Csmith's unavailable-reference diagnosis now samples all 68 unchanged
originals independently under GCC-14 and Clang-19: **136 diagnostic runs**,
each stopped after three symbolized PC samples. Source review identifies
fifteen further repeating paths, bringing the reviewed split to **19 identified
/ 49 undetermined**. Finite reduced transition checks pass both EvoCC
interpreters, native execution and both oracles at O0/O2. The campaign remains
**932 agreements / 68 oracle-unusable / 0 mismatches**; diagnostic exit 77 is
not a program PASS and no seed or timeout changes. The compact capture and
individual source arguments are in the
[applicability notebook](benchmark-applicability.md#csmith-unavailable-reference-accounting).

The runtime binding also now covers the **17 external BPF/livepatch test-support
modules**, not just the 745 installed kernel modules. Their original GCC/Kbuild
rebuild against the exact reference BTF/version base passes, and the dedicated
reference disk runs **9 BPF groups / 33 subtests, all PASS**. These helpers are
test infrastructure, not EvoCC-compiled kernel-object credit. The stale-helper
regressions and **183 K1 harness tests** pass; details, runtime hashes and the
original native capture are in the kernel runtime ledger. Subject loading and
the full paired suite still await the active complete composition.

KVM's no-`/dev/kvm` gap is now reproduced as a machine prerequisite rather than
declared unapproachable. A separate fixed Cortex-A72 EL2/GICv3 profile passes
four original reference programs, without changing the locked K6 machine or
the test budgets. The remaining 33 KVM programs and subject execution are not
claimed. The lower-level all-SKIP display and hidden inner-skip accounting have
tailored regressions; **188 K1 harness tests** pass. The source-bound controls
and remaining register skips are recorded in the kernel runtime ledger.

FUSE also now has a real reference-side execution: loading its already
installed module before mounting fusectl changes the original program's
inner result from **0 PASS / 1 SKIP** to **1 PASS / 0 SKIP**. The kernel and
test are unchanged. Only one generated prerequisite file was added to the
stopped disk; the previous disk is retained through copy-on-write. Subject
execution and whole-suite counts remain pending.

## Latest completed campaign and targeted closure — 2026-09-19

The previously running application queue has finished: unified run
`20260915T210233.318258Z`, capture
`.evidence/bench/20260915T210233.318258Z/summary.json`, immutable compiler
`bbeac066b71954fb239d5876f7ee0fda5bf2218fff9f4ebba76a6a40a33a99eb`,
HEAD `9f571ca` plus the recorded working-tree changes. All fifteen lanes ran
serially, with two emission workers, unchanged program limits and Csmith seeds.
Neither that queue nor the supplemental kernel survey is still running.

| Lane | Measured subject result | Remaining boundary in that run |
|---|---:|---|
| c-testsuite | 213/220 PASS | 7 evidenced FP deferrals; 0 compiler failures |
| torture | 1,407/1,692 PASS | 2 compiler failures, 135 FP, 145 not applicable, 3 oracle-unusable |
| Csmith | 932/1,000 agreements | All 1,000 emitted; 0 mismatches; 68 oracle-unusable, not PASS |
| zlib | 19/19 objects | Original upstream tests PASS |
| zstd | 6/44 objects | 37 target-header failures, 1 floating-value refusal |
| cJSON | 0/2 objects | 2 floating call interfaces |
| TinyCC | 8/11 objects | 3 floating call interfaces |
| toybox | 195/203 objects | 8 FP stops; unchanged reference test failure |
| musl | 1,026/1,339 objects | 313 refusals; 17 shared reference failures, no added test failures |
| Mbed TLS | 91/382 objects | 291 target-header failures; selftest/CTest pass only in the mixed build |
| PolyBench | 2/30 programs | 28 FP interface/value refusals |
| LLVM test-suite | 887/1,735 PASS | 9 compiler failures, 186 FP, 652 not applicable, 1 oracle-unusable |
| jq | 58/69 objects | 11 refused; mixed upstream tests pass |
| Lua | 11/34 objects | 23 refused; mixed upstream tests pass |
| SQLite | 67/86 objects | 19 refused; mixed upstream tests pass |

Object substitution fractions do not assert that the remaining reference
objects were compiled by EvoCC. Child run IDs, original commands, refusals
and upstream test outcomes are in the unified capture above.

The supplemental kernel survey also finished, independently on compiler
`41b2c0b61eef56397da03ffcad2696d1cd6d18b6faee639da7e8015929de7668`:
`survey-20260915T161109.904239Z`,
`.evidence/k1/survey-20260915T161109.904239Z/`.
It accounted for **6,916** target-C records (3,137 built-in, 3,779 module):
**6,907 assembled, 9 refused, 0 panic/timeout**. Six assembled objects additionally
miss reference symbols; assembly success is not symbol parity. Nothing from
this run was installed or booted. The prior 2,799-object K6 boot remains a
different configuration; EvoCC execution of the expanded KUnit image is NOT RUN.

Subsequent targeted work is deliberately separate from those numerators:

- The keyword-macro/symbol-path repairs restore all previously missing names
  in `vgic-its`, three SCSI objects, `decompress_unlzo` and `page_alloc`; netkit
  also assembles. Original source and Kbuild argv are retained.
- The two remaining Landlock failures reduce to cv conversion through arrays.
  The shared conversion now preserves bounds, atomic and nested-pointer
  distinctions. Both original objects assemble with 2/2 matching symbols each;
  six accepted small cases pass both interpreters and **24/24** native runs.
  Subject `d94f937c5c9ae232e414dcc6405ee7a65204a22e432037f1161bf77d4a161004`;
  details: `tests/fixtures/array-pointer-qualification.out`.
- All **45** musl floating-complex E3101 rows were replayed on that same
  subject and now explicitly refuse with E3049. This fixes diagnosis, not
  floating execution or the object substitution count.
- Source/argv/evidence bindings now cover **844 LLVM programs**. Six remaining
  runtime failures have positive UB evidence from independently GCC- and
  Clang-instrumented original programs; those new ledger dispositions are
  not retroactive PASSes. Torture `20051012-1` separately retains reviewed F01
  call-arity refusal with its own current source digest and oracle record.

The real target-header/intrinsic path now has targeted results (not a stub
header or an FP waiver): **five of the six** kernel NEON objects assemble with
exact reference symbol names/kinds, and the unchanged integer-only LLVM
`Vector/NEON/simple.c` executes with output identical to both fixed oracles.
The sixth kernel object, `xor-neon`, exposes **E3021 `=w`**, outside the current
Section 3.11 constraint table. Its SIMD binding/modifier/backend contract is
still pending; no validator relaxation is substituted for that contract. LLVM
`20090113-1` and `pr23135` retain their separately documented evidence gaps;
torture `930608-1` compares an uncalled FP function's address and is not waived
merely for its signature. Full kernel installation/KUnit and shared baseline
selftest failures remain in the kernel ledger, not closed by application tests.

The independent `page_alloc` placement gap is now repaired and targeted:
global `readonly=true` survives both IR texts and proof reconstruction; both
interpreters freeze initialized objects, including TLS without losing thread
identity; native PIC relocations use relocation-safe storage. The original
`page_alloc.o` now matches **81/81 symbol names and kinds**, including both
tables as `R`; eight default/optimized static/PIC native runs and four PIC
DSO load/run/close checks pass, as do all 40 language regressions. The DSOs
have relocation-safe data covered by GNU_RELRO and no TEXTREL. Subject
`2214908df66602b0fcbf72291e98f6909abcd2e391211f7cb279da27b8da860c`, details
in `tests/fixtures/static-readonly-storage.out`. Explicit sections retain
their previous placement rule despite the recorded GCC/Clang disagreement.
This is not an expanded-configuration installation or boot claim.

The array-conversion gate attempt is **not PASS**: it exposed an outdated
E3101 expectation in the complex-varargs test; continuing shared-type edits
during the gate also invalidated its nested producer build and doctest.
The expectation is updated to the named ABI diagnostic. The subsequent
serial gate held code and build artifacts fixed and completed with **2,487
PASS / 0 FAIL / 4 historical ignored**, Clippy `-D warnings` and fmt clean
(`.build/vector-targeted/readonly-gate-20260919.txt`). This is the changed-crate
gate, not a new full application or kernel campaign.
The latest completed ladder tooling test run is **169 PASS**. No additional
full campaign is started before the tailored repairs are verified.

### Real AArch64 target-header follow-up

The unchanged GCC14 `arm_neon.h` is now parsed with nominal half/bfloat/
polynomial types, real vector tuples, and exact builtin prototypes. The
2,980-row interface table reproduces byte-for-byte from the fixed compiler's
type queries; it is interface data, not a replacement header or return types
guessed from wrapper bodies. Registered but unimplemented operations remain
named E3052 refusals and do not escape as unresolved libc calls.

Integer loads/stores, immediate shifts, table lookup, reinterpretation and
byte carryless multiplication use the existing memory/vector/SSA paths. RED
also caught a parameter-conversion edge: `0x100000001ULL` becomes the declared
`int` value 1 before checking the shift range. Both consumers now perform that
conversion while retaining the source-ICE rule and argument side effects.

Fresh targeted results, recorded in `tests/fixtures/arm-neon-target.out`:

- **89/89** execution/semantic regressions and **15/15** parser regressions;
  **169/169** benchmark-tool unit tests. The first broad changed gate completed
  with **2,498 PASS / 2 FAIL / 4 historical ignored**. The two obsolete
  opaque-vector/DWARF and E3101 expectations are repaired, with **59/59**
  affected checks green and independent ARM64 readelf confirmation of the
  four-lane vector DIE. The retry was deliberately interrupted after compilation
  and its first test groups when the application replay exposed another
  intrinsic family; it is not a complete gate PASS, and the older 2,487-test
  result does not cover this implementation.
- Three public-header sources pass both fixed compilers at O0/O2 and the
  subject under each native assembler; the polynomial probe checks all
  **65,536** input-byte pairs. These are not floating execution tests.
- Original LLVM `NEON/simple.c`: both subject-native runs and four oracle
  controls exit zero, with identical 876-byte output.
- Original kernel `neon1`, `neon2`, `neon4`, `neon8`, `recov_neon_inner`:
  **5/5** assembled, **2/2** matching symbol names/kinds each. Serial release
  emission takes 0.580–13.060 seconds under the unchanged 60-second cap;
  another small targeted process overlapped, so this is not a timing benchmark.
- Three original zstd and three Mbed TLS objects now emit successfully on
  their original projected commands. Full object substitution and original
  application suites have not yet been rerun for this fix.

The six-row kernel replay used release digest
`1d5dbe34fbb8d5d73a33708528f4be0615a64abd604ac246235f73f1d703dff6`,
before the subsequent immediate-conversion edge repair. No expanded kernel
objects were installed and no old campaign numerator changed. The small
`arm-neon-assembly.c` reproduces `w/=w` and passes both fixed compilers at
O0/O2; current refusal is a documented contract gap, not FP or PASS.

Release `1b6e1fa3a18f6d25a5e85f5f02e76507a09f86d3532c690f736ecae042deb2c0`
then reconfirmed those five assemblies byte-for-byte, with fresh assembly and
symbol checks after the immediate-conversion repair. Fresh layout observations
match both frozen consensus digests (**128/128**, **22/22**).
The preceding application replay subject was
`4bc75461c69a599043363d462f2bd415be10a0a6033f86791689c9026072dc94`,
which additionally uses the authorized-range diagnostic E3052. Invocation
`20260919T125039.295434Z` stopped at the case-sensitive-sysroot guard before
measurement; the corrected run `20260919T125112.793889Z` explicitly sets
`EVOCC_BENCH_SYSROOT` to the existing `.build/k7-coverage/case/ladder-sysroot`.
It remeasures only zstd and Mbed TLS through `tools/bench.py`; no new workspace
or kernel copy is made. The zstd child `20260919T125112Z` completed: **30/44**
EvoCC objects in the unchanged passing upstream suite, with 9 E3049, 4 E3050
and one E3052 refusal (`zstd_lazy.c`). The last refusal has since motivated
the narrowing/insertion/interleaving fixture; it is not added to that numerator.

The Mbed TLS child `20260919T125412Z` was stopped, not passed. Its first mixed
selftest/CTest run passed all 148 registered tests, but the provenance guard
found **148 generated test objects rebuilt by GCC**: relinking `libtfpsacrypto.a`
regenerated test C files and made their installed objects stale. The old
adapter incorrectly delta-debugged this build-graph event as a compiler fault.
The parent, adapter and remaining owned container have all exited; the capture
retains the interrupted attempt. New harness regressions require recompiling
the regenerated sources with their identical original commands and comparing
actual assembly before reinstalling only those objects, a second relink with
no replacements, and object checks before the test; changed assembly, external
assembler inputs or continued replacement stays a failure, not a compiler
bisection. This does not rely on the fidelity of a preprocessor text renderer.
The updated ladder tooling suite has **173 PASS** with Ruff clean. The corrected
real Mbed TLS run and changed gate completed as recorded below.

The subsequent immutable subject is
`bb298881b208e0059e6feb3518d6d6b17ace7f58da85ed393520ed84c8f2a310`,
unified run `20260919T132907.970164Z`. Its completed zstd child
`20260919T132908Z` installs **31/44** subject objects, including the original
`zstd_lazy.o`, with no object replacements or exclusions; both the baseline
and unchanged mixed upstream suite pass. The remaining refusals are **9 E3049
and 4 E3050**. This closes the measured non-FP first-error family in that lane,
not full EvoCC compilation of zstd. Mbed TLS child `20260919T133245Z` installs
**379/382** subject objects: the unchanged selftest and **148/148 CTest tests
pass**, with zero exclusions and zero installed objects replaced at final
verification. Each relink's 148 regenerated test inputs were recompiled with
their original commands to identical assembly, reinstalled and verified
through a second relink before testing. The three remaining original objects
are `aesce.c` (E3052 crypto AES intrinsic), `chacha20_neon.c` (E3052 Combine)
and `platform_util.c` (E1005 pointer-to-VLA cast in a memory asm operand).
These are not floating-point refusals. The unified run is **INCOMPLETE**,
not a fully compiled Mbed TLS result.

The frozen changed-crate gate completed **2,503 PASS / 0 FAIL / 4 historical
ignored**, across 305 test groups; Clippy `-D warnings` and fmt are clean
(`.build/vector-targeted/neon-closure-gate-20260919.txt`). This gate predates
the following targeted fixes and is not claimed as their regression gate.

The generated-input audit independently exposed a driver defect: `-E` drops
`#pragma pack`, so `preprocessed-packing.c` accepted directly but its emitted
text failed E2911 on reparsing; the raw target-header marker also failed
E0112. Declared RED was two failures and one passing visibility control,
without build errors. Rendering the existing semantic markers as directives
now passes all three driver regressions, including both interpreters, plus
the actual GCC-14 `arm_neon.h` preprocessed-text reparse. Both fixed oracles
compile/run the packing source, the subject's emitted text and its assembly
with exit zero. The assembly-reuse guard remains independent of this renderer.

The returned-record array reduction additionally covers `make().items->v`,
which failed E2102 while indexed access passed. The existing pointer-root
fallback now handles both: `returned-array-projections.c` passes C-HIR,
EvoIR and assembly generation, four fixed-oracle controls and two native
subject executions. Combine has its own small RED and now passes the three
subject paths and both native executions. Release
`cb2a38518e3e3acc5a53a86fda03b0daf01d1d2fcb9a84222879203252062a8a`
replays the exact `platform_util.c` and `chacha20_neon.c` commands in 0.52 s
and 25.84 s: both assemble and match the reference's exported symbol names
and kinds. The earlier debug-only Chacha attempt reached the unchanged
300-second cap and is not PASS. Neither object has yet been installed in a
new passing Mbed TLS run at this checkpoint.

The remaining `aesce.c` reduction comprises four AES round/matrix operations,
full-width polynomial products, byte-bit reversal and low/high-half selection,
not floating arithmetic.
Small RED tests precede the implementations. Fixed ARM64 GCC/Clang O0/O2
controls and native subject tests pass for all four families; C-HIR/EvoIR,
AES MIR execution, text round trips, incorrect-type/arity rejection,
initializedness, `general-regs-only` refusal and independent proof tampering
are checked. The polynomial path also exposed and repaired the interpreter's
missing nominal-scalar object classification. The tailored vector/language/
driver group now has **93 PASS**, with no added ignores. The half-selection
checks also caught Combine's eager reads of indeterminate lanes in C-HIR;
representation copying now agrees with EvoIR and does not zero them. VLA
cast captures additionally pass direct-label, goto-loop and switch-entry
controls without another production change. This is targeted
evidence, not a replacement for original-object installation and the full
unchanged upstream test suite; the next complete run is still required.

The post-intrinsic changed-crate gate is now complete: **2,519 PASS / 0 FAIL /
4 historical ignored**, across 305 groups, with Clippy `-D warnings` and fmt
clean (`.build/vector-targeted/neon-execution-gate-20260919-retry.txt`).
The preceding attempt stopped at a missing `MInst::Aes` arm in a test's
instruction-name observer, not a declared RED; that mechanical match is
repaired, and all-targets compilation and the complete gate were rerun.
The frozen production tree also passes 173 ladder-tool tests, Ruff, 51/51
textual instruction shapes and fresh **128/128 + 22/22** layout observations
against the frozen consensus. On the debug subject named below, all six AES,
polynomial-long, byte-reversal, half-selection, combination and VLA-cast-entry
fixtures were freshly emitted and independently linked/run by the two fixed
ARM64 compilers: **12/12 PASS**. No isolated clone replay, new application
numerator or kernel boot is claimed by this in-place verification.

The latest unchanged `aesce.c` replay now reaches line 579's
`asm volatile ("" : "+w" (r))`, with **E3021**, on debug subject
`09346f2a6072f8b60f03e64b9b4b4a3307b44c4bfe9195ea948653031b13499c`.
It no longer stops on the reduced target intrinsics, but it is still refused,
not assembled or installed. This is the same pending SIMD-constraint contract
as kernel `xor-neon`; Section 3.11 still explicitly excludes `w`.
The empty template is an original optimization barrier and is not deleted.
Accordingly, a new complete Mbed TLS campaign is not started while this known
blocker remains. Full `w` operand/register/modifier/ABI admission has been
requested; targeted successes do not silently amend that explicit boundary.
The separate `arm-neon-assembly-update.c` reduction retains the original
empty `+w` barrier: four fixed ARM64 GCC/Clang O0/O2 controls pass, while
the current subject still reports E3021 and emits no assembly. Its small
record is `tests/fixtures/arm-neon-assembly-update.out`; it is not counted
among passing subject tests.

### Floating report intake and finite-constant repair — 2026-09-20

The report is useful for scalar binary32/binary64 implementation; it is not the
pending SIMD-asm adjudication above. Its GNU-inline/raw-byte implementation
status is older than this tree and has been corrected in the existing FP brief.
That brief now gives the implementation order and distinguishes completed
constant work, numerical qualification, native environment, IR/ABI and the
eventual unchanged application campaign.

Two declared REDs, with zero compilation errors, reproduced a silent decimal
binary32 double-rounding error and rejection of hexadecimal literals. The
source-format representation now uses the pinned software arithmetic engine;
the constant folder, integer conversion and parser's constant path selection
share it instead of performing independent host `f64` arithmetic.
Three small programs pass both interpreters, all **6/6** subject native
assembly/link/run combinations and **12/12** fixed GCC-14/Clang-19 O0/O2 source
controls; **47/47** language regressions and four engine groups pass.
Compiler SHA-256:
`152b16bb5b205dc55ddaf748ad472b1797eea6238c27ec7aab74aa0672283193`.
The unchanged original GCC `20020225-2`, `20031214-1` and LLVM
`ieee/builtin-nan-1` also retain zero exits in **6/6** subject native runs
and **12/12** fixed-source controls; these are regressions, not new suite gains.

`floating-target-environment.c` additionally measures the report's open native
boundary: FPCR=0 gives `1 + 2^-53` bits `3ff0000000000000`; upward mode gives
`3ff0000000000001`. FZ changes subnormal results and DN changes NaN payloads.
All four fixed compiler/optimization controls agree; these are hardware
observations, not subject runtime FP acceptance. The proposed runtime checks
for a fixed-mode first release, and independent SIMD `w` admission, await the
two requested decisions. No runtime FP or strict-mode support is enabled.
Direct qualification also found the pinned engine's default qNaN/sNaN
operand selection differs from those ARM64 instructions; it needs a target
adapter before runtime admission. The new finite-only API keeps this mismatch
outside the accepted path instead of claiming the dependency already models it.

The fresh changed gate exited **0**: **2,544 PASS / 0 FAIL / 4 historical
ignored**, 308 result groups, all 17 crates; Clippy `-D warnings` and fmt
also pass. Its log is `.build/vector-targeted/floating-constants/gate-20260920.txt`;
the earlier 2,519-pass gate describes the previous binary. Fresh layout
comparisons pass **128/128** and **22/22**; textual shapes remain 51/51,
`render-status --check` and `git diff --check` pass. No test from this batch
is still running. Broad software-engine differential tests,
runtime FP/HFA/varargs, GNU folding of the newly observed non-ICE static array
bound, and original application/full-kernel campaigns remain open.
The compact source/native records are `tests/fixtures/floating-constants.out`;
no full benchmark numerator or historical failure has been relabeled.

### Approved SIMD assembly and scalar FP execution — 2026-09-20

The two decisions requested above are now approved: integer/polynomial SIMD
assembly `w/=w/+w` and early/tied forms, plus fixed-RNE scalar FP with explicit
refusal of an incompatible native FPCR (strict fenv remains deferred).
The SIMD implementation now passes 48 CLI, 10 IR and 13 MIR assembly tests,
independent allocation/proof mutation checks, and all nine separately compiled
GCC/Clang/EvoCC caller-callee combinations. Clippy is clean. The compact record
is `tests/fixtures/simd-assembly-bindings.out`, including the unpassed GCC O2
fixed-register exploration; that case is not silently counted as a pass.
Original `xor-neon.o` now assembles with 3/3 symbol-kind parity, and original
Mbed TLS `aesce.c` emits assembly accepted by both fixed assemblers. Neither
targeted result changes the historical kernel/suite denominator. Mbed TLS
run `20260919T175507Z`, frozen binary `4f6f2291…d4f515b`, completed with
381/382 EvoCC objects, unchanged selftest and 148/148 CTest PASS and no objects
rebuilt by make. `chacha20_neon.c` exceeded the unchanged 300-second compile
limit: that one reference object makes the full-object milestone INCOMPLETE.
The bounded replay localized the timeout to spill-slot conflict construction;
the replacement packed graph matches the set-based reference on 128 cases,
including backedges, dead stores, sparse IDs and mixed-width slots.
The strengthened sparse implementation also covers dense and 16,384 disjoint
spill identities (5/5 tests). Original ChaCha20 emits in 166.932 seconds with
the new **debug** compiler, below the unchanged 300-second limit; release
emits in 11.437 seconds. Their assembly hashes are identical, and the object
passes original ChaCha20 selftests 0 and 1. This targeted replay does not
replace the historical 381/382 whole-build reading.

The scalar C test first failed at C-HIR execution with zero build errors.
Runtime f32/f64 now connects the shared software target adapter, both
interpreters, exact-bit IR, one SIMD register bank and AAPCS64 HFA/varargs.
All 3,072 arithmetic vectors agree bit-for-bit across GCC/Clang O0/O2,
the engine and EvoCC native execution. Seven separate/mixed ABI combinations
pass; two native drivers pass 24-double/8-float/4-vector call-clobber pressure.
Classification builtins preserve single evaluation and NaN payloads. The
integer-conversion corpus checks 8–128-bit boundaries, E3063 negatives and
uninitialized E3001; all 51 language regressions pass. Four incompatible FPCR
settings trap under each of two target drivers, without resetting FPCR.
These are targeted results, not an application or full-kernel claim. Original
cJSON replay `20260919T192032Z` first exposed the target header's finite
binary128-literal-to-double cast. The shared constant conversion repairs it
without admitting binary128 arithmetic; the reduced source passes both
interpreters and four fixed-oracle/native controls. HFA classification now
checks completed layout, with GCC/Clang mixed-call agreement for the padded
record. Fresh cJSON run `20260919T193013Z` on compiler
`0b9219f8ca579af4c4e8af6ebfd20d487cf267f6aa3d0b58def5dee76ed2c125` is
**2/2 objects, public test plus 54 Unity assertions PASS**, no make rebuilds.
This is the existing public-API lane, not the full private-helper suite.
jq run `20260919T193126Z` installs 68/69 units and passes original `make check`;
the remaining `src/builtin.c` calls `nexttoward(double, long double)`, an actual
deferred binary128 ABI dependency, not an unused header declaration. Lua run
`20260919T193448Z` installs **34/34**, with matching original `all.lua` reaching
`final OK` without soft/portable skips or make rebuilds.
SQLite run `20260919T193619Z` compiles/installs **86/86** but its original
veryquick execution reports **9 failures / 328,897 assertions** (GCC baseline:
0 / 329,803). The upstream runner exits zero even with failed jobs; the old
adapter's initial PASS was incorrect and the current receipts retract it.
The immutable original logs remain intact. The adapter now checks every job
state and error count in upstream `testrunner.db`, including zero-exit failed
and incomplete jobs, before it can report PASS (175 Python adapter tests pass).
All nine crashes reduce to a switch-default environment error: a default arm
written before later cases lost its live pointer on the failed-case dispatch
chain. The shared lowering now retains the default/fallthrough environment
through every comparison; no SSA or validator rule was relaxed. The reduced
case passes both interpreters, four oracle controls and two subject native
runs. Recompiling original `sqlite3.c` and relinking the existing fixture gives
**16 jobs / 1,207 assertions / zero errors** on the nine formerly failing files.
The original full-suite failure is retained until a fresh uniform-subject
whole-suite replay passes. See `switch-default-pointer-environment.out`.
The compact FP record is `tests/fixtures/floating-runtime.out`. Shared
test/example matches were repaired without weakening canonical comparisons;
old blanket-FP-refusal expectations now test the supported/deferred boundary.
The completed post-repair gate and fresh application results are recorded below.

Independent numerical qualification now covers **661,848 cases across 32
operations**, with zero bit differences and **1,898 invalid integer conversions
correctly refused**. These are complete level-1, seed-1 TestFloat streams, not a
sample; the SoftFloat reference also passes its independent slowfloat checks.
Exact versions, archive hashes, per-operation counts and scope limits are in
`tests/fixtures/floating-testfloat.out`. The three locked Rust dependencies
are vendored (1.1 MiB), so offline execution no longer requires a populated
registry cache. An empty-Cargo-home gate exposed four nested-client targets
that failed to inherit the repository's dependency configuration; they now
pass the same configuration explicitly, without changing their assertions.
The isolated client replay and the subsequent complete gate remain distinct
from the earlier failed attempt.

The benchmark classifier also has a declared RED and repair: an old reviewed
FP deferral must not conceal a newly emitted program's link/runtime failure,
or prevent a fresh native pass from becoming PASS. Source/command/evidence
checks and the separate not-applicable/oracle-unusable categories are retained.
Surviving refusals additionally require a source-bound review explicitly naming
a still-deferred feature; an old "uses floating point" review is insufficient
after scalar f32/f64 admission. All **178** ladder-tool tests pass. Old results
are not reclassified in place; the next measured run applies the corrected rule
to its actual outcomes.

The complete post-repair offline gate now exits **0**: **2,571 PASS / 0 FAIL /
4 historical ignored**, 310 result groups across all 17 crates, followed by
Clippy `--all-targets -D warnings` and fmt PASS. The four nested-client targets
also passed their separate 13-test replay using the empty Cargo home.
Log: `.build/floating-targeted/gate-20260920-4.txt`. This is in-place isolated
offline verification, not a claim that a new clean clone was built. Fresh
layout observations match the frozen **128/128 + 22/22** consensus; grammar is
**53/53** with no differences, Ruff passes, and **159** K1 tool tests pass.
The forthcoming application replay uses a separately frozen release binary;
the historical application/kernel numerators above are unchanged by this gate.

Fresh application queue `20260919T204938.935065Z` uses immutable release
`86633b8b6c552ac638f0da70a2e73929ccc95e8c8570eb38a2699417eab0e6c4`.
Its SQLite child `20260919T204939Z` is now **PASS: 86/86 objects installed,
1,238 files / 329,803 assertions / zero errors**, matching the unchanged GCC
baseline. Both the initial complete trial and final relink/retest pass, with
zero exclusions and zero objects replaced by make. The prior nine crashes
are closed for this original veryquick configuration; this is not SQLite TH3
or its multi-configuration release suite. Mbed TLS child `20260919T205613Z`
also closes **382/382 installed objects**, zero refusals, exclusions or replaced
objects, and original selftest plus **148/148 CTest**. Both the complete trial
(442.63 seconds of CTest) and final relink/retest (439.45 seconds) pass.
Regenerated test sources are checked by re-emission against the recorded
assembly before reinstallation; this is not timestamp-only acceptance. The
former ChaCha20 timeout no longer leaves a reference object in this build.
This closes the pinned default release test configuration, not every Mbed TLS
configuration. Zstd child `20260919T212644Z` also passes: **44/44 objects**,
zero exclusions/rebuilds, and unchanged `make check` in the existing lane.
The six-lane parent has completed, not paused.

The other three results are deliberately retained as failures/incomplete:
ctest `20260919T212256Z`: **218/220 PASS**, one missing-libm link and one
binary128 deferral. Torture `20260919T212324Z`: **1,499/1,692 PASS** under the
five-way ledger, **44 compiler failures**, 146 not applicable and 3 oracle
unusable. The 44 comprise 39 stale floating-scope reviews, four missing-libm
links and one genuine HFA ABI wrong value (`pr92904`). PolyBench
`20260919T213113Z`: **22/30 PASS**, seven output differences and one oracle
disagreement; all 30 emitted and assembled.

Post-campaign targeted repairs, not retroactive changes to those counts:

- HFA now preserves natural transport alignment separately from whole-object
  alignment. **26/26 native/mixed-call controls** pass for member alignments
  8/16; unchanged `pr92904` exits zero. Text/validation and independent proof
  mutation checks include the new field; no validator/SSA rule was relaxed.
- The existing static E0 link now places `-lm` after the subject object.
  Unchanged ctest `00174` and all four fixed-oracle controls pass. The linker
  failure was not an authorization to synthesize a compiler `sin` intrinsic.
- With the approved `-ffp-contract=off` passed to every compiler, all eight
  differing PolyBench sources match the recorded subject array dumps exactly
  under both fixed oracles (**16/16**). Exact stdout/stderr comparison remains;
  no numerical tolerance was introduced.
- All 39 remaining refused torture sources have rechecked original hashes:
  **18 binary128, 15 floating-complex, 6 floating-vector**. These are reviewed
  deferrals, never PASS. The complex-zero comparison formerly labeled E3101
  now has an E3050 RED/GREEN regression; integer complex remains executable.
- Receipt `frontEndGaps` is derived from current measured rows, removing a
  hard-coded obsolete target-header failure from future passing receipts.
  All **183 Python adapter tests** and Ruff pass. A just-completed passing
  trial is no longer reinstalled/relinked/retested a second time when the
  object set is unchanged; failed or bisected builds still require a final
  trial. This is same-run reuse, not cached acceptance across compiler builds,
  and future receipts name the actual completed trial.

Compact records: `floating-hfa-varargs-alignment.out`, `benchmark-static-libm.out`,
`polybench-contraction-profile.out`, and the dated source review appended to
`torture-floating-frontier.out`. The post-HFA complete offline gate now exits
**0: 2,575 PASS / 0 FAIL / 4 historical ignored**, followed by Clippy all-targets
`-D warnings` and fmt PASS (`gate-20260920-6.txt`). The earlier attempt's
unsynchronized Section 9 field manifest failure is retained; the repaired
ABI alignment has a nondefault sentinel rather than relying on round-trip alone.
The subsequent layout-identity regression was RED with zero compilation errors,
then **7/7 library tests**, the standalone persistent-state replay and all
**12/12 independent field-preservation proof tests** passed after advancing the ABI identity to
`evoir-layout-v4-hfa-natural-alignment`; original source-object layout is unchanged.
That post-gate identity adjustment is checked separately, not folded into the
2,575-test count. Python adapters remain **183/183** and grammar **53/53**.

One option-accounting refinement is source-bound rather than a new compiler
waiver: torture and LLVM `pr39501` explicitly require `-ffast-math`, outside
the approved first-release FP profile. All **eight** unchanged fixed ARM64
oracle controls pass; EvoCC explicitly refuses the flag. The entries now
say profile-not-applicable, not blanket scalar-FP-deferred or PASS; source
hashes and commands are in `floating-profile-options.out`.

## Initial campaign (historical checkpoint)

The initial extension below records measurements and subsequent targeted
repairs. Floating point is not the only remaining obstacle. Historical kernel
boot and selftest results are not promoted by this application benchmark batch.

## Identity and command

- Checkout: `428980207a421cc3b8273a6a952600a20a82a535`.
- Driver: `target/frontend/release/evocc`, SHA-256
  `160568e35535306cb0a673baf922945c998959365707becbcb4ae316dbaf2fe6`.
- Main run: `20260914T154622.690858Z`,
  `.evidence/bench/20260914T154622.690858Z/summary.json`.
- Corrected Mbed TLS run: `20260914T165259.428885Z`, child
  `20260914T165302Z`; same compiler, with the new adapter and the two build-path
  repairs recorded as dirty harness changes.
- Lanes run sequentially. Csmith and mixed-lane emission workers: 2; the shared
  whole-program runner emits serially. Reference-build settings and original
  per-program watchdogs, source pins, applicability evidence and seeds are
  unchanged.
- No compiler rebuild or semantic edit during the batch. The new Mbed TLS
  adapter is measured separately; the other nine lanes are not repeated for
  its build-path repairs.

```sh
python3 -u tools/bench.py \
  mbedtls polybench llvm-test-suite jq lua sqlite \
  ctest torture csmith zlib zstd cjson tinycc toybox musl \
  --evocc target/frontend/release/evocc \
  --jobs 2 --programs 500 --campaign all --budget 45

python3 -u tools/bench.py mbedtls \
  --evocc target/frontend/release/evocc --jobs 2
```

The [unified receipt](../../tests/contracts/bench-receipt.json) and each run's
capture are the measurements. This document interprets them; it does not
replace their denominators or count oracle failures as passes.

## Results

| Lane | Previous recorded reading | Fresh reading | Run / limit |
|---|---:|---:|---|
| c-testsuite | 209/220 PASS | **210/220 PASS** | `20260914T154622Z`; 3 compiler failures, 7 reviewed FP deferrals |
| GCC torture | 1,268/1,692 PASS | **1,330/1,692 PASS** | `20260914T154648Z`; five-way counts below |
| Csmith, no float | 932/1,000 agreements | **932/1,000 agreements** | `20260914T154951Z`; all 1,000 emitted; 0 mismatches, 68 oracle-unusable |
| zlib | 19/19 units | **19/19 units, PASS** | `20260914T162614Z`; original upstream test suite |
| zstd | 6/44 units | **6/44 units** | `20260914T162626Z`; mixed tests pass, 38 units still reference-built |
| cJSON | 0/2 units | **0/2 units** | `20260914T162800Z`; reference API tests pass, no EvoCC object substituted |
| TinyCC | 8/11 units | **8/11 units** | `20260914T162811Z`; upstream tests pass, 3 refusals, no symbol exclusions |
| toybox | 195/203 units | **195/203 units** | `20260914T162859Z`; zero added test failures, one reference failure |
| musl + libc-test | 1,010/1,339 units | **925/1,339 units** | `20260914T163005Z`; 93 old assembled units lost, 8 gained; no added test failures, 17 reference failures |
| Mbed TLS | no adapter | **91/382 units** | `20260914T165302Z`; selftest and 148/148 CTest PASS in a mixed build, 291 refusals; not ssl-opt/compat/all.sh |
| jq | no adapter | **58/69 units** | initial `20260914T181023Z`: 57/69; repair retest `20260914T185027Z`: original `make check` passes, 11 refused, no assembler failure |
| Lua | no adapter | **1/34 units** | `20260914T182735Z`; matching official `all.lua` reaches `final OK`; 33 refused |
| SQLite | no adapter | **59/86 units** | full retest `20260914T190334Z`: 329,803 original Tcl tests, 0 errors; core was refused at atomic store. Targeted repair now reaches FP `__builtin_isnan`; full denominator unchanged. |
| llvm-test-suite | no adapter | **856/1,735 C programs PASS** | full retest `20260914T192632Z`: 868 compiler-refused, 10 runtime failures, 1 oracle-unusable; one original failure fixed, zero old PASS lost; 141 non-C programs excluded by language |
| PolyBench | no adapter | NOT RUN | lack of an adapter is not a measured FP exclusion |

Torture's raw native counts are 1,331 pass, 265 refused, 89 not-applicable and
7 execution failures. The disposition counts differ deliberately:

| Disposition | c-testsuite | GCC torture |
|---|---:|---:|
| PASS | 210 | 1,330 |
| compiler-failure / unresolved compiler-side result | 3 | 189 |
| FP-deferred, with existing reviewed evidence | 7 | 11 |
| not-applicable, with existing source-bound evidence | 0 | 162 |
| oracle-unusable | 0 | 0 |
| Fixed total | 220 | 1,692 |

`pr124358` runs successfully but retains its reviewed oracle-conflict
disposition. Six of the seven raw runtime failures retain their existing
overflow or oracle-conflict evidence; they are not silently converted to
PASS. The remaining failure is `pr85156`, below. Compared with
`20260909T215349Z`, no previous raw PASS was lost and 62 programs gained one.
The fresh zlib/zstd/cJSON receipts also use the original exported sysroot:
their old `__extension__`/whitespace/include-next source rewrites are gone.
The unchanged unit counts therefore do not mean those front-end fixes were
irrelevant. Those compiler/tooling fixes predate this evaluation; this batch
measures them rather than taking credit for implementing them.

The 189 unresolved torture rows are **not** 189 proved non-FP bugs. Many
generic E3101 diagnostics need source-level FP/vector/complex classification.
Searching for `float` or `double` is not an admissible way to waive a program:
an unused floating member or constant representation does not necessarily
require floating arithmetic.

Csmith retains 1,000 unique `(campaign, seed)` pairs and emits all 1,000.
All 68 oracle-unusable cases time out in EvoCC, GCC O0/O2 and Clang O0;
Clang O2 times out in 48, exits nonzero in 17 and finishes in 3. These are
not 68 demonstrated compiler failures, but neither are they 68 passes or
proofs of nontermination. No seed or runtime limit was changed for this run.

## Current issue inventory and closure order

### Reviewed semantic decisions — 2026-09-15

The user supplied the Q1–Q7 review. These decisions authorize implementation;
they are not fresh benchmark results. The review's host probes are not the
fixed AArch64 oracle evidence, and its referenced research-package files are
not present in this checkout. Reuse the existing benchmark entry point and
affected-program replays; no new campaign framework or full clone is required.

| Item | Approved contract / next executable check | Implementation state |
|---|---|---|
| Q1 expectation | Source-ICE projection; otherwise evaluate e then c once, fixed before optimization. The ordering is a profile choice. | Implemented in revision 1.9.98: both interpreters and native boundary cases pass; unchanged `pr85156.c` passes default/optimized native runs. Full campaign not rerun. |
| Q3 integer vectors | Existing S01 authority: real lane type/layout, masks, lane definedness and ABI; keep general-regs-only restrictions. Header types, vector operations and architecture intrinsics are separate populations. | Implemented in 1.9.101 / 0.32; targeted values and 9 fixed-ABI combinations pass. Final changed gate green; non-consensus tiny-vector ABI is explicitly refused. |
| Q7 scalar atomics | Naturally aligned 1/2/4/8-byte integers and pointers; explicit order/volatile/effects; valid load/store orders differ. Runtime legal order may strengthen to SC; no automatic 128-bit, aggregate or RMW support. World is not a concurrent memory model. | Implemented in working-tree EvoIR 1.9.100 / EvoMIR 0.31; targeted interpreter/native/proof checks pass, including seven mixed-compiler thread combinations. SQLite core passes its former atomic first error and now refuses FP `__builtin_isnan`; full-library rerun remains pending. |
| Q4/Q5 symbol visibility | C linkage, weak strength and ELF visibility remain independent. Protected binds inside the final executable/DSO, not a translation unit. Source internal normalizes to ELF hidden on AArch64, never C static. | Implemented in EvoIR 1.9.99 / EvoMIR 0.30; 32 native DSO combinations pass, three expected copy-relocation refusals. Original E2943 subset: musl 93/97 and Lua 10/21 assembled; full-library installation/test rerun pending. |
| Q2 initialization/finalization | Module registrations are retention roots; priority 101–65535/default 65535. Program entry differs from calling main. CRT/loader owns each load instance; do not assume musl dlclose unloads. | Implemented; both program interpreters, native termination modes, DSO lifetime and original LLVM program pass. Final changed gate green. |
| Q6 TLS | Existing Object instances keyed by symbol and logical thread; escaped pointers retain owner identity. Explicit address/state flow and correct ELF TLS, including PIC and late load. | Implemented; all four executable models and eight mixed-compiler, pre-existing-thread late-load combinations pass. Final changed gate green. |

Complete a semantic unit with RED, shared implementation, tailored tests,
the original affected program/object replay, changed gate, and measured status.
Only after the targeted fixes are green run the next full campaign. FP remains
deferred; existing refusal counts below remain the previous measured baseline.

### Targeted repairs after the review

Measured 2026-09-15 at HEAD `9f571ca` plus working-tree changes. Current release
SHA-256: `59dae2616510eead3fed79285bc8fe0c3831e028efde2156b9e81d43cdca3db1`.
These measurements do not replace any full-run numerator in the Results table.

**Expectation.** The initial RED failed on the original 349/350 value, the
two interpreters' 307/308 value, and the invalid second operand diagnostic;
there were no compilation errors. A distinct C-HIR runtime expectation keeps
its source-selected contract across inlining. Both operands are type checked;
the source-ICE form projects only the first, while the runtime form saves the
first value and evaluates the second even through calls and branches. A gate
caught two lost pure-constant branch folds; restoring executable folding only
when both operands safely fold fixed them without changing source ICE status.

The fixed GCC-14/Clang-19 AArch64 O0/O2 boundary results and their disagreements
are in `.evidence/builtin-probes/20260915-benchmark-frontier/expect-boundaries.out`.
The subject returns `const-local 1 8` and `nested-dynamic 3 8` at both tested
optimization settings, explicitly following the reviewed policy rather than
claiming oracle agreement. The unchanged LLVM `pr85156.c` exits 0 with default
flags and `-DNDEBUG -O3`; the boundary source passes default and `-O2 -Oeq-scalar`.
All four native replays used this current release binary, assembled/linked by
GCC-14 in the existing sealed container. The original inline-asm-bearing program
was not claimed to pass either interpreter; the reduced controls test both.

**ELF visibility.** One shared effective-visibility map now reaches functions,
declarations, global objects, extern objects and aliases, preserving weak
binding and source attribute spelling. Default-visible Linux PIC definitions
use GOT addresses even when defined in the current module; hidden/protected
strong definitions bind locally. The latter correction was necessary for the
native interposition control, not just attribute acceptance. DCE still asks
whether C linkage is internal. Darwin does not silently substitute hidden for
the newly admitted ELF protected form.

`tests/fixtures/symbol-component.out` records the commands and 32 native passes:
four visibility modes, separate translation units, GCC/Clang/EvoCC combinations,
an executable interposer, function/data addresses and weak aliases. Three
non-PIC callers (one per compiler) attempting a protected-data copy relocation
are correctly rejected by the linker. Five proof-checker mutations independently
change the visibility of each symbol-record kind and are rejected. Initial RED
had five declared failures and zero compilation errors.

`tests/contracts/visibility-targeted-replay.json` records every selected unit.
Using the existing emit/assemble helpers with two workers and the original
captured commands, **musl: 93 assembled + 4 refused out of 97; Lua: 10 assembled
+ 11 refused out of 21**. All 118 previously E2943 rows are now past that error.
The 15 remaining rows report E3101 (function-value type, object-subvalue type or
non-integer scalar); this diagnostic alone is not an FP applicability ruling.
No library object was installed and no complete Lua/musl suite was run in this
targeted replay. The prior complete receipts remain unchanged.

Verification: the expectation-only changed gate was 1,871 passed / 0 failed /
4 pre-existing ignored. The later visibility gate exposed two old tests that
required direct ELF addresses for default-visible PIC definitions; they were
corrected to check GOT interposition, with hidden/internal direct-address
controls retained. Its initial reading was 2,099 passed / 2 failed / 4 ignored;
the corrected visibility gate passed **2,101 / 0 failed / 4 pre-existing ignored**,
with Clippy and fmt clean. The subsequent atomic gate is recorded separately.

**Scalar atomics.** Initial RED: six declared failures, zero build errors.
The shared memory implementation preserves pointer provenance and immutable
snapshots, while one atomic event carries order/volatile alongside the common
volatile-event sequence. C-HIR, EvoIR and EvoMIR execute the single-thread
accesses; IR validation requires natural-width guards and current tokens.
The independent checker rejects changed order and an atomic-to-ordinary-store
substitution; field sentinels and the textual shape audit include both forms.
This does not introduce a concurrent interpreter or admit atomics into P0.

`tests/fixtures/scalar-atomic-access.out` records the fixed GCC-14/Clang-19
O0/O2 values, subject native execution, eight width/order instructions, and
seven GCC/Clang/EvoCC worker combinations. Each combination completed 5,000
release/acquire publications and 5,000 SC rounds, with zero publication errors
and zero forbidden SC outcomes. Stress success is not a memory-model proof.
The original SQLite core replay, using the existing command projection and
its recorded omissions, now stops at `sqlite3.c:36452:8`, `__builtin_isnan`
in `sqlite3IsNaN(double)`. It is still refused and was not installed; no new
full benchmark numerator is inferred from this targeted result.

Final reviewed-subset validation (working tree based on `9f571ca`):

- `CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 9f571ca --offline -j 2`:
  **2,311 passed / 0 failed / 4 pre-existing ignored**, 282 test targets;
  Clippy `-D warnings` and fmt passed. This count is the changed/reverse-dependent
  crate set, expanded by the memory change, not a newly measured full workspace.
- `python3 -m unittest discover -s tools/ladder/tests`: **148 passed**.
- `ruff check tools/bench.py tools/ladder`, `git diff --check`,
  `python3 tools/check-textual-grammar.py`: passed; 48 renderer/spec shapes,
  zero disagreement (a shape check, not a semantic completeness claim).
- `python3 tools/render-status.py` rerun; full benchmark receipts deliberately
  retain their original measurements. All test/build/container processes
  started for this subset have exited. No offline clone or full campaign
  was started. Changes remain uncommitted for combined review.

### ELF lifecycle, integer vectors and TLS implementation round

The Q2/Q3/Q6 review has now been implemented in the shared C-HIR, EvoIR and
machine paths. `tests/contracts/elf-vector-tls-targeted-replay.json` and
`tests/fixtures/elf-vector-tls.out` record the targeted population; they do
not alter the previous full benchmark denominators. The final changed gate
is green: **2,409 passed / 0 failed / 4 historical ignored**, 16 affected and
dependent crates, Clippy `-D warnings` clean, fmt clean. Ladder adapter tests
pass **148/148**, Ruff is clean, and textual instruction shapes agree **50/50**.
The final release binary repeated the native matrices and original programs;
its digest is recorded in the targeted receipt. Changes remain uncommitted
for combined review; no full-campaign numerator or new kernel claim is made.

- Q2: registration metadata roots internal functions, survives text/proof
  reconstruction, and emits init/fini arrays. Program entry shares one memory
  state and budget across initialization, main and termination; ordinary calls
  do not repeat initialization. The glibc DSO test checks two handles, final
  close, and a fresh load; musl unload semantics are not inferred from it.
- Q3: real integer-vector identity/layout, nonpromoted lane arithmetic,
  full-width masks, lane initializedness, dynamic/static shuffle, numeric
  conversion and representation casts. Eight-/sixteen-byte values use SIMD;
  wide values use objects, HVAs keep their ABI identity, and varargs use the
  VR save area. Nine fixed-ABI combinations pass after the mixed test caught
  and repaired 128-bit stack alignment following a stacked SIMD argument.
  Original `pr105613` exposed a missing 128-bit lane type and signed-mask
  assignment conversion; the shared repairs now pass both interpreters and
  native execution. Layout state identity advances to v2.
- Q6: thread-instance identity lives in immutable memory state; aliases share
  an origin and escaped pointers retain their owner. ELF TLS supports GD/LD/
  IE/LE and explicit model conflicts. Eight cross-compiler DSO combinations
  pass with two threads created before dlopen, cross-TU accesses and main
  reading/writing their escaped addresses. Logical thread termination and
  snapshot lifetime are unit tested; pthread/dlopen is not simulated by the
  program interpreter.

Five unchanged original programs now run successfully: LLVM `divides.c` and
`ConstructorDestructorAttributes.c`, torture `pr105613.c`, `20050607-1.c` and
`pr94591.c`. The last uses the explicitly admitted GCC-only shuffle builtin.
jq `jv_aux.c` passes its TLS declaration and next refuses `__builtin_isnan`
at 77:7. It is not installed or counted as a newly passed jq unit.

Three measured boundaries remain visible: GCC-14 O2 fails its own HVA-varargs
control (oracle-unusable, not a compiler waiver); GCC/Clang disagree on
sub-8-byte vector calling convention (E4002 at the native boundary); Clang
rejects vector-element addresses and vector increment, so those forms remain
refused. Original `pr108292.c` uses the address form and is not a PASS.
The source TLS-alias comparison also exposes a Clang fold disagreement;
EvoCC/GCC and both subject interpreters retain the alias object's identity.
None of this closes NEON polynomial/floating types, architecture intrinsics,
the remaining non-vector blockers below, or the full benchmark campaigns.

Fresh subject layout observations match the frozen base **128/128** and
attributed **22/22** manifests. Existing build output is reused; the small
native artifacts and one gate log live under `.build/vector-targeted`.

Counts are first-error observations, not independent root causes. A program
can expose another class after its first error is repaired. References to
torture sources below mean the unchanged
`gcc/testsuite/gcc.c-torture/execute/<name>.c` in the pinned archive.

| Priority | Construct / remaining work | Fresh evidence and example | Required closure |
|---|---|---|---|
| 1 | `__builtin_expect` second-operand side effects | Historical `pr85156` exit 134 and reduced wrong value repaired under reviewed revision 1.9.98; targeted original/default/optimized execution now passes. | Full torture/LLVM campaign still required; no body ruling is outstanding. |
| 1 | `visibility("protected")` admission and ELF semantics | Historical 97 musl E2943 rows now replay as 93 assembled / 4 E3101; native component-binding and metadata-mutation tests pass. | Implementation repaired under the review; full musl installation/test rerun remains. No PIC macro or source attribute was removed. |
| 2 | Integer-vector values, element places and initializers | 17 E2102 projected-lvalue rows all use integer vector elements (`pr105613`, `pr108292`); 22 E2201 initializer rows contain both integer and FP vector cases (`20050607-1` is integer). `pr94591` additionally needs `__builtin_shuffle`. | Shared vector typing/place/initializer semantics; tailored values and original vector programs. Do not classify all vectors as FP. |
| 2 | Target integer-vector types in system headers | Mbed TLS: 291/382 units; zstd: 37/44 units. Both stop at `arm_neon.h:40`, `typedef __Int8x8_t int8x8_t;`. | Admit the specified target types and then measure the newly exposed constructs. Do not suppress the header or call it an FP-only failure. |
| 2 | VLA scope and captured bounds | Six torture E2203 rows: `20040811-1`, `20221006-1`, `920721-2`, `920929-1`, `pr43220`, `vla-dealloc-1`; c-testsuite `00207` also fails. | Preserve bound evaluation, scope re-entry and deallocation semantics; values and goto cases, not acceptance alone. |
| 2 | Remaining inline-asm operand constraints | Four `+g` refusals (`pr65053-1`, `pr65053-2`, `pr65956`, `pr88904`); tied `0` input of a different width in `20030222-1`. | Constraint-specific RED and native results, including read/write and tied-width behavior. |
| 3 | GNU89 call/declaration compatibility | E3101 arity failures: `920501-1`, `921202-1`, `921208-2`; return mismatch: `920728-1`. The runner consumes their declared GNU89 option. | Probe original semantics and source definedness; shared call/prototype treatment, not validator bypass. `921208-2` also evaluates FP and cannot be called an integer-only case. |
| 3 | Static address/representation constants | `pr70460`: relative label-address table, E2208. `20030216-1` and `20031214-1`: static floating representations, E3026; the latter reads only integer fields. | Separate label relocations and data representation from FP execution. Tailored initialization and actual consumer values. |
| 3 | Mixed-member aggregate varargs | c-testsuite `00140`, E3030: aggregate has an unused float member but the checks consume integer members; torture `920625-1` / `pr44575` additionally execute FP. An integer-only record control already emits successfully. | Narrow the remaining ABI/type boundary: `is_integer_variadic_record` rejects the mixed-member shape. Do not claim all aggregate varargs are missing or waive this case merely because the type contains float. |
| 3 | Qualifier discard after a null-pointer conditional | c-testsuite `00144`, E2102 at `p = i ? 0 : (const void *)0`; a defined-condition initializer reduction gives E2202. Both oracles accept with qualifier-discard warnings. | Explicit profile disposition and shared conversion handling; retain the warnings and do not use the original uninitialized condition as an execution oracle. |
| 3 | Legacy atomics and higher frame traversal | `pr122000`: `__sync_add_and_fetch` undeclared; `20010122-1` / `20030323-1`: return-address level > 0, E2913. | Probe the supported target/profile boundary, including an uncalled function; keep runtime validity separate from parsing/lowering. |
| 4 | Integer complex / literal and type coverage | `20041124-1` has `_Complex unsigned short` and `100 + 200i`, E0112; the ten stray-token rows include both integer and floating complex forms. | Source-width complex classification before the FP deferral; original value tests where admitted. |
| 4 | Unclassified E3101 and other front-end rows | E3101: 56 function-value, 20 object-subvalue, 12 expression-model, 11 scalar-store and 4 scalar-local rows, plus the four call/return rows above. Smaller buckets include type combinations, FP builtins/literals and non-UTF-8 source. | Split by source construct and reviewed applicability. No broad message-based waiver; every remaining row stays visible in `results.jsonl`. |
| 4 | Build-option and applicability accounting | Torture retains 88 programs with unhandled requirements; the Mbed TLS driver rejects the reference build's `-std=c11` and `-std=c99`, which the adapter explicitly records as omitted. | Consume or adjudicate each semantic option. A passing projected build is not a claim of identical original compiler options. |
| 5 | Remaining unmeasured roadmap population | PolyBench. The four newly requested adapters and their measurements are recorded below. | Pinned sources, actual original tests and explicit FP/eligibility accounting. NOT RUN is not PASS. |

Application first-error details:

| Subject | Observed first error | What it establishes |
|---|---|---|
| zstd | 37 E1001 at GCC `arm_neon.h:40`, `typedef __Int8x8_t int8x8_t;` | Integer target-vector/header admission is a blocker before floating arithmetic. It is not evidence that all 37 units execute NEON or need FP. |
| zstd | 1 E3101, `programs/benchfn.c:142`, assigning `(double)totalTime / nbLoops` | A real runtime FP path; distinct from the header failures. |
| Mbed TLS | 291 E1001 at `include/arm_neon.h:40:9`; no other compiler refusal in the corrected run | The same integer target-header blocker as zstd. All 91 compiled objects are installed, zero are rebuilt or symbol-excluded, and the original default selftest/148 CTest tests pass. The remaining 291 objects are reference-built. |
| cJSON | `cJSON.o`: undeclared `__builtin_nanf`; `cJSON_Utils.o`: noninteger function value | The unchanged sources contain numeric APIs; the current API-test PASS is reference-only, not EvoCC execution. |
| TinyCC | `libtcc.o`, `tccgen.o`, `tccpp.o`: noninteger function value | Concrete coverage remains missing. A generic E3101 alone does not certify an FP waiver for the entire source. |
| toybox | Eight E3101 units: `lib_args`, `lib_hash`, `lib_xwrap`, `od`, `printf`, `seq`, `sort`, `uptime` | All 195 emitted objects pass symbol parity and the mixed test. The eight refusals remain unresolved, not waived by a generic diagnostic. |
| musl | 97 E2943; 211 E3101; 83 E1006; 14 E2001; 8 E2203; 1 E1005 | Complete 1,339-unit first-error accounting. The visibility regression is non-FP; the FP and literal/builtin rows need separate source-bound dispositions. |

Musl's eight VLA rows are `dcngettext`, `res_msend`, `execl`, `execle`,
`execlp`, `execvp`, `vfprintf`, and `getcwd`. `if_nameindex` fails specifically
on `sizeof(struct if_nameindex[ctx->num+1])`; the standalone
`sizeof(struct S[n])` reduction is E1005 while both oracles produce the
expected size 12. Its 14 builtin failures name `__builtin_inff`,
`__builtin_nanf` or `__builtin_fma`, rather than a generic missing identifier.

The eight musl gains are `rcrt1`, `semctl`, five `mallocng` units and `lsearch`.
They do not cancel or conceal the 93 newly refused units: net coverage is down
85. The first batch exits **1 / INCOMPLETE**, not PASS.

Torture's unhandled requirement inventory includes 32 `-fpermissive`, 13
`-std=gnu17`, 12 `-fgnu89-inline` and smaller optimization/diagnostic option
groups. These are overlapping option counts, not additional programs. The
source-bound applicability records remain authoritative; no option or
denominator was changed to improve this run.

Per-run `refusals` and captured source locations, not the adapters' older
generic `frontEndGaps` notes, supply this inventory. cJSON coverage is only the
public Makefile test and five public-API Unity programs (54 assertions); its
private-helper/CMake suite is still unmeasured. This is separate from the
remaining PolyBench adapter gap.

## Requested four-lane extension

### Tailored-first closure round (in progress)

#### Remaining-work order (2026-09-15 follow-up)

No full campaign starts until the in-profile reduced cases below are green.
The historical result files and denominators remain unchanged. A retained
profile refusal, an unusable oracle and an unmeasured case are not PASS.

| Order | Work | Closure condition / current state |
|---|---|---|
| 1 | Revisit historical first errors by original source construct, including E3101 | Reproduce each distinct still-failing non-FP shape; do not infer applicability from a diagnostic message. The vector/initializer subset newly exposed `simd-4`'s inline parameter plan defect. |
| 2 | Fix shared implementation paths and run the reduced cases | Keep all validator, provenance, initializedness and ABI checks. Inline vector parameter initialization and signed-scalar broadcast to unsigned vector lanes have completed RED/GREEN, interpreter and native verification; the already-green reductions below remain regression tests. |
| 3 | Preserve the original effective compiler options | Application projection and E0 had omitted/alleged unsupported optimization and target options already implemented by the driver. New RED covers optimization, PIC/TLS, wchar, initialization and machine flags in written order. Truly unsupported options remain named; no GCC option is mapped to an unrelated EvoCC optimization. |
| 4 | Close source-encoding and target-header decisions | The bounded source-byte ruling is approved and implemented; original-program and full-build validation follows below. Current unmodified `arm_neon.h` reaches line 44 `__Float16x4_t`, not the historical integer type at line 40; floating/polynomial nominal names and architecture intrinsics remain distinct capabilities. |
| 5 | Complete source-bound dispositions and baseline diagnostics | Keep pointer-VLA typeof, integer-complex division, ancestor return-address and tiny-vector ABI conflicts explicit; keep GCC HVA/Csmith unusable controls. Both fixed compilers' analyzers identify the uninitialized read in LLVM `20090113-1`; dynamic sanitizer evidence remains unavailable because the sealed image lacks the AArch64 MSan runtime. No timeout or generic E3101 becomes an FP waiver. |
| 6 | Targeted original-program replay, then serial full campaign | Use `tools/bench.py`, preserving sources/options/seeds/outputs. Recompute numerators from that run only. LLVM MultiSource/External, PolyBench and application sub-suite gaps remain separately NOT RUN until actually measured. |

At this historical checkpoint floating-point implementation was under research.
The handoff is `docs/notebook/floating-point-research-brief-20260915.md`;
its 2026-09-20 intake section now records the received report, finite-constant
implementation and still-open runtime/ABI decisions. No research recommendation
is an implemented capability or a replacement benchmark result.

This follow-up completed three new repairs:

- Inline vector parameters now receive complete-object initializer plans,
  matching their canonical type; `simd-4` no longer trips the independent
  initializer-identity validator. The validator was not weakened.
- Scalar-to-vector conversion admits the measured signed-to-unsigned lane
  conversion, including negative values, while retaining narrowing and
  signed-lane conflicts. Original `pr94412` and the reduced case execute
  correctly at the default and `-O3` subject settings.
- The shared benchmark option projector now forwards already implemented
  optimization/PIC/TLS/wchar/initialization/machine flags, in written order.
  Truly unsupported options remain named; historic projected runs do not
  become faithful-option PASS retroactively.

Final changed gate after these repairs: **2,441 PASS / 0 FAIL / 4 historical
ignored**, 301 result groups, 16 affected/dependent crates, Clippy
`-D warnings` and fmt green. The small language-semantics group is **28/28**;
Python ladder tests are **150/150**, Ruff is clean, textual shapes are
**51/51** with no differences. Fresh subject layout observations match the
frozen **128/128** base and **22/22** attributed cases exactly. The base
corpus comparison CLI is fixed to 128 cases and is not an attributed gate;
the 22-case check validates the source manifest and compares fresh TSV bytes
with its frozen consensus using the existing observation renderer.

The affected-vector original-source replay inspected 38 programs: **32
emitted and passed with each native assembler (64 zero exits)**; six remain
refused. Four of the six have source-bound floating-execution deferrals and
O0/O2 runs under both oracles; `pr60960` retains the measured tiny-vector ABI
conflict. `pr85331` executes floating shuffle, but Clang does not implement
its `__builtin_shuffle`, so that control is NOT RUN and the case remains
separately unresolved. Details and unchanged original names are in
`tests/fixtures/vector-original-replay.out`. No full campaign has run and
no historical numerator has changed.

The work is **not closed**: the source-encoding ruling has since been approved
and implemented in the follow-up below, but its full campaign is not complete;
`arm_neon.h`'s floating/polynomial nominal types and architecture intrinsics
are not certified by integer-vector tests; remaining application/LLVM E3101
rows and unavailable controls still require source-bound accounting.
`tests/fixtures/llvm-uninitialized-original.out` records both static
analyzer traces and the failed MSan link without changing its disposition.
After the remaining in-profile reductions are green, the existing serial
`tools/bench.py` campaign, not these targeted counts, decides closure.

### Approved GNU-inline and source-byte follow-up

Working-tree revision 1.9.102 records the approved rules, not a blanket
diagnostic waiver. Function bodies are classified from the complete declaration
chain before linkage, allocation, inlining and interpretation. An inline-only
GNU body supplies direct expansion but no external definition; an address or
unexpanded call retains the external symbol. ISO and GNU modes remain separate.
The GNU-mode option does not change the selected C language-version macro.

The initial five inline tests failed without build errors. Six current focused
tests pass, including local objects in distinct inline/real bodies and a
function both expanded and taken by address. Sealed ARM64 GCC-14/Clang-19
mixed linking gives six zero exits; two missing-external-definition links
correctly fail. The complete-chain probe accepts inline-body-then-real-body
and both orders of a non-extern inline redeclaration. Two GNU extern-inline
bodies remain refused: GCC rejects while Clang accepts that duplicate form.

The last body-selection check found and corrected an additional wrong-value
bug: with an inline-only body followed by a real definition in the same unit,
the earlier resolver still selected the inline-only body for direct expansion.
All four fixed oracle modes use the real body, even for a call written before
that definition. `gnu-inline-definition-chain.{c,out}` pins the real body's
local identities and absence of superseded-body side effects. Both interpreters
and both native assemblers now pass. The earlier test's contrary expectation
was wrong and was replaced; the running Csmith binary predates this repair.

Raw source loading and token spelling now preserve bytes, with E0002 retained
at unsupported lexical/metadata consumers. Five focused route/value tests and
two lexer tests cover main/stdin/include, invalid sequences, comments, splicing,
macro transport and original offsets. `-fsigned-char`/`-funsigned-char` and their
inverse spellings are consumed in written order; character constants and #if
use the selected representation without conflating plain char's type identity
with either explicitly signed type. Invalid pragma/line filenames, stringizing,
prefix paste and wide/mixed literals remain refused. The benchmark preparation
workaround that rewrote all high bytes has been removed; fresh builds must use
the original bytes, not reuse a previously rewritten header.

An expanded regression caught vector rvalues escaping an ended statement-
expression scope. The interpreter now copies the complete value to the parent
scope before exit; pointers still retain their original lifetime. Inline
parameters and return buffers use complete-object plans. All 28 language-
semantics tests pass after this repair, including nested vector expressions.

Original kernel replays, using the debug subject and existing K1 projection,
assemble and match reference symbols: `.vmlinux.export.o` has zero external
definitions and `arch/arm64/kernel/process.o` has the same 26 definitions.
Neither produces the former 15 extra FORTIFY wrappers. These two objects are
not an installation or complete 6,916-record survey. Original LLVM encoding
rows also pass their old E0002 frontier but are not PASS: `fbench.c` now reaches
E1001 at its implicit-int function definition; `ffbench.c` reaches E3101 in
floating signatures; `20000227-1.c` reaches undeclared `abort` E2001. These
remain separately recorded requirements, not source-encoding regressions.

The final follow-up gate passes **2,454 tests / 0 failed / 4 historical ignored**
in 304 result groups across the 16 affected/dependent crates; Clippy
`-D warnings` and fmt pass. This includes the representation-store repair below.
The earlier gate that found the vector lifetime error remains a failed attempt.
Small native/source outputs stay in the existing `.build/vector-targeted`
directory, not a new private build tree.

The subsequent language-mode gate is **2,461 PASS / 0 FAIL /
4 historical ignored** across the same 304 groups, with Clippy `-D warnings`
and fmt clean. Its 18 focused inline/mode/raw-source checks include three
new declared RED failures: pre-C11 prefix tokenization, the same behavior on
main/stdin/include byte inputs, and the GNU89 declaration-for boundary.
The two fixed-oracle extension disagreements are recorded in
`tests/fixtures/language-mode-lexical-boundaries.out`, not silently admitted.

Source review of the completed LLVM run also isolates a non-FP E3101 in
`UnitTests/conditional-gnu-ext.c`: an integer-complex parameter crosses an
ordinary CFG edge, but parameter storage collection omitted its aggregate
representation. `aggregate-parameter-edges.{c,out}` records the declared RED
and shared parameter/conditional repairs. GCC-14/Clang-19 at O0/O2, both
interpreters, and both native assembly/link combinations pass. The fixture
covers wide-vector parameters, complex zero/nonzero omitted-middle operands,
single evaluation and an unselected uninitialized arm; an actual uninitialized
read still reports E3001. The unchanged original LLVM program also passes its
timeit/fpcmp reference check with both assemblers. This is a repaired non-FP
defect with targeted original-program evidence, not a new full-run numerator.

One additional storage-only reduction found that named floating assignments
did not use the constant-representation path already used by projected places.
The shared checked destination projection now handles both. A discarded
assignment stores the constant bits; consuming a floating assignment result
or converting a runtime integer to float still refuses. Named, volatile,
indexed and pointer-postincrement targets pass both interpreters and native
execution with both assemblers. Both fixed oracles at O0/O2 return zero for
the expanded `constant-floating-representation.c` fixture.

The fresh source-bound audit checks 119 previously unclassified originals:
116 require named floating operations and run successfully under all four
fixed oracle configurations. `torture-floating-frontier.out` records their
unchanged source hashes and specific operations. The other three have Clang
compile refusals (`__builtin_isinff`, decimal floating types, and floating
`__builtin_shuffle`); they are not dual-oracle passes. The two ancestor-return
queries and integer-complex division receive their already-reviewed conflict
dispositions, not floating exclusions. Historical raw results stay unchanged.

The 2026-09-15 follow-up freezes the existing original-program inventory above:
no seeds, source files, options or denominators are replaced. Small C reductions
live in `tests/fixtures/` and run through `language_semantics.rs`. After the
tailored checks and gate, the existing `tools/bench.py` started c-testsuite,
torture and both 500-seed Csmith campaigns in serial order. The release driver
SHA-256 is `2c53462626cf80ebe59b0b064d51d75a999fb553bf434c100f0b6539b127f41f`.
Results remain pending; no final benchmark numerator is inferred from the
targeted repairs or oracle classifications.

The first complete fresh c-testsuite run, `20260915T142739Z`, reports **213
PASS / 7 FP-deferred / 0 compiler failures** over the unchanged 220 programs.
Torture run `20260915T142805Z` records **1,396 raw PASS / 214 refused / 76
requirement exclusions / 6 runtime failures**, with no previous raw PASS lost.
Its five-way account, frozen at launch, has 1,395 PASS, 131 FP-deferred,
159 not-applicable and seven unresolved rows. Three of those seven now have
the unusable oracle controls above; four newly admitted option paths require
their own floating-interface checks. Those four original programs subsequently
compiled and exited zero in all four oracle modes; their floating-interface
requirements are now source-bound in `torture-floating-frontier.out`. The raw
run is not rewritten.

Csmith run `20260915T143112Z` completed both original 500-seed campaigns:
**932 agreeing / 68 oracle-unusable / 0 refusals / 0 mismatches** (base
469/31, bitfields 463/37). It used the fixed `2c5346...` driver above, before
the final same-unit inline-body-selection repair. These are completed old-
binary results, not an assertion that the subsequent binary has been measured.
The next release gate/build and original application replays run serially.

Preparing the later application replays exposed another measurement-config
defect: the legacy mixed-link projector still omitted supported optimization,
PIC, GNU-inline, character and machine options. It now consumes the same
ordered option table as E0 and the other application projector. Both
projectors preserve an explicitly unsupported `-std=` mode for the driver's
refusal rather than silently changing it to GNU11. RED tests pin these
changes. Subsequent application receipts must record the new commands;
historical mixed-link counts do not establish these newly faithful commands.
The same review also removed the legacy mixed adapter's `_FloatN` and
AdvSIMD/SVE header rewrites. TinyCC/toybox now use the same original exported
sysroot as the other application adapter, without making another header copy.
Any newly reached header rejection is reported, not hidden by a disabled
preprocessor branch; old adjusted-header results stay historical.

The supplemental kernel survey reuses its existing case-sensitive trees via
`tools/bench.py kernel --survey-only --kernel-work-root ...
--kernel-source-root ... --low-disk`. Low-disk mode emits, assembles and checks
64 records at a time, retaining successful objects and all failure artifacts,
and deleting successful assembly only after both assembly/object hashes match.
The temporary object copies used for symbol comparison are also released.
Ordinary survey cache reuse still requires its assembly bytes; deleting them
does not turn a hash into reusable code. This is a storage/measurement change,
not a compiler fix or a new kernel verdict. K1's 156 harness tests and Ruff pass.

The subsequent original-command application campaign is unified run
`20260915T151427.374334Z`, release SHA-256
`49e3623b694936a9543ea459da19c6948b17647b6c64f46c352041b1564a74a0`.
Its c-testsuite child `20260915T151427Z` has 213 PASS / seven FP-deferred /
zero compiler failures. Torture child `20260915T151452Z` has 1,395 PASS /
135 FP-deferred / 159 evidence-backed not-applicable / three oracle-unusable /
zero compiler failures. Raw torture results remain 1,396 PASS / 214 refused /
76 requirements exclusions / six runtime failures; reviewed dispositions do
not rewrite those raw outcomes or turn the unusable controls into PASS.

Completed application children at this checkpoint: TinyCC `20260915T151752Z`
installs eight of 11 objects in the passing original test build, with no symbol
exclusions; zlib installs all 19/19; zstd installs 6/44; jq
`20260915T151957Z` installs 58/69. These are object-substitution counts, not
whole-program non-FP classifications. TinyCC's remaining original sites are
`libtcc.c:2231` (runtime double division in statistics), `tcc.h:1455`
(`ieee_finite(double)`), and system `stdlib.h:118` (`strtod`'s double return).
The current generic calling-convention message also covers scalar floating
values; none of these three sites is evidence of a heterogeneous-union ABI bug.

Preserving the original command changes the first frontier for cJSON
`20260915T151949Z` (both objects: unsupported `-std=c89`) and Lua
`20260915T152156Z` (all 34: unsupported `-std=gnu99`). Those options come from
the original Makefiles (`cJSON/Makefile:27`, `Lua/src/Makefile:9`); they are
neither FP dispositions nor regressions measured under the previous rewritten
commands. The declared language modes must be implemented with their actual
semantics, or remain explicit option limitations. Dropping them is not a fix.
SQLite and the remaining applications are still running at this checkpoint;
the expanded kernel survey and latest-binary Csmith are not yet completed.

The GNU99 option limitation has now been reduced and implemented separately:
`gnu99-language-mode.{c,out}` measures the actual version macro, ISO-inline
definition roles, for-declarations and agreed GNU extensions. Four fixed oracle
configurations and both subject/native assembler combinations pass; seven
focused inline/mode tests pass after a declared unsupported-option RED.
The driver selects a language mode rather than treating the flag as an ignored
GNU11 alias. Strict `c89`/`c99` are still explicit unsupported modes. This source
change is not in the running `49e362...` application campaign; Lua must be
rerun after the next release build, not credited with an inferred improvement.

That application campaign has now finished. SQLite child `20260915T152227Z`
installs 67/86 EvoCC objects; its unchanged `veryquick` test executes 329,803
assertions with zero errors. LLVM child `20260915T152557Z` keeps all 1,735
original C identities: **875 PASS / 845 compiler-refused / six link failures /
eight runtime failures / one oracle-unusable**. The reference still has
1,734 PASS and the same `gramschmidt` timeout. Toybox keeps 195/203 installed
objects, no symbol exclusions, and the unchanged shared reference failure.
Musl has 1,339 explicit C99 refusals; Mbed TLS has 176 C99 and 206 C11 refusals.
Those results invalidate no earlier raw run, but do show why the old option-
dropping projector could not establish faithful-build coverage.

After this full measurement, the mode repair was extended through separate
RED tests to ISO C99/C11 and GNU17. The ISO modes change keyword roles as well
as macros, keep reserved GNU spellings and normal call diagnostics, and consume
user overrides afterward. `language-mode-versions.out` records 20 fixed oracle
compile/run passes and ten subject-native mode/assembler passes, plus the
separate C99 fixture. C89 remains unsupported rather than aliased to GNU89.
These changes are subsequent source work, not present in the completed
`49e362...` campaign. Original Lua/musl/Mbed TLS replays remain required.

Two new LLVM link failures (`Shootout-hash` and `Shootout-lists`) exposed the
bounded ordinary-inliner eligibility check accepting only private definitions,
not ISO/GNU inline-only bodies. The existing callee-first substitution now
also accepts small acyclic inline-only bodies, retaining its size, noinline,
address-use and control-flow boundaries. No external definition is invented.
The new focused case was RED on a remaining `call @sum`; all ten inline/mode
tests now pass. Both original programs emit under their original projected
argv; both fixed assemblers link and run them, and upstream timeit/fpcmp checks
match the unchanged reference output in all four combinations. The first
manual comparison omitted timeit's `exit 0` suffix and therefore failed; the
correct wrapper replay, not that comparison, is the passing observation.

The other four LLVM link failures were the original optimization-dependent
`20041114-1`, `compare-3`, `pure-1` and `shiftopt-1`: undefined `link_error*`
calls remained in branches expected to be eliminated. Their small RED cases
now pass after bounded condition/shift facts and private-object store tracking.
Original condition evaluation, poison checks, UB obligations and call effects
remain; no undefined function is stubbed. All four unchanged originals pass
their upstream RUN checks with both fixed assemblers (eight native passes).
`tests/fixtures/llvm-link-closures.out` binds sources, original command options,
subject and results. The 30 CFG tests include mutation/narrowing/poison
counterexamples. These are targeted repairs, not a replacement full LLVM run.
New runtime row `pr23135` reaches signed
vector multiplication overflow at source line 131 in both subject interpreters;
the LLVM copy does not request the newer torture source's `-fwrapv`. The prior
seven runtime rows retain their source-specific analysis above. No blanket
E3101 or runtime-signal classification is applied.

The subsequent ISO C89 implementation consumes `-std=c89`, `-std=c90` and
`-ansi` as real language modes: strict keyword roles, absent C99 version macro,
legacy declarations and GNU inline definition rules. It is not a GNU11 alias.
`iso-c89-language-mode.{c,out}` records the focused RED/GREEN, fixed-oracle
keyword matrix and two native passes. Original cJSON replay is still required;
the old two unsupported-option results above remain historical.

The running supplemental kernel survey exposed a separate missing definition,
`vgic_its_get_abi`: the preprocessor skipped macro names lexed as C keywords,
so the kernel's `#define inline ... __gnu_inline__` lost its attribute. Macro
lookup/rescan now uses the existing preprocessing-identifier classification.
The unchanged `arch/arm64/kvm/vgic/vgic-its.o` assembles with all 11 reference
global symbols/kinds and no extras; `keyword-macro-kernel.out` records the
subject and object hashes. The live survey uses its older frozen subject and
is not rewritten or restarted to hide this observation.

K1 command projection also now maps original `/usr/...` include paths into an
explicit exported toolchain root, preserving argv order and binding those
headers into the input snapshot. Its 158 tests pass. This is a measurement
configuration change, not a waiver of `arm_neon.h`: targeted xor-neon replay
now reaches E1001 at the unimplemented nominal `__Float16x4_t` declaration.
All 44 GCC compiler-include files match the sealed image. A separate export
audit found eight case-colliding Linux UAPI header names in the broader shared
sysroot; its whole-tree byte identity is therefore not established. Neither
that header-export defect nor architecture-intrinsic coverage is counted as
floating execution or a compiler PASS.

That export defect is now repaired for subsequent runs. All adapters use one
helper, which refuses a case-insensitive destination and accepts an explicit
`EVOCC_BENCH_SYSROOT`. The fresh export is only the headers, in the existing
case-sensitive volume at `.build/k7-coverage/case/ladder-sysroot`; no kernel
or workspace copy was made. The sealed-image comparison follows symlinks and
checks **3,974 file paths, zero byte mismatches**. The old 3,492-path check did
not traverse symlink directories. Ladder tests are 159/159, K1 tests 158/158,
Ruff clean. The live survey's inputs stay unchanged; future receipts record
the new header root through their actual include argv/input snapshot.

The original LLVM `pr38151` E3030 is also repaired, not classified as FP:
integer complex was absent from the shared variadic composite predicate;
default-promotion casts and va_arg type recovery then exposed two further
paths losing that same type. `integer-complex-varargs.{c,out}` covers direct,
nested, aligned and indirect composites, with/without argument-register
exhaustion. Both interpreters, four fixed-oracle modes and six native
self/crossed ABI checks pass; the unchanged original RUN test passes with
both assemblers. Floating-complex refusals remain pinned. The focused group
is 30/30. The subsequent changed gate passes **2,470/0**, with four historical
ignored tests, 304 result groups and 16 affected/dependent crates; Clippy and
fmt pass. Fresh layout observations are **128/128** and **22/22**, and textual
grammar is **51/51**, zero differences. This is not a fresh isolated-clone
replay or a full benchmark result. The next release binary has SHA-256
`eea74611efc97b2cdee5d3a6715539f0a9b3da471ba3b2e7229179ebde9a3f1c`;
the running kernel survey retains its older frozen binary.

The source-bound first-error replay of all 148 old LLVM E3101 rows is in
`.build/vector-targeted/llvm-e3101-targeted.jsonl`: GNU conditional decay now
emits; 147 retain a diagnostic. The residual messages group into 98 floating
call-interface cases, 27 noninteger subvalue reads, 11 noninteger scalar
operations, nine noninteger expressions, one store conversion and one call
arity. The latter is `20051012-1.c`: `foo()` defines zero parameters but
`foo(t)` supplies a record. Reviewed F01 explicitly retains this call-arity
refusal; it is not floating point. `pr79354.c`'s store is the actual conversion
`float e; unsigned long d; e = d`, not an integer aggregate-store defect.
These are diagnostic/source classes, not automatic five-way dispositions:
each FP claim still requires a necessary executed operation, not just a header
declaration. The original source and options remain unchanged.

A separate fixed-oracle declaration audit removes only diagnostic-suppression
options and preserves all original semantic options, explicitly using GNU11
where the original command supplied no dialect. Among 655 E1001/E2001 rows,
636 are rejected by both fixed compilers for implicit declarations/types;
19 require separate review. Commands, removed warning options, source digests
and first diagnostics are in
`.build/vector-targeted/llvm-profile-declarations2.jsonl`. This is evidence of
the frozen profile boundary, not a claim that the original warning-suppressed
LLVM reference commands failed, and it does not change raw benchmark outcomes.

Original language-mode replay now gets beyond the old mode refusals:
cJSON's two objects reach a floating call interface; four selected Lua objects
and seven selected musl objects emit, including `setenv`, `malloc`, TLS address
resolution and `pthread_create`. Five selected Mbed TLS objects reach the
separate `arm_neon.h` nominal-type frontier. The exact selected commands and
source digests are in `.build/vector-targeted/language-mode-originals.jsonl`;
no objects were installed and none of these emission-only checks is a program
PASS. Full application campaigns must use the new case-sensitive sysroot.

The frozen supplemental survey next exposed `drivers/net/netkit.o`: a static
initializer combines preceding explicitly initialized const feature masks.
The shared constant evaluator now resolves this GNU initializer context without
changing ordinary ICEs or runtime expect side effects. The 17-case fixed ARM64
matrix, two interpreter paths, 20 native passes and retained refusal/profile
boundaries are recorded in `static-const-initializers.{c,out}`. The full language
fixture group is **32/32**, with a separate declaration-rollback regression.
The unchanged original object assembles with **4/4 global symbols and kinds**,
zero extras/missing; no object was installed and no full-survey count is
credited from this targeted replay.

That static-initializer changed gate completes at **2,473/0**, four historical
ignored tests, with Clippy and fmt green. The subsequent diagnostic-only slice
names actual floating call interfaces **E3049**, required floating scalar
values/conversions **E3050**, and canonical call arity mismatch **E3051**.
Its six C reductions pass, as do the 32-case language group and 24 resolved-form
checks; other unsupported types and invalid generated IR remain distinct and
are not declared floating merely to reduce an E3101 count.

LLVM now uses the existing five-way ledger and binds each disposition to both
source bytes and its original full command. A changed dialect or wrapping
option invalidates the entry. The 636 jointly refused GNU11 declaration/type
cases are recorded under reviewed F01; original warning-suppressed reference
PASS results remain untouched. `llvm-declaration-profile.out` explains that
measurement difference. Another 53 entries reuse already reviewed torture
evidence only where the original source bytes match exactly, retaining their
current LLVM commands. These are classifications, not new execution passes.
The ladder test suite is **162/162**, Ruff clean.

For `pr23135`, the unchanged original under GCC's UBSan at O0/O1 reports
signed vector-lane multiplication overflow at line 131; Clang's independently
instrumented O0/O1 objects report none. The raw results are in
`.build/vector-targeted/llvm-vector-ubsan.jsonl`. This strengthens the source
diagnosis but does not meet the reviewed dual-instrumentation waiver: the row
stays unresolved, with neither `-fwrapv` nor the expected value changed.

The named-diagnostic slice completes its changed gate at **2,474 PASS / 0
FAIL / 4 historical ignored**, 304 result groups, with Clippy and fmt green.
The next release subject is
`f625ac5db894bd836ecc3b46c684cb9fcd3465107b86ff7283b5abf90a953184`;
the live supplemental kernel survey continues to use its frozen older binary.

The remaining E8 adapter gap now has a pinned PolyBench/C 4.2.1-beta adapter
and five focused tests; the whole ladder group is **167/167**, Ruff clean.
It uses the upstream 30-kernel inventory, explicit GNU99/MINI_DATASET and
`POLYBENCH_DUMP_ARRAYS`, retaining the source's original data types. One
unchanged `nussinov` smoke program compiles and exits zero under both fixed
oracles and the release subject, with identical complete array-dump digest
`7154f627c3262d16a3cb15358a6bff1595356d6bb6c48287af265a5c0383d7f8`.
The initial strict-C99 smoke failed in the reference utility's undeclared
POSIX `posix_memalign`; its output remains in `adapter-smoke-20260916`.
The explicit GNU99 rerun is `adapter-smoke2-20260916`, both under
`.build/ladder/polybench/native`. The reference-built upstream utility is
allocation/instrumentation support, excluded from the 30 kernel sources;
this is not an all-C-source compilation or a performance claim. Full E8 and
the fresh application campaigns remain unrun while the kernel survey owns
the unified benchmark lock.

The unified entrypoint subsequently registers E8 and consumes LLVM's five-way
account using the same denominator checks as E0, leaving raw outcomes intact.
Two declared failing adapter tests become green; the ladder suite is
**169/169**. These are future-run reporting changes only: the already running
kernel process retains its loaded observer code and unchanged compiler/input
identity; no second complete campaign is started through the held lock.

An additional approved-byte-boundary audit found a genuine serialization
error: backslash followed by a raw invalid byte acquired literal octal digits.
The shared lexer now preserves backslash parity while retaining original
token bytes/spans. `raw-byte-escape.out` records two declared REDs, **3/3**
lexer and **7/7** CLI byte tests, eight fixed-oracle passes and four native
subject passes, including both plain-char signedness settings. Unsupported
encoding consumers still refuse; this does not claim an original-program gain.

Fifteen more original LLVM builtin cases now have source/command-bound FP
dispositions after **60/60** fixed-oracle compile-and-run checks. Their actual
executed comparisons, copysign calls and arithmetic are listed in
`llvm-floating-builtins.out`. Two superficially similar rows are not waived:
`ieee/builtin-nan-1.c` only constructs static NaN bytes then uses `memcmp`, so
it is a constant-representation capability gap, not required floating runtime;
`Float/classify.c` contains a builtin unavailable in GCC-14. Both remain outside
this FP disposition batch. No existing raw result or original source changes.

The raw-byte escape slice completes its changed gate at **2,476 PASS / 0
FAIL / 4 historical ignored**, with Clippy/fmt green; release digest
`efbee7aedf65dc86aa202dc02be20c7de6e40d21fd05f0dd63edcf6acf240027`.
It is not the frozen subject of the still-running supplemental survey.

The representation-only LLVM NaN case now has a shared exact-payload constant
implementation, not a runtime-FP disposition. Two declared REDs become green;
the language regression group is **35/35**. The fixed ARM64 matrix covers 19
payload strings at O0/O2, retaining five oracle-conflict forms and three joint
constant-context refusals. The fixture and unchanged original each pass both
native assemblers; `nan-constant-representation.out` binds the source, subject
and original command. Dynamic floating calls/conversions remain refused.

This slice's changed gate completes at **2,479 PASS / 0 FAIL / 4 historical
ignored**, 304 groups, Clippy/fmt green. Release subject
`4eba8ceeaddd6f03b148cfb97fbfec190f49404c5a8dd4ef95de9c37c1c0a98a`
is frozen separately from the live kernel survey binary.
The 141 still-unclassified historical LLVM refusals replay to six emissions,
82 E3049, 24 E3050 and 29 other named refusals, with no remaining E3101 in
this selected set. The six originals (`methcall`, `objinst`, `pr38151`,
`pr7284-1`, `builtin-nan-1`, `conditional-gnu-ext`) pass twelve native runs
under both assemblers at their unchanged default inputs; upstream timeit
exit-status framing and fpcmp validate the three output-checking programs.
Commands and outputs are in `.build/vector-targeted/llvm-green-originals`;
the initial direct-output check lacked timeit's `exit 0` trailer and is
retained separately, not misreported as a compiler mismatch or used as closure.

Ten further LLVM rows have source/command-bound reviewed-profile dispositions
in `llvm-residual-profile.out`; the ledger now verifies **714 LLVM entries**.
This does not automatically classify the 106 floating-diagnostic rows by code,
erase the uninitialized/vector-overflow evidence gaps, or excuse NEON header
support as floating runtime. A serial application campaign is queued behind
the existing benchmark lock using the frozen release above; queued is not run.

The live supplemental survey later exposes three SCSI symbol-drift rows:
`megaraid_sas_base`, `megaraid_sas_fusion`, and `mpt3sas_scsih`. Their five
missing definitions all use the kernel's `inline` keyword macro; the already
repaired preprocessing path restores GNU definition classification. Fresh
original-command replay with the gated `4eba8cee` release assembles all three
and matches respectively **30/30, 25/25, 30/30** global names/kinds, with no
extra symbols. `keyword-macro-kernel.out` records the object/assembly digests.
The old frozen survey rows remain failures; no object is installed, no
device runtime is claimed, and these three replays are not a new full survey.

A follow-up K1 command-projection RED finds that the compiler's newly supported
`-fgnu89-inline`/`-fno-gnu89-inline` and `-fsigned-char`/`-funsigned-char`
options were still omitted by that adapter. They now pass through in written
order, and the omission audit no longer calls unsigned-char unconditionally
equivalent to the default. K1 tests are **159/159**, Ruff clean; twelve small
end-to-end projected-command checks cover four override sequences with C-HIR,
EvoIR and emitted function-definition roles. This is an explicit observer
change for future runs, not a mutation of the already loaded survey process.
The affected SCSI commands gain only `-funsigned-char`; their prior replay
commands and results remain separately identifiable. Re-emission with that
explicit option is byte-identical for all three; `explicit-options.json`
records the exact new commands. No Rust implementation
changed after the 2,479-test gate for this adapter correction.

The LLVM disposition audit adds 29 source/command-bound PolyBench rows,
bringing that ledger to **743 entries**. All original commands enable
`POLYBENCH_DUMP_ARRAYS`, whose output passes through `print_element(double)`;
12 disable the secondary floating check, 16 invoke it, and `fdtd-2d` defines
but does not call it (its floating stencil/output establish applicability).
`llvm-polybench-floating-paths.out` records those distinctions and source
digests. This is a source audit against the existing successful Clang control,
not a fresh native run or a rewrite of the 1,735-program denominator.

A separate Q2 regression finds that an empty constructor/destructor had no
source call or memory access, so function indexing omitted its mem/world
interface and the existing lifecycle validator correctly refused it as E3101.
Indexing now treats registered loader callbacks as state-carrying calls;
registration is resolved once and reused, without weakening validation.
The declared RED is now green across constructor/destructor and declaration
placement variants; lifecycle/inline tests are **26/26**. The minimal
`elf-pure-lifecycle.c` emits both registration sections and runs with GCC-14
and Clang-19 assemblers. Clang warns that the late destructor attribute is
ignored in its source control; the project's existing declaration-chain
registration rule, not that warning-bearing control, determines our behavior.
An exploratory `used`/`retain` emission expectation was withdrawn after the
fixed-oracle attribute matrix and spec audit: retention is not a blanket
requirement to emit a function. It is not counted as a repaired compiler RED.
Release `bbeac066b71954fb239d5876f7ee0fda5bf2218fff9f4ebba76a6a40a33a99eb`
passes ten existing native lifecycle checks: four termination modes and
two-handle DSO unload/reload, each with both assemblers. The pure fixture's
release assembly is byte-identical to its tested debug emission.
This fix's changed gate completes at **2,480 PASS / 0 FAIL / 4 historical
ignored**, 304 groups, Clippy/fmt green. The waiting application queue was
replaced before it started, so its immutable subject is now `bbeac066`; the
old waiting log remains identifiable and contributes no benchmark result.
The active supplemental kernel survey continues unchanged on `41b2c0b6`.

Fifteen further original LLVM programs receive source/command-bound FP
dispositions in `llvm-floating-value-paths.out`, bringing the ledger to
**758 entries**. These include complex floating arguments and real-component
comparisons, executed hexadecimal floating values, and the puzzle generator's
floating scaling; they are not classifications based only on declarations or
diagnostic codes. Their existing Clang controls are PASS; this audit is not a
new campaign. All **169** ladder tool tests remain green.

The next supplemental row, `lib/decompress_unlzo.o`, has the same keyword-macro
classification defect: current original-command replay restores `parse_header`
and matches **2/2** symbols/kinds. Five later RAID6 NEON rows reproduce the
header capability gap after fixing their include projection: each reaches
`arm_neon.h:44`, `__Float16x4_t`, and remains refused. Together with `xor-neon`,
these are not waived as FP merely because the vendor header names an FP type;
the executed kernel algorithms require real integer NEON support.

`mm/page_alloc.o` likewise restores its missing `__free_pages_prepare` on
current replay (**81/81 symbol names**), but an additional exact nm-kind
comparison finds `zone_names` and `migratetype_names` in writable data (`D`),
versus GCC read-only data (`R`). That stricter check is **not PASS**. The IR
has no independent global mutability field and native placement currently
special-cases only string literals. This is a separate metadata/placement
gap, not another inline repair; the current survey's names-only predicate
is unchanged. In particular, no full readonly-data or expanded-boot claim
can be inferred from this targeted symbol-name result.

| Reduction | Construct | Current targeted result |
|---|---|---|
| `assembly-general-update.c` | General read/write asm output | Compiler emission and sealed native execution PASS |
| `assembly-tied-width.c` | Narrow output tied to wider integer input | Compiler emission and sealed native execution PASS |
| `assembly-tied-pointer-width.c` | Pointer input tied to a narrower integer output (`stkalign` shape) | RED E3021; shared explicit address conversion repaired it; both native assemblers PASS and six typed IR comparisons match explicit casts |
| `mixed-record-varargs.c`, `mixed-record-varargs-{driver,worker}.c` | Mixed record, unused floating representation | Both interpreters and emission PASS; 12 sealed native self/crossed controls cover 8/16/24-byte records with register exhaustion and indirect copies |
| `variable-array-objects.c` | Bound captures, indexing, scope re-entry/release | Both interpreters and native PASS; ended-object reads retain E3010 |
| `variable-size-type-query.c`, `variable-array-typeof*.c` | Dynamic `sizeof` and `typeof` bounds/operand scopes | Agreed type-name/array-expression forms PASS; pointer-to-VLA operand evaluation conflicts and retains E2904 |
| `variable-array-zero.c` | Automatic zero representation, padding and Null pointer subobjects | Both interpreters/native PASS; GCC O2 discrepancy retained, existing explicit option rule governs |
| `relative-label-table.c` | Same-function label-difference static table | C-HIR/EvoIR/MIR interpreters and native PASS; a narrowing-then-widening cast remains E3026 rather than being erased |
| `null-conditional-qualification.c` | Immediate qualifier discard after null conditional | Both interpreters and native PASS; oracle warnings retained |
| `atomic-read-modify-write.c`, `atomic-rmw-widths.c` | Legacy fetch/new-result RMW and CAS | Both interpreters, native and GCC/Clang mixed-link two-thread check PASS; explicit operation/order validation and metadata survive the changed gate |
| `integer-complex-values.c`, `integer-complex-operations.c`, `integer-complex-initialization.c` | Integer imaginary literals, values, conversion, arithmetic | Both interpreters and native PASS; two ABI self-controls and four crossed controls PASS |
| `integer-complex-profile.c` division | `(9+7i)/(2+1i)` | New measured oracle conflict: GCC 4+3i, Clang 5+1i at O0/O2; E2948 refusal, not FP or PASS |
| `ancestor-return-address.c` | Nonzero return-address query | Confirmed existing oracle conflict, E2913 retained; not an implementation RED |
| `constant-floating-representation.c` | Constant bytes in static/automatic floating subobjects, no floating reads | Both interpreters and two native assemblers PASS; existing source-literal wrapping is preserved, while runtime FP conversion, used floating results and invalid integer division remain refused |
| `wide-vector-control-flow.c`, `variable-array-label-scopes.c`, `representation-only-array-places.c` | Wide-vector copies/joins, VLA label re-entry, and representation-only array places | Already GREEN regression extensions; both interpreters and sealed GCC/Clang native execution PASS, not three new fixes |
| `vector-declaration-attributes.c` | Direct `vector_size` on declarations, comma lists, records and signatures | RED E2201/E3101; shared attribute transformation and independent canonical-type check repaired it; both interpreters/native PASS, original `pr123625` now exits 0 |

All ten reductions were compiled and executed with sealed GCC-14 and Clang-19,
each at O0/O2. Nine return 0 in all four modes; ancestor return address returns
1 under GCC and 0 under Clang, confirming the existing Appendix D ruling.
Commands and observations are in `tests/fixtures/language-semantics.out`.
The initial subject run was 0/10 with no build error; the ancestor positive
expectation was corrected to the specified refusal after that oracle audit.

The remaining E3101, GNU89 and build-option rows above still require individual
source-bound classification/reproductions. These ten are a first reduction set,
not a claim that every historical failure has already been explained.
GCC's failing HVA self-control stays oracle-unusable; conflicting tiny-vector
external ABI stays an explicit refusal; runtime `__builtin_isnan` remains in
the deferred floating-point bucket, not in an integer-only PASS numerator.

The follow-up native run also exposed GNU-as-only `:pg_hi21:` spelling in local
ELF address materialization. An ordinary ADRP symbol operand now preserves the
same relocation while working in both assemblers; Clang's originally failing
atomic mixed-link check passes without disabling its integrated assembler.
These are targeted results, not new measurements of the frozen 189 torture
compiler failures. Source-level classification remains open; no full benchmark
has been launched during this round.

The expanded small-case group is now **25/25**, with **46/46** inline-assembly
lowering tests, **23/23** typeof scope
regressions and **175/175** lifecycle/TLS/vector/atomic/call/query regressions.
These totals include negative cases pinning retained refusals; they are not
twenty-five newly passing benchmark programs. Textual shapes are **51/51**, zero
differences. This round's final changed-code gate against `9f571ca` passed **2,438
tests / 0 failed / 4 historical ignored**, in 301 result groups across 16
affected/dependent crates; Clippy `-D warnings` and fmt passed. The ladder
Python tests passed **148/148**, Ruff and `git diff --check` were clean.
The final gate includes the pointer-tie/direct-vector repairs; fresh layout
observations also match the frozen **128/128** base and **22/22** attributed
cases. These are targeted/dependency checks, not a full benchmark replay.
The original targeted programs `stkalign`, `pr126405`, `20221006-1`,
`20031214-1`, `20030216-1`, `920929-1`, `pr123625`, `pr94524-1`, `pr94524-2`
and `20041124-1` produce 20 zero exits across the two native assemblers.
These are default/projected-command checks, not a full optimized campaign:
in particular `20030216-1`'s `__OPTIMIZE__` branch is inactive, and the LLVM
adapter's recorded omitted options remain omitted. No old numerator changes.

The source-position audit distinguishes the historical E3101 families:
floating parameters/results/va_arg, actual floating arithmetic or object
reads, constant representation-only stores, and invalid legacy call/return
forms. In particular `20020225-2`, `930930-2` and static-relocation row
`20031214-1` have the storage-only reduction above, not an arithmetic-FP waiver.
`920501-1`/`921202-1`/`921208-2` call a zero-parameter definition with arguments;
`921202-1` also reads uninitialized values and `921208-2` performs FP.
`920728-1`'s bare non-void return is isolated in `legacy-discarded-return.c`:
GCC accepts it but Clang rejects it even in GNU89, so acceptance is not a shared
profile requirement. Original raw results are unchanged pending actual replay.

The baseline for this extension is `9f571ca`, with the same release-driver
SHA-256 listed above. The initial batch keeps that binary unchanged.
The first repair retest uses driver `607934eeed76306920ea07644acda53bf268155b0482c23fae470225d9783245`
(distant block-address emission); the next uses
`3962928d6d8238f8c521b06e5d4f6dfa4e4b4f26aef2ce72d7ce96674a0a4393`
(the full-barrier builtin as well). The final LLVM retest uses
`cdfdc2c676572bfd61e0dfcabe483d7a8dabd23d2ed48291cd77a2e50431f01b`
(the global-value propagation repair). No binary is replaced during its batch.

| Subject and pinned source | Original test consumer | Measured scope |
|---|---|---|
| [jq 1.8.2](https://github.com/jqlang/jq/releases/tag/jq-1.8.2) | `make check V=1`, including bundled Oniguruma | 69 captured C compilation units, no source exclusions |
| [Lua 5.5.1 and matching tests](https://www.lua.org/tests/) | public `all.lua`, no `_U`, `_soft` or `_port` overrides | 34 Lua units; five dynamic-loading helper libraries remain reference-built; optional internal `T` module is not built |
| [SQLite 3.53.4 canonical source](https://www.sqlite.org/download.html) | `testfixture test/testrunner.tcl --jobs 2 veryquick` | original public Tcl population, per-file isolation; not TH3 or all SQLite configurations |
| [LLVM test-suite](https://llvm.org/docs/TestSuiteGuide.html), `llvmorg-23.1.0`, commit `2919e55176a572ee085cb46b291c47b7092c363e` | upstream lit reference-output checks, original inputs, 60 s per test | 1,735 SingleSource C programs inventoried from CMake; 141 non-C programs excluded by language, not outcome; MultiSource/External not run |

All four retain floating-point sources in their compilation denominator.
Neither an uncompiled source's reference executable nor a whole mixed suite
PASS counts as an EvoCC-compiled program.

### New measured issues and repairs

| Issue | First measurement | Repair / status |
|---|---|---|
| Large function label addresses use out-of-range `adr` | jq `vendor/oniguruma/src/regexec.o`; assembler reports `pc-relative address offset out of range` at several `match_at` labels in 42 MiB of assembly | FIXED: forward/backward RED, 29/29 focused MIR tests; original jq run `20260914T185027Z` now installs regexec and passes unchanged `make check`, raising coverage **57 to 58 of 69**. |
| Combined TLS/storage declaration | Historical jq `src/jv_aux.c:31`, `static __thread struct sort_cmp_state sort_cmp_state;`, E1001 | Reviewed TLS implementation passes targeted lifetime/ABI checks; unchanged original now reaches `__builtin_isnan` at line 77. Full jq installation/test rerun remains; no TLS property was dropped. |
| ELF `visibility("internal")` | Historical Lua: 21 E2943 rows at `LUAI_FUNC` / `LUAI_DDEC`, defined in `src/llimits.h:317` | Reviewed internal-to-hidden mapping implemented without changing C linkage/strength; targeted replay 10 assembled / 11 E3101, zero E2943. Full Lua installation/test rerun remains. |
| FP signatures and builtins | jq: 8 E3101 signature rows and 2 E2001 (`__builtin_isnan`, `__builtin_nanf`); Lua: 10 E3101 signature rows and 2 E2001 (`__builtin_huge_val`) | Observed FP frontiers; remaining source beyond them is not certified FP-only. |
| Full memory barrier | SQLite core `fixture-objects/085-sqlite3.o`, amalgamation line 30695: undeclared `__sync_synchronize`, E2001 | FIXED: RED followed by 11/11 compiler-hint tests and native exit 0; emits ordered volatile `dmb ish` with memory clobber, not an external call. Original retest `20260914T190334Z` passes the same suite but exposes the next atomic builtin; coverage stays 59/86. |
| GNU atomic load/store builtins | Historical SQLite core E2001 at `__atomic_store_n`, amalgamation line 31591; `AtomicLoad`/`AtomicStore` at 14982–14983 use `__ATOMIC_RELAXED` | Implemented with explicit atomic/order metadata, interpreter events and native instructions. Targeted original replay now stops at FP `__builtin_isnan`, line 36452; no full coverage gain is claimed. |
| SQLite noninteger values | 26 other units refuse E3101 in signature/value/store paths | Remain source-bound refusals; the generic code alone is not an FP waiver for an entire unit. |
| Global constant propagation across calls | LLVM `2020-01-06-coverage-010.c`: prints `x = 0`, expected `x = 5`; original lit FAIL | FIXED: the inliner counted only this function's mutations, mistaking file-scope `StorageClass::None` for an automatic local. Global assignments no longer enter those local constant facts. Direct, indirect and transitive calls, plus a local-shadow control, pass both interpreters; full retest `20260914T192632Z` passes the original program with zero prior PASS lost. |

Capture directories: jq initial `20260914T181023Z`, retest `20260914T185027Z`;
Lua `20260914T182735Z`; SQLite valid initial measurement `20260914T185226Z`,
all under `.evidence/ladder/<subject>/`. SQLite is **59/86 EvoCC objects**
in a passing build, **329,803 tests, 0 errors, 1,238 Tcl scripts**; its core
amalgamation is still reference-built, so this is not "SQLite compiled by EvoCC".
All three have zero symbol exclusions/failing substituted units; post-test
hash checks confirm that the original test targets did not rebuild installed
EvoCC objects. Receipts enumerate every object and omitted compiler option.

The initial atomic reduction was
`.evidence/builtin-probes/20260915-benchmark-frontier/atomic-access.{c,out}`:
both sealed compilers pass O0/O2 and use one aligned 32-bit load/store for the
relaxed mode. The reviewed implementation and expanded order/type/native
checks are now recorded in `tests/fixtures/scalar-atomic-access.out`;
SQLite's fallback nonatomic macros were not changed. The TLS control and visibility ELF probes are in
the same directory (`tls-plain.c`, `internal-tls.out`).

### LLVM results and original failing programs

Complete run `20260914T190715Z` actually attempted all **1,735 C programs**:
866 objects assembled, 869 refused, no assembler failures. The reference
Clang-19 lit run had 1,734 PASS and one FAIL (`gramschmidt`, about 60 seconds,
no completed time record). That failed reference stays oracle-unusable even
though EvoCC also refused its source. Consequently the independent subject
outcomes are **855 PASS, 868 compiler-refused, 11 runtime failures and one
oracle-unusable**. The 141 non-C programs are separately listed by identity.
The final mixed lit output's 1,723 PASS includes reference executables: it is
explicitly **not** 1,723 EvoCC passes. No installed object was rebuilt by make.

The largest observed refusal messages are 351 `expected-declaration`, 217
undeclared `abort`, 89 unsupported function-value types, 45 undeclared `exit`,
26 excess initializers and 21 unsupported expression forms. These are first
errors, not a determination that all such programs are FP or outside GNU11.
The source/command/diagnostic of every row is in that run's `results.jsonl`
and `compile-commands.json`. In particular, the original Clang commands allow
several legacy constructs by suppressing diagnostics; this adapter does not
silently add GNU89 or `-fwrapv` to EvoCC to increase its numerator.

| Original program (under SingleSource) | Measured cause / disposition |
|---|---|
| `UnitTests/2020-01-06-coverage-010.c` | Global read folded to the pre-call value. Reduced to `tests/fixtures/addressed-local-before-loop.c`; RED returned 1 instead of 0. Fixed by restricting automatic-local assignment facts to non-file-scope objects. Both interpreters and native now return 0, as do GCC/Clang O0/O2. The unchanged original lit test passes in full retest `20260914T192632Z`. |
| `Regression/C/ConstructorDestructorAttributes.c` | Historical failure printed neither `Create!` line. Reviewed lifecycle implementation now prints both in the unchanged original's targeted native replay; full-campaign totals above remain historical. |
| `UnitTests/Vector/divides.c` | Historical scalar-layout and lane-execution defect repaired under the reviewed integer-vector profile. The unchanged original now prints both expected `<1 1 1 1>` vectors in targeted native replay; this is not an inferred full-campaign gain. |
| `Regression/C/gcc-c-torture/execute/pr85156.c` | Historical side-effect failure repaired under revision 1.9.98. Targeted unchanged native default/optimized runs pass; complete LLVM campaign not rerun. |
| `Regression/C/gcc-c-torture/execute/20090113-1.c` | Reads the never-initialized `adata[9]` through `msum_i4`; subject traps. Source evidence recorded, but no dual-sanitizer initializedness measurement: leave unresolved rather than label PASS or waive by exit signal. |
| `Benchmarks/Shootout/nestedloop.c` | `x++` overflows signed int on the original input. Both compilers' UBSan instrumentation reports `2147483647 + 1`. |
| `UnitTests/2003-05-26-Shorts.c` | UBSan: `-976897885 * -976897885` overflows int. |
| `UnitTests/2003-07-06-IntOverflow.c` | UBSan: `2097152 * 2097152` overflows int. |
| `UnitTests/2003-07-09-LoadShorts.c` | UBSan: `-976897885 * -976897885` overflows int. |
| `UnitTests/2003-07-09-SignedArgs.c` | UBSan: `-128 * -1899251712` overflows int. |
| `UnitTests/SignlessTypes/shr.c` | UBSan: right-shift count 73 on a 32-bit int. The profile's GNU signed-left-shift rule does not legalize an invalid count. |

The last six rows have positive undefined-behavior evidence, not merely exit
133. `llvm-ubsan.out` and `llvm-clang-ubsan.out` in the probe directory record
original source hashes, commands and diagnostics. Diagnostic-only flags are
`-O1 -fsanitize=signed-integer-overflow,shift-exponent -fno-sanitize-recover=all`;
the benchmark itself retains its original flags. The sealed Clang UBSan
archive is absent, so its independently instrumented object was linked by
GCC against the installed UBSan runtime; that adaptation and the first link
failure are recorded explicitly. These six raw failures remain visible in
the benchmark; R01 permits their evidence-backed non-applicable disposition,
not removal of compiler obligations or counting them as PASS.

`llvm-attributes.out` records both sealed oracles at O0/O2 on the unchanged
constructor/destructor and vector programs plus two minimal reductions.
Both reductions originally returned 1 under EvoIR and 0 under both oracles.
The reviewed 1.9.101 implementation replaces those preserve-only semantics;
the targeted round above records their repair and unchanged original output.
Atomic/TLS/visibility results likewise retain their own measured populations,
without substituting them for the old full-campaign totals.

Final full LLVM retest: unified run `20260914T192629.586877Z`, child
`20260914T192632Z`, command
`python3 -u tools/bench.py llvm-test-suite --evocc target/frontend/release/evocc --jobs 2`.
All 1,735 identities match the first measurement. **856 PASS, 868 refused,
10 runtime failures, one oracle-unusable**; the only disposition change is
`2020-01-06-coverage-010` from runtime failure to PASS. No old PASS was lost,
no object was rebuilt, and the reference remains 1,734 PASS / one FAIL.
The mixed lit count is 1,724 PASS / 11 FAIL, not the subject numerator.
This completes the requested four measurements and three repair/retest loops,
not all remaining language/ABI work. The overall benchmark verdict remains
**INCOMPLETE** at that checkpoint; the subsequently reviewed extensions and
their separate targeted implementation results are recorded above.

Validation after the compiler repairs:

- `CARGO_TARGET_DIR=target/frontend CARGO_BUILD_JOBS=2 CARGO_NET_OFFLINE=true tools/gate-changed.sh 9f571ca --offline -j 2`:
  **1,868 passed / 0 failed / 4 ignored**, over the nine changed/dependent
  crates, followed by Clippy `-D warnings` and fmt **PASS**. This is the
  changed-crate closure, not a new full-workspace count; the four existing
  ignored long campaigns are unchanged, not counted as executions.
- `python3 -m unittest discover -s tools/ladder/tests`: **148 PASS**.
- `ruff check tools/bench.py tools/ladder`, `git diff --check`: **PASS**.
- `python3 tools/check-textual-grammar.py`: **46/46 shapes, 0 differences**,
  subject to that tool's documented limitations.
- No full clone or offline workspace copy was made. The new LLVM cache uses
  under 1 GiB at this checkpoint; sources/downloads are reused rather than
  copied into repeated private workspaces.
- All four benchmark processes and the final changed-crate gate have exited;
  no benchmark is left running by this batch.

### Reference-environment findings (not EvoCC failures)

- Lua's upstream REPL test needs the development `libreadline.so` name;
  the sealed image has only `libreadline.so.8`. A container-local symlink
  supplies that name. The public file test needs non-seekable standard input,
  supplied as an empty pipe instead of the container's seekable `/dev/null`.
  Neither test nor source is changed.
- SQLite refuses execution as root, so tests run as `nobody` in a fresh
  container-local copy of the built tree. A first non-root serial run
  (`20260914T181859Z`) executed **332,097 tests, 1 failed**: `symlink.test`
  left directory `x`, changing the subsequent `zipfile-25.0` error message.
  SQLite's own per-file-isolated `testrunner.tcl` on the same `veryquick`
  test-file set then passes 329,803 tests. These are different driver counts,
  not the deletion of a failed assertion.
- SQLite attempt `20260914T184616Z` had a harness-only false link failure:
  a trailing shell comment in `relink` swallowed the shared runner's status
  capture. Its reported 58/86 is invalidated, not a compiler regression;
  removing the comment and pinning it in a regression test yields the valid
  59/86 run above.
- An initial all-language LLVM reference build was killed while Clang was
  compiling C++ `matrix-types-spec.cpp` (137). The C compiler measurement
  now inventories C versus C++ from the full compilation database before
  building; all C programs, including FP ones, remain in scope. The failed
  attempt is retained as a reference-build error, not an EvoCC refusal.
- LLVM's first C-only outer-Makefile invocation repeated the complete CMake
  dependency traversal for each of 1,735 targets. Its deliberate interruption
  (`20260914T182816Z`) is not a test result. The adapter now invokes the same
  generated Makefile2 recipes in one shared graph, without changing inputs.
- LLVM attempt `20260914T185601Z` built the C population but did not run tests:
  the pinned lit wheel exposes `lit.main:main`, not `python -m lit`, and its
  per-test timeout needs psutil. Both are fixed and pinned locally; one original
  `Shootout-hello.test` passes as an adapter smoke check, not as the full result.

### Reference-suite failures are still failures

The old toybox inventory counted 56 distinct names, not a full suite: the
September 23 audit above invalidates that acceptance claim and records all 57
prefix occurrences. Musl retains 17 libc-test failures on both builds. Its captured
`baseline-REPORT` names the missing `_PC_TIMESTAMP_RESOLUTION` and
`_SC_XOPEN_UUCP` API constants; static-build dynamic-loading/TLS failures;
`strptime` parsing and `strtold` rounding differences; `wordexp` shell/output
differences; `powf`; daemon timeouts; and pthread-atfork errno failures.
These require baseline/configuration or source-level resolution before an
absolute suite PASS. `libc-test-clean` in this adapter means **no additional
failures relative to that baseline**, not zero failures. No test is dropped.

### Reduced wrong-value evidence

`.evidence/builtin-probes/20260915-benchmark-frontier/expect.c`:

```c
int x, y;
int f(int z) {
    if (__builtin_expect(x ? y != 0 : 0, z++)) return 7;
    return z;
}
int main(void) { x = 1; return f(10) != 11; }
```

GCC-14 and Clang-19, each at `-std=gnu11 -O0` and `-O2`, exit 0 in
`evo-flight-lab:sealed`. EvoCC assembly linked by `gcc-14 -no-pie` exits 1;
`--interpret-evoir` prints 1. Commands, versions, source and driver digests
are in `expect-results.txt` beside the probe. At that baseline §10.1.14 said the
second operand is not evaluated and claims oracle agreement. This evidence
contradicted that sentence. That measurement preceded the review; revision
1.9.98 and the targeted repairs above now resolve the conflict without changing
the original benchmark or applying an always-evaluate rule to source ICEs.

`protected.c` and `protected-results.txt` beside that probe record the second
body conflict, including the original musl replays and actual ELF visibility.
The compiler's stricter attribute validation may be correcting an earlier
silent omission; loss of coverage is real, but restoring the omission is not
the proposed fix.

`coverage-results.txt` records the array-type `sizeof`, defined-condition
qualifier-discard, and integer-record-varargs control probes. The control
prevents overgeneralizing the remaining mixed-member ABI refusal.

## Complete inline values — 2026-09-23 targeted follow-up

Two new small explicit-always-inline cases independently reproduce E3101 on
the prior release: a floating builtin retains a foreign SSA operand after a
later argument opens CFG blocks, and an integer-complex parameter/result uses
the generic scalar conversion/assignment path. These are **not FP deferrals**.
Both shared paths now retain explicit value transport and complete complex
pair semantics, without weakening validators or initializer admissibility.

The two fixtures plus a bounded private-leaf reduction pass both interpreters
and 18 native GCC14/Clang19/EvoCC O0/O2 runs. All 1,812 affected tests, Clippy
and fmt pass (three historical ignored). This closes those reproduced E3101
subcases, not the entire historical T01 audit or a new application campaign.
[Detailed record](../../tests/fixtures/private-leaf-substitution.out).

## Additional atomic demand inventory — 2026-09-23

TODO L11 is now an inventory closure, not an atomic feature claim. SQLite's
captured paths use the already supported scalar load/store operations. Broader
`_Atomic`, `__atomic` RMW/CAS and generic aggregate operations occur in TinyCC's
separate runtime/test sources and zlib's nondefault dynamic-CRC configuration,
not the presently captured 11-object/default-library populations. Compiler
keyword strings and generated source duplicates do not create new demand.

Three bounded C controls pass GCC14/Clang19 at O0/O2 (12 native runs); EvoCC
continues to refuse all three at their named boundaries, verified across three
CLI modes. The existing scalar/legacy atomic controls remain green. Full scope,
source locations and limitations are in
[the inventory](../../tests/fixtures/atomic-scope-boundaries.out); L11a–L11c
track the missing capability contracts. No benchmark numerator changes.

## Tooling changes and verification

The new Mbed TLS adapter reuses `tools/ladder/subject.py`: original reference
build, original selftest/CTest, actual EvoCC object substitution, relink and
rebuild detection. The official pinned release includes its submodules and
generated files. A checksum-checked portable CMake 3.31.8 supplies the missing
build tool without modifying the sealed compiler image. The adapter does not
disable tests or claim all TLS configurations are covered.

The first Mbed TLS attempt (`20260914T163312.634305Z`, child
`20260914T163315Z`) is retained as **ERROR**. Its reference selftest and
148 CTest tests passed, but the shared decoder recognized `make` and not
`gmake`: several root-build recipes consequently lost their `build/` object
prefix, and installation failed before any subject-test result. A nested
directory RED now covers that spelling. After the two-regex repair, all
382 captured Mbed TLS object paths exist; decoded commands for the completed
zlib/zstd/cJSON runs are exactly unchanged (19/44/2 respectively). Only Mbed
TLS is rerun; the failed attempt is not relabeled as a successful run.

The next attempt (`20260914T164024.340693Z`, child `20260914T164026Z`)
passed the original selftest and all 148 CTest tests with 88/382 objects
substituted, with zero rebuilt or symbol-excluded EvoCC objects. Inspection
identified three of its E0003 rows as one **harness path defect**, not a
compiler refusal: CMake's `build/programs/test/query_config.c` symlink points
at the container-only absolute `/work/mbedtls/.../programs/test/query_config.c`.
The shared runner now rebases only in-tree absolute links to equivalent
relative links before the reference test and host compilation, records each
mapping, and leaves source bytes and external links unchanged. A RED pins
readability, unchanged bytes, external/escaping-link preservation and
idempotence. The corrected run supersedes that incomplete coverage reading,
without altering its original capture.

The corrected run `20260914T165259.428885Z` / `20260914T165302Z` installs
**91/382** EvoCC objects, retains **291 E1001** refusals, and has **0** rebuilt,
symbol-excluded or failing substituted objects. The GCC baseline, first mixed
trial and final installed-object trial each pass selftest and **148/148**
CTest tests. `adjustments.portableBuildSymlinks` records the 27 equivalent
in-tree link mappings. The unified command exits **1 / INCOMPLETE**, correctly:
a mixed test PASS does not close the full 382-object milestone.

- Adapter/unit suite: `python3 -m unittest discover -s tools/ladder/tests`:
  **136 PASS** after the directory and symlink repairs.
- `ruff check tools tests/acceptance`:
  **PASS**.
- `git diff --check`: **PASS**.
- `tools/gate-changed.sh 428980207a421cc3b8273a6a952600a20a82a535`:
  no crate changed; no Rust tests or compiler rebuild required.
- Real Mbed TLS subject execution: **selftest and 148/148 CTest PASS**;
  capture `.evidence/ladder/mbedtls/20260914T165302Z/`.

No full clone, Rust rebuild or workspace replay is needed to validate this
tool-only change. The existing K6/K7 results and unresolved module/BTF/config
items remain separately tracked in
[the kernel selftest ledger](kernel-selftest-gaps-20260914.md).

## Historical issue reconciliation — 2026-09-23 (TODO T01)

[The identity crosswalk](../../tests/fixtures/language-issue-reconciliation.out)
joins all earlier named asm, GNU89/prototype, VLA, relative-label/representation,
varargs, qualifier and E3101 cases to checked-in regressions, measured later
originals or specific open tasks. It is not a new campaign or claim that the
compiler has only floating-point work left.

Of the old torture run's 114 E3101 rows, **107** had been compiler failures;
the other seven were already dispositioned. Those 107 map to **88 PASS,
15 named FP deferrals (14 binary128, one floating-vector), four evidenced
not-applicable** in the complete September 19 run. The two E3101 diagnostics
remaining in that whole later run are a non-void bare return and an extreme
array-layout conflict, neither a generic FP waiver. Existing classification
code revalidates each old identity against the source/hash/evidence rules.

All **148** historical LLVM source hashes match the retained pinned sources.
The later complete run has **119 PASS, 16 compiler failures, ten named FP
deferrals, two evidenced exclusions, one unusable reference** for this subset.
All sixteen failures are already covered by B01: three subsequently pass
targeted replays, twelve receive precise FP deferrals, and LARGE
`floyd-warshall` remains B02. Historical totals are not rewritten by this join.

Current targeted regressions are **63 language-semantics + five private-leaf
tests PASS**, zero failures/ignored. Original c-testsuite 00140/00144/00207 and
the previously named ordinary VLA/asm/relative-label torture programs passed
their cited full replays; 00144's uninitialized condition is not reused as a
semantic oracle. The defined-condition qualifier fixture supplies that check.
The crosswalk retains all 262 historical E3101 identities and points to L02,
L05, L07–L11 and B02/B10 rather than reopening already repaired families.

## Compared-value machine selection — 2026-09-23 (TODO B02c)

The [minimal C/IR cases](../../tests/fixtures/compared-value-csel.out) close the
bounded CMP/CSEL selection repair. A single-use full-width bit mux over the
comparison's already evaluated operands becomes an adjacent compare/select;
all guards, effects and local SSA constraints remain. Ninety-six structure and
allocation checks, 600 exact MIR comparisons, seven CLI tests and twelve sealed
native controls pass. The affected gate is 980 PASS, zero FAIL, four historical
ignored; Clippy and fmt also pass. EvoMIR 0.36 records this machine selection.

The unchanged original LARGE Floyd workload still times out at 60 seconds,
despite 108 → 102 core instructions. B02 is OPEN; no reduced workload, source
change, unchecked policy or kernel runtime claim substitutes for that result.
Release/debug emission is byte-identical for the replayed original.

## 2026-09-27: hosted binary128 and complete jq replacement

The existing common benchmark lane reruns unchanged jq 1.8.2 with the original
bundled Oniguruma configuration. Final run `20260927T082205Z` compiles and installs
**69/69 target-C units**, including `src/builtin.o`'s real `nexttoward` binary128
interface. There are zero refusals, unmeasured units, exclusions or make-rebuilt
target objects. Original `make check V=1` passes **9/9 jq scripts**, **7/7 Oniguruma
test programs** and **14/14 sample programs**, with zero reported failures/skips.
Those are program counts, not a claimed count of every internal assertion.

Compiler SHA-256 is
`523a698237c9ab72f868fdd69b7d1a1f6e0fd3285694d986b3bf007dee691eaa`.
The prior successful run `20260927T080457Z` exposed an omitted semantic flag:
`-pthread`. A minimal driver/projection RED and fixed ARM64 macro controls now
establish `_REENTRANT=1` with explicit user override precedence; final replay
retains the flag. Diagnostic/dependency-file/debug options still listed as
omitted are not promoted into debug-information or warning-conformance claims.
The hosted native quad numeric/ABI and both-interpreter tests are recorded in
[the FP implementation ledger](strict-floating-implementation-20260927.md).

This closes B07 and the admitted hosted Linux L02 profile. It does not rerun the
other applications, kernel tests or B10's final frozen campaign. The original
c-testsuite `00204.c` also passes a targeted expected-output replay, but the
historical 219/220 full-suite receipt is unchanged until a fresh full run.

The follow-up full **c-testsuite run `20260927T082713Z` is 220/220 PASS**,
zero deferrals, exclusions, unknown outcomes or adjusted source parameter lists.
The targeted `00204.c` source SHA-256 is
`cfa12d12c391f845fbc18176b14ae8f169cd680e1971902b10a4ac6f556530ea`;
its upstream expected-output hash is
`6197bddf75d156ab3ff383a78c6476de788c973f91a2ee6d411a99d32b34e940`.
Old FP disposition records stay historical, but binary128 and strict-fenv are
removed from the current refusal-waiver set; a fresh failure cannot be hidden
behind an implemented capability. Mutation tests enforce that distinction.

**TinyCC run `20260927T083017Z` reaches 11/11 installed units**, no refusals,
symbol exclusions or unchanged-object audit failures. `tccgen.o` and `tccpp.o`
now use real quad support. Reference and complete EvoCC builds pass unchanged
upstream `make test`. The original target-selected Makefile still excludes
some architecture-inapplicable tests; this does not claim every TinyCC target.

Before replay, a five-case local-shell reduction exposed three false successes
in the old runner: relink failure with a surviving executable, nonzero test exit
with a success marker, and a malformed marker. The runner now requires both
zero exits and the exact upstream completion line; it also rejects a changed
or missing installed object before/after execution. Separate container-exit
and object-mutation negatives pass. The shared object audit reuses toybox's
established check. **260 ladder tests and Ruff pass**. The receipt retains all
eleven expected hashes, the final executable hash and per-trial log hashes;
it is not based only on the phrase `ALL TESTS PASSED`.

## 2026-09-27: complete torture binary128 replay

Run `20260927T084053Z` uses the same frozen `523a6982...` compiler and all
**1,692** unchanged source programs. It yields **1,522 PASS, zero compiler
failures, 21 named FP deferrals, 146 not-applicable and three unusable oracles**.
Exactly the eighteen historical binary128 refusals change to PASS; no other
program changes its outcome or disposition from the prior complete run.

The raw runner observes 1,523 successful exits, 102 refusals, 61 applicability
exclusions and six failing executions. One successful exit still has a
source-bound not-applicable disposition, so the accepted PASS numerator is
1,522, not 1,523. The six raw failures remain the already-evidenced signed
overflow cases, inactive-union-tail conflict and two wide-bitfield promotion
conflicts. Their source hashes/evidence are rechecked; they are not relabeled
PASS. Remaining named FP deferrals are floating complex/vector, not binary128.
The original options, fixed time budgets and target predicates are unchanged.
This closes the original torture quad population, not the other B10 lanes.

## 2026-09-27: complete toybox execution and remaining non-passes

Full run `20260927T084656Z` compiles and installs **203/203** frozen units with
the same subject, zero refusals, symbol exclusions or rebuilt objects. Both
reference and subject complete **1,669 occurrences: 1,648 PASS / 1 FAIL /
20 SKIP**. Generated applet-table `main.c` stays reference-built outside the
frozen denominator; this is not an all-C-object replacement claim.

The [small prerequisite and tar reductions](../../tests/fixtures/toybox-runtime-prerequisites.out)
repair six shared environment failures: absent USER, missing disposable-runtime
hostname/mount permissions, and an overlong loop backing filename. The shared
runner keeps original tests/flags, binary/object hashes and full occurrence
accounting. Eight native prerequisite controls, the five-collection replay and
the fresh full pair agree; 261 ladder regressions and Ruff pass.

The sole remaining FAIL is `tar ownership`: a later expected digest differs
from an earlier expectation for the same empty-file command, even though both
commands explicitly override owner/group. Both audited executables produce
byte-identical complete archives before/after the chown. No assertion is edited;
the common bench still returns FAIL. Twenty skips are individually attributable
to ten disabled applets, four explicit nonroot guards, two >5-CPU guards, one
unconditional upstream performance skip, two unavailable filesystem flags and
the absent dummy network module/device. The fixture record retains exact errors.
This closes B06's compilation/full-execution/diagnosis task, not all-tests PASS.

## 2026-09-27: complete LLVM SingleSource replay

Run `20260927T085137Z` uses the frozen `523a698237c9...` subject and all **1,735**
original C programs. Final accounting is **1,039 PASS / 1 compiler failure /
30 FP-deferred / 664 not-applicable / 1 oracle-unusable**. All **1,048** assembled
objects are installed and survive the original relink; none is rebuilt by make.

Compared with the old complete run, **25** programs newly pass: nineteen former
compiler failures and six former FP deferrals. The 44 former compiler failures
now split nineteen PASS, nineteen precise deferred-feature dispositions, five
source-bound not-applicable dispositions, and the one still-open integer-vector
`pr23135` case (L10). Existing dispositions are checked against unchanged source
hashes and original commands. In particular, LARGE Floyd-Warshall, LU and LUDCMP
pass the full lane without changing their original inputs or 60-second budget.

The reference has 1,734 PASS and one unchanged Gram-Schmidt FAIL. The overall
lit invocation prints 1,726 PASS / 9 FAIL because it also executes executables
whose target objects EvoCC refused; **those reference-built executions are not
EvoCC PASS credit**. Compiler accounting credits only the 1,039 validated subject
programs. Of the eight subject runtime failures, seven retain evidence-backed
source-definedness exclusions and `pr23135` remains unwaived. This is INCOMPLETE,
not all-program PASS, and does not close the final B10 campaign.

## 2026-09-27: retained no-alias option and fail-closed library replays

The driver and application/Kbuild command projections now retain
`-fno-strict-aliasing`. It selects EvoCC's existing conservative access semantics,
not a new type-based alias analysis. The positive `-fstrict-aliasing` option
remains unsupported; provenance, lifetime, initializedness, bounds/alignment and
read-only-object checks are unchanged. EvoIR 1.9.113 records this bounded rule.
The [small aliased integer/FP accesses](../../tests/fixtures/no-strict-aliasing.out)
pass both interpreters and **4 reference / 8 subject native combinations**;
const-write negatives still produce E3014. All **530 affected Rust tests** pass,
with no failures or ignores, plus Clippy/fmt. Eight original musl commands retain
the flag: four assemble and four still refuse scalar FP inline asm (B08a).
This is not a full musl build or a waiver of its other omitted options.

Separate minimal shell and Python tests expose musl adapter false successes:
nonzero make exits with stale artifacts, incomplete case inventories, object
replacement and a cached final passing set. The adapter now verifies a fresh
final relink/test, original runtime inventory, unchanged installed objects and
watchdog, and matching rebuilt/installed libc archives. Ordinary shared test
failures remain failures. The original full musl campaign has not been rerun.

A controlled-timestamp cJSON reduction also catches reuse of a previous bad
archive after good objects were restored. Removing generated archives at relink
fixes the dependency problem without changing sources/assertions. Fresh full
run **20260927T092953Z**, subject
`f21a796f169aa3c0f88b97d6cdccd31210af987f2c9c7b5c9de61bb8e6522c68`,
compiles/installs **23/23 objects**, with zero exclusions or make replacements.
Reference and subject pass the original Makefile/public tests. The five public
Unity programs have **54 PASS**; all 21 upstream Unity programs have **161 PASS,
zero FAIL, one upstream IGNORE**. Four disabled JSON-patch fixtures retain their
skip notices, and Unity remains reference-built. This receipt predates the
no-alias driver change above; identities are not conflated.
[Regression and replay record](../../tests/fixtures/benchmark-relink-boundaries.out).

## 2026-09-27: musl reference-failure ledger

The fresh reference build in `.evidence/ladder/musl/20260927T104940Z` has
**17 failing cases**. `baseline-REPORT` retains the actual diagnostics; they
are not converted to PASS when an EvoCC mixed build reproduces them.

| Cases | Count | Observed reason / remaining work |
| --- | ---: | --- |
| `api/main` | 1 | `api/unistd.c` requires `_PC_TIMESTAMP_RESOLUTION` and `_SC_XOPEN_UUCP`, absent from the pinned musl headers; its missing object prevents linking the test. Reconcile the pinned libc-test/API version, not compiler acceptance. |
| `dlopen`, `tls_align`, `tls_align_dlopen`, `tls_init_dlopen`, `tls_get_new-dtv` | 5 | This lane configures `--disable-shared` and links statically. Logs explicitly report dynamic loading unsupported or attempted static link of a DSO; some tests continue after failed lookup and segfault. Genuine DSO/TLS validation needs a separate shared-loader build. |
| `strptime` static/default | 2 | `%s` gives the wrong parsed date and `%z` rejects `-06` in the reference too. Source/library compatibility diagnosis remains open. |
| `strtold` static/default | 2 | Two exact hexadecimal binary128 rounding expectations differ in the reference. Retain expected and actual bits; a library/compiler/root-cause attribution is not yet established. |
| `wordexp` static/default | 2 | Reference results expose positional arguments and numerous shell syntax/error differences. The log identifies shell behavior; it does not by itself establish which library/shell contract is wrong. |
| `powf` | 1 | Upward `powf(max_finite, 1)` reports INEXACT/OVERFLOW where the test expects no flags. Other `X`-prefixed math observations remain upstream expected-failure records, not extra compiler failures. |
| `daemon-failure` static/default | 2 | Original five-second supervisor times out in the reference. Root cause remains unassigned; this is not a timing waiver. |
| `pthread_atfork-errno-clobber` static/default | 2 | `fork` succeeds despite the test's resource-limit setup; the expected failure is not induced. The privilege/limit/environment cause still needs a focused probe. |

These counts describe the recorded reference failures, not all possible libc
defects. B08d tracks closure; no test source, assertion, or watchdog was changed.

### Targeted environment and assembler repair (September 27)

The [small native matrix](../../tests/fixtures/musl-reference-boundaries.out)
contains 96 completed build/link/run records and 16 unchanged-program executions,
not a new full campaign. It establishes root's RLIMIT_NPROC exemption as the
atfork test failure's cause. The adapter now uses upstream RUN_WRAP to run just
that regression's two variants as uid/gid 65534, identically in both compiler
lanes. All four original-variant/archive combinations pass through the actual
wrapper and unchanged reference supervisor; no assertion is removed.

The isolated original daemon case passes against both archives in about 0.126
seconds; million-fd exhaustion reaches EMFILE in about 0.12 seconds. The previous
timeout remains unexplained, not waived. The other four library cases reproduce
with both archives and all three callers: strptime's source deliberately ignores
the parsed `%s` value and requires four `%z` digits; wordexp passes two helper
arguments to `/bin/sh`, which its nested expansion exposes. Two strtold quad
rounding errors and upward powf's infinity/overflow remain numerical-library
investigations, not demonstrated EvoCC miscompilations.

The mixed assembler runner also had a real command-transport defect: it retained
`-Wa,` but dropped `-Xassembler`, then split its plan using the shell. Ordered
extraction plus JSON argv removes that loss; six real ELF controls verify stack
flags, overrides and an absolute symbol. A failed container cannot contribute
partial success verdicts. B08b still tracks the other tuning/diagnostic omissions.

### Genuine shared-loader supplement (September 27)

The [new supplement](../../tests/fixtures/musl-shared-loader.out) executes all
five original DSO/TLS programs with actual musl PT_INTERP loaders: **5/5 reference
and 5/5 candidate PASS**, repeated independently by the durable adapter. It
reuses 1,268 verified EvoCC PIC objects and newly compiles the two loader TUs.
The linked library still contains 68 reference-built floating-complex C objects
and 13 original assembly objects: **1,270/1,338 target-C**, separately from the
static lane's denominator. No source or installed static archive was changed.

The shared-loader adapter is called by the existing musl lane after static
validation/restoration and records its own composition, library hashes and exact
paired population. The common bench cannot claim complete musl with absent,
failed, or stale-compiler shared evidence. This closes the five-case static/DSO
coverage gap, not the remaining reference-library failures or full musl.

### Binary128 strtold source reduction (September 27)

The [hex-tail reduction](../../tests/fixtures/musl-hex-tail-reduction.out) now
explains both strtold failures: the pinned `hexfloat` captures 29 hexadecimal
digits for 113-bit long double, then substitutes half a final digit for every
nonzero remaining tail. With a leading 1 this creates two artificial ties;
both round to even ...1112, although one exact input is above the midpoint
(...1113 required) and the other below (...1111 required). An additional captured
guard digit distinguishes these two inputs. Four interpreter and six native
executions assert the old behavior and the diagnostic control. No pinned source
was patched; the original failures remain FAIL, not compiler regression or PASS.
