/* Finite reached-path controls; fuel is diagnostic only.
 * Original Csmith inputs and execution limits are unchanged. */
#include <stdint.h>

static int constant_parameter(const uint64_t p)
{
    int8_t byte = 0x2f;
    uint16_t half = 0;
    int32_t result = 0;
    uint32_t counter;
    unsigned visits = 0;
restart:
    result = (half = (0xd7 > (byte &= (uint32_t)7 * 0x0d62b6e2U))) && p;
    for (counter = 0; counter < 38; counter = (uint32_t)(counter + 8)) {
        if (p) {
            if (counter != 0 || !result || half != 1)
                return 1;
            if (++visits == 256)
                return 0;
            goto restart;
        }
    }
    return counter != 40 || visits != 0;
}

static int preserved_member(int8_t value)
{
    struct S { int8_t member; } object = {value};
    struct S *pointer = &object;
    int32_t counter;
    unsigned visits = 0;
restart:
    for (counter = 0; counter <= 3; counter += 1) {
        *pointer = *pointer;
        if (object.member) {
            if (object.member != value || counter != 0)
                return 1;
            if (++visits == 256)
                return 0;
            goto restart;
        }
        return value != 0 || visits != 0;
    }
    return 1;
}

static int pointer_label(int nonnull, uint32_t flag)
{
    int64_t object = 7;
    int64_t *base = nonnull ? &object : (void *)0;
    int64_t *slots[3] = {&object, &object, &object};
    int64_t *other = (void *)0;
    int32_t a = 1, b = 2;
    int32_t *selected = &a;
    unsigned visits = 0;
restart:
    if ((slots[2] = base) == (other = (void *)0)) {
        return nonnull || visits != 0;
    } else {
        int32_t *matrix[3][9] = {
            {&a, &a, &b, &b, &b, &a, &a, &a, &b},
            {&a, (void *)0, &b, &b, (void *)0, &a, &b, &a, (void *)0},
            {&a, &b, &b, &a, &b, (void *)0, &b, &a, &b}
        };
        selected = matrix[1][8];
        if (flag) {
            if (selected != (void *)0 || slots[2] != &object || other != (void *)0)
                return 1;
            if (++visits == 8)
                return 0;
            goto restart;
        }
    }
    return visits != 0 || selected != (void *)0;
}

static int32_t called_counter;

static int completed_call(int32_t *local)
{
    for (int8_t i = 0; i <= 7; ++i) {
        for (called_counter = 5; called_counter >= 2; --called_counter) {
            for (int64_t j = 0; j <= 7; ++j) {}
            *local |= 1;
        }
    }
    return called_counter != 1;
}

static int reset_after_call(void)
{
    uint32_t other = 0xfffffff8U;
    int32_t local = 0;
    unsigned visits = 0;
restart:
    ++other;
    if (completed_call(&local) || local != 1)
        return 1;
    for (called_counter = 2; called_counter >= 0; --called_counter) {
        for (uint32_t member = 1; member <= 9; ++member) {
            if (called_counter) {
                if (called_counter != 2 || member != 1)
                    return 1;
                if (++visits == 8)
                    return other != 0;
                goto restart;
            }
        }
    }
    return 1;
}

static int xor_guard(int32_t guard, int alias)
{
    const int32_t initial = guard;
    int32_t object = 0x34567;
    int32_t *address = alias ? &guard : &object;
    int32_t counter;
    unsigned visits = 0;
restart:
    *address ^= 3;
    for (counter = 29; counter < -16; counter = (uint8_t)(counter - 8))
        return 1;
    if (guard) {
        int32_t value = alias ? guard : object;
        int32_t start = alias ? initial : 0x34567;
        if (counter != 29 || value != (start ^ ((visits & 1) ? 0 : 3)))
            return 1;
        if (++visits == 32)
            return value != start;
        goto restart;
    }
    if (alias)
        return guard != 0 || visits != (unsigned)(initial == 0);
    return visits != 0 || object != (0x34567 ^ 3);
}

