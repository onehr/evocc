/* Tier A of the reviewed registry, one identity per line.
 *
 * Every rewrite this program admits is a rule of `p0.scalar.v1` whose side
 * condition is `true` or `all_const`: adding zero, multiplying by one, the two
 * zero-absorbing bitwise identities, double negation, and the reflexive
 * comparisons. Nothing here depends on the known-bits analysis, so a checker
 * that refused the fixture would be refusing the oldest part of the registry.
 */

int main(void)
{
    int seed = 37;
    int add_zero = seed + 0;
    int mul_one = add_zero * 1;
    int or_zero = mul_one | 0;
    int and_zero = mul_one & 0;
    int double_neg = -(-seed);
    int self_eq = (seed == seed);
    int self_lt = (seed < seed);
    int self_le = (seed <= seed);
    int folded = 2 + 3;
    return (or_zero + and_zero + double_neg + self_eq + self_lt + self_le + folded) % 251;
}
