# L09: `__builtin_return_address(N)` for a nonzero depth

This is a decision packet for the lead. It was prepared on 2026-10-13. It is a
proposal. It does not approve a rule.

## Subject

- Compiler: `.build/frozen/366fd796/evocc-darwin`, `evocc 0.1.0 (546fe46a26e8
  2026-10-13)`, SHA-256
  `93eacee4e6e917049d041346c9ecbeff1266f8b17dd26bc684eeeed1a53c4c04`.
- Specifications: `spec/evoir.md` Revision 2.1.0 and `spec/evomir.md`
  Revision 1.0.1 at HEAD `546fe46a`.
- Linux references: GCC 14.2.0-19 and Clang 19.1.7 in `evo-nexlab:sealed`,
  image `sha256:e8d52bdced1fa49a5a56cde7e1173c54bd81832cc111441f6a31148ec03ead93`,
  glibc 2.41. The container runs on Apple silicon, so PAC instructions execute.
- Darwin reference: host Apple clang 21.0.0 (clang-2100.1.1.101),
  arm64-apple-darwin25.6.0.
- ABI documents: AAPCS64 release 2025Q4 (`ARM-software/abi-aa` main at
  `ec3e2fb6`, read 2026-10-13), "The Frame Pointer"; Apple, "Writing ARM64
  code for Apple platforms".

## 1. Question

EvoCC accepts `__builtin_return_address(0)` and `__builtin_frame_address(N)`
for `N` in 1..65535, and refuses `__builtin_return_address(N)` for `N >= 1`
with E2913. The rule must state for `N >= 1`:

1. The value on each target, and whether it may be a real ancestor address.
2. The frame, pointer-authentication (PAC) and optimization assumptions.
3. Which executions are reached-only unsupported, which are undefined, and
   what each interpreter does. No engine may invent a valid ancestor address.
4. The depth domain and the conversion of the depth operand.

## 2. Evidence

### 2.1 C standard

C has no return-address builtin. The behavior is a GNU extension and a target
choice. Nothing in C11, C17 or C23 constrains it.

### 2.2 GCC 14

- GCC 14.2 manual, "Return Address": `level` is the "number of frames to scan
  up the call stack". "On some machines it may be impossible to determine the
  return address of any function other than the current one; in such cases,
  or when the top of the stack has been reached, this function returns an
  unspecified value." "Calling this function with a nonzero argument can have
  unpredictable effects, including crashing the calling program." Such calls
  "should only be made in debugging situations" (`-Wframe-address`).
- Measured AArch64 code: GCC 14 emits `mov x0, 0` for any `N >= 1`, at `-O0`
  and `-O2`, with and without `-mbranch-protection=pac-ret`. For `N = 0` it
  emits `hint 7` (`xpaclri`) and `mov x0, x30`. `__builtin_frame_address(1)`
  loads `[x29]`.
- The retained record `tests/fixtures/ancestor-frame-operands.out` adds: GCC
  accepts `return_address(-1)` and depths above 65535 as constant zero.

### 2.3 Clang

- Clang 19 (Linux) walks `N` saved frame-pointer links from `x29`, loads the
  saved LR at offset 8 and strips it with `hint #7` (`xpaclri`). It does this
  at every level, with or without `pac-ret`.
- Apple clang walks the chain the same way and does not strip. The arm64 slice
  does not sign return addresses; `arm64e` is not an EvoCC target.
- Neither checks for the end of the chain. Clang rejects depths outside
  0..65535 after conversion to `unsigned int`.

### 2.4 Target ABI

- AAPCS64 "The Frame Pointer": conforming code builds a linked list of frame
  records; the record holds the previous record's address and "the value
  passed in LR on entry"; a signed LR is stored signed; "the end of the frame
  record chain is indicated by the address zero". The platform chooses the
  conformance level, down to "not maintain a frame chain" at all.
- GNU/Linux does not mandate frame records: GCC code built with
  `-fomit-frame-pointer` may use `x29` as an ordinary register. A chain walk
  through such a frame reads unrelated data.
- Apple: "The frame pointer register (x29) must always address a valid frame
  record. Some functions — such as leaf functions or tail calls — may opt not
  to create an entry in this list. As a result, stack traces are always
  meaningful, even without debug information." So on Darwin a walk always
  reads genuine saved return addresses, but a tail call can remove a frame.
- EvoCC (EvoMIR 6.8, 6.13, 6.14): every function keeps a frame record by
  default, and EvoCC never performs sibling calls. `--frame-opt=leaf-frames`
  removes records only in leaves that read no activation address.

### 2.5 Probes

Probe source: `tests/fixtures/rules-20261013/l09-ancestor-return-address.c`
(SHA-256 `9d29b823…533c`). Exit 0 means "the ancestor value equals the
caller's own depth-zero value", 1 means "null pointer", 2 means "other
value"; bit 2 (value 4) means `__builtin_frame_address(1)` differs from the
caller's frame address. CASE=0 tests depth 1, CASE=1 depth 2.

