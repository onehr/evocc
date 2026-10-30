# Your First Proved Optimization

Start with `x + 0 → x`. In EqSat mode, you specify an equality instead of writing a new pass
over the whole program. Lean proves the rule. A separate checker checks each use of it.

EvoCC ships this rule, so you can follow each step without a change to the compiler.

## 1. Choose an equality with a clear scope

The rule in [the registry](https://github.com/onehr/evocc/blob/main/spec/evoir.md#116-what-is-shared-and-what-is-not) is:

```text
p0_001_add_zero_rhs add(?x,const_like(?x,0)) => ?x when true
```

`?x` matches a value. `const_like(?x,0)` matches zero with the same bit width. `when true` means
this rule needs no extra fact. It does not bypass the type or eligibility checks. The integer
EqSat profile admits widths from 1 to 64 bits; floating-point values, memory operations and
poison-tainted regions are outside this rule's scope.

Here is the function from [`examples/add_zero.c`](https://github.com/onehr/evocc/blob/main/examples/add_zero.c):

```c
unsigned int add_zero(unsigned int x)
{
    return x + 0u;
}
```

Use unsigned arithmetic for this first example. Its result wraps modulo `2^32` on both supported
targets. The rule holds for every input, including `4294967295u`. The example's `main` checks 0,
42 and that maximum value. These checks show that the program runs; they do not prove the rule.

## 2. Prove the registered rule

[`examples/AddZero.lean`](https://github.com/onehr/evocc/blob/main/examples/AddZero.lean) contains this proof:

```lean
import EvoIR.Proofs.Rules

open EvoIR

theorem add_zero_example : RuleCovered rule_p0_001_add_zero_rhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hw : Γ "x" = out
  · subst hw
    simp_all [rule_p0_001_add_zero_rhs, denotePat, denoteRhs, elabPat,
      denoteOp, CondHolds]
  · simp [rule_p0_001_add_zero_rhs, denotePat, elabPat, hw] at hl

#print axioms add_zero_example
```

[`RuleCovered`](https://github.com/onehr/evocc/blob/main/lean/EvoIR/Rule.lean) states that the two patterns have the same value for every
well-typed substitution that meets the rule's conditions. The proof handles two cases. If `x`
has the result width, bit-vector addition by zero is the identity. If it does not, the left
pattern cannot be well typed. This is a proof for all widths, not a loop over test values.

After the [source build](https://github.com/onehr/evocc/blob/main/CONTRIBUTING.md#build), run from the repository root:

```sh
(cd lean && lake env lean ../examples/AddZero.lean)
```

Expected output:

```text
'add_zero_example' depends on axioms: [propext, Quot.sound]
```

There is no `sorry`, new axiom or `native_decide` in this proof.

## 3. Connect the rule to the compiler and checker

Use the integration points of the shipped rule as a model for your own rule. Do not add the same
rule a second time.

| Part | Where to look | What it does |
|---|---|---|
| Contract | [EvoIR registry](https://github.com/onehr/evocc/blob/main/spec/evoir.md#116-what-is-shared-and-what-is-not) | Gives the rule a stable ID, patterns and conditions. The registry has a version and digest. |
| Producer | [`rewrite_opportunity`](https://github.com/onehr/evocc/blob/main/crates/evocc-eqopt/src/scalar_eq.rs) | Recognizes addition by zero and proposes an alias of the other operand. |
| Certificate | [`exact_rule`](https://github.com/onehr/evocc/blob/main/crates/evocc-eqopt/src/evp1.rs) | Records `p0_001_add_zero_rhs` and the binding of `?x`. |
| Independent replay | [`replay.rs`](https://github.com/onehr/evocc/blob/main/crates/evocc-proofcheck/src/replay.rs) | Checks the operation, operand order, same-width zero and replacement without trusting the producer. |
| Theorem | [`sound_p0_001_add_zero_rhs`](https://github.com/onehr/evocc/blob/main/lean/EvoIR/Proofs/Rules.lean) | Proves `RuleCovered` for the registered rule. The example above expands its proof template. |

For example, the producer's right-zero case proposes:

```rust
if is_constant(right, facts, 0) {
    alias(ScalarRewrite::AddZero, left)
}
```

The checker separately uses `identity_binary` with `operation: "add"`, `constant_left: false`
and `constant: 0`. It also checks the binding, types and replacement. The producer does not get
to declare its own proposal correct.

For a new rule, first add a failing test. Add the rule to a new reviewed registry revision;
keep previous IDs and meanings unchanged. Update the producer, certificate mapping, independent
checker and registry digests. Generate `lean/EvoIR/Registry.lean` with
[`registry_to_lean.py`](https://github.com/onehr/evocc/blob/main/tools/lean/registry_to_lean.py); do not edit that generated file. Add the
`RuleCovered` theorem and tests that reject a wrong constant, width, binding or replacement.
Keep `ScalarRewrite`, its admission checks and the rule-ID lists in sync. If the rule must also
work in the proof-required Core path, its separate Lean certificate checker must support it.
Update the specification pins as described in [Contributing](https://github.com/onehr/evocc/blob/main/CONTRIBUTING.md#specifications).
Before accepting the rule, run both `tools/gate-changed.sh --full-workspace --offline` and
`tools/gate-lean.sh`. A proof file alone does not install an optimization.

## 4. See the change in a real program

On an AArch64 macOS or Linux host, compile the same C file with `-O0`, then with
`-O0 -Oeq-scalar`. Both builds keep native UB checks enabled. `-O0` holds the other optional
optimizations off so this comparison isolates EqSat. Set `target` to
`aarch64-unknown-linux-gnu` on Linux. Run from the repository root after the source build:

```sh
export PATH="$PWD/target/release:$PATH"
target=aarch64-apple-darwin
out=$(mktemp -d)
checker="$PWD/target/release/evocc-checker"
checker_sha=$(shasum -a 256 "$checker" | awk '{print $1}')
evo check examples/add_zero.c
evocc --target="$target" -O0 --emit=evoir examples/add_zero.c > "$out/before.evo"
evocc --target="$target" -O0 -Oeq-scalar --eq-checker="$checker" \
  --eq-checker-sha256="$checker_sha" --eq-proof-dir="$out/proof" \
  --emit=evoir examples/add_zero.c > "$out/after.evo"
```

The digest selects the checker from your trusted local build. Hashing an untrusted download
does not make it trusted. The proof directory must be new for each compilation.

Inside `add_zero`, the result changes from:

```text
%v3 = const.bits<32> 0
%v4 = add.bits<32> %v0, %v3
ret %v4, %v1, %v2
```

to:

```text
%v3 = const.bits<32> 0
ret %v0, %v1, %v2
```

The unused constant remains in EvoIR. The shared backend removes it. The memory and world
tokens are unchanged. `proof/000.proof.evp` names `p0_001_add_zero_rhs`;
`proof/chain.json` records `CHANGED`, then `UNCHANGED`, with publication `VERIFIED`.

The driver already checked each transaction. You can replay its certificates separately:

```sh
for before in "$out"/proof/*.before.evo; do
    prefix=${before%.before.evo}
    "$checker" "$before" "$prefix.after.evo" "$prefix.proof.evp" || exit 1
done
```

Both checker calls exit 0 without output. Now emit assembly and run both programs. `cc` only
assembles and links these files; EvoCC compiled the C source:

```sh
evocc --target="$target" -O0 --emit=asm examples/add_zero.c > "$out/before.s"
evocc --target="$target" -O0 -Oeq-scalar --eq-checker="$checker" \
  --eq-checker-sha256="$checker_sha" --emit=asm examples/add_zero.c > "$out/after.s"
diff -u "$out/before.s" "$out/after.s" || test "$?" -eq 1
cc "$out/before.s" -o "$out/before"
cc "$out/after.s" -o "$out/after"
"$out/before"; echo "before: $?"
"$out/after"; echo "after: $?"
```

On the tested AArch64 macOS build, the complete assembly diff removes just this instruction
from `add_zero`:

```diff
-    add w0, w0, #0
```

`add_zero` goes from five instructions to four. Both executables report:

```text
before: 0
after: 0
```

The change removes four bytes of instructions from one function. Alignment and linking can change
the size of the executable, and the tutorial measures no speed. `-Oeq-scalar` checks the EvoIR
rewrite chain only. It does not prove C lowering or machine-code emission; the
[proof-required path](https://github.com/onehr/evocc/blob/main/spec/verification.md) checks those
stages for its Core profile. A new rule needs its own producer code and its own independent checker
code: EvoCC has no rule plugin and no generator of Rust matchers.

When you finish, remove the temporary directory `$out`. Generated files do not belong in Git.
