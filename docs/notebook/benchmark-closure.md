# Benchmark issue closure

Baseline: `e6c7938`; all-lane measurement `20260907T141450.311159Z`,
musl rerun `20260907T153648.272156Z`, compiler SHA-256
`b13d4a08539dfb6056e61f47631768357ced2e9f404247d7893593643c9ca6e6`.
The [evaluation](benchmark-evaluation.md) and its linked raw records are the
inventory authority. Counts below overlap where a row is a subcategory.

## Execution and closure rules

Each semantic family gets a minimal failing regression, a shared-path fix,
focused tests, `tools/gate-changed.sh`, and an incremental measurement through
`python3 tools/bench.py <affected lanes>`. A focused PASS alone is not closure.
Keep unavailable references, unsupported programs and baseline failures visible.
No compiler changes are justified merely by reducing a diagnostic count.
Do not rerun all lanes after every patch; the final all-lane run follows the
incremental closures. Floating-point execution remains deferred by the user's
standing instruction; FP-dependent milestones cannot be called complete.

Order is by correctness first, then shared-path reach. Small independent
wrong-result fixes precede changes that require new machine constraint plumbing.
Only one heavyweight benchmark invocation runs at once.

| Order | Issue family | Baseline and closure requirement | Incremental bench | State |
|---|---|---|---|---|
| 1 | Macro push/pop state | `00206` wrong stdout and `pushpop_macro` abort; restore saved definitions, including undefined and nested states; original programs pass | `ctest torture` | SPEC_BODY_CONFLICT: Section 10.4 makes other pragmas ignored-safe |
| 2 | Object aliases and weak symbols | `alias-2/3/4` undefined `b`; musl 167 symbol-parity exclusions require subtyping, not assuming one cause; pin object symbol tables and native alias accesses | `torture musl` | SPEC_BODY_CONFLICT: preserved-only rule, already recorded in Appendix D |
| 3 | Explicit register operands | Singleton `sigsetjmp_tail.lo` breaks two setjmp tests; syscall assembly does not preserve the specified register mapping; pin register/ABI semantics and native execution | `musl` | OPEN |
| 4 | Remaining native failures | ctest 2 failures; torture 35 runtime and 3 link failures remain; the one assembly failure now passes natively | `ctest torture` | PARTIAL: static label relocation CLOSED; other failures OPEN |
| 5 | Kernel substitution mismatches | Six named units now verify using their own ELF extents; whole configuration replay, mixed boot PASS, zero mismatches | `kernel` | CLOSED: `measure-20260907T170640Z`; overall kernel coverage remains incomplete |
| 6 | Assembly register alternatives | `Kr` or operand type rejects 2,745 kernel units; implement shared selection/validation and native operand tests | `kernel` | SPEC_BODY_CONFLICT: Section 3.11 explicitly refuses unlisted combinations; only `Ir` was admitted |
| 7 | Remaining kernel refusals | Frame address and computed goto, one unit each; preserve existing explicit deferrals until a supported implementation exists | `kernel` | OPEN |
| 8 | Return and edge typing | void return effects repaired: toybox 177 to 191/203, zero regressions; the remaining three edge-value refusals still need reductions | `toybox` | PARTIAL: return fix benched, edge typing OPEN |
| 9 | Other toybox lowering refusals | Remaining nine of 26 refusals: seven function-value types, one prototype, one object subvalue | `toybox` | OPEN |
| 10 | Unused system-header types | zstd 31 vector-type declarations, 7 `_Float32`, 4 pretty-function; cJSON 2 vector declarations; separate declaration admission from SIMD/FP execution | `zstd cjson` | OPEN |
| 11 | TinyCC compiler build | `arm64-link.o` and `tcc.o` relink failures; seven compile refusals across labels, places, function types and macro directives | `tinycc` | OPEN |
| 12 | Switch, labels, function prototypes | Named function addresses now pass original ctest 00095 and torture 941014-1; non-compound switch, unstructured labels and four no-prototype function-pointer refusals remain | `ctest torture` | PARTIAL: function designator addresses CLOSED; CFG and erased call signatures OPEN |
| 13 | Torture declaration coverage | 108 E1001 refusals, not 108 proven instances of one cause; classify source forms and fix admitted forms | `torture` | OPEN |
| 14 | Integer vectors | 23 excess initializers and 17 projected-place failures mention vectors; non-FP does not imply scalar; keep vector execution distinct from header admission | `torture` | OPEN |
| 15 | Variable-length objects | 19 torture refusals plus other lanes; explicit profile limitation, not a solved test or silent miscompile | `torture musl` | OPEN |
| 16 | Remaining diagnostic classes | Torture 478 total refusals and musl 354 remain exhaustively listed in raw results; classify the residual E3101/builtin/ABI/initializer classes after preceding fixes | `torture musl` | OPEN |
| 17 | Torture applicability/options | `dg-options`, target requirements and timeout factors are not consumed; define explicit applicability, preserve excluded outcomes, never claim ALL non-FP from a filtered success count | `torture` | OPEN |
| 18 | Unavailable Csmith references | 68 of 1,000 original seeds unavailable; 932 agree, zero mismatch; investigate termination/runtime without voting or deleting seeds | `csmith` | OPEN |
| 19 | cJSON subject test linkage | Existing `make test` recompiles reference sources; tests must consume substituted objects before a subject PASS is possible | `cjson` | OPEN |
| 20 | Reference baseline failures | musl 17 and toybox 1; separate environmental/applicability faults from subject regressions | `musl toybox` | OPEN |
| 21 | Missing milestone adapters | Mbed TLS, PolyBench, llvm-test-suite, jq, Lua, SQLite, full-kernel and kselftest: eight NOT_IMPLEMENTED entries; prerequisites and FP deferral remain visible, not silently waived | named adapters | OPEN |

## Progress

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
This remains an implementation issue, not a permission to weaken validation.

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
The other 67 unavailable inputs remain to be classified independently.

Final confirmation command (only after the applicable incremental checks):

```sh
python3 tools/bench.py all --jobs 4 --programs 500 --budget 40 --allow-qemu-drift
```

An incomplete lane, unresolved issue, or deferred prerequisite is reported as
such; it is never changed to PASS merely to reach an all-green summary.
