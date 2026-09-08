# tier-a-identities

The Tier A identities of the reviewed registry, sealed as one real triple.
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
| regions | 19 |
| witnesses | 173 |
| seeds | 0 |
| node events | 210 |
| rewrite events | 88 |
| congruence events | 42 |
| epoch markers | 78 |
| extraction entries | 19 |
| premises | 10 |

Rules fired, with the number of instances:

| Rule | Instances |
|---|---|
| `p0_001_add_zero_rhs` | 13 |
| `p0_003_mul_one_rhs` | 9 |
| `p0_005_and_zero_rhs` | 7 |
| `p0_007_or_zero_rhs` | 7 |
| `p0_009_double_neg` | 6 |
| `p0_010_icmp_eq_self` | 6 |
| `p0_012_icmp_slt_self` | 5 |
| `p0_013_icmp_sle_self` | 4 |
| `p0_100_fold_add` | 14 |
| `p0_101_fold_sub` | 7 |
| `p1_230_known_zext` | 10 |

The certificate names `p0.scalar.v3` rather than `p0.scalar.v1` because the
comparisons lower through `zext`, and folding a widening cast is a fact-driven
rule: the producer names the narrowest registry that contains every rule it
fired. Everything else here is Tier A.

`main` takes no value argument, so the driver also interpreted both modules and
confirmed that the return values agree; the certificate, not that run, is what
the checker verifies.
