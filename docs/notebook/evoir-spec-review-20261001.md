Archived review transcript: Appendix D ("Exhitect review") of the EvoIR specification, revision 1.9.116 of 2026-10-01; removed from the specification at revision 2.0.0 and not normative.

## Appendix D: Exhitect review

### Target-selected wide characters

The reviewed hosted target uses unsigned 32-bit `wchar_t`, while `-fshort-wchar` selects unsigned 16-bit `wchar_t`; `TargetDataLayout` controls character values, string element types, decoding, initialization and predefined macros, and replay preserves the original option.
The two compilers at O0/O2 agree on `.evidence/builtin-probes/20260909-wide-character-target/{wide.c,results.txt}`: the mixed literal has four UTF-32 units (16 bytes) or five UTF-16 units (10 bytes), including the terminator, and an exact two-element `L"ab"` initializer omits the terminator.
In `character.c` and `character-results.txt`, the hosted non-BMP character is 0x1f642 in both compilers, while short GCC keeps a trailing surrogate and Clang refuses; the profile refuses that short scalar character with existing E1006 rather than truncating it.
The original-program acceptance targets are c-testsuite `00220` and the two previously measured short-wide-string kernel sites; their replay is separate from the literal regression tests, and no new diagnostic or floating execution is introduced.

### Pending body revision: numeric assembly label references

The current §3.11 reference vocabulary admits named `%l[label]`, not numbered `%lN`; the latest mixed-link survey records 2,419 original units refused at the numbered form.
`.evidence/builtin-probes/20260909-numeric-assembly-labels/{labels.c,results.txt}` records GCC-14 and Clang-19 at O0/O2 agreeing on numbered labels, multiple labels, multi-digit indexes and the extra implicit-input count for a read/write output.
No validator rule or source acceptance has changed; normalization to the existing named-label representation awaits the requested body ruling.

### Builtin calls and a same-named static definition: probe boundary

All four GCC-14/Clang-19 O0/O2 executions of original `921007-1` pass, but the nonconstant-input probe `.evidence/builtin-probes/20260909-local-library-binding/{binding.c,results.txt}` is an oracle conflict.
Both O0 executions and Clang-19 O2 call the local `strcmp` (result -123, one call), while GCC-14 O2 bypasses it (result -128, zero calls).
No binding rule is inferred from the passing original alone; the later literal-string rule below records the authorized implementation and the nonliteral refusal.

### C function-name spelling and storage-only floating declarations

The reviewed C profile resolves `__PRETTY_FUNCTION__` through the same function-local name object as `__func__`; this is an explicit choice of GCC's C spelling, not a claim that the oracle spellings agree.
Evidence `.evidence/builtin-probes/20260909-function-name-profile/{name.c,results.txt}` records GCC-14's `name` versus Clang-19's `const char *name(void)` at O0/O2; no original torture source asserts this spelling, while the last zstd and TinyCC measurements have five and one E2921 first sites.
An uninitialized floating object may have storage, an address and an unevaluated `sizeof` without executing a floating value; the same probe returns size 8 in all four runs and original c-testsuite `00178` requires only this behavior.
No new diagnostic is allocated and actual floating value execution remains deferred.

### Wide bit-field promotions and unsupported scalar storage order

The reviewed profile refuses integer promotion of a bit-field wider than `int` with E2939 (`unsupported-wide-bit-field-promotion`), while retaining storage, explicit conversions and fields no wider than `int`; `.evidence/ladder/e0/20260908T213308Z` and `docs/notebook/benchmark-closure.md` record the four original wide-field oracle conflicts.
The attribute `scalar_storage_order`, including its underscored spelling, is refused with E2940 (`unsupported-scalar-storage-order`), not preserved as an ignored semantic promise.
Evidence `.evidence/builtin-probes/20260909-storage-order/results.txt` records GCC-14 O0/O2 passing and Clang-19 O0/O2 ignoring the attribute then aborting for both originals; the two refusal classes affect four and two named original programs respectively, not predicted passes.

### Pending body revision: source-width constant arithmetic

The reviewed constant-context rule is supported by `.evidence/builtin-probes/20260909-constant-contexts/{constants.c,results.txt}`: GCC-14 and Clang-19 at O0/O2 agree on signed 32/64-bit literal add/subtract/multiply, the `(unsigned)(INT_MAX + 2)` shape from `920730-1`, and integer promotions.
The source-literal probes do not establish a rule for runtime variable arithmetic or optimized propagation; runtime overflow obligations remain unchanged.
No diagnostic is allocated and no implementation pass is claimed; the body revision is pending the architecture owner, with one named original program and unmeasured siblings.

### Executed source-constant arithmetic (2026-09-10 authorization)

The approved literal-expression rule also applies outside required-constant contexts: constant add, subtract, multiply and negation wrap in their promoted source type before surrounding casts or runtime operations, without propagating object values into constant knowledge.
Evidence is the earlier `20260909-constant-contexts/{constants.c,results.txt}` probe plus `tests/fixtures/constant-source-width.c`, independently executed with sealed GCC-14 and Clang-19 at O0/O2 (four zero exits, including 128-bit multiplication and surrounding side effects).
Runtime variable overflow, invalid shift counts, division by zero and executed signed `MIN / -1` retain their existing obligations; no diagnostic is removed or allocated.
This implements the user's authorization to record the missing body distinction here, affects the named original `920730-1`, and does not claim a new full-campaign result.

### Wide-field consumers and stored results (2026-09-10 authorization)

The narrowed profile admits precision-invariant comparison, truth, right shift, full-declared-width arithmetic with agreed signedness, and conditional arms whose values fit the common field precision; unsigned total compound arithmetic and increments/decrements also agree after truncation back into the field, as do signed compound operations with a full-width other operand.
A result that is unsigned under both field precisions, from add/subtract/multiply/bitwise arithmetic and compared with integer zero solely for its sign, is precision-invariant: this discharges only that operation's E2939, without deleting its evaluation, side effects or other pending diagnostics.
Evidence is `tests/fixtures/wide-bit-field-contexts.{c,out}` (sealed GCC-14/Clang-19, O0/O2) and the value assertions in `wide-field-consensus.c`; the three affected original torture programs are `991118-1`, `bf64-1` and `bf-sign-2`, with no new full-campaign count claimed.
This refines the body's broader compound/update refusal under the user's authorization, without allocating a diagnostic or weakening runtime overflow obligations.

### Precision must survive expression wrappers

E2939 remains required when a disputed wide-field value reaches narrow arithmetic through unary plus, a comma, a conditional, a right shift, or an assignment/update result; a wide shift-count type never widens the shifted value.
The same `wide-bit-field-contexts.{c,out}` probe measures GCC's zero versus Clang's `0x10000000000` in those nested additions, and `0xfffffffffe` versus `0x1fffffffffe` for a 40-bit value shifted by `1UL`.
A negative conditional arm converted to an unsigned field-precision result and narrow division/remainder assignments also disagree and remain E2939; admitting a stored unsigned add does not admit those different operations.
These corrections preserve the original refusal code, close the measured comma-wrapper acceptance hole, and do not infer a corpus count from the probes.

### Wide-field mixed signedness

An unsigned wide field combined with a signed operand can disagree even in comparison: the probe above gives GCC/Clang `0/1` for `< -1`, `1/0` for `== -1`, and `0/1` for `< -1L`; these comparisons retain E2939 unless the signed operand is known nonnegative or has a wider type that represents both full declared ranges.
With a signed operand of the field's declared width, arithmetic and conditional results can have different signedness, so those forms also retain E2939 rather than discharging a subsequent sign comparison.
The same sealed probe gives `ffffff0000000001/0` for division by `-1L`, `0/ffffffffff` for its remainder, and `1/0` for the negative wide sum's sign, at both optimization levels.
No diagnostic is added; this records measured boundaries of the authorized narrowing, not evidence that every comparison or full-width operand is safe.

### Absolute static integer addresses (2026-09-10 authorization)

Per S12, an integer address constant initializes pointer storage with its target bits and no symbolic relocation; each intermediate integer cast retains its own width and signedness before the pointer-width conversion.
Both sealed GCC-14 and Clang-19 at O0/O2 execute `tests/fixtures/absolute-static-addresses.c` with exit zero, including narrow signed/unsigned casts, 128-bit truncation, range overrides, function-pointer values, and scalar/bit-field union representations.
Static initialization preserves known symbolic pointer fragments and otherwise permits decoding a complete initialized pointer-sized representation as `ptr.from_int` would: no object or function becomes exposed merely by storing those bits, and ordinary runtime byte writes do not create this initializer marker.
No instruction or diagnostic is added; the existing E3013/E3016 access/call failures remain, and original torture `20021010-2` plus the previously surveyed absolute-address kernel shapes still require final campaign replay.

### Zero-byte objects and empty record arguments (2026-09-10 authorization)

Per S09, an object may have size zero with a nonzero power-of-two alignment and a live allocation identity: zero-byte initialization/copy accesses no bytes, while an actual byte access still reports E3011 and an ended lifetime still reports E3010.
Existing globals and `alloca` represent these objects, a zero-sized global has empty `bytes=` and `initialized=` fields, and `byval(0, A)`/`sret(0, A)` convey no argument/result bytes or alignment-filler registers.
Both sealed GCC-14 and Clang-19 execute `tests/fixtures/zero-sized-objects.c` at O0/O2 with zero exits, and caller/callee cross-links in both reference directions agree; the fixture includes an aligned empty argument after one integer, nine integer arguments across the register/stack boundary, empty returns, and static/local empty initializers.
No instruction or diagnostic is added, no equality between distinct zero-object addresses is promised, and the original-program gain remains unmeasured until the final campaign.

### Pending body revision: dynamic automatic objects

The reviewed VLA/alloca rules have positive GCC-14/Clang-19 O0/O2 probes in `.evidence/builtin-probes/20260909-dynamic-objects/{objects.c,results.txt}` for one-time bounds, recorded `sizeof`, prior parameters, scope exits and function-lifetime `alloca`.
Both compilers reject `goto-in.c`, which jumps into the scope of a variable-length object.
`parameter-bounds.c` in the same directory separately measures prototype-only bounds as unevaluated and definition-entry bounds as evaluated once, with all four runs reporting `prototype=0 entry=1 result=8`; no diagnostic is allocated or changed, and the body revision and original-program impact count remain pending.

### Integer aggregate variable arguments (2026-09-10 authorization)

Under S04, integer/pointer records pass by value using AAPCS64 general-register/stack slots through 16 bytes and a pointer to a caller-owned copy above 16 bytes; floating/vector/HFA records remain deferred, and the 4/16/24-byte and mixed-call probes are `.evidence/builtin-probes/20260909-memory-abi/{varargs.c,results.txt}` and `.evidence/builtin-probes/20260912-aggregate-varargs/mixed-objects-results.txt`.
An anonymous ordinary operand of `call` or `call.indirect` may append ` byval(size, align)` to its value reference; the pointer must be `ptr<generic>`, size may be zero, alignment must be a nonzero power of two, and neither fixed operands nor effect tokens may carry a call-site mark (`sret` remains signature-only).
`(%mem2, %world2) = va_arg.byval %list, %mem, %world, %destination [size=N, align=A]` copies the next record into an allocated `ptr<generic>` destination and advances the same list as scalar `va_arg`, preserving initialization/provenance and linear effects; the abstract interpreter checks the recorded record shape, distinguishing it from a scalar pointer and reporting existing E3038 on a mismatch or invalid list.
Both GCC-14 and Clang-19 O0/O2 callers and callees agree with EvoCC in both mixed-link directions (GNU assembler for the subject output); no diagnostic is added, no optimizer allowlist is extended, and original-unit gains are recorded only after replay.

### Whole-union volatile representation access

The approved full-kernel adjudication admits one exact-width representation read/write for a complete naturally aligned 1/2/4/8-byte volatile union, through Section 3.8's existing `copy` family with explicit read/write sides; selected-member authority is neither required nor invented, and larger/misaligned shapes retain E2922.
The shared memory transfer preserves every byte's initializedness and pointer fragments, while its world events retain unknown-byte information and record only the requested side once; it adds no fence, atomicity, MMIO or optimizer eligibility.
Both sealed compilers at O0/O2 run `tests/fixtures/volatile-union.c` successfully; `volatile-union.out` records O2 assembly's single source-width access and the exact frozen `CONFIG_LTO_NONE`/`rwonce.h`/`tasks.h:1992` source chain.
One original first-error unit, `kernel/rcu/update.o`, is affected; source acceptance, IR/MIR text/validator/interpreters, native execution and original-unit replay remain distinct measurements.

### Function addresses and byte arithmetic

Following the full-kernel adjudication, GNU function-pointer addition/subtraction and their compound/prefix/postfix forms use byte scale one through `ptr.to_int`, pointer-width arithmetic and `ptr.from_int.fnptr`, with the original lvalue and integer operand each evaluated once; no interior address acquires named Function provenance.
Function-pointer difference is the mathematical byte-address difference with type `long`, checked for signed representability (existing E3004); a reconstructed call retains the existing null, known-target and signature checks, and an interpreter's inability to resolve dynamic native code is not a claim that such code is invalid in native execution.
Both sealed GCC-14 and Clang-19 at O0/O2 execute `tests/fixtures/function-address-arithmetic.c` with zero exit, covering signed offsets, every update form, array/member/static places, single evaluation and a recovered real entry; the existing E3015/E3016/E3019 runtime boundaries are not weakened, and E3034 is withdrawn for this defined lowering.
The affected original units are `arch/arm64/net/bpf_jit_comp.o`, `kernel/bpf/verifier.o` and `kernel/bpf/core.o`; their actual outcomes are recorded only after replay, not inferred from this rule.

### Direct wide-field shifts: bounded declared-base profile

The full-kernel adjudication chooses the declared standard integer base type as the operation type when a direct (possibly parenthesized) bit-field read wider than `int` is the left operand of `<<`; storage width still controls later stores, not arithmetic precision, and this also applies to the read/shift/store of `<<=`.
After that type is selected, existing Section 6.1 GNU shift semantics apply: legal counts produce the low N bits even for negative signed left operands, while the independently decoded count retains its full width and invalid counts report E3005, never a newly generated E3006.
`tests/fixtures/wide-field-shift-probe.c` and `wide-field-shift.out` record GCC-14/Clang-19 O0/O2 disagreement for widths 33/40/52/58; this is an explicit Clang-style profile choice, not oracle consensus, and comma/conditional/unary-result/assignment-result precision is not broadened by it.
The frozen `include/linux/msi.h` uses a 58-bit `iommu_msi_iova` and 6-bit `iommu_msi_shift`; four original IRQ/MSI units are affected, with actual results counted only after replay.

### Computed branches approved; label differences remain separate

Both sealed compilers at O0/O2 execute the direct-label and relative-label dispatch tables in `.evidence/builtin-probes/20260909-memory-abi/{label-differences.c,results.txt}` with identical results.
The reviewer-approved 1.9.92 Section 3.9 now specifies complete indirect edges with local storage addresses and explicit effects, without relaxing local SSA; static symbol-difference tables are separate from that absolute-address dispatch rule.
E3048 names invalid dynamic branch targets after registry review; no original-program gain or native pass is inferred from approval or these probes.

### Integer absolute-value builtins and ordinary-call boundaries

`__builtin_abs`, `__builtin_labs` and `__builtin_llabs` have respectively the prototypes `int(int)`, `long(long)` and `long long(long long)`, evaluate their converted operand once, and return its nonnegative absolute value; a reached minimum signed value remains E3004 even with wrapping enabled, while existing arity/conversion/declaration diagnostics apply.
Compatible direct calls to the externally linked standard names `abs`, `labs` and `llabs` use the same operation by default, but `-fno-builtin` disables that ordinary-name recognition (`-fbuiltin` restores it, last option wins), never the explicit builtin spelling; internal functions, pointer calls and explicitly assembly-renamed functions retain their existing call/linkage semantics.
Sealed GCC-14/Clang-19 O0/O2 evidence is `.evidence/builtin-probes/20260908-integer-absolute/{values.c,runtime.c,override.c,internal.c,renamed.c,results.txt,runtime.out,renamed.out}`: value/type/one-evaluation and external override/disabled rows agree, ICE folding disagrees and is refused by the context's existing constant-expression diagnostic, and the internal/renamed disagreements do not override the body's existing call/linkage rules.
No diagnostic is allocated: the original `20021127-1` runtime failure and two `__builtin_abs` E2001 refusals are affected candidates, not predicted passes, and lowering uses existing arithmetic-shift/xor/subtract instructions with an explicit overflow obligation rather than an external dependency or an EvoIR intrinsic.

### Register-or-memory source constraints

Section 3.11 admits only its listed `rm`/`mr` alternatives by selecting an already implemented register form, without changing IR instructions or backend constraints.
Both sealed compilers at O0/O2 build and execute `.evidence/builtin-probes/20260908-register-memory/operands.c` with exit zero, recorded in `results.txt`.
No code is allocated or withdrawn; unknown constraints and invalid register operands remain E3021.
The measured original first-error class has one unit, `lib/math/div64.o`; a probe is not a claimed corpus pass.

### Unused target-header types are not floating-point execution

Section 10.1.14 admits the twelve measured target type spellings with distinct identity and only their measured fixed layouts, leaving the three scalable types sizeless and all value execution deferred.
Evidence `.evidence/builtin-probes/20260908-header-types/{types.c,results.txt}` records GCC-14 and Clang-19 separately, including seven names absent in Clang; this is the user's authorized GNU target-header admission, not an inferred agreement or scalar substitute.
No new diagnostic is allocated: unsupported layout/value consumers retain their existing refusal and unused declarations do not reach EvoIR.
The inventory records 31 vector-header and seven extended-float first errors in zstd and two vector-header errors in cJSON; only original-unit replay can establish their new outcomes.

### Explicit arithmetic options in compiler benchmarks

Section 6.1 defines opt-in signed wrapping rather than weakening the default execution contract; memory, shift and division obligations remain in force.
The sealed probes `.evidence/builtin-probes/20260908-wrapping-arithmetic/{wrap.c,results.txt}` pass GCC-14 and Clang-19 at O0/O2, including the 128-bit boundary, and the preceding `20260908-torture-options/results.txt` retains the default-mode failures.
Nine original torture runtime failures are positive with their named wrapping option; the adapter must preserve each source and record applied options separately from unhandled directives and options.
No diagnostic is added, and unsupported target/dialect requirements or an oracle conflict cannot be relabelled as a passing measurement.

