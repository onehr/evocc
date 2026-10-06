# Checked Optimization

An optimizer changes your program to make it smaller or faster. It must not change what the
program means. In most compilers, you trust the optimizer on this point. EvoCC checks it.

EvoCC has the usual levels `-O0` to `-O3`. `-O0` keeps the control flow of the source, and `-O1`
to `-O3` simplify integer control flow and remove unused functions. This chapter is about one more
option, `-Oeq-scalar`. It runs an optimizer that keeps a certificate of each change, and a separate
checker replays each certificate before EvoCC writes output.

## Multiply by one

Make a file `times-one.c`:

```c
int f(int x) {
    return x * 1;
}
```

Without `-Oeq-scalar`, the EvoIR keeps the multiply:

```console
$ evocc --emit=evoir times-one.c
func @f(%arg0: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>):
    %v1 = const.bits<32> 1
    %v2 = signed_mul_defined %v0, %v1
    ub.assume.signed_overflow %v2
    %v3 = mul.bits<32> %v0, %v1
    ret %v3
}
```

You know `signed_mul_defined` and `ub.assume` from [chapter 4](undefined-behavior.md). Now try
`-Oeq-scalar`:

```console
$ evocc --emit=evoir -Oeq-scalar times-one.c
evocc: verified optimization failed: -Oeq-scalar requires --eq-checker-sha256 or EVOCC_CHECKER_SHA256 from a trusted build/release
```

EvoCC refuses to optimize until you tell it which checker to trust.

## Pin the checker

The checker is the program `evocc-checker`. You pin it by its SHA-256 digest, with the option
`--eq-checker-sha256` or the variable `EVOCC_CHECKER_SHA256`. Take the digest from a source that
you trust, such as the `SHA256SUMS` file of the release, or your own build. Here we take it from the
file itself:

```console
$ export EVOCC_CHECKER_SHA256=$(shasum -a 256 "$(command -v evocc-checker)" | cut -d' ' -f1)
$ evocc --emit=evoir -Oeq-scalar times-one.c
func @f(%arg0: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>):
    %v1 = const.bits<32> 1
    %v2 = signed_mul_defined %v0, %v1
    ub.assume.signed_overflow %v2
    ret %v0
}
```

The multiply is gone: `f` now returns `x` itself. The obligation is still there. The optimizer can
remove work, but it never removes a fact that the program must satisfy.

A checker with another digest does not run, and EvoCC writes no output:

```console
$ EVOCC_CHECKER_SHA256=0000000000000000000000000000000000000000000000000000000000000000 evocc --emit=evoir -Oeq-scalar times-one.c
evocc: verified optimization failed: checker SHA-256 does not match trusted identity
```

## How the optimizer works

`-Oeq-scalar` uses *equality saturation*. A usual optimizer replaces one form of an expression with
another, one step at a time, and forgets the old form. An e-graph keeps all the equal forms of an
expression together. The optimizer adds forms with rewrite rules until nothing new appears. Then
it picks the cheapest form.

Only pure scalar code enters the e-graph. Memory access, calls and the obligations stay outside,
byte for byte. So a rewrite can never move or delete a `ub.assume`.

## The certificate