static const int64_t tail_predicate = (int64_t)UINT64_C(0xf8738ac43aaa7995);

static int constant_tail(int take_else)
{
    uint16_t half = 65532;
    uint32_t counter = 0;
    int32_t first = take_else ? 0 : 7, second = 0, replacement = 5, masked = -1;
    int32_t *left = &first, *right = &second, *destination = &masked;
    unsigned visits = 0;
restart:
    ++half;
    if (*left) {
        *destination &= (*right = *left);
        --counter;
    } else {
        int32_t *local = &replacement;
        left = (local = (right = local));
        *destination ^= (*local == *local);
    }
    if (tail_predicate) {
        if (++visits != 8)
            goto restart;
        return half != 4 || counter != (uint32_t)(take_else ? -7 : -8)
            || masked != (take_else ? 4 : 7)
            || second != (take_else ? 0 : 7) || replacement != 5
            || left != (take_else ? &replacement : &first)
            || right != (take_else ? &replacement : &second);
    }
    return 1;
}

static int unchanged_union_guard(int16_t guard)
{
    union U { int16_t small; uint64_t wide; int32_t word; } local = {guard};
    uint32_t counter = 2, object = UINT32_C(0x205300ce);
    unsigned visits = 0;
restart:
    if (object & 1) {
        uint32_t *addresses[3] = {&object, &counter, &object};
        --*addresses[1];
    } else {
        object |= 1;
    }
    object &= UINT32_C(0x9e29ef9d);
    if (local.small) {
        if (++visits < 8)
            goto restart;
        return local.small != guard || counter != UINT32_MAX - 4
            || object != UINT32_C(0x0001008d);
    }
    return visits != 0 || counter != 2 || object != UINT32_C(0x0001008d);
}

static int reset_before_nested_update(void)
{
    int32_t outer = -1;
    uint32_t inner = 0, bits = UINT32_C(0x12345678);
    unsigned visits = 0;
restart:
    bits ^= UINT32_C(0xabcdef01);
    for (outer = 0; outer <= 8; ++outer) {
        for (inner = 1; inner <= 8; ++inner) {
            if (inner) {
                if (outer != 0 || inner != 1)
                    return 1;
                if (++visits == 8)
                    return bits != UINT32_C(0x12345678);
                goto restart;
            }
        }
    }
    return 1;
}

static int skipped_unsigned_loop(int32_t guard)
{
    struct S { uint16_t index; int32_t guard; } object = {0, guard};
    unsigned visits = 0;
restart:
    for (object.index = -8; object.index < 49; object.index = (uint8_t)(object.index + 5))
        object.guard = 0;
    if (object.guard) {
        if (++visits < 8)
            goto restart;
        return object.index != 65528 || object.guard != guard;
    }
    return object.index != 65528 || guard != 0 || visits != 0;
}

int main(void)
{
    return constant_parameter(0) || constant_parameter(1)
        || constant_parameter(UINT64_MAX) || preserved_member(0)
        || preserved_member(1) || preserved_member(-128)
        || pointer_label(0, 1) || pointer_label(1, 0)
        || pointer_label(1, 3) || pointer_label(1, UINT32_MAX)
        || reset_after_call() || xor_guard(0, 0)
        || xor_guard(0x66c5d899, 0) || xor_guard(-1, 0)
        || xor_guard(0, 1) || xor_guard(3, 1)
        || xor_guard(1, 1) || xor_guard(2, 1) || xor_guard(-1, 1)
        || constant_tail(0) || constant_tail(1)
        || unchanged_union_guard(0) || unchanged_union_guard(-1)
        || unchanged_union_guard(32767) || unchanged_union_guard(-32768)
        || reset_before_nested_update()
        || skipped_unsigned_loop(0) || skipped_unsigned_loop(1)
        || skipped_unsigned_loop(-1);
}