Commands (`E=.build/frozen/366fd796/evocc-darwin`):

```sh
L="--target=aarch64-unknown-linux-gnu --sysroot=sysroot/aarch64-unknown-linux-gnu -std=gnu11"
$E $L -O0|-O2 -DCASE=N --interpret-c-hir|--interpret-evoir|-S FILE.c
# container evo-nexlab:sealed, --network none; FLAGS is empty, or
# "-fno-omit-frame-pointer -fno-optimize-sibling-calls", optionally plus
# "-mbranch-protection=pac-ret":
gcc-14|clang-19 -std=gnu11 -O0|-O2 -w FLAGS -DCASE=N FILE.c -o ref && ./ref; echo $?
# host:
clang -std=gnu11 -O0|-O2 -w FLAGS -DCASE=N FILE.c -o ref && ./ref; echo $?
```

| Flags | CASE | GCC O0/O2 | Clang 19 O0/O2 | Apple O0/O2 | EvoCC (all modes) |
| --- | --- | --- | --- | --- | --- |
| default | 0 | 1 / 5 | 0 / 6 | 0 / 5 | E2913 |
| default | 1 | 1 / 1 | 0 / 2 | 0 / SIGSEGV | E2913 |
| framed, no sibling calls | 0 | 1 / 1 | 0 / 0 | 0 / 0 | E2913 |
| framed, no sibling calls | 1 | 1 / 1 | 0 / 0 | 0 / 0 | E2913 |
| framed + `pac-ret` | 0 and 1 | 1 / 1 | 0 / 0 | 0 / 0 | E2913 (compile-time refusal; not rerun with `pac-ret`) |

With default flags at `-O2`, tail calls remove frames. Clang then reports a
different return address (2), and Apple clang crashes at depth 2 because its
walk passes the zero link at the end of the chain.

Original programs (pinned GCC torture commit
`78d4ac73dd391005b895a6148cd9831e28e1208b`, hashes equal to the ladder
record of run `20260927T084053Z`):

| Original | Use | GCC O0/O2 | Clang 19 O0/O2 | Apple O0/O2 | EvoCC |
| --- | --- | --- | --- | --- | --- |
| `20010122-1.c` (`4210cf56…e9f1`) | depth 1 after `alloca`; accepts zero or the correct value | 0 / 0 | 0 / 134 (abort) | 0 / SIGABRT | E2913, `not-applicable` |
| `20030323-1.c` (`7696430d…1453`) | depths 1..100 in a never-called function; a link test | 0 / 0 | 0 / 0 | 0 / 0 | E2913, `not-applicable` (also in LLVM SingleSource) |

The Linux kernel does not depend on this builtin on arm64: `CALLER_ADDR1` and
later use `ftrace_return_address(n)`, which `arch/arm64/include/asm/ftrace.h`
defines as the unwinder function `return_address(n)`. The only direct use,
`mm/kmsan/instrumentation.c`, is KMSAN code, not built for arm64.

## 3. Current EvoCC behavior

10.1.17: "`__builtin_return_address(0)` ... An argument other than the
constant `0` is `E2913`" (`builtin-operand-not-convertible`). All modes and
both targets refuse the probe and both originals at compile time.
`__builtin_frame_address(N)` with `N` in 1..65535 lowers to
`frame_address depth=N`; native code follows the `x29` chain; both
interpreters report E3031 (`not-executed: frame-address`) when they reach it.
`-fno-omit-frame-pointer` and `-fno-optimize-sibling-calls` are accepted and
name existing behavior (EvoMIR 6.14).

## 4. Recommended rule

Principle applied: on Linux, follow GCC 14, which gives a null pointer and
never walks foreign frames that GNU/Linux does not require to exist. On
Darwin, the platform ABI guarantees frame records, so Apple clang's walk reads
genuine saved return addresses; the proposal follows Apple clang there.

Proposed replacement for the `__builtin_return_address(0)` bullet of 10.1.17:

> `__builtin_return_address(n)`: type `void *`; `n` must be an integer
> constant expression, converted to `unsigned int` as for
> `__builtin_frame_address`; a converted depth of 65536 or more, including a
> negative one, is `E2913`. Depth 0 lowers to `return_address`.
>
> On `aarch64-unknown-linux-gnu`, a depth from 1 to 65535 yields a null
> pointer with `Null` provenance in both interpreters and in native code. No
> frame is read. The expression is not a null pointer constant and not an
> address constant.
>
> On `aarch64-apple-darwin`, a depth from 1 to 65535 lowers to
> `return_address depth=N`. Native code follows `N` saved frame-pointer
> links from `x29`. When a link is zero, the result is a null pointer;
> otherwise it is the saved LR of the record reached, with code provenance
> as for depth 0. A frame that the platform let a leaf or tail call omit
> makes the result the return address of an outer activation; it is never
> an invented address. Both interpreters report `E3031` with the message
> `not-executed: return-address` when they reach it.

