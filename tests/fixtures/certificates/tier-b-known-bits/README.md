# tier-b-known-bits

The Tier B rules of the reviewed registry, sealed as one real triple. Every
rewrite here carries a premise the checker recomputes from its own
implementation of the Section 8.10 transfers, so this is the fixture that
exercises the known-bits analysis rather than the syntactic identities.
`source.c` is the input; the other four files were written by

```text
cargo run -p evocc-eqopt --example certificate_triple -- source.c .
```

at commit `6dab924` under the shipped default extraction policy. The bytes are
what that driver produced; nothing here was edited by hand.

## Verdict

```text
evo-proofcheck before.evo after.evo proof.evp   -> exit status 0
```

## Counts

| Quantity | Value |
|---|---|
| registry | `p0.scalar.v3`, digest `aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15` |
| regions | 6 |
| witnesses | 35 |
| seeds | 8 |
| node events | 40 |
| rewrite events | 8 |
| congruence events | 9 |
| epoch markers | 8 |
| extraction entries | 6 |
| premises | 9 |

Rules fired, with the number of instances:

| Rule | Instances |
|---|---|
| `p1_200_known_and` | 2 |
| `p1_201_known_or` | 1 |
| `p1_214_known_icmp_ult` | 2 |
| `p1_220_and_cover_rhs` | 1 |
| `p1_224_xor_known_zero_rhs` | 1 |
| `p1_230_known_zext` | 1 |

Eight rewrites and nine premises: every rewrite reads at least one fact, and
`p1_220_and_cover_rhs` reads two, which is the pair `covers(?m, ?x)` names.

`tier_b`'s parameters are entry arguments and therefore top, so none of the
folded values is a constant class the before witness closure holds; the analysis
is what supplies them. `main` takes no value argument, so the driver also
interpreted both modules and confirmed that the return values agree.