### The digit builtin does not introduce a libc dependency

Section 10.1.14's already-admitted GCC-only `__builtin_isdigit` resolves to its one-evaluation unsigned-range expression, while ordinary `isdigit` calls retain the external execution boundary.
Sealed GCC-14 O0/O2 executes `tests/fixtures/builtin-digit.c` with exit zero, including all 256 byte inputs and signed boundaries; Clang-19's missing builtin is explicitly recorded in `.evidence/builtin-probes/20260908-isdigit/results.txt`, not described as agreement.
No diagnostic is added, and arity, conversion and incompatible visible-library-declaration checks remain in force.
The original mixed-link measurement `measure-20260908T102907Z` isolates two EFI objects failing on the invented external `__efistub_isdigit` dependency; original-unit replay determines closure.

### Packed members retain their access alignment

The exact target-derived layout of a packed subobject can have lower alignment than its scalar type; load/store guards carry that access alignment while retaining exact width and pointer identity.
Both sealed compilers at O0/O2 execute `tests/fixtures/integer-mode-memory.c` with exit zero; the complete measurement is in `.evidence/builtin-probes/20260908-integer-modes/results.txt`.
No diagnostic is withdrawn: an ordinary misaligned pointer dereference remains E3012, and malformed guard widths, zero alignment or non-power-of-two alignment remain invalid EvoIR.
This was exposed by the regression for the two original mode-attribute failures; no additional passing-unit count is inferred.

### Integer mode attributes select the storage type

Section 10.1.9 now explicitly applies measured integer modes rather than silently preserving a different-width declaration; the user's non-floating-point closure authorizes this body amendment.
Both sealed compilers agree on the declaration, member, parameter, qualifier, value and rank rows in `.evidence/builtin-probes/20260908-integer-modes/results.txt`; abstract type-name placement, boolean bases and enum compatibility disagree and are refused with existing E2903.
The original torture `memclr` and `misalign` are two native failures with these ignored attributes; their complete replay, not the focused probe, determines closure.

### Alternate qualifier spellings

`__const` and `__const__` denote `const`, `__volatile` and `__volatile__` denote `volatile`, and `__signed` and `__signed__` denote `signed` after macro expansion, preserving the ordinary type rules rather than ignoring the words.
Both sealed oracles execute `tests/fixtures/qualifier-spellings.c` at O0/O2 with exit zero and reject its const-assignment negative probe (`.evidence/builtin-probes/20260907-declaration-spellings/results.txt`).
No diagnostic is added: assigning to the resulting const object remains E2101.
The original torture `ffs-1` stops at `__volatile`, and `20111208-1` at `__const`; separate `const static` refusals follow the already normative free-specifier-order rule of 10.1.18.

### Extension prefixes on record member declarations

One or more `__extension__` prefixes on a record member declaration have no effect on its type, layout or execution, including a bit-field, anonymous record or comma-separated declarator list.
Both sealed oracles return zero at O0/O2 for `tests/fixtures/record-extension-prefix.c`; results are in `.evidence/builtin-probes/20260907-record-extension/results.txt`.
This uses the existing declaration-prefix rule and adds no diagnostic or execution instruction.
The first error in multiple torture units is the same prefix on `bits/atomic_wide_counter.h:27`; the next full run determines the affected count.

### Character constants in preprocessing conditions

Admitted single-character constants in `#if` use the same decoded escape value as in C expressions, including wide prefixes, octal/hexadecimal escapes and GNU `\e`, with the existing integer controlling-expression arithmetic.
Malformed character spellings are diagnosed as the existing `E1006 invalid-character-literal` before evaluation, never replaced with zero.
Both sealed compilers at `-std=gnu11 -fshort-wchar -O0/-O2` execute `tests/fixtures/preprocessor-character-values.c` with exit zero and reject the malformed `\x` probe (`.evidence/builtin-probes/20260907-character-conditions/results.txt`).
This identifies the wrong-value preprocessing cause of torture `widechar-1`, one failing unit in run `20260907T124739Z`; no new encoding or multi-character constant support is inferred.

### Identifier-list definitions and promoted call interfaces

A definition with an identifier list retains those parameters in identifier order and their declared object types after array/function adjustment; its effective call interface uses default argument promotions unless a preceding prototype supplies types compatible with each declared type or its promotion.
Every listed parameter must have exactly one declaration, with only no storage class or `register`; missing or extraneous declarations, initializers and implicit `int` remain `E1001`, while duplicate declarations and incompatible preceding prototypes retain `E2002`.
Sealed GCC-14 and Clang-19 execute `tests/fixtures/identifier-list-functions.c` at `-O0` and `-O2` with exit zero, including narrow addressed objects, adjusted parameters and a preceding narrow prototype; the eight negative/admission probes and the rejected narrow-prototype/int-object variant are recorded in `.evidence/builtin-probes/20260907-identifier-functions/results.txt`.
The preceding torture run has 185 first-error `E1001` refusals without an identifier-list/implicit-int split, so no number of newly passing programs is inferred from that count; floating execution remains deferred.

### Explicit legacy integer declarations

The explicit `-std=gnu89` compatibility mode admits omitted declaration types as `int`, undeclared identifier-list parameters as `int`, and an otherwise undeclared called name as an ordinary block-scope `extern int name()` declaration with linkage and default argument promotions; undeclared non-call names and incompatible redeclarations keep their existing diagnostics, and default/explicit GNU11 keeps the preceding strict rule.
For this explicitly selected mode only, the default GNU11 predefined set is overlaid by removing `__STDC_VERSION__` and `__GNUC_STDC_INLINE__` and defining `__GNUC_GNU_INLINE__=1`, before user `-D`/`-U`; the last language-mode option wins and the sealed default set is unchanged.
Both sealed compilers execute `functions.c`, `objects.c` and `calls.c` at O0/O2 in GNU89, reject them in GNU11, and agree on those mode macros (`.evidence/builtin-probes/20260908-legacy-declarations/results.txt`); no new diagnostic is allocated.
The original run `20260908T163937Z` contains 72 E1001 programs explicitly requesting GNU89, not 72 predicted passes; this admission rule does not claim closure of other language-mode differences such as external inline emission.

### A call requires a function designator or function pointer

After the ordinary expression conversions, the called expression must have pointer-to-function type; a visible non-callable object is not replaced by an implicit function declaration and reports E2937 (`invalid-call-target`) at the called expression in either language mode.
Both sealed compilers reject `noncallable.c` in `.evidence/builtin-probes/20260908-legacy-declarations/`, while both execute the combined legacy declaration fixture at O0/O2; `return-conflict.c` is separately rejected by both with the existing redeclaration boundary.
The parser previously silently accepted the non-callable probe; no benchmark pass count is inferred from this negative regression.

### Termination builtins and the existing external-call boundary

`__builtin_abort()` has prototype `void (void)` and resolves to the ordinary external `abort` call, with the same compatible-declaration check and no invented interpreter library model.
Both sealed oracles compile and execute the unreachable-abort control in `.evidence/builtin-probes/20260907-function-specifiers/abort.c`; GCC accepts `termination.c`, but Clang-19 rejects `__builtin_exit` and `__builtin__Exit` as unknown builtins, so those spellings remain refused as E2933 (`unsupported-termination-builtin`).
The result is recorded in `termination.out`; reached external `abort` still has the body's E3039 interpreter boundary and machine execution delegates to the linked library.
The architecture's preceding torture inventory counted 404 termination-builtin refusals without a spelling split; the next lane reports the actual effect rather than attributing all 404 to this one accepted spelling.

### Linkage attributes still conflict with the preserved-only body

Both sealed oracles make `alternative` an alias of `value` and emit `fallback` as a weak symbol in `.evidence/builtin-probes/20260907-function-specifiers/linkage.c`, recorded in `termination.out`.
Section 10.1.9 explicitly says `weak` and other non-layout attributes are preserved without effect; this body constraint takes precedence over inventing linker semantics in Appendix D, so the execution request's applied alias/weak feature is not closed.
No validator is loosened, alias body fabricated, or new diagnostic allocated; C-HIR retains the attributes, but linked program behavior is not claimed equivalent.
The supplied musl inventory identifies 108 affected symbols, not a measured first-error unit count; this remains an explicit incomplete requirement.
Closed by revision 1.9.79, which admits both as linkage marks with emission in EvoMIR 0.22.

### C11 function specifiers and the variable-length-array capability

`_Noreturn` is accepted on function declarations and definitions, retained as a `noreturn` attribute with written spelling `_Noreturn`, and supplies no new dead-code or control-flow licence.
Both sealed oracles accept the valid forms in `.evidence/builtin-probes/20260907-function-specifiers/valid.c`; GCC warns but accepts the object/typedef probes while Clang rejects them, so the profile refuses those placements with E2934 (`function-specifier-on-non-function`), as recorded in `results.out`; revision 1.9.74 reserves the previously proposed E2932 for `always-inline-cycle`, and that body allocation takes precedence.
The 2026-09-07 execution request adds the truthful profile capability `__STDC_NO_VLA__=1` after the unchanged sealed predefined baseline and before command-line overrides; this is an EvoCC profile declaration, not a claim that the VLA-supporting oracles define it.
No full-configuration first-error count is yet measured; the toybox lane currently strips this specifier from two files, so an adjusted lane alone cannot prove native acceptance.

### Empty parameter lists in function definitions

A function definition written with an empty parameter list has zero parameters; its preserved C declarator remains distinct from a prototype, while the lowered definition has zero data arguments.
Both sealed compilers accept and execute `.evidence/builtin-probes/20260907-function-specifiers/empty-definition.c` with exit zero, recorded in `empty-definition.out`.
Declarations without a definition keep the existing observed-call signature rule, and named old-style parameter declarations are not enabled by this change.
No diagnostic is added; the torture lane rewrites 957 empty lists, so its existing adjusted pass count does not measure this native acceptance.

### Equality of integer pointers with differing signedness

Equality and inequality accept pointers to integer types that differ only in signedness, using the existing pointer-conversion compatibility relation; ordered comparisons retain their existing rules.
Both sealed GCC-14 and Clang-19 accept and return zero for `.evidence/builtin-probes/20260907-function-specifiers/pointer-equality.c`, with distinct-pointer-type warnings recorded in `pointer-equality.out`.
No diagnostic is added and no memory or validator rule is relaxed.
This reproduces Csmith base seed 5 from run `20260906T184351Z`; that run has 164 E2102 refusals across both campaigns, but their individual construct attribution is not yet established.

### Address-only blocks versus terminator-edge reachability

Section 3.7 admits label addresses, but Section 2.5 still requires every validated block to be reachable by terminator edges; `unsigned long f(void){return (unsigned long)&&after; after:return 0;}` exposes the conflict.
Both sealed oracles produce a nonzero address (`.evidence/builtin-probes/20260906-code-addresses/address.c`, `address.out`); the body's reachability invariant takes precedence, so this form remains an existing E3101 validation refusal, with no fabricated edge or validator relaxation.
No new code is assigned; affected full-configuration unit count is not yet measured, and this is not an allowed completion residual.

### Ordered pointer comparisons remain excluded by the body

Sections 3.4 and 3.7 explicitly exclude ordered pointer comparisons; revision 1.9.67 admits pointer subtraction but does not override that separate exclusion.
Both sealed oracles print `15` for the same-array `<`, `<=`, `>`, `>=` cases in `.evidence/builtin-probes/20260906-pointer-ordering/order.c`, recorded in `order.out`; this does not authorize broadening the body's IR profile.
The existing lowering refusal is named E3045 (`not-lowered: ordered-pointer-comparison`) without changing front-end acceptance or inventing a pointer-ordering rewrite.
The construct has been located at `fdt.c:321` and `fdt_ro.c:276`; final unit counts must come from the sealed full-configuration survey, and E3045 is not part of the architecture's allowed terminal residuals.

### Checked-product representability at the signed boundary

Section 10.1.15's mathematical representability rule takes precedence over its claim that a high-bit-set unsigned result always compares unequal to a sign extension: equal-width extension preserves exactly those bits, whereas a wider signed destination can hold them.
Both sealed oracles at `-O2` report overflow for `~0ul * ~0ul` stored in signed 128 bits and no overflow for two unsigned 16-bit maxima stored in signed 64 bits (`.evidence/builtin-probes/20260906-unsigned-result/representability.c` and `representability.out`, including the initial Clang runtime-helper link failure).
The predicate therefore also rejects negative signed exact results stored unsigned, and high-bit-set unsigned exact results stored signed when the destination is no wider; no diagnostic is added.
This refines the 48 unsigned-product first-error units in sealed run `20260905T213244Z-60582`; subsequent impact must be measured.

### Checked operands beyond the available exact representation

Section 10.1.15 explicitly excludes 128-bit operands and result pointees as E2913; that specific boundary takes precedence over the general 128-bit execution opening, and the executor's earlier expansion and contrary statement here are corrected.
For admitted operands, the body's signed-width formula can still exceed 128 bits: unsigned 64-bit maximum multiplied by signed `-2L` has wrapped unsigned result `2` and overflow `1`, as both sealed oracles report in `.evidence/builtin-probes/20260906-mixed-product/product.c` and `product.out`.
Where that formula requires more than 128 bits, its available `bits<W>` set takes precedence: lowering remains fail-closed as E3044 (`not-lowered: checked-arithmetic-exceeds-128-bits`), not withdrawn E3042 or a wrapping substitute for exact arithmetic.
The prior sealed survey identifies all 48 width refusals as unsigned 64-bit multiplication, so this separate boundary has no identified first-error unit there; its next-run count must be measured.

### Checked arithmetic outside the signed exact-width bound

Section 10.1.15's signed-width formula remains authoritative: two unsigned 64-bit operands require 130 counted bits for multiplication, contradicting its claim that every cohort use fits 128.
Both sealed oracles print `0 4900000000`, `1 18446744073709551614`, and `1 1` for `.evidence/builtin-probes/20260906-checked-width/width.c`, recorded in `width.out`; this does not authorize replacing the body's signed exact representation with an unsigned one.
The existing refusal is named E3042 (`not-lowered: checked-arithmetic-width`), rather than hidden under E3101.
This construct is the first error in 47 units of sealed run `20260905T201056Z-67314`.

### Direct object/function pointer conversion boundary

Section 10.1.18 admits the C conversion, but Section 3.10 explicitly keeps `ptr<AS>` and `fnptr<Sig>` distinct without a direct conversion; this boundary is retained as E3043 (`not-lowered: object-function-pointer-conversion`).
Both sealed oracles print `7` for `.evidence/builtin-probes/20260906-function-pointer-conversion/conversion.c`, recorded in `conversion.out`; the front end still accepts that source, while an explicit whole-pointer integer round trip uses the separately specified Section 3 conversions.
No pointer retyping, invented callee, or validator relaxation is introduced.
Seven units first encounter the direct conversion in sealed run `20260905T201056Z-67314`.

### Compiler hints and dynamic stack allocation

Per the 2026-09-06 architecture request, `__builtin_prefetch` emits no memory operation or external call, but evaluates its address argument exactly once and preserves those effects.
Both sealed oracles report `prefetch-evaluations=3 stack-value=7` for `.evidence/builtin-probes/20260906-compiler-hints/hints.c`, recorded in `hints.out`; stack allocation is valid C but remains an explicit profile deferral rather than an unresolved library call.
Lowering a call to the synthesized `__builtin_alloca` prototype reports E3046 (`not-lowered: dynamic-stack-allocation`), while an ordinary user-defined function named `alloca` is unaffected.
The baseline torture run `20260906T070921Z` reports five prefetch link failures and four alloca link failures; this is not a new whole-kernel measurement.

### Assembly immediates before specialization

Section 3.11's `i` operand must be a proved integer constant, and Section 10.1.9 preserves `always_inline` without applying it, so an unspecialized parameter retains E3021 with constraint name `i`.
Both sealed oracles accept the constant-specialized form and reject the dynamic form in `.evidence/builtin-probes/20260906-immediate-parameter/immediate.c`, recorded in `immediate.out`.
No inlining promise is inferred from a preserved attribute and no new diagnostic is allocated.
This is the first error in three units of sealed run `20260905T201056Z-67314`.

### Pointer difference remains a separate execution obligation

The new pointer/integer casts do not discharge the same-object obligation of pointer subtraction, whose existing body-required lowering refusal remains E3032.
The measured value and type are retained in `.evidence/builtin-probes/20260901-2c23684c/p11.c` and `p11.out`, with a lowering-refusal control in `crates/evocc-lower/tests/fail_closed_forms.rs`.
Eight zstd units first encounter this construct in sealed run `20260905T201056Z-67314`; no diagnostic or pointer-difference instruction is invented.

### Static compound storage and block linkage

Both sealed oracles compile `.evidence/builtin-probes/20260906-static-storage/storage.c`
and print `12 7 7`: a static compound initializer copies its value, a discarded
pointer expression keeps its side effects, and a block-declared external function
may later be defined at file scope with the same linkage identity.
Section 2.1 already names `compound_literal` but Section 9 omits its kind spelling;
use that spelling and the private symbol `__evocc.compound.<origin>` without
changing allocation-site identity.
A function definition binds its file-child lexical scope to the existing
non-builtin function symbol even when its first declaration was inside a block;
this does not permit a definition inside that block.
No diagnostic is added; survey `20260905T185525Z-27277` measured one aggregate-copy
initializer, one block function-address lookup and two discarded-pointer sites.

### Qualified function parameter identities

Section 10.1.18's existing compatibility rule ignores parameter top-level
qualifiers when comparing a function declaration with its symbol or written
parameter list, without removing those qualifiers from the parameter object.
Both sealed oracles compile `.evidence/builtin-probes/20260906-parameter-qualifiers/qualifiers.c`
and print `7 7 7 7` in `qualifiers.out` (SHA-256
`29b8ca085beae90f9c40224f98b731817a064d4cb9dd118bdbac6d8e90cda26b`).
The implementation reuses TypeInterner compatibility instead of a second
comparison; no diagnostic is added, and ten units in lowering survey
`20260905T185525Z-27277` reached the corresponding identity failure.

### Completion target versus explicit execution deferrals

