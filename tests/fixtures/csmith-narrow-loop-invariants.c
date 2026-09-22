/* Finite controls of original Csmith timeout transitions, not replacement
 * benchmarks. The original programs and their unusable-reference verdicts
 * remain unchanged. */
#include <stdint.h>

static int narrow8(void)
{
    for (unsigned i = 0; i < 256; ++i) {
        /* base/327: g_14 is i16 but the subtraction returns u8. */
        int16_t a = (uint8_t)((uint8_t)i - 3U);
        /* bitfields/192 and /322: i32 destinations of u8 subtraction. */
        int32_t b = (uint8_t)((uint8_t)i - 9U);
        int32_t c = (uint8_t)((uint8_t)i - 2U);
        if (a < 0 || a == -9 || !(b > -9) || !(c >= -11))
            return 1;
    }
    int16_t a = 0;
    int32_t b = 29, c = 0;
    for (unsigned i = 0; i < 256; ++i) {
        a = (uint8_t)((uint8_t)a - 3U);
        b = (uint8_t)((uint8_t)b - 9U);
        c = (uint8_t)((uint8_t)c - 2U);
    }
    return a != 0 || b != 29 || c != 0;
}

static int narrow16(void)
{
    /* The conversion to u16 has range [0, 65535], wholly above each
     * negative termination threshold. Check wrap boundaries directly. */
    const uint16_t edges[] = {0, 1, 2, 3, 7, 8, 14, 29, 65534, 65535};
    for (unsigned i = 0; i < sizeof edges / sizeof *edges; ++i) {
        int32_t a = (uint16_t)(edges[i] - 8U); /* base/112 */
        int32_t b = (uint16_t)(edges[i] - 3U); /* bitfields/259 */
        int32_t c = (uint16_t)(edges[i] - 7U); /* bitfields/357 */
        if (a < 0 || a == -26 || !(b > -29) || !(c >= -29))
            return 1;
    }
    return 0;
}

static int narrow32(void)
{
    /* bitfields/404 starts at -27, then a u32 increment is assigned to
     * i64. Every later value is nonnegative and cannot equal -18. */
    int64_t initial = -27;
    initial = (uint32_t)((uint32_t)initial + 1U);
    if (initial != INT64_C(4294967270))
        return 1;
    const uint32_t edges[] = {0, 1, 4294967269U, 4294967270U, UINT32_MAX};
    for (unsigned i = 0; i < sizeof edges / sizeof *edges; ++i) {
        int64_t next = (uint32_t)(edges[i] + 1U);
        if (next < 0 || next == -18)
            return 1;
    }
    return 0;
}

int main(void)
{
    return narrow8() || narrow16() || narrow32();
}
