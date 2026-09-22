/* Finite checks of reduced transition invariants, not replacement Csmith
 * programs. Original timeout identities and checksum verdicts remain intact.
 */
#include <stdint.h>

static int narrow_unsigned_cycle(void)
{
    /* base/237: signed destination, but uint8_t subtraction result. Every
     * result is nonnegative, so the original condition x > -27 never fails.
     */
    for (int i = 0; i < 256; ++i) {
        int32_t next = (uint8_t)((uint8_t)i - (uint8_t)6);
        if (next < 0 || next > 255 || !(next > -27))
            return 1;
    }
    int32_t x = 0;
    for (int i = 0; i < 128; ++i)
        x = (uint8_t)((uint8_t)x - (uint8_t)6);
    return x != 0;
}

static int residue_cycle(void)
{
    /* base/287: g_8 starts at 29; subtraction in u64 followed by u32
     * assignment preserves x == 5 (mod 8). Thus x can never be 0 or 1,
     * the only values that would end the original unsigned x > 1 loop.
     */
    const uint32_t representatives[] = {29, 21, 13, 5, 0xfffffffdU};
    for (unsigned i = 0; i < sizeof representatives / sizeof *representatives; ++i) {
        uint32_t x = representatives[i];
        uint32_t next = (uint64_t)x - (uint64_t)8;
        if ((x & 7) != 5 || (next & 7) != 5 || next <= 1)
            return 1;
    }
    return 0;
}

static int restarted_loops(void)
{
    /* base/42 finishes its first loop at -26, then takes the backedge
     * before incrementing its second loop's counter. */
    int8_t p;
    for (p = 9; p != -26; --p) {}
    if (p != -26 || !p)
        return 1;

    /* base/64 and base/88 restart the initializing for-statement before
     * its update; those branches always see respectively 1 and 9. */
    for (unsigned i = 0; i < 256; ++i) {
        uint64_t g83 = 1;
        int32_t g6 = 9;
        if (!(g83 <= 7) || !g83 || !(g6 >= 0) || !g6)
            return 1;
    }

    /* base/128 and bitfields/128: the loop is never entered, so the
     * g15=5 controlling the goto is not modified. The other decrement
     * is unsigned and remains defined when it wraps. */
    uint32_t g9 = 0, g15 = 5;
    for (unsigned i = 0; i < 256; ++i) {
        --g9;
        int32_t g3 = 0;
        if (g3 <= -2 || !g15)
            return 1;
    }
    return g9 != 0xffffff00U;
}

int main(void)
{
    return narrow_unsigned_cycle() || residue_cycle() || restarted_loops();
}