Lowering survey `20260905T185525Z-27277` records 2,700 first errors at E3035
and two at E3029, with zero panic or timeout.
Section 10.1.18 explicitly requires refusal of global-register reads/writes
and used label addresses/computed jumps; these body rules remain authoritative
even though the requested completion target permits only E3030, E3033 and E3026.
Existing minimal controls are `lowering_diagnostic_frontier.rs` and
`computed_goto_lowering.rs`; no diagnostic or unmeasured register/branch
semantics is invented, and the overall completion target remains unmet.

### Signed relocation addends

Section 2.1 explicitly makes a relocation addend signed, while Section 9's
abbreviated `reloc` production refers to unsigned `integer` for that field;
the explicit signed rule takes precedence, with canonical optional `-` and no `-0`.
The positive, negative and malformed byte-form controls are in
`crates/evocc-evoir/tests/relocations_and_addresses.rs` and the independent
Section 9 field sentinel; this IR spelling conflict is not an oracle-measurable
C behavior, adds no diagnostic, and has no measured cohort count yet.

### Implemented instruction shape corrections

The existing Section 9 `copy` production supplies the omitted canonical shape
below, and the existing assembly forms are now implemented rather than pending.
This list also includes the dynamic allocation form authorized later in this
appendix; its semantics and evidence are in that entry, not inferred from this
instruction-shape metadata.

```text
(<>) = copy<> <>, <>, <>, <> [size=<>, align=<>]
(<>) = asm<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>]
(<>) = asm.goto<> <> <>, <> outputs [<>] inputs [<>] clobbers [<>] labels [<>] fallthrough <>
(<>, <>, <>) = alloca.dynamic <>, <>, <> [origin=<>, align=<>]
(<>) = va_arg.byval <>, <>, <>, <> [size=<>, align=<>]
(<>, <>, <>) = atomic.rmw.<> <> <>, <>, <>, <> : <>
```

### Negative checked results stored into an unsigned type

Section 10.1.15's mathematical representability rule requires overflow for a negative result stored into an unsigned type, even when that type is as wide as (or wider than) `W`; the abbreviated truncation/extension test alone does not establish this case.
The lowered overflow predicate additionally includes signed `exact_result < 0` for unsigned destinations, without changing the specified arithmetic width or runtime UB rules.
Both sealed oracles print `1 18446744073709551615 1 4294967295` for `.evidence/builtin-probes/20260906-checked-conversion/result.c`, recorded in `result.out` (SHA-256 `3a4f266c33c39dac31175baedf4d36afd03fbbd95c86f969dd957505630a315a`).
No diagnostic is added; affected full-configuration unit count is not yet measured.

### Variadic marker placement

Section 1.1's explicit placement rule takes precedence over the abbreviated
Section 9 productions: `...` appears immediately before `mem<R>, world`,
never after the effect-token parameters.
The minimal canonical-text reproducer and its rejection control are in
`crates/evocc-evoir/tests/external_symbols.rs`; this is an internal IR grammar
conflict, not an oracle-measurable C behavior, and introduces no diagnostic code.
Full-configuration impact has not yet been measured.

### External ctype execution boundary

Revision 1.9.64's seven-function external execution boundary takes precedence over
the older Section 10.1.14 ctype execution-table requirement: without a
definition, ordinary calls to the five ctype functions and the four unresolved
builtin spellings report the existing E3039 in both interpreters. Revision
1.9.82 resolves `__builtin_isdigit` before this external-call boundary.
Evidence is the minimal calls in `crates/evocc/tests/external_symbol_lowering.rs`;
the sealed `p26-builtins.c` and `p28-table.out` describe C behavior, not
availability of an implementation in this executor.
The original oracle-table fixture is unchanged; its interpreter test now
pins the explicit refusal, while front-end admission and typing are unchanged.
Full-configuration impact has not yet been measured.

### Array parameter identity checks

Identity checks apply Section 10.1.18's outermost array-to-pointer adjustment
when comparing a parameter's preserved written declarator with its canonical
type, including a typedef naming an array; remaining array layers, element
types and qualifiers are still checked without adjustment.
Both sealed oracles compile the four type assertions in
`.evidence/builtin-probes/20260905-external-parameters/parameters.c` and print
`7 7 7 7` in `parameters.out`.
This removes the existing E3101 identity refusal rather than introducing a
new code, and affects one first-error unit in sealed lowering run
`20260905T113121Z-43616` (`lib/crypto/chacha-block-generic.c`).
The normalization is restricted to parameter checks and does not relax
ordinary object or nested-array identity checks.

### Function parameter identity checks

A parameter written with function type, directly or through a typedef, is
adjusted to a pointer to that exact function type; preserved-spelling identity
checks undo only this outer adjustment and still check the function signature.
Both sealed oracles print `7 7 7` for the three spellings in
`.evidence/builtin-probes/20260906-function-parameters/parameter.c` and
`parameter.out` (output SHA-256
`69d15060f5a52403979ee890fa0e7ccd967e5ae9dc5483cf000b14974df57588`).
No diagnostic is added: this removes the E3101 refusal affecting 2,464 units
at `blockdev_direct_IO` in sealed lowering run `20260905T175021Z-94287`.

### Narrow bit-field integer promotions

Integer promotions use a bit-field's range even when its declared base is
`long` or `long long`: widths below 32 and signed width 32 promote to `int`,
and unsigned width 32 promotes to `unsigned int`; this does not change the
field's storage layout or introduce promotion in a non-arithmetic context.
Both sealed oracles execute `tests/fixtures/bitfield-integer-promotions.c`
with exit 0 (evidence `.evidence/builtin-probes/20260907-bitfield-promotions/oracle.txt`).
No diagnostic is added; one known torture failure, `bf-sign-2`, exercises
this rule, and no full-campaign improvement is claimed before replay.

### Nonnegative enumeration objects and integer constants

A non-packed enumeration whose values are all in `0..UINT_MAX` uses
`unsigned int` as its object representation; a range including a negative
value and fitting `int` uses `int`, including when either is a bit-field base.
Enumeration constants whose entire range fits `int` retain type `int`, not
the enum object's unsigned mode, and the existing wider-range rule is unchanged.
Both sealed oracles compile and run `tests/fixtures/enum-bitfield-signedness.c`
at `-O0` and `-O2` with exit 0, recorded in
`.evidence/builtin-probes/20260907-bitfield-promotions/enum-oracle.txt`.
No diagnostic is added; the reduced field from torture `20030714-1.c`
accounts for one known failure, with full-campaign impact still unmeasured.

### Named stack parameters precede the variadic starting address

With nine named `long` parameters, the first anonymous argument follows a
named stack slot, whereas EvoMIR Section 6 fixes `va_start.stack` at
`x29 + 16` without that displacement.
Both sealed oracles and both IR interpreters return zero for
`.evidence/builtin-probes/20260907-variadic-stack/probe.c`, but EvoCC's
native code returns one (`results.txt` in that directory).
The authorized correction in EvoMIR revision 0.21 places the starting
address after the named stack extent; no refusal code is allocated for
a valid scalar variadic call.
One minimized case reproduces torture `va-arg-1.c`; additional affected
unit counts await the next full run.

### Empty union initialization and inactive tail bytes

In this checked profile, empty union initialization initializes the first member, not the inactive tail bytes; reading the latter retains `E3001` rather than acquiring a new all-zero guarantee.
The sealed GCC-14 and Clang-19 both return zero at `-O0` for `tests/fixtures/empty-union-initialization.c` and original torture `pr19687.c`, but at `-O2` GCC returns zero and Clang returns one (`.evidence/builtin-probes/20260907-union-tail/results.txt`).
This oracle conflict is kept outside common positive compatibility evidence; no implementation or diagnostic code changes.
One currently failing torture unit is identified by this probe, not reclassified as a pass.

### Hosted library builtin prototypes

`__builtin_printf`, `__builtin_malloc`, and `__builtin_strcpy` use the
ordinary external prototypes `int printf(const char *, ...)`,
`void *malloc(size_t)`, and `char *strcpy(char *, const char *)` through
the existing unit-local builtin symbol mechanism, with default promotions
for `printf`'s anonymous arguments and no new intrinsic or library executor.
Existing checks remain `E2909` for arity, `E2913` for incompatible arguments,
and `E2927` for an incompatible visible function declaration; unsupported
external execution still follows Sections 3.10 and 5 rather than fabricating a result.
Both sealed oracles execute `tests/fixtures/hosted-library-builtins.c` at
`-O0` and `-O2`, print `abc 7\n` and exit zero, recorded in
`.evidence/builtin-probes/20260907-library-builtins/oracle.txt`.
Torture run `20260907T093504Z` has respectively 11, 7, and 5 first-error
refusals at these spellings; this entry does not predict how many become clean.

### Formatted and bounded string library builtins

`__builtin_sprintf`, `__builtin_snprintf`, `__builtin_strchr`, and `__builtin_strncpy` use their ordinary external C library prototypes through the existing builtin namespace, including default promotions for anonymous formatting arguments, with no new intrinsic or interpreter model.
Both sealed oracles execute `tests/fixtures/formatted-string-builtins.c` at `-O0` and `-O2` with exit zero; `.evidence/builtin-probes/20260907-formatted-builtins/results.txt` records the measured return values, truncation and zero padding asserted by that fixture.
The separate `puts.c` probe in that directory is accepted by GCC-14 but rejected by Clang-19, so the existing conflict policy refuses `__builtin_puts` with E2935 (`unsupported-library-builtin`), while ordinary declared `puts` remains unchanged.
Torture run `20260907T093504Z` has 3, 1, 1, 1, and 1 first-error units for `sprintf`, `snprintf`, `strchr`, `strncpy`, and `puts` respectively; the next run, not these counts, determines end-to-end impact.

### Effectful expressions returning void

A `return expression;` in a void function accepts an expression of type void, executes its effects exactly once, and returns no data value; discarded void conditional, comma, and statement expressions preserve the selected and sequenced effects without inventing a scalar result.
Both sealed oracles execute `tests/fixtures/void-return-effects.c` at `-O0` and `-O2` with exit zero, recorded in `.evidence/builtin-probes/20260908-void-return/results.txt`.
This original admission added no diagnostic. Revision 1.9.115 assigns the
existing void/value mismatch refusal the frontend diagnostic `E1207`
(`invalid-return-value`), also covering a bare `return;` in a non-void function.
`E1205` remains nested-function-definition and `E1206` remains
function-declaration-in-for-initializer; neither is repurposed. The frontend checks
every written return before inlining, execution or IR construction, including
returns in unreachable branches. It does not reject ordinary fall-through at
this statement check or replace its existing runtime/source rules.

For a non-void return expression, initialization of the result uses the same
simple-assignment compatibility contract as other value sites: arithmetic
conversion, compatible aggregate/vector results, null pointer constants and
the approved GNU pointer sign/immediate qualification/void-function conversions
remain admitted. Incompatible implicit pointer/aggregate conversions retain
`E2202` (`invalid-initializer-type`), as already required by Section 10.1.18.
An explicit valid cast supplies its resulting type; this is not a ban on casts.
Checking the written type performs no evaluation and does not duplicate
operand effects, floating conversions, aggregate copies or scope-exit actions.
The same compatibility check applies to each fixed prototype argument, after
the approved transparent-union member selection and before implicit numeric
conversion. Direct and function-pointer calls use the same rule. Unprototyped
arguments and the anonymous variadic tail retain default promotions, not an
invented fixed parameter type. Invalid fixed arguments retain `E2202`; builtin
specific arity/type diagnostics and unsupported representation boundaries remain
separate. This corrects missing enforcement of Section 10.1.18, not a new
permission to reinterpret incompatible pointers.
The check includes the existing 32-byte `__builtin_va_list` record value
contract (Section 3.12), including the caller-made complete copy when passed
by value. Integer classification includes the admitted signed/unsigned
`__int128` types for null pointer constants, pointer arithmetic and conditional
pointer typing; this does not narrow a wide index before the existing pointer
range/overflow checks. Neither boundary may be lost merely by enabling the
shared compatibility check at a previously unchecked value site.
Toybox run `20260907T144525Z` has 16 first-error return mismatches, with `blkid.o`'s `return xputsn(str)` reduced to this rule; actual improvement requires the original-unit replay.

### Fixed local-register operands

The authorized local-register binding is retained at an assembly operand as a canonical `@xN` suffix on its quoted constraint, with source `wN` and `rN` bindings normalized to the same physical register; this is lowered metadata, not a new source constraint spelling.
The binding applies only at the assembly boundary, including tied and read-write operands, and register outputs are transferred on every outgoing edge of `asm.goto`; the existing reserved-register rule and clobber conflicts remain fail-closed with E3021 rather than silently dropping a binding.
Both sealed compilers at O0/O2 execute `.evidence/builtin-probes/20260908-local-register-operands/forms.c` with exit zero (`forms-results.txt`), while the original input-only reduction exits one with the pre-fix subject.
The `r0/r1/r7/r8` aliases are independently measured at both optimization levels in `.evidence/builtin-probes/20260908-register-aliases/results.txt`; 2,260 first errors in `measure-20260908T102907Z` name `=r@r0` before normalization.
No new diagnostic code is introduced; the one independently measured wrong-result reduction is not a count of affected musl units, which the original-unit replay must determine.

### Explicit link-register assembly operands

An explicit local `x30` binding is admitted at the assembly boundary, including tied/read-write results on every outgoing edge, while `x30` remains excluded from general allocation and each function saves/restores its incoming link register.
The existing boundary copies constrain only the asm interval, so subsequent calls cannot overwrite an uncollected output; scratch/platform/frame/stack register bindings and operand/clobber conflicts remain refused with E3021.
Both sealed compilers execute `tests/fixtures/assembly-link-register.c` at O0/O2 with exit zero (`.evidence/builtin-probes/20260912-link-register/results.txt`); no diagnostic code is added.
Two original kernel compile identities, `arch/arm64/kernel/process.o` and `arch/arm64/kernel/stacktrace.o`, reach this binding; their subsequent replay determines closure.

### Unnamed parameters in function definitions

A function definition may omit a parameter name while retaining its canonical parameter type and positional call-interface slot; all arguments are evaluated and converted normally, but an omitted name introduces no source identifier or named parameter object.
Both sealed oracles execute `tests/fixtures/unnamed-definition-parameters.c` at O0/O2 with exit zero under gnu11, with Clang reporting C23-extension warnings (`.evidence/builtin-probes/20260908-unnamed-parameters/results.txt`).
No diagnostic code is added, and a named parameter with a missing or inconsistent symbol, or a body referring to a nonexistent parameter binding, remains invalid.
The original torture `pr103209` is one measured first-error refusal of this form; its end-to-end outcome must be remeasured rather than inferred from the reduced fixture.

### Equality between function pointers and qualified void pointers

Equality and inequality admit a function pointer and a pointer to qualified or unqualified void, comparing the represented function identity after each operand is evaluated exactly once; both operand orders and null values are included.
Both sealed compilers execute `tests/fixtures/function-void-pointer-equality.c` at O0/O2 with exit zero (`.evidence/builtin-probes/20260908-pointer-equality/results.txt`).
The existing pointer-compatibility predicate remains authoritative, no diagnostic code or IR instruction is added, and mixed-representation equality lowers through the existing pointer-to-integer form without authorizing new ordering or dereference rules.
Two original torture first-error refusals, `20041212-1` and `pr17377`, reach this construct; only their subsequent original-program executions determine whether they pass.

### Unprototyped function-pointer storage and calls

A function pointer whose C parameter list is unspecified retains its address and provenance in storage, and a call through it applies the existing default argument promotions before forming its call interface; an unspecified list is not an empty prototype.
Both sealed compilers execute `tests/fixtures/unprototyped-function-pointers.c` at O0/O2 with exit zero (`.evidence/builtin-probes/20260908-unprototyped-pointers/results.txt`).
Lowering carries the unprototyped address as `ptr<generic>` and uses the existing integer round-trip conversion to a `fnptr` with the promoted call's actual signature at the call site, leaving the validator and dynamic `E3019` signature check unchanged and adding no instruction or diagnostic.
Four original c-testsuite refusals (`00087`, `00088`, `00089`, `00209`) require unprototyped pointer storage or calls; their subsequent execution, not the reduced fixture, determines the passing count.

### Enumeration and underlying integer compatibility

A complete enumeration is compatible with its existing target-selected underlying integer type, including through pointers and function parameters, without making separately declared enum tags compatible with each other or discarding nested qualifiers.
Both sealed compilers execute `tests/fixtures/enum-compatible-pointers.c` at O0/O2 with exit zero (`.evidence/builtin-probes/20260908-enum-compatibility/results.txt`); for the separate qualified-pointer query GCC returns one and Clang zero, so the existing nested-qualifier distinction remains authoritative rather than adopting GCC's relaxation.
No diagnostic or representation is added: pointer conditional expressions retain the compatible pointee's signedness and the normal union of pointee qualifiers.
One original torture refusal, `enum-3`, reaches this rule; full-campaign impact must be remeasured.

### Preprocessing line control

`#line` macro-expands its operands and sets the next physical line's presumed decimal line number and optional filename for `__LINE__` and `__FILE__`; physical source identities, include lookup and token byte offsets do not change, and inactive directives have no effect.
Both sealed oracles agree on the valid macro, filename, decimal-leading-zero and GNU zero-line assertions in `.evidence/builtin-probes/20260908-line-control/valid.c` (`results.txt`); a number-only reset retains the presumed filename and included files have their own line-control state.
Malformed line numbers and filenames are E2936 (`invalid-line-directive`); the number exceeding u32 in `invalid-range.c` is accepted with a warning by GCC and rejected by Clang, so the profile rejects it with the same code.
One original c-testsuite refusal, `00152`, is caused by silently skipping `#line`; its original native replay remains the closure test.

### Prototype-only array declarators

An outermost `[*]` in a function-prototype parameter, named or abstract and optionally bracket-qualified, is adjusted to the corresponding pointer under Section 10.1.18; it creates no variable-length object and has no bound to evaluate.
Both sealed oracles compile and run `.evidence/builtin-probes/20260908-prototype-array/valid.c` with exit zero, and reject the object, definition, attributed-definition, `static *` and inner-qualifier probes (`results.txt`); those malformed placements report E1005 (`invalid-declarator`).
Inner variable array layers and actual variable-length objects retain the existing E2203 boundary, and `__STDC_NO_VLA__` remains one.
One original c-testsuite refusal, `00162`, reaches this rule; its original replay determines closure rather than prototype acceptance alone.

### Alignment queries on non-object types

