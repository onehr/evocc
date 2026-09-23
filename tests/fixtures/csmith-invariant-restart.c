/* Finite controls for eight unchanged Csmith bitfield cases. Fuel belongs only
 * to this reduction; it does not turn the original timeouts into PASS. */
#include <stdint.h>

static union {
    int8_t byte;
    int16_t half;
} shared;
static int32_t toggled;
static unsigned skipped_visits, callee_visits;

static uint16_t skipped_callee(void)
{
    for (shared.half = -11; shared.half >= -8; ++shared.half)
        ++skipped_visits;
    return toggled;
}

static int32_t *finite_callee(void)
{
    for (shared.byte = -25; shared.byte != -1; shared.byte += 6)
        ++callee_visits;
    return &toggled;
}

static int parameter_guard(uint64_t guard)
{
    unsigned visits = 0;
    toggled = 0x7d38c461;
    skipped_visits = callee_visits = 0;
restart:
    toggled ^= UINT32_C(0xda259c42);
    (void)skipped_callee();
    if (finite_callee() != &toggled || shared.byte != -1
        || skipped_visits || callee_visits != 4 * (visits + 1))
        return 1;
    if (guard) {
        if (++visits == 32)
            return toggled != 0x7d38c461;
        goto restart;
    }
    return visits != 0 || callee_visits != 4;
}

static int relational_guard(int32_t initial, uint32_t threshold)
{
    int32_t object = initial;
    int32_t *const address = &object;
    int64_t counter = (int64_t)initial + 1;
    uint16_t inner;
    unsigned visits = 0;
restart:
    if (counter != *address) {
        uint8_t guard = 3;
        for (counter = 0; counter >= -3; counter -= 2) {
            for (inner = 6; inner != 21; inner += 5)
                /* The nonzero constant also prevents the original alternative
                 * arm from writing any of the controlling state. */
                if (UINT8_C(0xa8) | (inner > threshold))
                    *address |= UINT8_C(0xa8) >= threshold;
            if (*address)
                break;
        }
        if (inner != 21 || counter != (*address ? 0 : -4)
            || counter == *address)
            return 1;
        if (guard) {
            if (++visits == 32)
                return 0;
            goto restart;
        }
    }
    return 2;
}

/* /132: writes to the guard occur only in an initializer-skipped loop. */
static int skipped_guard(int32_t guard)
{
    unsigned visits = 0;
    int32_t outer, inner;
restart:
    for (outer = 0; outer < 12; ++outer) {
        for (inner = 0; inner <= -18; --inner)
            guard = 0;
        if (guard) {
            if (++visits == 32)
                return outer != 0 || inner != 0;
            goto restart;
        }
    }
    return visits != 0 || outer != 12;
}

/* /54: the sampled descending loop is finite; the following guard restarts it. */
static int finite_loop_restart(int32_t guard)
{
    int32_t outer, inner;
    unsigned visits = 0;
restart:
    for (outer = 19; outer >= -3; --outer)
        for (inner = 0; inner >= 25; ++inner)
            guard = 0;
    for (unsigned argument = 0; argument != 8; ++argument) {
        if (guard) {
            if (++visits == 32)
                return outer != -4 || inner != 0 || argument != 0;
            goto restart;
        }
    }
    return visits != 0;
}

/* /162: a comma result, not the safe helper's value, selects the constant arm. */
static int constant_arm(void)
{
    unsigned visits = 0;
    int16_t guard = -6;
restart:
    for (int outer = 0; outer <= 1; ++outer) {
        uint32_t zero = 0;
        for (int inner = 0; inner <= 1; ++inner) {
            if ((1U % 7U, zero))
                guard = 0;
            else if (guard) {
                if (++visits == 32)
                    return outer != 0 || inner != 0 || guard != -6;
                goto restart;
            }
        }
    }
    return 1;
}

/* /206 and /360: each guard is independent of the copied/induction objects. */
static int independent_guard(int64_t guard, int use_remainder)
{
    unsigned visits = 0;
    struct { int first, second; } source = {868, 4}, dest = {0, 0};
restart:
    for (unsigned outer = 0; outer <= 2; ++outer)
        for (uint64_t inner = 0; inner <= 2; ++inner) {
            dest = source;
            if (use_remainder && ((1 % 1) & UINT32_C(0xd958023b)))
                guard = 0;
            if (guard) {
                if (++visits == 32)
                    return outer != 0 || inner != 0 || dest.first != 868;
                goto restart;
            }
        }
    return visits != 0 || dest.second != 4;
}

struct fields { signed f0 : 19; signed f1 : 31; unsigned f2 : 28; };
struct record { uint64_t f0; int16_t f1; int32_t f2; struct fields f3; };
_Static_assert(sizeof(struct record) == 32, "observed target record size");
_Static_assert(__builtin_offsetof(struct record, f3) == 16, "observed field offset");

/* /173: the callee resets the array; the constant-OR alternative cannot write
 * the independent bit-field guard. The original callee's other loops are
 * finite and contain no writes to this record or escaped guard. */
static int array[5];
static int reset_array(void)
{
    for (unsigned n = 0; n < 5; ++n)
        array[n] = 0;
    return array[0];
}

static int record_guard(int initial)
{
    struct record object = {UINT64_MAX - 1, 7, 0, {482, 18283, 11737}};
    unsigned visits = 0;
    const unsigned char *bytes = (const unsigned char *)&object;
    if (bytes[16] != 0xe2 || bytes[17] != 1)
        return 1;
    object.f3.f0 = initial;
restart:
    for (uint64_t n = 0; n <= 4; ++n) {
        if (UINT32_C(0x3632e6ba) | ((reset_array() || 0x6052, 0), array[n])) {
            struct record copy = object;
            if (copy.f3.f0 != initial)
                return 2;
        } else {
            object.f3.f0 = 0;
        }
        if (object.f3.f0) {
            if (++visits == 32)
                return n != 0;
            goto restart;
        }
    }
    return visits != 0;
}

int main(void)
{
    return parameter_guard(127) || parameter_guard(0)
        || relational_guard(0, 167) || relational_guard(0, 169)
        || relational_guard(7, 167) || relational_guard(7, 169)
        || relational_guard(-4, 167) || relational_guard(-4, 169)
        || skipped_guard(0x4e86) || skipped_guard(0)
        || finite_loop_restart(-7) || finite_loop_restart(0)
        || constant_arm() || independent_guard(-3, 0)
        || independent_guard(0, 0) || independent_guard(7, 1)
        || independent_guard(0, 1) || record_guard(482) || record_guard(0);
}
