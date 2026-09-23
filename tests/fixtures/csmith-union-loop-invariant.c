/* Finite transition control for bitfields/352, not a replacement benchmark.
 * The original's signed union member receives a u8 decrement result, and its
 * sole reachable break tests an independent object after AND with a mask.
 * Fuel below bounds this diagnostic only; original seed/budget stay intact. */
#include <stdint.h>

static int check(int32_t parameter, uint32_t mask, int should_break)
{
    union Counter { uint32_t word; int16_t small; } counter = {0};
    union Inner { uint32_t word; uint16_t half; } inner = {0};
    int32_t object = 3;
    int32_t *address = &object;
    unsigned visits = 0;

    if (!parameter)
        return 1;
    for (counter.small = 4; counter.small >= -8;
         counter.small = (uint8_t)((uint8_t)counter.small - 1U)) {
        for (inner.half = 0; inner.half < 12;
             inner.half = (uint32_t)inner.half + 9U) {
            if ((uint32_t)parameter - UINT32_C(0x841da2f4))
                break;
        }
        if (inner.half != ((uint32_t)parameter == UINT32_C(0x841da2f4) ? 18 : 0))
            return 2;
        *address &= mask;
        if (++visits == 256)
            return should_break || object != 0 || counter.small != 5;
        if (*address)
            break;
        if (counter.small != (uint8_t)(5U - visits))
            return 3;
    }
    return !should_break || visits != 1 || object != 3 || counter.small != 4;
}

int main(void)
{
    return check(1, UINT32_C(0xc538d2c8), 0)
        || check((int32_t)UINT32_C(0x841da2f4), UINT32_C(0xc538d2c8), 0)
        || check(1, UINT32_MAX, 1)
        || check((int32_t)UINT32_C(0x841da2f4), UINT32_MAX, 1);
}