On the AArch64 target, `_Alignof` and `__alignof__` of `void` are one, and of an unattributed function type or function designator are four; expression operands remain unevaluated.
Both sealed compilers at O0/O2 agree on `.evidence/builtin-probes/20260909-nonobject-alignment/align.c`, recorded in `results.txt`, including the distinct existing `sizeof` result of one for either type.
This is a query rule only: it does not make a void or function type an allocatable object, and adds no diagnostic code or IR instruction.
Original torture `20000223-1` reaches the missing void query; `align-3` also requires an explicit function alignment and is not covered by the unattributed-function probe, so neither full-program outcome is inferred.

### Flexible-member initialization stays on the declared record

The file-scope extension in 10.1.3 initializes the declared record's own final flexible member, not a flexible member reached through a nested record or union subobject; nonempty nested initialization reports the existing E2202 (`invalid-initializer-type`), while an empty list adds no flexible storage and remains accepted (`nested-empty.c`, both oracles execute successfully).
In `.evidence/builtin-probes/20260909-flexible-strings/`, both sealed oracles reject `nested-list.c`, while GCC accepts the nested strings in `strings.c` and `nested-record.c` and Clang rejects them, so the profile takes the refusing result (`results.txt`).
Both accept and execute `direct.c` at O0/O2, including the narrow, wide and empty strings, their terminating zero and unchanged record `sizeof`; automatic-object initialization still reports E2919.
This distinguishes the direct string in original torture `20010924-1` from the oracle-conflicting nested form in `pr28865`, and neither original outcome is inferred from the probes.

### Redundant sign bits and integer parity

`__builtin_clrsb`, `__builtin_clrsbl` and `__builtin_clrsbll` convert their sole operand once to `int`, `long` and `long long` respectively, then return an `int` count of leading bits equal to the sign bit excluding that sign bit itself (zero and negative one give N-1).
`__builtin_parity`, `__builtin_parityl` and `__builtin_parityll` instead convert once to the corresponding unsigned type and return the population count modulo two as `int`; all six forms are total, fold on constant operands, and use the existing bit-builtin arity, callable-form and operand diagnostics.
Both sealed compilers execute `tests/fixtures/integer-bit-counts.c` at O0/O2 with exit zero (`.evidence/builtin-probes/20260909-integer-bit-counts/results.txt`); one original torture program, `builtin-bitops-1`, reaches these missing families.
No instruction or diagnostic is added: lowering uses the existing width-typed bit operations and `ctlz`/`ctpop`, with no zero-operand UB obligation for these total operations.

### Obsolete GNU initializer designators

Within an initializer list, `member: value` is an alternative spelling of `.member = value`, and a single `[index]` or `[first ... last]` designator may omit `=`; these forms use the same subobject resolution, inferred bounds and evaluation rules as their existing spellings.
Both sealed compilers execute `tests/fixtures/obsolete-designators.c` at O0/O2 with exit zero (`.evidence/builtin-probes/20260909-obsolete-designators/results.txt`), including anonymous members and side effects.
No diagnostic or representation is added: an unknown member or out-of-bounds designator retains E2906, and this rule does not admit a missing `=` after a multi-component designation.
Three non-floating-point original torture refusals (`20030408-1`, `compndlit-1`, `struct-ini-4`) reach the field-colon form; their full executions, not the reduced fixture, determine passing counts.

### C11 alignment specifiers

`_Alignas(type-name)` and `_Alignas(integer-constant-expression)` among declaration specifiers set object or member alignment, independently of type identity; zero is ignored, multiple nonzero specifiers take their maximum, and a non-power-of-two, unrepresentable or weaker-than-natural request is E2938 (`invalid-alignment-specifier`).
The same code rejects alignment specifiers on parameters, functions, typedefs, register objects, bit-fields and type names; a specifier on a standalone tag declaration is preserved but does not alter its type.
The sealed GCC-14/Clang-19 positive O0/O2 and placement probes are recorded in `.evidence/builtin-probes/20260909-alignment-specifiers/results.txt`; oracle disagreements on a weaker `_Alignas` combined with GNU `aligned`, a weaker request on an unbounded array, and requests exceeding 2^28 are rejected, and the existing GCC-only attribute-query authority observes nonzero `_Alignas` as `aligned` but ignores zero.
Original torture `pr68532` reaches this missing declaration syntax; neither its passing outcome nor the separate, measured over-aligned-stack emission defect is inferred from front-end acceptance.

### Alignment validation is not an incomplete-array query

An unbounded array declaration's alignment request is checked against its element's target alignment, including the existing explicit type alignment, without requiring a bound or making that array allocatable.
`_Alignof(int[])` itself remains invalid and is refused by its requesting context, because GCC rejects the expression while Clang accepts it; the validation rule must not silently widen expression-query acceptance.
Both compilers accept an explicitly aligned external incomplete record declaration followed by either a weaker or stronger record definition, so the later definition does not retroactively invalidate the earlier declaration.
These boundaries are measured in `complete.c` and `results.txt` under `.evidence/builtin-probes/20260909-alignment-specifiers/`; they add no diagnostic beyond E2938 and are regression edges rather than additional counted original-program gains.

### Deallocation builtin and the external-call boundary

`__builtin_free` uses the ordinary external prototype `void free(void *)` through the existing unit-local builtin namespace, evaluates its argument once, and keeps the existing arity, conversion and compatible-declaration checks without adding an interpreter model.
Both sealed compilers at O0/O2 execute `.evidence/builtin-probes/20260909-library-and-assembly-spellings/forms.c` with exit zero (`results.txt`); an executed external `free` retains the existing E3039 interpreter boundary, while native execution uses the linked library.
No new diagnostic is allocated; torture run `20260908T191304Z` has two first-error units at this spelling, and their original replay determines the actual gain.

### Single-trailing GNU assembly spelling

`__asm` is the same contextual spelling as `asm` and `__asm__` in every assembly position, promoted only after preprocessing and preserving its written spelling in C-HIR; constraints, effects and native assembly remain unchanged.
Both sealed compilers at O0/O2 execute the integer inline-assembly assertion in `.evidence/builtin-probes/20260909-library-and-assembly-spellings/forms.c` with exit zero (`results.txt`).
No diagnostic is allocated; two original torture first errors use this spelling, one of which additionally requires x86 floating-point assembly and is not thereby admitted on AArch64.

### Function-pointer casts preserve the dynamic call boundary

A cast between pointers to different function types preserves the function identity while changing the pointer's static call interface; casting back recovers the original interface, including after storage, and a mismatched interface reports the existing E3019 only when the indirect call is actually reached after evaluating its arguments.
Lowering uses the existing `ptr.to_int` and `ptr.from_int.fnptr<Sig>` pair, without weakening the static call validator or the dynamic signature check.
Both sealed compilers at O0/O2 execute the round trip and argument-side goto in `.evidence/builtin-probes/20260909-function-pointer-casts/forms.c` with exit zero (`results.txt`); `reached.c` is separately an invalid-call diagnostic regression, not a claimed positive execution.
No diagnostic or instruction is added; one original torture first-error program, `call-trap-1` in `20260908T195511Z`, reaches this missing conversion and requires a fresh original replay.

### Qualified object addresses in a prefetch hint

The address argument of `__builtin_prefetch` accepts an object pointer with any pointee qualifiers and converts it to the builtin's existing `const void *` interface without reading the object; evaluation of the address expression, including volatile pointer loads and updates, occurs exactly once.
Both sealed compilers at O0/O2 execute `.evidence/builtin-probes/20260909-qualified-prefetch/forms.c` with exit zero, warning about the discarded qualifier (`results.txt`).
This address-only hint rule does not loosen ordinary implicit pointer conversions, change the optional argument checks, or add an instruction or diagnostic.
One original torture first-error program, `builtin-prefetch-3` in `20260908T204820Z`, reaches this refusal; the original-program replay determines its outcome.

### Value-preserving record casts

A cast to the same record or union identity, ignoring top-level qualifiers, produces that record value and evaluates its operand once; it does not permit conversion between distinct tags or make the result a modifiable lvalue.
Lowering retains the existing aggregate address-and-copy representation and preserves volatile source accesses, matching the already implemented C-HIR conversion.
Both sealed compilers at O0/O2 execute `forms.c` and reject `distinct.c` in `.evidence/builtin-probes/20260909-record-casts/`, with the exact outcomes in `results.txt`.
No instruction or diagnostic is added; original c-testsuite `00216` reaches this missing lowering after its flexible initializer is repaired, and its full replay remains required.

### Transparent union parameters (2026-09-14 runtime repair)

A `transparent_union` parameter accepts a value matching one of its members (including the ordinary pointer and null-pointer conversions), constructing that member exactly once before using the existing record-call representation; an already matching union value is unchanged, and an unannotated union does not acquire this conversion.
The initial applied profile admits equal-size/equal-alignment general-register integer and pointer members, whose AArch64 argument representation is also the first member's; incompatible arguments retain E2202 and unsupported attribute representations retain E2903 rather than guessing an ABI.
Both sealed GCC-14 and Clang-19 execute `tests/fixtures/transparent-union-calls.c` at O0/O2 with exit zero (`.evidence/builtin-probes/20260914-transparent-union/oracles.out`).
This repairs the observed `mm:run_vmtests.sh` kernel crash: `release_pages(mr->pages, nr)` must pass the array pointer, not copy bytes from its pointee as if the pointer already addressed a union object.

### Union member casts (2026-09-10 authorization)

A GNU cast to a union may construct a value from a compatible member type, ignoring only top-level qualifiers and applying ordinary array/function decay: the operand is evaluated once, the result is not an lvalue, and an initializer context still requires the member's actual value to be constant when storage is static.
The checked member initialization reuses existing allocation, copy and call representations; E2942 (`incompatible-union-cast`) rejects a missing compatible member rather than silently applying an assignment conversion, reinterpreting another tag, or discarding nested pointer qualifiers.
Both sealed GCC-14 and Clang-19 execute `tests/fixtures/union-member-casts.c` at O0/O2 with exit zero; `union-member-cast-errors.c` cases 1–6 and 9–10 are rejected by both, case 8's full-width bit-field is accepted by both, and case 7's narrow bit-field conflicts (GCC rejects, Clang accepts) and is refused by the profile.
No IR instruction is added; the original `960416-1` refusal in torture `20260909T215349Z` motivates this rule, while original-program replay remains deferred to the final campaign.

### Extension prefixes on expressions

One or more `__extension__` prefixes on an expression leave its type, value and operand evaluation unchanged, including in a constant-expression context and a for-initializer expression.
Both sealed compilers at O0/O2 execute `accepted.c` under `.evidence/builtin-probes/20260909-extension-expressions/` with exit zero (`accepted.out`); `forms.c` separately shows disagreement on a prefixed for-initializer declaration, so that declaration form remains refused by the existing E1003 rather than being silently reinterpreted.
No instruction or diagnostic is added, and a missing operand remains an expression error.
The mixed-link adapters previously erased this spelling before measurement, so original-program replay without that adaptation is required rather than attributing predicted gains to the focused probe.

### Hidden symbol visibility

The 2026-09-10 implementation authorization admits `visibility("hidden")` as linked-image binding, independent of C internal linkage and weak strength: global definitions spell `visibility=hidden`, and functions, declarations, extern objects and aliases optionally spell it before `linkage`; an external declaration cannot have C internal linkage.
ELF emission marks the symbol `.hidden` and addresses strong hidden references directly, but keeps the GOT for weak hidden references so unresolved weak addresses remain null; a hidden definition is still externally callable and is not discarded as an unused static function.
The paired `tests/fixtures/symbol-sections.c` / `symbol-sections-caller.c` probe passes both sealed compilers at O0/O2, with exact symbol/relocation observations and the `symbol-attribute-errors.c` edge cases recorded in `symbol-sections.out`; conflicting explicit default/hidden redeclarations are E2943 `invalid-or-conflicting-symbol-attribute`. Revision 1.9.99 extends this bounded rule to protected and the explicitly reviewed internal-to-hidden normalization, rather than treating either as C static linkage.
This replaces the earlier pending-authority entry without changing Section 10.1.9's body; the two measured vDSO units in K1 `measure-20260909T133504Z` motivate the change, and their new campaign outcomes remain unmeasured.

### Explicit object sections

The same authorization admits an object's `section` as placement metadata, with canonical ` section="name"` after its initialization bitmap and before optional `relocs`, using the existing quoted-string encoding; the object identity, layout, initialization and relocation rules are unchanged, and EvoMIR preserves the same global line rather than maintaining a second serializer.
On the GNU/Linux target, explicit writable data sections are emitted as named PROGBITS even for zero bytes, except `.bss` and `.bss.*`, which are NOBITS and may contain no nonzero initializer or relocation; absent placement retains the previous data/BSS selection.
The sealed probes in `tests/fixtures/symbol-sections.out` agree on `.data..percpu` size/alignment 4/64 and `.data.rel.ro.refs` 8/8; empty names, embedded control bytes, conflicting section redeclarations and nonzero NOBITS initializers are E2943 (the first three have a recorded oracle disagreement), with no silent choice between reference answers.
This replaces the earlier pending-authority entry only in this appendix: K1's isolated `cpuinfo.o` had misplaced `cpu_data` and a boot Oops, but a corrected single-unit boot has not yet established that placement was its sole cause, so one measured failing unit is not a newly passing kernel claim.

### Memory and general assembly operand probes

Per S07 and the 2026-09-10 implementation authorization, `m`/`=m`/`+m` select the existing address-only `Q`/`=Q`/`+Q` representation, while input `g` selects `r`; source operands are evaluated once, and memory operands introduce no implicit scalar load or writeback.
Memory operands require addressable objects, and memory outputs may name arrays but cannot write const subobjects; violations use E2914, while unrepresentable register inputs and unlisted constraint combinations retain E3021.
Both sealed compilers at O0/O2 pass `tests/fixtures/assembly-memory-operands.c` and the earlier probes under `.evidence/builtin-probes/20260910-memory-constraints/` (`results.txt`); both reject `assembly-memory-object-errors.c` cases 1–3 and accept 4–5, and the old register-only `operands.c` is not consensus evidence because Clang may select memory for `g`.
Register selection is the reviewed profile choice, not a universal oracle-selection claim; no original-program gain is measured here, and affected benchmark counts remain those of the last full campaign.

### Same-function relative label tables

The S05 label-table contract admits a difference of two labels in the same function as a static integer initializer, not an ICE: the canonical relocation extends its tuple with `, diff=@function^bN, bits=W`, with W in 8/16/32/64, storing the low W bits of target-minus-base plus addend and no pointer-provenance marker.
Both labels must name retained blocks in the same defined function, both contribute to the conservative address-taken successor set, the initialized zero-byte range is W/8 bytes with the existing nonoverlap checks, and this form cannot also carry `exposed`; both interpreters use their code-address mapping and native emission uses the corresponding width's assembler label-difference expression.
`tests/fixtures/relative-label-table.c` returns zero under sealed GCC-14/Clang-19 O0/O2 (`language-semantics.out`); the original first-error program is torture `pr70460`, whose full campaign result remains unmeasured.
GNU void-pointer byte arithmetic transports exposed object/code addresses without treating a label as a dereferenceable object, while actual object access and indirect-jump target checks retain their existing validity rules; no runtime instruction or diagnostic is added.

### Mixed-record variadic representations

A non-homogeneous record containing floating members can pass through the general-register composite ABI as an object representation: copying unused floating or padding bytes is not floating arithmetic, does not initialize those bytes, and does not consume them as scalar values.
The deferred HFA class is still recognized recursively as one to four equal floating base members (array multiplicity and union maximum included), and is not silently assigned the integer/composite ABI; supported HVA handling is unchanged.
Both sealed compilers at O0/O2 execute `tests/fixtures/mixed-record-varargs.c` with exit zero (`language-semantics.out`), reducing c-testsuite `00140`; this does not approve floating execution or claim that all aggregate varargs programs pass.

### General assembly outputs and narrowing ties

The S07 general-operand register choice also applies to `=g`, `+g`, `=&g`, and `+&g`, preserving output, read/write and early-clobber roles; a matching input wider than its register output supplies the output-width low bits through an explicit truncation, with an object/function pointer first exposed as the existing 64-bit address integer.
The reverse width mismatch remains refused unless separately represented, and the equal-type EvoIR tie invariant is unchanged.
Both sealed compilers at O0/O2 execute `tests/fixtures/assembly-general-update.c`, `assembly-tied-width.c` and `assembly-tied-pointer-width.c` with exit zero (`language-semantics.out`); the unchanged examples include `pr65053-1`, `pr65053-2`, `pr65956`, `pr88904`, `20030222-1`, and LLVM's `stkalign.c`.
No instruction or diagnostic is introduced, and the next benchmark replay, not these reductions, determines their program results.

### Automatic variable arrays and immediate pointer qualifiers

Per S02/S03, actual automatic variable arrays allocate at their declaration after evaluating each captured bound once; leaving the declaration scope, including a backward jump before it, ends that allocation, and `sizeof(type-name)` evaluates only bounds needed for the queried extent.
The existing `alloca.dynamic`/`lifetime.end` pair represents this lifetime; its machine lowering uses `stack.release %address` to restore the private previous-SP header only for a still-top dynamic allocation, never releasing an intervening function-lifetime `alloca`, while fixed frame slots remain reserved.
GNU conversions between otherwise compatible non-atomic pointer types may discard immediate const/volatile qualification in initialization, assignment, arguments and returns (as warnings in the references), without changing nested type compatibility, a const object's mutability or atomic access semantics.
`tests/fixtures/language-semantics.out` records four agreeing sealed runs for `variable-array-objects.c`, `variable-array-alloca-lifetime.c` and `immediate-pointer-qualifiers.c`, and their native subject executions; actual reads after a VLA lifetime retain E3010, and original-program counts remain unmeasured.

### Ancestor frame addresses and the return-address disagreement

Per F10 and the 2026-09-10 implementation authorization, stack-address builtin depths first convert to `unsigned int`; `frame_address` admits 0–65535, follows that many saved x29 links in native code, and preserves level zero's existing semantics, while both abstract interpreters report E3031 (`not-executed: frame-address`) only when a nonzero query is reached.
The canonical nonzero EvoIR form is `%v = frame_address depth=N : ptr<generic>` (`N` is decimal 1–65535, with no leading zero), the machine place is `frame.address [fp_N]`, and zero retains its old spelling; large depths use a bounded-size loop with reserved x16 and no flag-setting instructions.
The sealed GCC-14/Clang-19 probes `tests/fixtures/ancestor-frame-addresses.c` and `ancestor-frame-operands.c` have their outcomes in `ancestor-frame-operands.out`: both execute frame links correctly at O0/O2 with frame pointers and sibling-call optimization disabled, but GCC answers zero for nonzero `return_address` while Clang reads the ancestor's LR, so nonzero return queries retain E2913 instead of claiming consensus.
Negative depths and 65536 are acceptance conflicts and retain E2913 after the unsigned conversion; wider constants that narrow to an admitted depth use that depth, no new diagnostic is assigned, and no original-program or cohort gain is measured by these probes.

