/* Finite transition controls, not replacement benchmark programs. The original
 * sources/239 and /466 remain unchanged and oracle-unusable. */
#include <stdint.h>

static int unchanged_guard(int32_t guard)
{
    int32_t counter = 0;
    uint64_t value = UINT64_MAX - 1;
    unsigned visits = 0;
restart:
    for (counter = -10; counter < 17; counter = (uint64_t)counter + 2) {
        /* Csmith's safe signed shift returns its left operand if the unsigned
         * count is too large. No ordinary invalid shift is evaluated. */
        int64_t shifted = (uint64_t)guard >= 64 ? 1 : INT64_C(1) >> guard;
        value ^= (uint8_t)shifted / UINT8_C(0x51);
        if (guard) {
            if (++visits == 32)
                return counter != -10 || value != UINT64_MAX - 1;
            goto restart;
        }
    }
    return visits != 0 || counter != 18 || value != UINT64_MAX - 1;
}

static int32_t table[4][7];
static int32_t rows[5] = {-10, -10, -10, -10, -10};
static int32_t *published = &table[0][3];
static int32_t word;
static uint32_t counter;

static int32_t *finite_callee(int32_t *argument)
{
    for (counter = 0; counter <= 7; ++counter)
        if (table[0][2])
            break;
    for (int64_t n = 4; n >= 0; --n)
        rows[0] = table[0][2] = rows[n];
    if (*argument ^ (UINT8_C(0xa2) >> 3)) {
        for (int64_t n = 0; n <= 7; ++n)
            ++counter;
        *published = 1;
        word |= 1 & (int32_t)UINT32_C(0xc6de9556);
        if (word) {
            for (int32_t n = 28; n > 1; n = (uint16_t)(n - 3))
                *argument = UINT64_C(0xff190fdaafd3f0c0) >= counter;
        }
    }
    return argument;
}

static int reset_call(void)
{
    unsigned visits = 0;
    table[3][4] = 1;
    *published = 1;
restart:
    word = *published;
    if (finite_callee(&table[3][4]) != &table[3][4])
        return 1;
    if (counter != (visits ? 8U : 16U) || word != 1
        || table[3][4] != 1 || table[0][2] != -10 || *published != 1)
        return 2;
    if (counter) {
        if (++visits == 32)
            return 0;
        goto restart;
    }
    return 3;
}

int main(void)
{
    return unchanged_guard(0x6544d187) || unchanged_guard(0) || reset_call();
}
