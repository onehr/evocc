# Strict floating-point implementation checkpoint

This extends the existing scalar profile; it is not a new FP implementation,
a completed L01/L02 claim, or a kernel/application rerun.

## Numerical prerequisite

Commit `7eb4097` adds explicit rounding and exception results to the pinned
APFloat adapter, and immutable FP environment snapshots to `WorldState`.
The independent SoftFloat/TestFloat run checks **2,630,976 value/status pairs**.
The retained target instruction probe checks **22,568 records** in each of
GCC14/Clang19 O0/O2; the four outputs have SHA-256
`a9b2612bea820e5c1916aef16cfad6804030a1caba55b8c8dcc78d5448a7134e`.

Measured repairs include Arm tininess-before-rounding, missing APFloat
directed-overflow status when returning maximum finite, and Arm FMA NaN
selection in addend/first-multiplicand/second-multiplicand order. Production
semantics still use APFloat, not host arithmetic or the independent oracle.

## IR and machine extension

EvoIR 1.9.106 makes `fenv` mandatory and adds `fp.strict.*` with explicit current
world input/fresh world output. Exceptions are sticky; no memory, volatile or
atomic event is invented. Explicit FMA is distinct from multiply-plus-add.
Both FP families remain outside P0. All operands and effect metadata participate
in validation, renumbering, proof reconstruction and mutation rejection.

EvoMIR 0.45 preserves strictness and the logical width of narrow integer
conversions. Its FPCR admission permits the four rounding modes for strict
operations, retains the fixed-mode check for baseline arithmetic, and invalidates
admission across opaque calls/assembly. No check writes FPCR/FPSR. Scalar FP
continues to share the existing physical SIMD bank.

The pure conversion-definedness predicate first classifies NaNs as integer bits:
an FCMP on an sNaN would otherwise introduce an undeclared INVALID effect.
Strict raw invalid conversions return the specified total value and raise INVALID;
C's separate E3063 guard remains mandatory. Strict 128-bit integer helper
conversions still refuse pending their own status/rounding audit.

Historical sealed certificates are not rewritten. Their reader is explicitly
selected (`--legacy-pre-fenv`); current parsing rejects a missing field. The
historical reader rejects strict FP and new FMA forms. New proofs bind current
canonical bytes. The pinned random seed corpus retains its old hashes through
this explicitly selected historical serialization, not changed expected digests.

## Native targeted execution

Fixtures: `floating-strict-world.evoir`, `floating-strict-conversions.evoir`,
and the reference-compiled `floating-strict-ir-driver.c`.
`evocc-mir`'s `evoir_assembly` example uses actual validation, lowering,
allocation, independent allocation checking and Linux emission.
Each GCC14/Clang19 O0/O2 caller executes **62 checks, PASS**, covering four
rounding modes, prior and new flags, NaN payloads, signed zero, narrow conversion,
pure definedness checks, and FZ/DN rejection in disposable subprocesses.
This is **IR/backend evidence**, not C strict-lowering evidence.

All native runs reuse the offline `evo-flight-lab:sealed` configuration
`2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2`.
Target compilers are Debian GCC 14.2.0-19 and Clang 19.1.7.

Reproduction (from the repository root, existing target directory):

```sh
CARGO_TARGET_DIR=target/frontend cargo run --locked --offline \
  -p evocc-mir --example evoir_assembly \
  < tests/fixtures/floating-strict-world.evoir > /tmp/strict-add.s
CARGO_TARGET_DIR=target/frontend cargo run --locked --offline \
  -p evocc-mir --example evoir_assembly \
  < tests/fixtures/floating-strict-conversions.evoir > /tmp/strict-conversions.s
# In the sealed target, for each compiler and -O0/-O2:
gcc-14 -std=gnu11 -O2 -Wall -Wextra -Werror -frounding-math \
  -ffp-contract=off floating-strict-ir-driver.c strict-add.s \
  strict-conversions.s -o strict-ir-driver
./strict-ir-driver
```

Small local logs live in `.build/floating-targeted/strict-20260927/`; no new
kernel clone, guest disk or persistent TestFloat corpus was allocated.

## Binary128 audit, not implementation acceptance

`floating-quad-helpers.c` records 6,400 exact arithmetic/result/status/control
tuples. All four target compiler/optimization combinations produce SHA-256
`112819c39437cd68af96d7d9b929a76115cb691aa189ca6766e29abc0b484a11`.
The hosted archive is `/usr/lib/gcc/aarch64-linux-gnu/14/libgcc.a`, SHA-256
`60dbf748e1c310aa50300a33f30bf4ee070061071aee74a52cf115a7b8765e50`.
The helper inventory includes arithmetic, comparisons and f32/f64/integer
conversions. Compiler agreement here shares a runtime and is **not** an
independent numerical-engine differential. f128 IR, source lowering, ABI and
original jq replay remain required.

## Open work

Checkpoint verification: `cargo test --workspace --locked --offline
--no-fail-fast` exits 0 with **2,735 PASS / 0 FAIL / 4 historical ignored**,
336 result groups. Workspace/all-target Clippy with `-D warnings` and fmt pass.
Python K1 tests: 251 PASS; ladder tests: 256 PASS (including three grammar-hint
regressions). Ruff passes. The shape hint reports 54/54 shapes; it is not a
grammar-completeness proof. These tests reuse `target/frontend` and retain the
old release executable for historical measurements.

Source `-frounding-math`, runtime constant-folding boundaries, C-HIR strict
execution and complete fenv API models remain open. `FENV_ACCESS ON` is not
advertised. Binary128, musl, jq and full campaigns remain separately tracked
in `TODO.md`. No traps, Annex F, floating vectors/complex or half support is
implied by this checkpoint.