### Immediate qualifiers in character-pointer conversions

The character-pointer conversions of Section 10.1.3 include discarding immediate `const` or `volatile` pointee qualification in initialization, assignment, argument passing and return; this does not change atomic or nested-pointer qualification rules or permit a write through a const-qualified lvalue.
Both sealed compilers at O0/O2 execute `tests/fixtures/qualified-character-pointers.c` and the 36-combination `matrix.c` in `.evidence/builtin-probes/20260910-character-qualifiers/`, with the unchanged original torture `20121108-1` also printing `c0a8bea0`, `c0a8bea0`, `WORKS.` and returning zero (`results.txt`); qualifier discards are warnings, not errors.
No instruction or diagnostic is added, and ordinary non-character pointer conversions retain their current compatibility checks.
One original first-error program is measured in `20260909T162314Z`; the subsequent original-program replay, not the positive probe, determines the new PASS count.

### Bracketed attribute namespaces and attachment

Per the F03 ruling, `[[gnu::name]]` and `[[__gnu__::__name__]]` reuse the GNU attribute meanings at their measured declaration, member and tag-keyword positions, while unscoped or foreign names do not acquire GNU layout, linkage or inlining effects merely by spelling the same name; spelling and namespace remain in C-HIR.
The sealed GCC-14/Clang-19 probes in `.evidence/builtin-probes/20260909-attribute-brackets/` and `20260910-attribute-namespaces/` cover the grammar, preserved names, actual alignment and packed-member offsets, with outcomes in each `results.txt`.
Malformed placement retains E2903, including the mixed GNU-before-bracket specifier order and the measured layout conflicts for `aligned` on an array suffix or pointer layer; the function-suffix form is preserved on the function type without raising function alignment, as both oracles measure four bytes.
Four original first-error programs in torture `20260909T171533Z` reach bracket syntax; replay decides their actual outcomes, and the independent GCC O2 failure of `pr124358` is recorded separately rather than treated as a consensus value.

### Explicit builtin trap termination

Per F07, `__builtin_trap()` has type `void`, takes no arguments, has no constant value, and lowers to the zero-operand `trap.builtin_trap` reason of the existing typed terminator family, with `may_trap=true` and no licence for dead-edge reasoning.
When reached, both interpreters report E3047 `builtin-trap-reached` (explicit termination, not a failed UB assumption), and the AArch64 emitter uses `brk #1000`; an unevaluated or unselected occurrence does not terminate execution.
The sealed GCC-14/Clang-19 O0/O2 runs of `.evidence/builtin-probes/20260910-builtin-trap/trap.c` all return zero on the ordinary path and SIGTRAP (133) on the reached path (`results.txt`); GCC uses immediate 1000 and Clang uses 1, which are not an observable termination disagreement.
Three original torture first-error programs in `20260909T182954Z` reach this missing builtin; the next original-program replay determines gains rather than this probe.

### Unevaluated expression type classification

Per F09, `__builtin_classify_type(expr)` resolves to an `int` integer constant without evaluating its single expression operand: integer/enum values classify as 1, pointers including decayed arrays/functions as 5, real floating types as 8, complex types as 9, structures and the target `__builtin_va_list` as 12, and unions as 13.
The sealed GCC-14/Clang-19 O0/O2 probe `.evidence/builtin-probes/20260910-type-classification/classify.c` measures these values and unchanged side effects, and all four runs of `tests/fixtures/type-classification.c` return zero (`results.txt`).
The `_Bool` result conflicts (GCC 1, Clang 4), so it reports E2941 `unsupported-type-classification`, also used for opaque target-header types with no agreed category; the conflicting void, zero/multiple-argument and type-name forms remain refused with the existing E2913, E2909 or E1003 diagnostics, respectively.
This rule requires no floating execution or new IR node; two original torture first-error programs in `20260909T201535Z` name this builtin, and their next original-source replay, not type-query acceptance, determines their outcomes.

### Hosted wide integer division

Per the reviewed S08 rule and the 2026-09-10 implementation authorization, signed/unsigned division and remainder execute at `bits<128>`, preserving E3002 for division by zero, E3003 for remainder by zero and E3004 for signed MIN/-1 in either operator; other unsupported widths still retain E3033.
Native lowering uses ordinary caller-clobber-aware calls to `__divti3`, `__udivti3`, `__modti3` and `__umodti3`, with low/high halves in x0:x1 and x2:x3 and the result in x0:x1, requiring the hosted target's libgcc or compiler-rt; no new IR opcode, register class or waived UB assumption is introduced.
The machine interpreter models only these four unresolved helpers with that pair interface, executes a same-module function definition instead when present, and continues refusing unrelated missing callees.
`tests/fixtures/wide-division.c` and `wide-division.out` record GCC-14/Clang-19 O0/O2 agreement on full-width values, signed remainders, stores and live pairs across calls; these tailored results do not change any original-program count, and the following ruling addresses wider checked arithmetic separately.

### Exact checked arithmetic with wide operands

Per reviewed S08 and the 2026-09-10 implementation authorization, checked add/subtract/multiply accept signed and unsigned 128-bit operands and non-boolean integer destinations, overriding the previous width-only E2913/E3044 boundaries without changing invalid-operand or constant-expression rules.
The result is computed as an exact sign, low 128-bit magnitude and magnitude-overflow flag, then tested against the destination's mathematical range and stored modulo its width; cancellation yields nonnegative zero and operands, including the destination address, are evaluated once.
For multiplication, magnitude overflow is equivalent to `a > UMAX / b`, with denominator 1 substituted for zero, so existing unsigned IR operations and hosted division suffice without introducing a 256-bit type, signed-overflow assumptions or undefined zero division.
`tests/fixtures/wide-checked-arithmetic.c` and `wide-checked-arithmetic.out` record GCC-14/Clang-19 O0/O2 agreement and the exact official compiler-rt prerequisite; original-program gains remain unmeasured until the deferred campaign replay.

### Volatile bit-field containers

Per reviewed S11, an ordinary volatile bit-field read accesses its declared allocation container and a write performs a read/modify/write preserving neighboring bits; compound updates additionally read the field value, and each underlying load/store advances `world` and contributes one event rather than treating volatile as atomic.
Packed allocation spans use bounded power-of-two scalar units of at most 16 bytes, in increasing byte order, so a three-byte record never receives a four-byte access and fields crossing units retain their full value.
`tests/fixtures/volatile-bit-fields.c` and `volatile-bit-fields.out` record four agreeing numerical executions, but explicitly different oracle instruction widths: the ordinary-container rule chooses the declared 32-bit width rather than GCC's 16-bit small-field stores, while the packed-unit traversal fixes this profile's trace instead of claiming both compilers have the same internal access order.
This extends the previous volatile bit-field refusal without changing aggregate-copy restrictions, atomic semantics, bounds checks or the requirement that C-HIR and EvoIR produce identical value-and-access traces; original-program gains remain unmeasured.

### Array parameter bound evaluation

Per reviewed S03 and the 2026-09-10 implementation authorization, an outer array parameter adjusts to a pointer even when its written bound is nonconstant; a definition evaluates that bound once on each entry after binding its parameters and before its body, while file/block prototypes and nested callback prototypes never evaluate theirs.
The existing prior-parameter symbol identity is retained, and ordinary C-HIR entry expressions carry these effects through interpretation, lowering and inline substitution without reevaluating them on a body label or using them as the adjusted parameter's `sizeof`.
The four sealed reference runs of `tests/fixtures/parameter-bound-effects.c`, the earlier `parameter-bounds.c` probe under `.evidence/builtin-probes/20260909-dynamic-objects/`, and their recorded outputs agree; both compilers reject all three `parameter-bound-errors.c` noninteger-bound cases, which use E2923 `invalid-array-bound` rather than being admitted by pointer adjustment.
This replaces the outer-parameter E2203 boundary only: inner variable dimensions and actual dynamically sized objects still require the separately scheduled size-capture, allocation and lifetime implementation, and no original-program count is changed here.

### Function-lifetime dynamic stack objects

Per reviewed S02 and the 2026-09-10 implementation authorization, `__builtin_alloca(n)` evaluates and converts `n` to `size_t` once, creates fresh uninitialized storage aligned to 16 bytes, and preserves it until the containing function returns, not merely until its allocating block exits; no stack-overflow guarantee is made.
The canonical form is `(%pointer, %mem_out, %world_out) = alloca.dynamic %mem_in, %world_in, %size [origin=N, align=A]`: `size` is `bits<64>`, the result is an object pointer, `A` is a nonzero power of two, origin uniqueness and mem/world threading match fixed `alloca`, and its effects are reads/writes All and may_trap; the interpreters use the same bounded object model and E3010 on a dangling access.
The machine form `%p:gpr64 = stack.alloc %n, [origin=N, align=A]` subtracts aligned storage from `sp`, preserves fixed objects and spills relative to the entry frame, retains a disjoint outgoing-argument area below the current dynamic storage, and restores `sp` from `x29` on return; it does not participate in pure equality regions.
`tests/fixtures/dynamic-stack-objects.c` and its `.out` record the sealed reference probes for block survival, separate loop/recursive allocations, fixed-frame alignment and register/stack arguments; E3046 is superseded for this builtin, actual variable-length declarations remain separate work, and original-program gains remain unmeasured.

### Ordinary stack-allocation names and binding

With standard builtin recognition enabled, an externally linked `alloca` with the compatible `void *(size_t)` prototype uses the same allocation semantics as `__builtin_alloca`; GNU89 legacy implicit-call mode supplies that prototype, while GNU11 still requires a declaration.
`-fno-builtin` disables this ordinary-name recognition, not the explicit builtin, and a same-named static function remains an ordinary function.
`tests/fixtures/dynamic-stack-names.c` and its `.out` record GCC-14/Clang-19 O0/O2 agreement for these forms and disagreement for an externally defined or assembly-renamed `alloca`, so a recognized call with either conflicting binding reports E2944 `ordinary-builtin-binding-conflict`, including when the definition follows the call.
This supersedes only the earlier blanket statement that every user-defined `alloca` is unaffected; it targets original torture `941202-1`, and focused execution does not change the frozen campaign counts.

### Captured variable array bounds

Per reviewed S02/S03, a block-scope variable array typedef evaluates each written bound once at its declaration, retains a distinct compiler-owned size object for that bound, and answers later `sizeof` queries from the captured values, including typedef chains, multiple declarators and repeated activations; these sizes are not integer constant expressions, while alignment remains an element-type query.
The canonical array bound carries the capture's semantic object identity rather than a placeholder count, so different captures stay distinct but a variable bound does not by itself make compatible element types incompatible; size elaboration uses ordinary C-HIR reads and arithmetic, preserving ordinary inline symbol substitution.
Entering the scope of a variably modified declaration without passing its declaration, by direct/assembly goto or switch dispatch, reports E2945 `jump-into-variable-type-scope`; noninteger bounds retain E2923 and contexts requiring an ICE retain their own diagnostic.
The four sealed executions of `tests/fixtures/captured-array-bounds.c` agree, and both compilers reject all five `captured-array-bound-errors.c` cases (`captured-array-bounds.out`); this size-capture slice does not yet admit actual VLA objects, variably modified typeof operands or claim their scope reclamation implemented.

### Literal string operations and local bindings

Per reviewed F02 and the 2026-09-10 authorization, `__builtin_strcmp` of two narrow string literals folds to `int` -1, 0 or 1 by unsigned code-unit order up to the first NUL, before consulting any same-named declaration; explicit builtins retain this behavior under `-fno-builtin`.
Literal `__builtin_strlen` likewise stops at the first NUL, without shortening the literal array's extent or evaluating arbitrary pointer expressions; nonliteral calls and operand/arity checks retain their ordinary paths.
`tests/fixtures/literal-string-comparison.{c,out}` and original `921007-1.c` execute successfully with sealed GCC-14/Clang-19 at O0/O2, whereas the earlier `20260909-local-library-binding/{binding.c,results.txt}` records disagreement for nonliteral comparisons with a visible same-named static function, now refused as E2946 `builtin-local-binding-conflict` rather than assigned a guessed callee.
This repairs the named original in tailored native execution and makes no new full-campaign claim or binding rule for other library functions.

### Captured strides for variable array views

Per reviewed S02/S03, pointer operations on variable array views use the bounds captured by the type declaration for subscripts, either order of addition, updates and pointer differences, including nested typedefs, recursion and independent inline expansions.
Layout factorization is shared with sizeof elaboration; inline substitution renames captured object identities in types and semantic places, and control-flow edges retain active captures even without an ordinary expression use.
Checked pointer execution retains the full scaled index and reports the existing E3011 for unrepresentable deltas or zero/overflowing captured strides, rather than wrapping to a valid offset or dividing by zero; static layouts and ordinary pointer obligations are unchanged.
`tests/fixtures/captured-array-strides.{c,out}` records four agreeing reference executions and two subject Linux executions over existing fixed storage, not completion of actual VLA allocation, inner variable parameter dimensions or scope-aware stack reclamation.

### Captured multidimensional parameters

Per reviewed S03, prototype-style definitions capture each variable array dimension once on entry after argument binding, retaining inner dimensions through outer-array adjustment and pointer-to-array declarators, while prototype-only and nested callback declarations never evaluate their bounds.
Array-bound composition uses a known constant size when either side provides one (different constants are incompatible), otherwise retains a specified capture before an unspecified variable or incomplete bound, without relaxing element-type or qualifier compatibility; inline parameter copies rename the capture identities before their types are copied.
`tests/fixtures/parameter-array-strides.{c,out}` records four agreeing reference executions and two subject Linux executions covering mutations of source parameters, dependent sizes, recursion, callbacks and independent constant/nonconstant inline calls; malformed noninteger bounds remain E2923 and prototype stars remain forbidden on definitions.
The separate `legacy-array-bound-probe.{c,out}` measures GCC/Clang disagreement (stored 2/3) for bound side-effect ordering against the first local initializer in an identifier-list definition, so this implementation does not admit that legacy path, infer a blanket inapplicability ruling for other legacy bounds, or claim actual VLA storage/reclamation complete.

### Local pointers to variable arrays

Per reviewed S02/S03, an automatic pointer declaration, including a fixed array of such pointers, captures its written variable array bounds once before its initializer; the pointer objects themselves retain their fixed storage size.
The same size-capture path serves typedefs, parameters and these local declarators, and inline substitution renames the capture in both canonical types and written declarator layers without weakening allocation identity checks.
`tests/fixtures/local-array-pointers.{c,out}` records four agreeing reference executions and two native subject executions over fixed storage, including sequential declarations, for initializers, register objects and unevaluated sizeof operands; noninteger bounds remain E2923 and entry bypasses remain E2945.
This does not admit actual variable-sized object storage, static variably modified pointers, legacy variable-bound definitions or variably modified typeof operands, whose remaining behavior and scope reclamation are separate work.

### Block-static pointers to variable arrays

Per reviewed S02/S03 and `tests/fixtures/static-array-pointers.{c,out}`, block-static pointers and fixed arrays of pointers keep one static object and one-time initialization while their written pointed-to dimensions are captured at each declaration execution, as both references agree at O0/O2.
Inline substitution retains one lifted static object and expresses each invocation's captured type as an ordinary address cast and checked memory place, including assignments, compound updates and array projections; it neither duplicates the static object nor weakens symbol/type validation.
The final fixture passes both interpreters and two subject Linux executions; entry bypasses remain E2945, top-level const remains nonmodifiable, and actual static VLA objects or linked variably modified declarations remain E2203.
This supersedes the preceding static-pointer deferral only, not the outstanding actual VLA allocation/reclamation or variably modified typeof work.

### Variably modified record members and prototype scope

Under the reviewed common-GNU-extension rule, a variably modified record member, including a typedef-hidden array or pointer to one, remains E2203; `tests/fixtures/variable-record-members.{c,out}` records GCC accepting all six reduced forms and Clang rejecting them.
The check follows the canonical member type rather than only written brackets, but dimensions scoped inside a callback's parameter prototype do not make the callback member itself variably modified; `prototype-array-members.{c,out}` records four agreeing reference and two subject native executions.
Eight unchanged torture originals require the non-consensus record extension and four additional originals require excluded nested definitions, individually evidenced in `variable-array-original-refusals.out` and bound to source digests in the existing disposition table.
These twelve are prospective evidenced-not-applicable classifications, not new PASS results or an exclusion of ordinary variable-length automatic objects, whose allocation and reclamation remain required.

### Static address offsets and exposure

Static numeric pointer offsets retain pointee scaling and all intermediate integer casts, and a pointer-width integer static initializer can hold a symbol plus a byte addend, as `tests/fixtures/static-addresses.out` records for both sealed compilers at O0/O2.
To retain Section 3.7's exposure when a symbolic initializer casts an address to an integer, including a cast back to a pointer, its relocation admits the optional fourth field `exposed`: `(offset, @symbol, addend, exposed)` exposes that target at module initialization, while the existing three-field form does not.
This field changes neither the emitted address bytes nor access checks; raw numeric addresses and ordinary pointer relocations do not expose unrelated objects, and E3013 remains required for an unexposed integer-derived dereference.
No diagnostic is allocated; the preceding survey attributes 29 refusals to numeric pointer offsets and 21 to integer address initializers, pending original-unit replay.

### Explicit position-independence options

The default address model retains EvoMIR Section 8's PIE-safe recipe, but explicit `-fno-pic`/`-fno-PIC`/`-fno-pie`/`-fno-PIE` selects direct page-relative addresses for strong undefined symbols, while weak undefined symbols retain nullable GOT addresses.
The last PIC/PIE option controls both code generation and the four `__PIC__`/`__pic__`/`__PIE__`/`__pie__` macros: `-fpic` and `-fpie` select level 1, uppercase variants level 2, PIC omits the PIE pair, and disabling the mode undefines all four before explicit command-line macro actions.
Both sealed compilers agree in `tests/fixtures/relocation-mode.{c,out}`; this is an explicit build-mode extension, not permission to disable PIE in an existing benchmark or to alter weak-symbol resolution.
No diagnostic is allocated; Kbuild replay must preserve these recorded options, addressing the observed nVHE `Unexpected RELA type 311` link failure without changing the kernel relocation checker.

