/* The shift and width rows of Section 8.10, reached through facts.
 *
 * `p0.scalar.v3` folds a shift or a width change only through `fully_known`:
 * the registry has no `fold.shl`, `fold.zext` or any other syntactic fold for
 * these operators, so the only way a checker sees one folded is a premise it
 * recomputes from the transfer table. `cleared` and `signed_cleared` are known
 * on every bit without being constant classes, which is what lets each of the
 * six rules fire on a value the before witness closure does not hold as a
 * constant:
 *
 *   p1_206_known_shl    up            cleared << 3
 *   p1_207_known_lshr   down          cleared >> 3
 *   p1_208_known_ashr   arith         signed_cleared >> 3
 *   p1_230_known_zext   wide          the widening cast
 *   p1_231_known_sext   signed_wide   the signed widening cast
 *   p1_232_known_trunc  narrow        the narrowing cast
 */

unsigned int widths(unsigned int x, int y)
{
    unsigned int cleared = x & 0u;
    int signed_cleared = y & 0;

    unsigned int up = cleared << 3;
    unsigned int down = cleared >> 3;
    int arith = signed_cleared >> 3;
    unsigned long long wide = (unsigned long long)cleared;
    long long signed_wide = (long long)signed_cleared;
    unsigned char narrow = (unsigned char)cleared;

    return up + down + (unsigned int)arith + (unsigned int)wide
         + (unsigned int)signed_wide + narrow;
}

int main(void)
{
    return (int)(widths(0x1234u, -5) % 1000u);
}
