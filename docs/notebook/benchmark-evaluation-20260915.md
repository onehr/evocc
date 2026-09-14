# Roadmap benchmark refresh — 2026-09-15

Status: **ten available application lanes measured; overall INCOMPLETE**.
Five advertised roadmap lanes still lack adapters and are explicitly NOT RUN.
This is a benchmark and issue-inventory change, not a compiler fix. Floating
point is not the only remaining obstacle. Historical kernel boot and selftest
results are not rerun or promoted by this application benchmark batch.

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
| PolyBench, jq, Lua, SQLite | no adapter | NOT RUN | roadmap FP-dependent rungs; lack of an adapter is not a measured FP exclusion |
| llvm-test-suite | no adapter | NOT RUN | mixed integer/FP population; integer coverage is still missing |

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

Counts are first-error observations, not independent root causes. A program
can expose another class after its first error is repaired. References to
torture sources below mean the unchanged
`gcc/testsuite/gcc.c-torture/execute/<name>.c` in the pinned archive.

| Priority | Construct / remaining work | Fresh evidence and example | Required closure |
|---|---|---|---|
| 1 | `__builtin_expect` second-operand side effects; specification-body conflict | `pr85156`: EvoCC exit 134. Reduced `expect.c`: four oracle passes, EvoCC native exit 1 and EvoIR result 1. | Reconcile §10.1.14 with the measured original; RED for side effects, then original torture rerun. Do not weaken the assertion. |
| 1 | `visibility("protected")` admission and ELF semantics | musl has 97 E2943 rows; all 93 previously assembled units lost in this run now hit this code, including `fclose` and `pthread_create` through `stdio_impl.h:70`. The minimal probe passes GCC/Clang O0/O2 with `GLOBAL PROTECTED`; EvoCC refuses. | The current body admits only hidden/default: specify protected behavior and carry it to the object file. Do not restore silent attribute dropping or undefine `__PIC__` to bypass the real header. |
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
| 5 | Unmeasured roadmap populations | llvm-test-suite integer subset; PolyBench/jq/Lua/SQLite adapters. | Pinned sources, actual original tests and explicit FP/eligibility accounting. NOT RUN is not PASS. |

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
private-helper/CMake suite is still unmeasured. This is separate from the five
roadmap lanes without adapters.

### Reference-suite failures are still failures

Toybox's unchanged 56-outcome test inventory retains `chattr immutable` on
both builds. Musl retains 17 libc-test failures on both builds. Its captured
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
are in `expect-results.txt` beside the probe. §10.1.14 currently says the
second operand is not evaluated and claims oracle agreement. This evidence
contradicts that sentence. Neither the specification nor compiler was changed
to make the benchmark green.

`protected.c` and `protected-results.txt` beside that probe record the second
body conflict, including the original musl replays and actual ELF visibility.
The compiler's stricter attribute validation may be correcting an earlier
silent omission; loss of coverage is real, but restoring the omission is not
the proposed fix.

`coverage-results.txt` records the array-type `sizeof`, defined-condition
qualifier-discard, and integer-record-varargs control probes. The control
prevents overgeneralizing the remaining mixed-member ABI refusal.

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