### Address-only projections of static numeric pointers

In a static address initializer, `&*p` retains the numeric pointer value and `&p->member[index]` adds the existing target-layout projection offsets without reading the pointed-to storage, including a zero base used by the kernel's offset tables.
Both sealed compilers at O0/O2 execute `tests/fixtures/absolute-place-addresses.{c,out}` with the asserted null, 8, 20 and 0x1010 values.
This creates no allocation provenance and does not admit a load, a bit-field address or a nonconstant subscript; existing runtime access diagnostics remain unchanged.
No diagnostic is allocated; the two measured residual units are `drivers/tty/sysrq.o` and `net/core/neighbour.o`.

### Pointer arithmetic over zero-sized elements

Arithmetic and indexing over a GNU zero-sized element evaluate the index normally and produce zero byte displacement, preserving the original pointer identity.
Both sealed compilers at O0/O2 execute `tests/fixtures/zero-sized-pointer-offsets.{c,out}` with zero exits, including empty records, zero-length array elements, compound updates and prefix/postfix operations.
Lowering consumes the evaluated index in a zero product and uses a positive unit-scale `ptr.offset`; neither its validator invariant nor runtime poison, lifetime and actual byte-access checks are relaxed.
No diagnostic is allocated; this addresses the five measured zero-scale failures in `fs/super.o`, `kernel/auditsc.o`, `kernel/rcu/tree.o`, `net/core/sock.o` and `net/netlink/af_netlink.o`.

### Small aggregate register assembly operands

A four- or eight-byte record used as a general-register assembly operand transfers its complete object representation as `bits<32>` or `bits<64>`, not its address or a guessed active union member.
Outputs use the existing write-only/read-write rules and write back all representation bytes on every outgoing edge; explicit register inputs load the representation, while memory operands keep their address-only behavior.
Both sealed compilers at O0/O2 execute `tests/fixtures/aggregate-register-operands.{c,out}` with zero exits; existing access guards, effect tokens and reached-assembly E3021 remain unchanged, and other aggregate sizes retain E3021.
No diagnostic is allocated; this addresses the five measured nVHE ticket-lock union operand failures.

### Numeric pointer round trips in integer case labels

An integer case label formed by casting an integer constant to a pointer and back retains the target-width numeric value, without allocating or accessing an object.
Both sealed compilers at O0/O2 execute `tests/fixtures/address-case-constants.{c,out}` with zero exits for the kernel's zero/one signal-info sentinels and a distinct default case.
The shared static integer evaluator processes the numeric casts through its existing iterative machine and explicit fuel budget; this does not make a symbolic address a known integer or change executed pointer provenance checks.
No diagnostic is allocated; the measured original unit is `kernel/signal.o`.

### Packed relocation alignment correction

The former Section 2.1 eight-aligned offset requirement conflicted with `tests/fixtures/packed-pointer-relocations.{c,out}`, where both sealed compilers at O0/O2 accept initialized pointers at offsets 2 and 12 in a 20-byte packed array.
The two measured ACPICA units are `drivers/acpi/acpica/exdump.o` and `drivers/acpi/acpica/rsdumpinfo.o`; their record has two byte fields before the pointer.
Revision 1.9.91 follows the user's authorization to adopt [AArch64 ELF Section 5.7.5](https://github.com/ARM-software/abi-aa/blob/main/aaelf64/aaelf64.rst#575-static-data-relocations): ABS64 writes eight bytes at a byte-aligned place; pointer width is independent of access alignment.
No diagnostic is allocated or source layout changed; bounds, nonoverlap, initializedness, target identity, provenance and executed access alignment checks remain required.

### Indirect entry into a captured type scope

The complete address-taken successor set of Section 3.9 is also used to check entry into captured variably modified scopes; bypassing such a declaration reports the existing E2945 at the indirect goto, including when its written operand names another member of that conservative set.
In `.evidence/builtin-probes/20260913-computed-edges/capture-{entry,unselected}.c`, sealed GCC-14 accepts both forms while Clang-19 rejects both for bypassing the VLA typedef; `capture.out` records the conflict, resolved toward refusal under the existing review protocol.
Entering an already active capture scope remains allowed, and its hidden dimension object address travels with the other live storage addresses rather than losing the dimension or recapturing it.
No diagnostic is allocated, and no additional kernel unit count is claimed for these scope probes.

### Explicit automatic initialization and hosted environment options

`-ftrivial-auto-var-init=zero` adds a zero representation initializer, including padding, at each executed declaration of an otherwise uninitialized automatic object; typed pointer subobjects receive Null provenance, explicit member values are unchanged, and the declaration attribute `uninitialized` exempts that object.
The final `-ftrivial-auto-var-init=uninitialized` restores ordinary uninitialized semantics; storage activation or merely transporting its address does not execute the implicit initializer.
`-ffreestanding` sets `__STDC_HOSTED__=0` and disables ordinary-name library builtin substitution, `-fhosted` restores both, and later explicit `-fbuiltin`/`-fno-builtin` options override substitution independently; reserved `__builtin_*` spellings remain available.
On the admitted AArch64 Linux target, `-pthread` predefines `_REENTRANT` to `1` before all explicit `-D`/`-U` actions, regardless of the option's position among those actions. Repetition is idempotent; without the option the macro is not predefined. User actions still apply in their written order and may undefine or replace it. This is the compile-time option contract; the assembly-only driver does not acquire a linker, and the final link must retain the target runtime's thread-link options. It does not create threads or expand interpreter concurrency coverage.
Both sealed compilers at O0/O2 execute `tests/fixtures/automatic-zero.{c,out}` with exit zero; the option occurs in all 2,799 frozen commands, but this targeted measurement is not a full-kernel result and introduces no diagnostic.

### Automatic constructor padding

With `-ftrivial-auto-var-init=zero`, an automatic brace initializer also zeroes representation gaps between and after its members and beyond the selected union member, recursively through arrays and records; explicit member expressions still execute exactly once without implicit initialization of their own destinations.
This includes block-scope compound literals on each evaluation, before any subsequent representation copy; `automatic-compound-padding.c` reproduces Poly1305's whole-context reset, with zero exits for EvoCC and sealed GCC-14/Clang-19 at O2 in `.build/floating-targeted/kunit-regressions-20260922/native.json`.
The declaration's `uninitialized` exemption and final `uninitialized` option retain the ordinary behavior, and whole-aggregate copy initializers retain the source representation, including padding and provenance, rather than clearing it.
Sealed GCC-14 and Clang-19 both return zero at O0/O2 for `tests/fixtures/automatic-initializer-padding.c`; `.evidence/builtin-probes/20260914-constructor-padding/oracles.out` records the results (SHA-256 `dc452fbe6e4815d273b8330f3cbbe3f3fba09db4ec0a270b5a55bfec97393082`).
This clarifies the earlier explicit-initializer statement without changing member values or aggregate-copy semantics; the first full subject KUnit run exposed 28 failures in one `stackinit_kunit.o` unit, and no diagnostic is allocated.

### Strong frame protection and native command options

With final `-fstack-protector-strong`, lowering marks functions containing automatic arrays (including aggregate members), dynamic stack allocation or source references to local-frame addresses; addressable parameters count as frame objects, and a definition's `no_stack_protector` attribute or final `-fno-stack-protector` suppresses the mark.
The canonical function header adds optional ` stack_protect` after its optional alignment, omitted when false, preserved through textual/proof reconstruction; it requests a native guard check, not an extra abstract C operation or implicit EvoIR token channel.
`tests/fixtures/protected-frames.{c,out}` and `protected-frames-failure.c` give native exits 0 and 77 for the subject and both sealed compilers; GCC/Clang differ on addressed-parameter selection, for which this profile explicitly chooses protection, while option-dependent predefined macros retain the frozen GCC-14 values of Section 10.4.
The remaining original options independently select the guard's symbol or system-register/byte-offset source, return-address PAC and general-register-only generation; the 2,732 protected-command and 2,792 PAC-command counts overlap, introduce no diagnostic and are not boot evidence.

### Effective optimization and address-model options

The final `-O0` selects `__NO_INLINE__=1`, positive optimization levels select `__OPTIMIZE__=1`, and `-Os` also selects `__OPTIMIZE_SIZE__=1`, before explicit command-line macro overrides; these settings do not promise GCC's optimization algorithms or change C definedness.
`-mcmodel=tiny` uses short PC-relative symbol addressing (including nullable GOT references), and requested unwind tables describe the actual generated frame, saved registers and return-address authentication state.
`tests/fixtures/unwind-and-tiny.{c,out}` records the two sealed compilers' macro values and successful native backtraces through over-aligned/dynamic frames, plus subject execution with and without PAC.
No diagnostic is allocated; all 2,799 command records contain optimization options and two request the tiny model/unwind tables, with full compile/boot still a separate measurement.

### Return-address extraction on the fixed AArch64 target

`__builtin_extract_return_addr(p)` evaluates its pointer operand exactly once and returns that same address and provenance, without an external call; `return_address` already performs the target's return-address normalization.
Both sealed compilers at O2 execute `tests/fixtures/extract-return-address.c` with exit zero, including a side-effecting operand and a null address (`resolved-compiler-queries.out`).
No diagnostic is allocated; the original `lib/vsprintf.o` emission exposed one observed kernel unit with an unintended unresolved builtin call, not a completed link measurement.

### Symbol linkage marks inside a named pointer declarator

The symbol-only `weak`/`alias` rules of Section 5.2 also govern a mark after `*` in a named declarator: it describes the declared object or function, not the pointer type; retain its written placement and span once in the declaration inventory.
This does not apply pointer type attributes such as `aligned` or `mode`, change layout, or propagate a parameter's mark to its enclosing function.
Both sealed GCC-14 and Clang-19 at O0/O2 emit the weak function/object bindings and one-address alias in `tests/fixtures/pointer-symbol-linkage.{c,out}`, and link `pointer-symbol-override.c` with exit zero.
The existing E3060/E3061 checks remain; no diagnostic is allocated, and run `20260913T024049Z` exposed four duplicate definitions in three original kernel units before this correction.

### Scope-exit cleanup: authorized body correction

Revision 1.9.96 adds the user-authorized `cleanup` exception to Section 10.1.9's preserved-only default.
Both sealed GCC-14 and Clang-19 at O0/O2 execute `tests/fixtures/cleanup-scope-exits.c` with exit zero, while both subject interpreters return program value 2 and native execution exits 2, as `cleanup-scope-exits.txt` records.
The current complete kernel diagnostic stalls on the second IRQ descriptor lock acquisition because `irq_modify_status` never calls its scoped guard destructor; this is one measured boot failure, not a claimed count of all affected units.
The implementation uses ordinary reverse-order calls before object lifetime end on each actual scope exit; interpreter/native probe success does not establish a successful kernel boot.

### Raw variadic arguments after GNU comma elision

For `, ##__VA_ARGS__` (including named GNU variadics), a present argument is substituted without argument prescan; normal replacement-list rescan may then supply its raw tokens to a nested macro's `#` or `##` operand.
Elision of an omitted argument and retention of an explicitly empty argument are unchanged; suppressing the comma paste does not authorize early expansion of the argument.
Both sealed GCC-14 and Clang-19 produce `29, 29, 12, 31` for the four independent lines in `.evidence/builtin-probes/20260914-variadic-paste/input.c`, as `oracles.out` records; no diagnostic is added.
The partial expanded KUnit/module survey exposes 20 E0110 units using Nouveau's variadic dispatch idiom, selected in `.evidence/kunit/variadic-replay-20260914/units.txt` alongside two stray-token refusals; only original-unit replays establish the repair's actual coverage.

### Narrowed low-bit masks of unsigned wide fields

An explicit cast to a non-Boolean unsigned integer no wider than the field may consume a direct unsigned wide-field read ANDed with a nonnegative integer constant mask contained within the field's storage width; both intermediate precisions preserve the resulting low bits.
This discharges only that AND operation's E2939, preserving evaluation and all nested or neighboring refusals; it does not admit arbitrary wide-field arithmetic or change its uncast type-query boundaries.
Both sealed compilers at O0/O2 execute `tests/fixtures/narrowed-wide-field-mask.c` with exit zero, including both operand orders, 16/32-bit destinations and one pointer-call evaluation; `.evidence/builtin-probes/20260914-narrowed-wide-mask/oracles.out` records the values.
The two measured Nouveau refusals use `lower_32_bits` on a 40-bit offset; their original-unit replay remains required, and no diagnostic is added.

### Named SIMD clobbers in scalar code

The physical-register clobber rule includes `v0` through `v31`; these names are carried separately from `x0` through `x30` and never reserve a same-numbered general register.
Normal AAPCS64 emission preserves the low 64 bits of clobbered `v8` through `v15` on every return, including asm-goto paths, with ordinary frame addressing and unwind locations; other SIMD clobbers need no callee save, and no vector-valued operand support is implied.
Explicit `-mgeneral-regs-only` continues to forbid compiler-generated SIMD accesses: both sealed compilers with an explicit `.arch_extension simd` accept the probe but do not preserve those lanes in this mode, whereas both preserve them at ordinary O0/O2, as `.evidence/builtin-probes/20260914-vector-clobbers/oracles.out` records (general-only exit one is a negative control, not a pass).
One measured module unit, `drivers/net/ethernet/mellanox/mlx5/core/wc.o`, names `v0` through `v3` after `kernel_neon_begin`; its original replay remains required, and malformed names remain E3021.

### GNU alignment spelling and local-label declaration lists

`__alignof` is the same operator as `__alignof__`, including type and unevaluated-expression operands and declared object alignment.
`__label__` accepts a comma-separated list of identifiers, each receiving its own identity in the same existing block-local label scope; nested scopes do not share those identities.
Both sealed GCC-14 and Clang-19 at O0/O2 execute `tests/fixtures/alignof-alternate-spelling.c` and `tests/fixtures/local-label-list.c` with exit zero, as `.evidence/builtin-probes/20260914-gnu-parser-forms/oracles.out` records; no diagnostic is added.
The frozen expanded-configuration survey exposes one original zstd unit at the alignment spelling and one ALSA unit at the first local-label comma; scope and malformed-list checks are unchanged.

### Pure floating arithmetic resolved to integer values

Pure finite binary32/binary64 constant arithmetic may resolve to an integer before execution, using each operation's source precision, the conditional arms' common type, and the destination's ordinary truncating conversion; initialization, assignment, arguments and returns use that same conversion.
This does not make a floating arithmetic expression an integer constant expression, admit runtime floating object reads, saturate out-of-range conversions, evaluate an unselected branch, or approximate binary128 with binary64.
Both sealed GCC-14 and Clang-19 at O0/O2 execute `tests/fixtures/floating-constant-integers.c` with exit zero (recorded in the adjacent `.out` file), including binary32 rounding, negative truncation and an integer-selected conditional with a floating common type.
Seven original expanded-configuration units are affected; no diagnostic is allocated, and their compile/assembly replay is separate from kernel execution.

### Library primitives used by fortified string wrappers

`__builtin_strcat`, `__builtin_strncat`, and `__builtin_memchr` use the ordinary external prototypes `char *(char *, const char *)`, `char *(char *, const char *, size_t)`, and `void *(const void *, int, size_t)` through the existing unit-local builtin namespace, evaluating each argument once and introducing no intrinsic or new interpreter library model.
Existing E2909 arity, E2913 argument-conversion, and E2927 visible-declaration checks remain unchanged.
Both sealed GCC-14 and Clang-19 execute `tests/fixtures/fortified-library-builtins.c` at O0/O2 with exit zero, recorded in `.evidence/builtin-probes/20260914-fortified-library/oracles-consensus.out`; GCC's warning about the deliberately bounded append is retained, and the earlier Werror probe is recorded separately.
Two selected original BPF units in the supplemental FORTIFY/BTF configuration first refuse in `fortify-string.h`; complete build and runtime coverage are measured separately rather than inferred from this prototype rule.

### Numeric pointer comparisons in static integer initializers

A static integer initializer may compare pure numeric address constants using their unsigned target-width address bits, preserving intermediate integer casts and requiring no object access or symbolic address guess.
This does not admit pointer comparisons as source integer constant expressions, nor reads of pointer objects or function calls as static constants.
Both sealed GCC-14 and Clang-19 execute `tests/fixtures/static-numeric-pointer-comparisons.c` at O0/O2 with exit zero, including all six comparisons, an incomplete pointee type and a narrowed unsigned source cast; evidence is `.evidence/builtin-probes/20260914-static-pointer-order/oracles.out`.
The two measured supplemental-configuration refusals are `arch/arm64/kvm/arm.o` and `arch/arm64/kvm/handle_exit.o`; no diagnostic is added and original-unit replays remain required.

### Legacy full memory barrier

Per F07, `__sync_synchronize()` has type `void`, takes no arguments, and performs a full memory barrier on the fixed AArch64 target; it is not a constant expression and introduces no external function declaration.
Lowering uses the existing ordered volatile assembly representation with template `dmb ish`, a `memory` clobber, no input/output values, and the current explicit `mem`/`world` tokens; both interpreters retain reached-assembly E3021 rather than pretending to model concurrent execution.
Both sealed GCC-14 and Clang-19 at O0/O2 execute `tests/fixtures/full-memory-barrier.c` with exit zero and emit `dmb ish` between the two stores, as the adjacent `.out` records; unevaluated operands do not execute the barrier and ordinary E2909 arity checks remain.
One measured SQLite core-amalgamation unit first refuses at this builtin; no diagnostic or IR instruction is added, and its original suite replay remains a separate coverage measurement.

### Small integer-vector calling convention boundary

The reviewed integer-vector profile supports local sub-8-byte vector operations, but its native external argument/result ABI remains E4002 where GCC-14 uses GPRs and Clang-19 uses SIMD registers.
Both sealed self-controls pass and both crossed controls fail for `tests/fixtures/integer-vector-small.c` and `integer-vector-small-driver.c`, recorded in `tests/fixtures/elf-vector-tls.out`; the supported 8/16-byte ABI is independently exercised in nine passing combinations.
No new diagnostic is allocated, and no original benchmark count is inferred from this two-file ABI probe.

