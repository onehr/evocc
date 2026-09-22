/* Finite controls for the reached loops in base/315 and bitfields/69.
 * These do not replace either original Csmith program or its timeout. */
#include <stdint.h>

int main(void)
{
    const uint16_t edges[] = {0, 1, 2, 6, 7, 8, 16, 65529, 65534, 65535};
    for (unsigned i = 0; i < sizeof edges / sizeof *edges; ++i) {
        /* Both original induction variables are i32, but their safe
         * subtraction helpers return u16 before assignment. */
        int32_t a = (uint16_t)(edges[i] - 7U);
        int32_t b = (uint16_t)(edges[i] - 1U);
        if (a < 0 || !(a > -7) || b < 0 || b == -25)
            return 1;
    }

    int32_t a = 16;
    const int32_t expected[] = {9, 2, 65531, 65524};
    for (unsigned i = 0; i < sizeof expected / sizeof *expected; ++i) {
        a = (uint16_t)((uint16_t)a - 7U);
        if (a != expected[i] || !(a > -7))
            return 2;
    }
    int32_t b = 0;
    b = (uint16_t)((uint16_t)b - 1U);
    if (b != 65535 || b == -25)
        return 3;
    return 0;
}