Consequences:

- Linux: the probe gives 1 in all modes (native CASE=0 also checks
  `frame_address(1)`, which stays native-only); `20010122-1` and `20030323-1`
  pass at `-O0` and `-O2`, exactly as with GCC.
- Darwin: EvoCC code keeps every frame record and makes no sibling calls, so
  the probe gives 0 natively; the interpreters stop with E3031.
- `pac-ret` needs no new rule on Linux, because no saved LR is read. On
  Darwin no stripping is needed for the arm64 slice; if `arm64e` is ever
  admitted, the walk must strip with `xpaci`.

Rejected alternatives:

- **Clang's walk on Linux.** GNU/Linux does not mandate frame records. A walk
  through a GCC `-fomit-frame-pointer` frame reads unrelated data, and Clang
  itself aborts `20010122-1` at `-O2`.
- **Null on Darwin too.** It is simpler and keeps both interpreters running
  on both targets. It ignores Apple's frame-record guarantee and Apple
  clang's answer. Kept as open point 1.
- **An interpreter model of return addresses.** The interpreters have no
  machine frames; any value would be invented.
- **A walk without the zero-link check on Darwin.** It reproduces Apple
  clang's crash past the end of the chain, which AAPCS64 marks with zero.

## 5. Implementation scope

### 5.1 Compiler

- `crates/evocc-parser/src/lib.rs`: accept depths 1..65535 for
  `__builtin_return_address` with the existing `__builtin_frame_address`
  depth check; keep `CHirExpr::BuiltinReturnAddress` with the depth.
- Linux: resolve the expression to a null `void *` value that is not a null
  pointer constant, in C-HIR execution and lowering
  (`crates/evocc-chir/src/interp.rs`, `crates/evocc-lower/src/lib.rs`).
- Darwin: add `%p = return_address depth=N : ptr<generic>` beside
  `frame_address depth=N` in `spec/evoir.md` 3.7, the grammar (9.3,
  `crates/evocc-evoir/src/text.rs`, `text_parse.rs`), the effect table (4.2)
  and the proof interface (11) if it lists activation addresses.
  `crates/evocc-mir` lowers it to `frame.address [fp_N]` followed by a
  checked zero test and a load at offset 8 (EvoMIR 3, table near line 570).
  Both interpreters (`crates/evocc-chir/src/interp.rs`,
  `crates/evocc-evoir/src/lib.rs`) report E3031 with the new message.
- `spec/evoir.md` 6.3: give E3031 the second message, or add a new code.

### 5.2 Runtime helpers

None.

### 5.3 Tests for the TODO exit

- The probe: Linux expects 1 for CASE=1 in both interpreters and native code
  at `-O0` and `-O2`; CASE=0 expects 1 natively and E3031 (from
  `frame_address(1)`) in the interpreters. Darwin expects 0 natively and E3031
  in the interpreters.
- Depth domain: reuse `tests/fixtures/ancestor-frame-operands.c` cases 1, 3,
  5, 8 and 11 for `return_address` (negative and too large: E2913; variable
  depth: E2913; `0x100000000UL` converts to depth 0).
- Darwin end of chain: a native test that requests a depth beyond `main`
  returns a null pointer instead of faulting.
- Linux `pac-ret`: depth 0 still strips; depth 1 is still null.
- Negative metadata: a Linux module that contains `return_address depth=N`
  is rejected by validation, so the Linux lowering cannot use the walk.

### 5.4 Original programs to replay

- GCC torture `20010122-1.c` and `20030323-1.c` (E0 ladder) and LLVM
  SingleSource `Regression/C/gcc-c-torture/execute/20030323-1.c`; all three
  should move from `not-applicable` to PASS on Linux.
- `tests/fixtures/ancestor-return-address.c` and
  `ancestor-frame-addresses.c -DCHECK_RETURN_CHAIN`: Linux gives GCC's exit
  values; Darwin gives Apple clang's.

## 6. Size, dependencies and risks

- Size: **small** for Linux (front end and a null value). **Medium** with the
  Darwin walk, because it adds one EvoIR instruction with text, validation,
  interpreter and emission work.
- Dependencies: none on other items.
- Risks:
  - Linux programs that expect a real caller address (for logging) receive
    null, as with GCC. This matches the platform compiler.
  - Darwin results depend on foreign tail calls, as with Apple clang.
  - The `Null`-provenance result must not let optimizations treat the call as
    a null pointer constant in typing; the rule states this.

## 7. Open points

1. **Darwin.** The proposal walks the chain on Darwin (Apple ABI guarantees
   records). The alternative is GCC's null on both targets, which needs no new
   instruction and keeps the interpreters running. The evidence supports
   both; the owner's Darwin principle favors the walk.
2. **Zero-link check on Darwin.** Apple clang has none and crashes; the
   proposal checks and returns null. This differs from Apple clang only where
   Apple clang faults.