### Integer-complex values and division boundary

Per F05, GNU integer `_Complex` uses two adjacent real/imaginary components of its declared integer type, with component-width arithmetic, per-component conversions, lvalue real/imaginary projections, conjugation, and ordinary composite AAPCS64 passing rather than vector or floating ABI classification.
`tests/fixtures/integer-complex-{values,operations,initialization,profile,abi-worker,abi-driver}.c` and `language-semantics.out` record sealed GCC-14/Clang-19 O0/O2 agreement for the positive values, layouts and six ABI self/crossed controls; no new original-program coverage count is claimed.
Integer-complex division and compound division report E2948 `integer-complex-division-oracle-conflict`: the retained profile probe gives 4+3i under GCC and 5+1i under Clang for `(9+7i)/(2+1i)` at both optimization levels, so this non-floating boundary is not assigned either answer silently.
Component overflow, uninitializedness, qualification and ordinary object/provenance checks remain in force, and floating-complex execution remains deferred.

### Legacy scalar atomic read-modify-write

Per F07, the naturally aligned 1/2/4/8-byte `__sync_*` fetch/new-result add, sub, bitwise and/or/xor/nand, compare-exchange and lock operations evaluate operands once and use explicit `atomic.rmw.<op> <order> [volatile] %mem, %world, %address, %value [, %expected] : T`, producing the old value and the new mem/world pair; compare-exchange compares with the optional expected operand and failed comparison does not store.
Arithmetic is width-truncated; pointer exchange/compare-exchange preserve the complete pointer representation, seq_cst is retained except acquire lock-test-and-set and release lock-release, and existing type/order/alignment diagnostics apply.
`tests/fixtures/atomic-{read-modify-write,rmw-widths,rmw-worker,rmw-driver}.c` and `language-semantics.out` record sealed values and both native mixed-link threaded checks; these checks do not claim a weak-memory-model proof or a full benchmark result.
The AArch64 implementation uses the width-correct exclusive load/store loop, preserves the compare-exchange failure path and branch target/input registers, and keeps these operations outside the equality-optimization allowlist.

### Variable-array type queries and zero initialization

Following S02's variable-object authorization, `typeof` type-name bounds capture once when the declaration executes, and an array-typed variable-length expression operand evaluates once while preserving its operand-local declaration scope; these cases supersede the old blanket E2904 refusal based on absence of variable objects.
Expression operands of other variably modified types retain E2904: `tests/fixtures/variable-array-typeof-profile.c` shows GCC leaving the counter zero and Clang incrementing it for pointer-to-VLA operands, while both increment it for array operands at O0/O2 (`language-semantics.out`).
The existing automatic-zero option applies to dynamic array extents too, including representation padding and typed Null pointer subobjects, without initializing declarations when the option is absent.
The sealed type-name and array-expression probes agree; the scalar-only and final expanded zero probes return one under GCC O2 and zero in the other three modes (an intermediate integer/record expansion returned zero in all four), a retained discrepancy that does not override the existing explicit option rule, and no new diagnostic or campaign gain is claimed.

### Constant floating object representations without floating execution

A pure finite binary32/binary64 constant can supply an object's initialized representation or a discarded simple memory assignment: round to the destination format and store its bits using the existing width, alignment, volatility and object checks, without manufacturing an integer-valued C expression.
Implicit zero initialization also supplies those formats' positive-zero representation, while runtime floating reads, conversions, used assignment results, floating ABI and atomic floating stores remain outside this slice; integer literal arithmetic follows the existing executed source-constant rule, while runtime-variable overflow and invalid division retain their obligations.
Both sealed compilers at O0/O2 and the subject under both native assemblers pass `tests/fixtures/constant-floating-representation.c`, including negative zero, binary32 rounding, projected stores, VLA storage, an unselected floating member and a volatile union (`language-semantics.out`).
This reduces torture `20020225-2`, `930930-2` and `20031214-1` without claiming their original-program results or a full floating-point implementation, and allocates no diagnostic.

### Quiet NaN constant object representations

Literal `__builtin_nanf`, `__builtin_nan` and `__builtin_nanl` payloads are represented exactly in binary32/64/128, with the low significand bits retained and the quiet bit set; the empty string means zero payload, and complete unsigned decimal/octal/hexadecimal strings follow their written base, including arbitrarily long inputs.
Same-format object initialization and discarded memory assignments use those bytes (unary minus changes the sign bit), without routing payloads through host floating arithmetic; dynamic floating values and cross-format NaN conversions retain the existing execution boundary.
The fixed ARM64 GCC-14/Clang-19 O0/O2 and additional edge measurements in `tests/fixtures/nan-constant-representation.out` agree on these representations; signed/space-prefixed strings, empty hexadecimal digits and embedded NUL differ and are not constant-projected, while malformed strings and dynamic operands follow the ordinary typed `nanf`/`nan`/`nanl` call boundary (E2208 where a static constant is required, E3049/E3050 for deferred floating execution).
This closes the representation-only construct in LLVM `ieee/builtin-nan-1.c`, not floating arithmetic, a full benchmark result or a newly allocated diagnostic.

### Variable-array capability declaration

With S02 automatic variable-array storage implemented, the target overlay no longer defines `__STDC_NO_VLA__=1`; the immutable oracle macro snapshot and explicit user `-D`/`-U` precedence are unchanged.
The small `tests/fixtures/variable-array-capability.c` checks the absent refusal macro and two runtime extents; sealed GCC-14 and Clang-19 at O0/O2 all exit zero (`language-semantics.out`).
This supersedes the earlier capability declaration made while variable objects were unavailable, without admitting variable record members or the separately refused GNU typeof evaluation conflict.
No diagnostic or original-program result is changed by this record.

### Integer scalar broadcast into unsigned vector lanes

Under the reviewed integer-vector profile, an integer scalar no wider than an unsigned lane converts to that lane's unsigned representation before broadcast, including a negative signed value; signed lanes accept the whole source range only when it fits.
A wider constant may broadcast if its value fits the destination range, additionally admitting a negative value at least the signed minimum of an unsigned destination's width, with the same unsigned conversion.
`tests/fixtures/vector-scalar-conversions.out` records the sealed GCC-14/Clang-19 matrix and the original `pr94412` O0/O2 values; unsigned-to-equal-width-signed variables, out-of-range positive signed-lane constants and below-signed-minimum negative unsigned-lane constants remain E2102 at the measured disagreements or joint refusals.
This repairs one original torture first-error construct without permitting arbitrary truncating broadcasts, changing lane arithmetic obligations, or claiming a full campaign gain.

### Inline-mode option conflict

The final effective combination `-std=gnu89 -fno-gnu89-inline` is refused at the driver option boundary; a later `-std=gnu11` changes that combination and remains accepted.
Sealed GCC-14 rejects the former even for an empty input while Clang-19 accepts it (`tests/fixtures/gnu-inline-definition-chain.out`), so the existing conflict rule selects refusal rather than implicitly changing GNU89 inline semantics.

### Explicit GNU99 build mode

`-std=gnu99` selects the existing non-legacy GNU expression/declaration profile with `__STDC_VERSION__=199901L`, no `__STRICT_ANSI__`, and ISO inline semantics unless the explicit GNU-inline override applies; normal undeclared-call rejection remains, and the last language option wins before user macro overrides.
The shared parser already implements the admitted C99 constructs and agreed later-standard GNU extensions; this option does not enable the GNU89 implicit-declaration policy or alias strict ISO C89/C99 options.
Both fixed ARM64 compilers at O0/O2 and both subject interpreters/native assembler combinations pass `tests/fixtures/gnu99-language-mode.{c,out}`; this addresses the 34 original Lua mode refusals in run `20260915T152156Z`, without predicting how many complete objects pass afterward and without allocating a new diagnostic.

### Original-build ISO keyword modes and GNU17

The admitted project profile also consumes `-std=c99`, `-std=c11` and `-std=gnu17`: their version macros are respectively `199901L`, `201112L` and `201710L`, and only the ISO modes define `__STRICT_ANSI__=1` and treat bare `asm`/`typeof` as identifiers while retaining reserved `__asm__`/`__typeof__` spellings.
All three retain normal undeclared-call rejection, ISO inline defaults subject to the explicit GNU-inline override, the existing admitted type/execution boundaries, final-language-option precedence and later user macro overrides; this is not an admission of C89, C23 or previously deferred types and operations.
Both fixed ARM64 compilers at O0/O2 agree on the macro/keyword matrix and execute the sources in `tests/fixtures/language-mode-versions.out`, with ten subject-native mode/assembler passes; affected first errors are musl's 1,339 C99 records, Mbed TLS's 176 C99 and 206 C11 records, and seven LLVM records in the completed `20260915T151427.374334Z` campaign, not predicted new passes.
No diagnostic code is allocated; this is an option error, and no affected benchmark count is inferred from the probe.

### Language-mode lexical boundaries

Before C11, `u`, `U` and `u8` are identifier tokens, not literal prefixes: this classification precedes macro expansion and applies equally to main/stdin/include inputs, command-line replacements and token-paste validation, while `L` remains a wide prefix.
The fixed ARM64 matrix in `tests/fixtures/language-mode-lexical-boundaries.out` records GCC's GNU99 UTF-prefix extension versus Clang's refusal, and Clang's GNU89 declaration-for extension versus GCC's refusal; neither extension is admitted, so GNU89 for initializers remain expressions or empty and existing E1001/E0110 token/context diagnostics apply.
Ordinary identifier macro replacement still applies to those pre-C11 tokens; the change does not reinterpret raw bytes, admit new wide encodings, or disable the separately admitted underscore-prefixed type extensions.

### Original-build ISO C89 mode

`-std=c89`, `-std=c90` and `-ansi` select the legacy declaration and GNU-inline policies, define `__STRICT_ANSI__=1`, leave `__STDC_VERSION__` undefined, and treat bare `inline`, `restrict`, `asm` and `typeof` as identifiers; GNU89 likewise treats bare `restrict` as an identifier but keeps its other GNU keywords.
Reserved extension spellings remain available, preprocessing still treats every keyword spelling as an identifier, pre-C11 literal tokenization applies, and declaration-for initializers remain refused under the existing legacy-mode rule; the last language option wins before user macro overrides.
Both sealed ARM64 oracles agree on this keyword/macro matrix and execute `tests/fixtures/iso-c89-language-mode.c` at O0/O2; this addresses the original two cJSON mode refusals without admitting floating execution or predicting object/test results.
The final `-fno-gnu89-inline` conflict boundary applies to both C89 and GNU89; no new diagnostic code is allocated.

### Initialized const integers in static initializers

In a static object's integer initializer, a preceding explicitly initialized non-volatile, non-atomic const integer object may supply its declared-type-converted constant value, including an automatic or thread-local const object's explicit constant initializer; unknown values and declarations without a written initializer remain E2208.
This GNU initializer extension uses the existing width-typed constant operators, affects only the initialization plan, and does not turn object reads into source ICEs or change the separately fixed `__builtin_expect` evaluation contract; scope and failed-declaration rollback include these bindings.
The fixed ARM64 GCC-14/Clang-19 O0/O2 matrix in `tests/fixtures/static-const-initializers.{c,out}` records agreement on initializer cases and refusals, the retained E2912 ICE conflict, and the existing expect profile difference; no diagnostic is allocated.
One observed supplemental-kernel unit, `drivers/net/netkit.o`, reaches this rule and its repaired original replay assembles with exact reference symbol parity, without changing the ongoing survey's frozen results.

### Named residual lowering classes

The existing refusal of a floating scalar, floating complex or homogeneous floating aggregate call interface is E3049 (`not-lowered: floating-call-interface`), and a required floating scalar value or conversion in an integer-only lowering path is E3050 (`not-lowered: floating-value`).
A call whose actual arity differs from its resolved canonical interface retains F01's refusal as E3051 (`call-arity-mismatch`), rather than the generic E3101 wrapper; these names change neither acceptance nor execution semantics.
The source cases in `crates/evocc-lower/tests/fail_closed_forms.rs` and the source-bound original LLVM audit in `docs/notebook/benchmark-evaluation-20260915.md` distinguish the 98 observed call-interface stops from scalar value stops and `20051012-1.c`'s non-floating arity failure.
No diagnostic alone proves whole-program FP necessity or not-applicability, and other unsupported types, malformed C-HIR and generated-IR validation failures are not relabeled as floating point.

### Raw-byte escape serialization

Within an admitted ordinary narrow literal, a written backslash immediately followed by a raw non-UTF-8 byte is a GNU unknown escape producing that byte; pairs of written backslashes still produce literal backslashes, and the original token bytes and spans remain unchanged.
The UTF-8-safe equivalent spelling consumed by existing literal decoders must preserve escape boundaries rather than turn an introduced numeric escape into literal digits; this adds no permission to invalid-encoding metadata, prefixed literals or macro stringization.
The fixed ARM64 GCC-14/Clang-19 O0/O2 byte and signed/unsigned-char measurements in `tests/fixtures/raw-byte-escape.out` agree, with oracle warnings retained and no new diagnostic allocated.
No original benchmark incidence is claimed: this is a boundary regression discovered during the approved raw-source-byte review.

### Qualification in pointers to arrays

The immediate non-atomic cv-qualification conversion rule also applies to the element type through consecutive array layers in initialization, assignment, arguments and returns; compatible bounds may combine a known extent with an incomplete extent, but different known extents remain incompatible.
The conversion stops at an intervening pointer, preserves atomic distinctions and does not change `__builtin_types_compatible_p`, object mutability or layout.
The fixed ARM64 GCC-14/Clang-19 O0/O2 matrix in `tests/fixtures/array-pointer-qualification.out` agrees on six executed forms; GCC rejects the three incompatible-bound/nested-pointer/atomic forms that Clang only warns about, so those retain E2202 (E2102 in assignment).
This addresses the two observed supplemental-kernel `security/landlock` units; no new diagnostic or whole-configuration result is implied.

### Read-only static object storage

A global may carry optional `readonly=true` immediately after its `initialized` bitmap and before `section`/`relocs` in the shared EvoIR/EvoMIR global syntax; omission preserves old semantics (string-literal storage remains implicitly read-only), while C lowering sets the property for const-qualified static objects, following array elements but not pointees or individual record members.
Initialize its bytes and relocations before freezing the existing object against stores with E3014, including each TLS instance without changing its thread owner, and preserve this property in text, module identity, proof reconstruction and machine lowering; this does not license folding volatile reads.
Automatic native placement is read-only data even for all-zero objects, except that PIC objects with relocations use writable-during-relocation read-only-after-relocation data (`.data.rel.ro` on Linux); TLS sections and explicit user section placement retain their existing rules, with no permission inferred from section names.
`tests/fixtures/static-readonly-storage.out` records fixed ARM64 GCC-14/Clang-19 O0/O2 agreement for automatic placement and the disagreement for an explicit `.data.explicit` section (existing placement retained); the observed impact is two const pointer tables in one supplemental-kernel `mm/page_alloc.o`, not a new boot result or new diagnostic.

### GCC AArch64 header interface

The GCC14 target-header profile includes nominal 64/128-bit half, bfloat, floating and polynomial vector types, polynomial scalars with 1/2/8/16-byte layouts, and the 90 `val`-array tuple typedefs introduced by `#pragma GCC aarch64 "arm_neon.h"`; these are real canonical types and records, not header substitutions or size-only aliases.
Builtin prototypes are the exact versioned interface in `crates/evocc-types/src/neon-signatures.tsv`, reproducible by `tools/extract-aarch64-builtins.py`, including pragma-injected reinterpret declarations; declaration-only floating bodies are type-checked without admitting floating execution, and scalar broadcast retains the measured non-truncating conversion boundary.
`tests/fixtures/arm-neon-target.out` records fixed-image interface extraction and the public header layout/runtime controls, without claiming Clang exposes GCC's private names or polynomial nominal identity.
This addresses the header construct seen in six supplemental-kernel units, one LLVM program, 37 zstd objects and 291 Mbed TLS objects; these are observed first-error populations, not promised successful-program counts.

### Executed integer target intrinsics

The registered integer `ld1`/`st1`, immediate left/right shifts, single-table byte lookup, equal-representation reinterpretation and byte polynomial multiply lower to the existing vector, integer and memory instructions, evaluating operands once with declared-parameter conversions; loads/stores require element alignment, table indices outside the table return zero, and polynomial multiplication is the low eight bits of the carryless product.
Immediate operands must be source integer constant expressions after their declared parameter conversion (left shift 0..N-1, right shift 1..N); neither optimizer-discovered constants nor GCC-only comma folding relax that requirement, and a full-width right shift still consumes its source lane's initializedness.
Registered but unimplemented target operations and unsupported immediate operands report E3052 (`target-builtin-not-executable`, with the builtin named) rather than becoming external library calls; wrong arity/type retains E2909/E2913 and floating execution retains its existing boundary.
The fixed-image byte-table, memory, shift and exhaustive polynomial probes in `tests/fixtures/arm-neon-target.out` define this bounded extension, not an admission of every registered intrinsic; five original RAID6 objects have targeted assembly/symbol evidence, not a new configuration boot result.

### SIMD assembly constraint remains pending

The reduced `tests/fixtures/arm-neon-assembly.c` builds and returns zero with both fixed ARM64 compilers at O0/O2, matching the `=w`/`w` requirement newly exposed in `arch/arm64/lib/xor-neon.c:188`.
Section 3.11 still explicitly refuses constraints outside its table, so this measurement does not authorize a validator-only relaxation or silently selecting a general register for a SIMD operand; E3021 remains pending a SIMD operand/register/modifier and machine-binding contract.
The affected original replay and the four oracle results are recorded in `tests/fixtures/arm-neon-target.out`; one supplemental-kernel unit remains refused, and this is not an FP deferral.

### Integer target narrowing, insertion and interleaving

Registered integer vector `shrn_n` operations logically shift the source lane and keep the low half-width result (signedness does not change those bits); the converted source-ICE count is 1 through the destination lane width, while left/right insertion preserves the unshifted low/high part of the first lane and inserts shifted bits from the second, with counts 0..N-1 / 1..N respectively and each operand evaluated once.
Registered integer/polynomial `ld2`/`ld3`/`ld4` operations read element-aligned ordinary memory into the existing canonical tuple: `result.val[j][i] = source[i*count+j]`; they preserve normal access faults and initializedness, do not assert atomicity or vector alignment, and introduce no new IR instruction.
`tests/fixtures/arm-neon-interleave-shifts.{c,out}` records fixed ARM64 GCC-14/Clang-19 O0/O2 value agreement and the 24 rejected immediate-boundary builds; unsupported operations/counts retain E3052, with no new code allocated.
One original zstd `zstd_lazy.c` unit in `20260919T125112Z` reaches these constructs after header admission; targeted and whole-program retests, not this rule, determine its closure.