Each change comes with a certificate. `--eq-proof-dir` keeps the certificates in a new directory:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm -Oeq-scalar --eq-proof-dir=certs times-one.c > times-one.s
$ ls certs
000.after.evo
000.before.evo
000.policy.json
000.proof.evp
001.after.evo
001.before.evo
001.policy.json
001.proof.evp
chain.json
input.evo
output.evo
resources.json
```

The optimizer ran twice. The first run, `000`, made the change. The second run, `001`, found
nothing more to do. Each run has the EvoIR before and after it, and a certificate in a `.proof.evp`
file:

```console
$ cat certs/000.proof.evp
evp2
registry p0.scalar.v1 ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9
input 4f633d389312cf7cf7eb0542de9f55df13735da914cbcc748978bb0e78e04f6f
budget nodes=2/100000 epochs=1/1024 fuel=1/256 n=2 stop=saturated
region f/entry/3 root %v3 witnesses 2 seeds 1
witness 0 f/entry/0
witness 1 f/entry/3
seed 0 %v0 bits<32> input c0
node 0 n0 c1 source=before op=const sort=bits<32> effect=pure attrs=value=1 children=-
node 1 n1 c2 source=before op=mul sort=bits<32> effect=pure attrs=- children=c0,c1
epoch 1
rewrite 2 p0_003_mul_one_rhs c2 c0 c0 node:n1 class:c0 bind ?x:c0
extract 0 c0 seed:0
root c0
endregion
output 558669f736598e8b474f1a74170ac29da154c702c6dc2231cc5299dd19d89f49
```

Read it from the top:

- `registry` names the set of rules, with a digest.
- `input` and `output` are the digests of the EvoIR before and after.
- `region` is the part of `f` that entered the e-graph. Its root is `%v3`, the multiply.
- The `node` lines rebuild the e-graph: the constant 1 and the multiply.
- The `rewrite` line applies the rule `p0_003_mul_one_rhs`, *x × 1 = x*, with `?x` bound to the
  input `%v0`.
- `extract` picks the cheapest form: `%v0` itself.

`chain.json` records the whole chain and its result:

```console
$ jq '{schema, publication, stop, policy: .policy.registry}' certs/chain.json
{
  "schema": "evocc-verified-chain-v1",
  "publication": "VERIFIED",
  "stop": "NO_CHANGE",
  "policy": "p0.scalar.v4"
}
```

## Replay a certificate

`evocc-checker` is a separate program. It does not contain the optimizer, and it takes the rules
from its own copy of the registry, so a certificate cannot bring its own rules. EvoCC ran it on each
certificate before it wrote `times-one.s`. You can replay the chain again with the evaluation
tool of a source checkout. `EVOCC_SRC` is the directory of your checkout:

```console
$ python3 "$EVOCC_SRC/tools/bench.py" --verify-eqsat-chain certs \
    --checker "$(command -v evocc-checker)" | jq -c "{accepted, replay}"
{"accepted":true,"replay":[{"index":0,"rejection_reason":"NONE","verification":"VERIFIED"},{"index":1,"rejection_reason":"NONE","verification":"VERIFIED"}]}
```

Now change one word in a copy of the certificate. Claim that the step used the rule
`p0_004_mul_one_lhs`, *1 × x = x*, in place of the rule that it used:

```console
$ cp -R certs bad-certs
$ sed -i.bak "s/p0_003_mul_one_rhs/p0_004_mul_one_lhs/" bad-certs/000.proof.evp
$ python3 "$EVOCC_SRC/tools/bench.py" --verify-eqsat-chain bad-certs \
    --checker "$(command -v evocc-checker)" | jq -c "{accepted, replay}"
{"accepted":false,"replay":[{"index":0,"rejection_reason":"BUNDLE_MISMATCH","verification":"REJECTED"}]}
```

The replay refuses the changed certificate. When a replay refuses during compilation, EvoCC
rejects the whole chain and writes no output.

## The rules are proved

The rule registries are text in the
[EvoIR specification, section 11.6](https://github.com/onehr/evocc/blob/main/spec/evoir.md#116-what-is-shared-and-what-is-not).
The rule of our example reads:

```text
p0_003_mul_one_rhs mul(?x,const_like(?x,1)) => ?x when true
```

Lean reads the same text, and a theorem covers each rule, for every bit width. For this rule, the
theorem in `lean/EvoIR/Proofs/Rules.lean` starts:

```lean
theorem sound_p0_003_mul_one_rhs : RuleCovered rule_p0_003_mul_one_rhs := by
```

There are 62 rules: 55 scalar rules in `p0.scalar.v4` and 7 guard rules in `g0.ub.v1`. Each proof
uses only the Lean kernel and its standard axioms.

## The result in machine code

Here is the assembly that `-Oeq-scalar` produced:

```console
$ cat times-one.s
// evocc-native-ub-checks=checked
.text

.globl f
.p2align 2
.type f, %function
f:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
.Lf_block0:
    movz w1, #1
    smull x1, w0, w1
    sbfx x2, x1, #0, #32
    cmp x1, x2
    b.ne .Lf_block1
.Lf_block2:
    ldp x29, x30, [sp], #16
    ret
.Lf_block1:
    brk #3004
.size f, .-f

.ident "EvoCC 0.0.0 aarch64-unknown-linux-gnu"
```

The `mul` instruction is gone. The overflow check, `smull` to `brk #3004`, is still there, because
the obligation is still there. A separate family of guard rules can remove a check when a proved
fact shows that it can never fail. That family is part of the same checked chain.

## Optimization and proof together

You can combine `-Oeq-scalar` with `--proof-required` from [chapter 5](proof.md). Then the Lean
checker also checks each rewrite step, in the stage `rewrite`. The option `--eq-checker-sha256` is
required in this mode:

```console
$ evocc --target=aarch64-unknown-linux-gnu --emit=asm --proof-required=proof-times-one \
    --proof-checker="$(command -v evoir-sem)" --core-entry=f -O0 -fno-builtin \
    -Oeq-scalar --eq-checker-sha256="$EVOCC_CHECKER_SHA256" times-one.c > times-one-checked.s; echo "exit status $?"
evocc: proof-required compilation refused: stage rewrite for entry `f` step 0 rejected: STAGE_REJECTED stage=rewrite entry=f rewrite not confirmed or form not supported: registry=p0.scalar.v1 regions=f/entry/3 rules=p0_003_mul_one_rhs; no output was written
exit status 3
```

This time the compilation is refused. `evocc-checker` accepted the step, but the Lean `rewrite`
checker does not confirm this form of it, so the proved path stops. EvoCC does not fall back to the
unoptimized program, and `times-one-checked.s` is empty. A checker may refuse a correct program.
It must never accept a wrong one.

The project has checked optimized cases on this path, for example a guard rule in the `FixedMul`
function of Doom. [Chapter 8](evaluate.md) tells you where those results are.
