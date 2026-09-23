/* Finite controls for reached Csmith reset/unchanged-guard source cycles.
 * Fuel bounds these reductions only, never the original Csmith programs. */
#include <stdint.h>

static int independent_array_guard(int32_t limit, uint32_t enabled)
{
    int32_t values[2] = {limit, 0};
    int32_t *written = &values[1];
    const int32_t *tested = &values[1];
    int32_t mask = (int32_t)UINT32_C(0xb46d125d);
    int8_t counter;
    int64_t inner;
    uint64_t outer;
    unsigned visits = 0, iterations = 0;
restart:
    for (counter = -24; counter <= 26;
         counter = (uint8_t)((uint8_t)counter + 8U)) {
        *written = (int16_t)UINT16_C(0x4abc) % (int16_t)UINT16_C(0xbfe4);
        for (inner = 2; inner >= 0; --inner) {}
        if (*tested)
            mask &= 1;
        else
            return 1;
        ++iterations;
    }
    if (counter != 32 || inner != -1 || *written != 2720 || mask != 1
        || iterations != (visits + 1) * 7)
        return 2;
    if (1 > values[0])
        return enabled != 0 || visits != 0;
    for (outer = 14; outer <= 39; ++outer) {
        if (enabled) {
            if (++visits == 8)
                return values[0] != limit || outer != 14;
            goto restart;
        }
    }
    return visits != 0 || outer != 40;
}

static uint64_t reset_word;
static int32_t minus_nine = -9;
static const int32_t *published;

static int8_t reset_callee(void)
{
    for (reset_word = (uint64_t)-9; reset_word == 16; reset_word += 7) {}
    published = &minus_nine;
    return UINT32_C(0xebe94b88);
}

static int unchanged_signed_comparison(uint16_t boundary, int8_t enabled)
{
    uint16_t half = 1;
    uint16_t *address = &half;
    int32_t written = 0;
    int64_t accumulated = INT64_C(0x0e0430d33df87db8);
    uint8_t counter = 0;
    unsigned visits = 0;
restart:
    accumulated |= 0xaf >= reset_callee();
    for (counter = (uint8_t)-19; counter != 26; ++counter)
        break;
    if (((*address == ((written = -7) || minus_nine)) ^ minus_nine)
        >= boundary)
        return 1;
    if (reset_word != (uint64_t)-9 || published != &minus_nine
        || written != -7 || counter != 237
        || accumulated != INT64_C(0x0e0430d33df87db9))
        return 2;
    if (enabled) {
        if (++visits == 16)
            return 0;
        goto restart;
    }
    return visits != 0;
}

static int reset_before_update(int8_t enabled)
{
    int32_t object = 0, counter = 1;
    int32_t *address = &object, *counter_address = &counter;
    uint16_t half = 0;
    uint8_t inner = 0x71, unrelated = 250;
    unsigned visits = 0;
restart:
    *counter_address |= ((*address = -10) / 3) || inner;
    for (counter = 9; counter <= 24; counter = (int8_t)(counter + 1)) {
        for (inner = 0; inner < 57; inner = (int16_t)(inner + 4)) {
            for (half = (uint16_t)-12; half == 42; half += 6)
                return 1;
            ++unrelated;
            if (enabled) {
                if (counter != 9 || inner != 0 || object != -10 || half != 65524)
                    return 2;
                if (++visits == 16)
                    return unrelated != 10;
                goto restart;
            }
        }
    }
    return counter != 25 || inner != 60 || unrelated != 234 || visits != 0;
}

static int reset_after_array_initialization(void)
{
    int64_t array[8][1];
    int32_t row, column;
    uint32_t skipped = 0;
    unsigned visits = 0;
restart:
    for (row = 0; row < 8; ++row)
        for (column = 0; column < 1; ++column)
            array[row][column] = (int64_t)UINT64_C(0x849f845d811f07b4);
    for (row = -12; row < -4; ++row) {
        column ^= (int16_t)array[2][0] - (int16_t)array[6][0];
        for (column = 0; column > -20; --column) {
            skipped = row;
            for (skipped = 0; skipped == 60; ++skipped)
                return 1;
        }
        if (row) {
            if (row != -12 || column != -20 || skipped != 0)
                return 2;
            if (++visits == 8)
                return 0;
            goto restart;
        }
    }
    return 3;
}

static int skipped_guard_update(uint32_t guard)
{
    uint16_t parameter;
    int32_t skipped;
    unsigned visits = 0;
restart:
    for (parameter = 0; parameter != 7; ++parameter) {
        for (skipped = 0; skipped >= 12; skipped += 8)
            --guard;
        if (parameter)
            continue;
        if (guard) {
            if (++visits == 16)
                return parameter != 0 || skipped != 0;
            goto restart;
        }
    }
    return parameter != 7 || visits != 0;
}

static int skipped_union_guard(uint16_t enabled)
{
    union U { uint16_t half; uint32_t word; unsigned field : 12; } guard = {enabled};
    union U temporary = {3};
    int32_t counter = 0, value = -4;
    int64_t outer;
    unsigned visits = 0;
restart:
    for (counter = 25; counter <= -21; counter = (uint8_t)(counter - 8)) {
        value = 0;
        guard.half = 0;
    }
    temporary.half = -((int16_t)-1 - (int16_t)-1);
    counter = (uint16_t)(temporary.half + 1U);
    for (outer = 0; outer != 10; outer = (uint64_t)outer + 5) {
        if (((uint16_t)(1U - UINT16_C(0x2ed2)) == UINT32_C(0x99265bf2)) || value) {
            if (guard.half) {
                if (counter != 1 || temporary.half != 0 || outer != 0)
                    return 1;
                if (++visits == 16)
                    return value != -4 || guard.half != enabled;
                goto restart;
            }
        }
    }
    return enabled != 0 || visits != 0 || outer != 10;
}

static int skipped_unsigned_body(uint32_t enabled)
{
    struct Bits { unsigned field : 2; } source = {3};
    uint8_t counter;
    int32_t object = 0, other = -1;
    int32_t *address = &object, *comparison = &other;
    unsigned visits = 0;
restart:
    for (counter = (uint8_t)-9; counter <= 33; ++counter) {
        enabled = 0;
        *address = -7;
    }
    *address = source.field >> (1 <= *comparison);
    if (enabled) {
        if (counter != 247 || *address != 3 || other != -1)
            return 1;
        if (++visits == 16)
            return 0;
        goto restart;
    }
    return visits != 0 || *address != 3;
}

int main(void)
{
    return independent_array_guard(INT32_C(0x53494abc), UINT32_C(0xb2c6d27b))
        || independent_array_guard(INT32_C(0x53494abc), 0)
        || independent_array_guard(0, 0)
        || unchanged_signed_comparison(UINT16_C(0x5bb0), (int8_t)0xcc)
        || unchanged_signed_comparison(UINT16_C(0x5bb0), 0)
        || reset_before_update(-1)
        || reset_before_update(0)
        || reset_after_array_initialization()
        || skipped_guard_update(5)
        || skipped_guard_update(0)
        || skipped_union_guard(UINT16_C(0xd534))
        || skipped_union_guard(0)
        || skipped_unsigned_body(UINT32_C(0x60d347fa))
        || skipped_unsigned_body(0);
}