### Array members of record values

An array selected from a record value decays in value contexts as specified by C11 6.3.2.1p3; the record-producing expression is evaluated once, whether it is a call, comma, conditional or assignment, without treating the record itself as an addressable lvalue.
Nested member selection preserves the base expression's value category, and conditional record values retain their record type in both consumers rather than entering integer conversion.
`tests/fixtures/returned-array-members.{c,out}` records fixed ARM64 GCC-14/Clang-19 O0/O2 agreement for reads within the containing full expression; this does not extend temporary lifetimes or authorize modifying a temporary (C11 6.2.4p8).
The observed trigger is the chained array/vector selection on an interleaved NEON result in a targeted negative test, not a newly counted application unit; no diagnostic code changes.

### Integer target vector combination

Registered integer and polynomial `combine` operations concatenate the low 64-bit operand and the high 64-bit operand into the corresponding 128-bit vector, preserving lane bits and evaluating both operands once.
The scalar `di` interface has the same representation rule; floating variants are not admitted by this extension.
`tests/fixtures/arm-neon-combine.{c,out}` records fixed ARM64 GCC-14/Clang-19 O0/O2 agreement and subject native executions; the original Mbed TLS `chacha20_neon.c` now emits but is not yet counted in a new passing-build numerator.
No diagnostic is allocated and unsupported target operations retain E3052.

### Variable array type names in casts

An executed cast with newly written variable array bounds captures them once on that expression path, using the existing ordinary size objects and captured-stride rules; an unselected expression does not evaluate them.
A `sizeof` operand of variable array type executes its operand before reading the captured extent, whereas an operand of fixed pointer size remains unevaluated.
`tests/fixtures/variable-array-cast.{c,out}` records eight agreeing fixed ARM64 oracle executions with and without the original memory-assembly idiom, plus subject interpreter and native checks; it covers Mbed TLS `platform_util.c` without source changes.
This extends S02/S03, preserves the variably modified `typeof` disagreement boundary, and allocates no diagnostic.

### AES integer target round operations

The four registered AArch64 AES interfaces lower to `vec.aese`, `vec.aesd`, `vec.aesmc`, `vec.aesimc` with result and inputs `vec<16, bits<8>>`: the first two take data and key, XOR before forward/inverse byte substitution and row permutation, while the latter take one value and perform the AES forward/inverse column matrix in GF(2^8).
These pure operations evaluate operands once, preserve indeterminate bytes through their actual byte dependencies, have no memory/world effect or pointer provenance result, remain outside the P0 allowlist, and retain their exact opcode and operands in text and independent proof reconstruction.
EvoMIR carries the corresponding typed `crypto.<opcode> (%inputs)` form with one `simd128` result and one/two `simd128` inputs; emission uses the AES extension, preserves live inputs and the destructive round's key through the reserved SIMD scratch register, and retains the `-mgeneral-regs-only` refusal rather than claiming generic CPU availability.
`tests/fixtures/arm-neon-aes.{c,out}` records four agreeing fixed ARM64 GCC-14/Clang-19 O0/O2 executions with `-march=armv8-a+crypto`, including all byte substitutions and inverse checks; one Mbed TLS `aesce.c` object reaches this family, no diagnostic is allocated, and unimplemented intrinsics still report E3052.

### Quiet floating comparison builtins (2026-09-20)

The f32/f64 `__builtin_isgreater/isgreaterequal/isless/islessequal/islessgreater/isunordered` forms require two real arithmetic operands with at least one floating operand, apply the usual arithmetic conversions, evaluate each once in source order and return an `int` predicate; only `isunordered` is true for unordered inputs.
They lower to the existing FP comparison and representation operations, without new IR nodes, strict-fenv support or P0 eligibility.
Both sealed GCC-14 and Clang-19 execute `tests/fixtures/floating-comparison-builtins.c` at O0/O2 with exit zero; `benchmark-small-frontier.out` identifies four original LLVM first errors, not four completed-program passes.
Existing E2909/E2911 diagnose arity/type errors and deferred formats retain their existing refusal.

### Full-width polynomial target products

The registered `crypto_pmulldi_ppp` and `crypto_pmullv2di_ppp` interfaces multiply two 64-bit polynomial operands without carries, returning the complete 128-bit polynomial representation; the latter consumes lane 1 of each vector, not lane 0.
Operands are evaluated once and the selected coefficients retain their initializedness checks; lowering widens before the shifts and XORs using existing integer instructions, rather than using ordinary integer multiplication or an external call.
Nominal polynomial scalars retain their source type identity while sharing ordinary scalar object storage and unsigned bit representation.
`tests/fixtures/arm-neon-polynomial-long.{c,out}` records the fixed GCC-14/Clang-19 ARM64 O0/O2 controls for the `aesce.c` family; no diagnostic is allocated and unsupported intrinsics still report E3052.

### Target byte-bit reversal

The registered `rbitv8qi` and `rbitv16qi` interfaces reverse the eight bits within each byte lane, without changing lane order or interpreting a signed byte arithmetically.
They evaluate their operand once and retain the normal initializedness check for each consumed lane; the implementation may use the existing integer shift/mask operations and does not require a new external symbol or EvoIR instruction.
`tests/fixtures/arm-neon-byte-reverse.{c,out}` records exhaustive byte-value agreement with fixed ARM64 GCC-14/Clang-19 O0/O2 for this next operation reached in Mbed TLS `aesce.c`.
No diagnostic is allocated and other unimplemented target intrinsics retain E3052.

### Integer target vector halves

Registered integer `get_low`/`get_high` interfaces select the lower/upper 64 bits in lane order, with the `v2di` interface returning its declared scalar `long` representation and the narrower-lane interfaces returning their declared 64-bit vector types.
Selection and combination evaluate each operand once and transport the selected vector representation without initializing unread lanes; polynomial wrappers preserve the same lane bits without making polynomial and ordinary integer source types compatible.
`tests/fixtures/arm-neon-halves.{c,out}` records fixed ARM64 GCC-14/Clang-19 O0/O2 agreement for the half-selection operation reached by the original Mbed TLS `aesce.c` replay.
Lowering reuses existing extraction/packing operations, no diagnostic is allocated, and the unrelated floating interfaces remain unimplemented.

### Exact finite floating constant evaluation

Floating literals retain the bits rounded directly to their source format (binary32 for `f`, binary64 without a suffix, and the Linux profile's binary128 for `L`), including hexadecimal spellings; a later conversion must not recover digits lost by that first rounding.
The admitted finite binary32/binary64 constant arithmetic, constant path selection, integer projection and object-representation stores share host-independent round-to-nearest-ties-to-even evaluation at the operation's semantic width, with no multiply-add contraction and no saturating invalid integer conversion.
`tests/fixtures/floating-literal-rounding.c`, `floating-hex-representation.c`, `floating-constant-arithmetic.c` and `floating-constants.out` record agreeing fixed ARM64 GCC-14/Clang-19 O0/O2 values and subject interpreter/native checks; these are three focused cases, not a new original-program or kernel numerator.
This corrects the existing constant-representation slice without admitting runtime floating instructions, changing the current nonfinite-arithmetic or long-double execution boundary, defining native FPCR handling, or allocating a diagnostic.

### Approved integer SIMD assembly bindings

The 2026-09-20 approval extends §3.11 with `w`, `=w`, `+w`, `=&w`, `+&w` and existing matching operands for 64/128-bit integer or polynomial vectors, using the existing typed opaque asm, local SSA, effects and per-edge result binding. At that stage scalar/floating operands remained E3021; revision 1.9.114 subsequently admits scalar f32/f64 under the same machinery. Tiny/wide/floating vectors, other floating formats and SVE remain outside this assembly profile, and `r` does not acquire a SIMD alternative.
Default references print `vN`, `%b/%h/%s/%d/%q` select views of that same physical register, and accepted `vN/dN/sN` clobber names alias that identity; `qN/hN/bN` clobbers remain E3021 because fixed Clang-19 rejects them while GCC-14 accepts them, and modifiers cannot change register bank.
Tied inputs share their output's location, while early-clobber outputs cannot overlap independent inputs or another output; allocation, spills, selected-edge output writeback, text identity and independent proof/allocation checks must preserve that contract, and reached asm still reports interpreter E3021.
`tests/fixtures/simd-assembly-bindings.c` and the existing `arm-neon-assembly*.c` are the reduced acceptance inputs for the kernel xor and Mbed TLS consumers, not a full campaign count; the general-register-only restriction remains, and this approval is not a passing execution result.

### Approved first-release floating environment boundary

The 2026-09-20 approval selects round-to-nearest-ties-to-even with gradual underflow and target NaN propagation for initial executable binary32/binary64, with exact-bit transport, no implicit contraction and no source UB obligation for IEEE exceptional arithmetic results.
Native execution must verify compatible FPCR state before admitted operations, including after opaque or external execution that may change it; an incompatible environment is an unsupported execution environment, not C UB, and the compiler must not silently reset its caller's FPCR.
At that approval, dynamic strict-fenv, executable binary128, floating complex and floating vectors were deferred; revisions 1.9.106–1.9.109 subsequently add the bounded strict scalar contract in §3.13. The software arithmetic adapter must match measured target NaN handling rather than blindly inherit its dependency's policy.
`floating-target-environment.c` and `floating-constants.out` are the existing environment counterexamples; runtime IR, native checks and ABI acceptance remain implementation work, not results implied by this approval.

### Finite literal narrowing in target headers

A pure cast of a finite floating literal into binary32/binary64 converts the already-rounded source representation directly with RNE, including a binary128 source literal; this admits no binary128 arithmetic, object read or call interface.
The constant evaluator, static initialization, C-HIR execution and EvoIR lowering share that conversion, rather than changing system-header macros or reparsing the spelling as binary64.
`tests/fixtures/floating-header-constants.c` passes fixed ARM64 GCC-14/Clang-19 O0/O2 and reduces both cJSON `DBL_EPSILON` first errors; original-program acceptance remains separate, and no diagnostic is allocated.

### HFA transport alignment

Under the approved AAPCS64 contract, HFA transport uses natural member alignment clamped to 8/16, independently of the complete object's explicit alignment; the optional `, abi_align=16` after `hfa=Nxf32/f64` records the nondefault 16-byte case, and omission means 8.
Calls, entries and `va_arg` consume the same recorded alignment; packed stack copies must not claim the destination object's stronger alignment for their source, while original object layout is unchanged.
`tests/fixtures/floating-hfa-varargs-alignment.c` and `floating-hfa-varargs-alignment.out` record the fixed GCC-14/Clang-19 controls and native RED for original torture `pr92904`, including the opposite-direction failure when a member is explicitly aligned; this affects one measured original failure, not a predicted campaign count.
The field participates in text, validation and independent proof identity, advancing the frozen layout identity to v4 without expanding EqSat eligibility; no diagnostic is allocated.

### Reusing floating environment admission checks

Native FPCR admission may be reused only when every reachable predecessor path has checked it and no intervening call, opaque asm or asm-goto can change it; function entry carries no assumption about its caller.
The complete existing CFG supplies the must-analysis, including indirect successors; no check writes FPCR or changes FP operation, token or proof identity.
`tests/fixtures/floating-environment-dominance.c` and its driver compare both sealed oracles' O0/O2 values, then test native rejection after a foreign call and on both branch entries while transport leaves FPCR unchanged.
`benchmark-small-frontier.out` records the targeted instruction-count improvement, not an inferred whole-benchmark speedup; no diagnostic is allocated.

### Floating comparison partitions after evaluation

Repeated comparisons of the same stable f32/f64 values may use the four LT/EQ/GT/unordered partitions to determine a conditional result only after retaining the original evaluations and their poison/FP-environment obligations.
The bounded source-lowering analysis excludes object/volatile reads, calls, updates and changed formats, and combines only identical operand pairs; it does not add an EqSat rule.
`tests/fixtures/floating-comparison-partitions.c` and its controls cover NaNs, signed zeros, reverse operands, narrowing, writes and uninitialized values; both sealed oracles run the positive reduction at O0/O2, and `benchmark-small-frontier.out` records the one original compare-fp-3 link failure this fixes.
No diagnostic is allocated and no full-suite improvement is inferred before replay.

### Standard floating absolute calls

With standard builtins enabled, a direct call to the external, non-asm-renamed function symbol `fabs` or `fabsf` with its exact single-argument floating prototype uses the existing corresponding builtin; static functions, function-pointer variables, incompatible prototypes, `-fno-builtin` and `-ffreestanding` retain ordinary external/call semantics.
This is symbol-and-prototype recognition, not spelling-only replacement or a weaker effect summary for an unresolved external call, following the existing integer `abs` profile and [GCC 14.2's builtin contract](https://gcc.gnu.org/onlinedocs/gcc-14.2.0/gcc/Other-Builtins.html).
After the original comparison has executed, a sign-cleared f32/f64 representation cannot compare less than either zero, including NaNs; this permits only conditional-result reasoning and does not admit `>= 0` as a tautology or remove operand consumption/FPCR admission.
`tests/fixtures/floating-absolute-calls.c` passes both sealed oracles at O0/O2 and reduces the one original 20020720-1 link failure, with symbol/option and NaN controls recorded in `benchmark-small-frontier.out`; no diagnostic or EqSat rule is added.

### Patchable native function entries

The final `-fpatchable-function-entry=N[,M]` supplies a default, with `0 <= M <= N <= 65535` and omitted M zero; a function's `patchable_function_entry` ICE arguments override it, including `(0,0)`, and `__has_attribute` reports support.
The effective optional ` patchable_entry=N,M` follows `stack_protect` in the canonical function header, participates in validation/text/proof identity without changing its callable signature or abstract effects, and requests N AArch64 NOPs with M before the callable symbol plus an ELF link-ordered patch-table relocation to the first NOP when N is nonzero.
`tests/fixtures/kernel-function-entry.{c,out}` and the sealed O0/O2 probe record the two oracles' native contract; earlier declarations merge up to the definition, but their differing treatment of a conflicting later declaration is rejected as E2943, while malformed arguments/placement remain E2903.
Those same controls align the patch start rather than an explicitly aligned callable symbol, which conflicts with Section 5.2: that body rule takes precedence, so additional padding is placed outside the patch region to align the callable symbol; no source-unit count or tracing-enabled boot is inferred from these probes, and nonzero patching is Linux ELF-only.

### Minimum native function alignment

The final `-fmin-function-alignment=N` accepts zero (the target's four-byte default) or powers of two through 65536 and raises each definition's existing `align` to at least `max(4,N)`, including cold functions, without changing source declarations or `__alignof__`.
Both interpreters retain this machine minimum when assigning virtual function addresses; native emission and patchable entries use the same callable-symbol alignment contract from Section 5.2.
`tests/fixtures/kernel-minimum-function-alignment.c` and `kernel-function-entry.out` record the real Kbuild `=8` input, source-query controls and target checks; GCC14 accepts the option while Clang19 does not recognize its spelling, so Clang's rejection is not counted as an oracle pass.
Non-power-of-two values and larger values are explicitly unsupported rather than adopting an unmeasured rounding rule; no diagnostic is allocated and no kernel runtime result is inferred.

### Function section declaration-order boundary

The approved function placement rule applies a request present before or on the definition and verifies the complete declaration chain; a first request only after the definition is E2943 rather than a silent layout choice.
`tests/fixtures/function-section-boundaries.c` with `LATE_SECTION_ONLY` gives `.exit.text` for `late` under sealed GCC-14 and `.text` plus an ignored-attribute warning under Clang-19 at O0/O2, while the ordinary preceding-declaration control agrees on placement.
The code is recorded in the adjacent output; no original kernel unit requiring the conflicting late-only form is currently identified, and this boundary does not waive any section-mismatch or link result.

### Function section UTF-8 and numeric-escape boundary

Source function-section names preserve decoded UTF-8 bytes, rather than encoding each byte as a separate Unicode character; invalid UTF-8 and control bytes are E2943.
`tests/fixtures/function-section-utf8.c` agrees under sealed GCC-14/Clang-19 at O0/O2 for raw and adjacent UTF-8 literals, whereas `NUMERIC_SECTION_ESCAPE` is accepted by GCC and rejected by Clang as an invalid escape in an unevaluated string literal, so that numeric-escape spelling is E2943.
`tests/fixtures/function-section-boundaries.out` retains both observations; no original kernel unit requiring the rejected numeric-escape spelling is currently identified, and existing object-section rules are unchanged.

### Approved strict floating and binary128 extension (2026-09-27)

The reviewed extension preserves Section 3.13's fixed scalar profile and pins
the existing host-independent APFloat engine. Strict arithmetic is a separate
world-consuming/producing instruction family, with a distinct `fenv` effect
and four rounding modes plus five sticky exception flags in each logical
thread's world snapshot. It accesses no memory and is neither volatile, atomic
nor I/O. External calls conservatively include fenv effects. Translation-time
literal/static initialization remains RNE; executed strict operations cannot
be folded or reordered across observable environment changes. Masked exceptions
only are approved; traps, Annex F claims and implicit contraction are not.

Explicit FMA computes a*b+c with one rounding. The admitted Arm scalar profile
detects tininess before rounding. FMADD NaN selection examines addend, a, b,
signaling operands first; a quiet addend does not suppress invalid zero*infinity,
which produces default NaN. `floating-strict-target.c` measures these boundaries
and quiet versus signaling comparison status independently of source optimization.
This supplements, rather than changes, binary add/sub/mul/div NaN operand order.

Real binary128 is approved through the existing object, ABI and SIMD register
paths, with audited hosted helpers selected in EvoMIR rather than in EvoIR.
Fixed-RNE helper execution and dynamic-rounding/status helper execution require
separate acceptance. Half, floating vectors/complex and the unrelated dialect
questions remain outside this approval. These are approved implementation
contracts, not acceptance results by themselves. Revisions 1.9.107–1.9.109 add
dynamic-rounding source and lexical non-trapping environment access; executable
f128 remains a separate implementation gate. L01/L02 track their staged
IR/text/proof/native/original-program closure.
