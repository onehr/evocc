# shift-and-width-facts

The six shift and width rules admitted at revision 1.9.44, sealed as one real
triple. The registry has no syntactic fold for `shl`, `lshr`, `ashr`, `zext`,
`sext` or `trunc`, so each of these rewrites exists only because a fact says the
result is fully known, and the checker confirms every one of them by recomputing
the Section 8.10 transfer itself. `source.c` is the input; the other four files
were written by

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
| regions | 17 |
| witnesses | 128 |
| seeds | 20 |
| node events | 139 |
| rewrite events | 62 |
| congruence events | 40 |
| epoch markers | 55 |
| extraction entries | 17 |
| premises | 36 |

Rules fired, with the number of instances:

| Rule | Instances |
|---|---|
| `p0_001_add_zero_rhs` | 5 |
| `p0_005_and_zero_rhs` | 20 |
| `p0_101_fold_sub` | 1 |
| `p1_206_known_shl` | 6 |
| `p1_207_known_lshr` | 6 |
| `p1_208_known_ashr` | 5 |
| `p1_230_known_zext` | 7 |
| `p1_231_known_sext` | 4 |
| `p1_232_known_trunc` | 8 |

`main` takes no value argument, so the driver also interpreted both modules and
confirmed that the return values agree.

The certificate names `p0.scalar.v3`, the narrowest registry containing every
rule fired. No fixture here reaches `p0.scalar.v4`: its emitting rules rewrite
one spelling of doubling into another, and under the default policy's
instruction count the two spellings tie, so the default retains the original and
admits no region for them. Reaching `p0.scalar.v4` needs the `prefer-add` or
`prefer-shift` policy, which is a producer decision the checker never verifies
(7.5), and a triple sealed under it would not be evidence about the shipped
default.
