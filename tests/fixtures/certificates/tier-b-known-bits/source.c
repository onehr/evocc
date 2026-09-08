/* Tier B of the reviewed registry: rewrites whose legality is a fact.
 *
 * `masked` and `shifted` are not constants -- their operands come from the
 * function's own parameters -- but the Section 8.10 transfers know some of
 * their bits, and that is what the `p1_*` rules read. The masks are chosen so
 * that the known-bits table decides each line on its own:
 *
 *   masked   = x & 0x0000ffff   bits 16..31 known zero
 *   raised   = y & 0xffff0000   bits 0..15 known zero
 *   erased   = masked & raised  every bit known zero, so the conjunction folds
 *   kept     = masked & 0x0000ffff   the mask covers masked
 *   spread   = masked | 0x0000ffff   the mask covers masked
 *   cleared  = masked ^ erased       erased is known zero
 *   below    = masked < 0x00010000   the intervals decide it
 *
 * `main` is a caller with no parameter of its own so that the triple can also
 * be executed; the region work is all in `tier_b`, whose parameters are top.
 */

unsigned int tier_b(unsigned int x, unsigned int y)
{
    unsigned int masked = x & 0x0000ffffu;
    unsigned int raised = y & 0xffff0000u;
    unsigned int erased = masked & raised;
    unsigned int kept = masked & 0x0000ffffu;
    unsigned int spread = masked | 0x0000ffffu;
    unsigned int cleared = masked ^ erased;
    unsigned int below = (masked < 0x00010000u);
    return erased + kept + spread + cleared + below;
}

int main(void)
{
    return (int)(tier_b(0x12345678u, 0x9abcdef0u) % 1000u);
}
