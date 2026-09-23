/* B03: finite controls for base/322, base/345 and bitfields/146. These retain
 * the decisive conversions, finite callee paths and caller restart guards;
 * they are not modified originals or terminating checksum evidence. */
typedef signed char i8;
typedef unsigned char u8;
typedef short i16;
typedef unsigned short u16;
typedef int i32;
typedef unsigned u32;
typedef long i64;
typedef unsigned long u64;

/* /322 func_19 resets the object, func_15's only loop is skipped, and the
 * reached func_22/func_92 paths do not write it. The nonzero modulo below
 * rules out func_7's alternative write before it returns this object. */
static i32 reset_four(void)
{
    i32 object;
    for (object = 0; object < 4; ++object) { }
    for (u64 skipped = 0; skipped == 44; ++skipped)
        object = 0;
    u32 wrapping = 1;
    for (i32 counter = 16; counter > -22; counter = (i8)counter - (i8)4) {
        for (i64 skipped = 0; skipped > 24; ++skipped)
            object = 0;
        u64 zero = 0;
        i16 numerator = (u16)((zero >= ((i32)0xe6a6b93dU > (i32)0x8456a9e5U)) - 0xa8b6U);
        if (numerator % (i16)65526U)
            --wrapping;
        else
            object = 0;
    }
    return object;
}

static int stable_return_restart(i64 guard)
{
    i8 unchanged[2] = {0x5e, 0x3e};
    unsigned visits = 0;
    i32 result;
restart:
    result = reset_four();
    if (result != 4)
        return 1;
    for (u32 outer = 0; outer <= 2; ++outer) {
        for (i32 inner = 0; inner <= 0; ++inner) {
            if (unchanged[1]) {
                result ^= 0x76c88016;
                if (guard) {
                    if (++visits == 2)
                        return outer != 0 || inner != 0;
                    goto restart;
                }
            }
        }
    }
    return visits != 0;
}

/* /345's only called helpers cannot write its independent g_90 guard.
 * func_2 returns its post-loop argument 28; signed-to-u64 comparison selects
 * the main arm without an early return. Even if the other global becomes
 * zero, the u16 helper loop terminates by wrapping to zero. */
static i32 bounded_return(u64 other)
{
    u32 argument;
    for (argument = 0; argument <= 27; ++argument) {
        for (u16 inner = -24; inner >= 9; ++inner)
            if (other)
                break;
    }
    for (u64 skipped = 0; skipped >= 2; ++skipped)
        argument = 0;
    for (unsigned once = 0; once <= 3; ++once)
        return argument;
    return 0;
}

static int independent_caller_restart(i32 guard)
{
    u64 ceiling = 18446744073709551606UL;
    unsigned visits = 0;
restart:
    if (bounded_return(0x87cf2e5664dc9ed9UL) > ceiling)
        return 1;
    for (i32 outer = 0; outer <= 3; ++outer)
        for (i32 inner = 3; inner >= 0; --inner)
            if ((u32)0x6b396263 - (u32)0x7c8b2613 == 0)
                return 2;
    if (guard) {
        if (++visits == 2)
            return 0;
        goto restart;
    }
    return visits != 0;
}

/* /146: the potentially guard-writing arm has a Boolean AND an even mask
 * as its numerator. Safe remainder/division return their first operand for
 * divisor zero; neither can make this zero equal the negative i16 constant. */
static i64 safe_remainder(i64 a, i64 b) { return b ? a % b : a; }
static i32 safe_divide(i32 a, i32 b) { return b ? a / b : a; }
static int masked_guard_restart(i8 guard, i64 divisor)
{
    u8 wrapping = 5;
    unsigned visits = 0;
restart:
    for (unsigned counter = 1; counter <= 8; ++counter) {
        i64 masked = ((++wrapping) && (counter & 1)) & 0xb9b8b638L;
        i16 negative = (i16)0xcb82;
        if (safe_divide(safe_remainder(masked, divisor) == negative, 0))
            guard = 0;
    }
    if (guard) {
        if (++visits == 2)
            return wrapping != 21;
        goto restart;
    }
    return visits != 0;
}

int main(void)
{
    if (stable_return_restart(1) || stable_return_restart(0)) return 1;
    if (bounded_return(0) != 28 || independent_caller_restart((i32)0x991ee505U)
        || independent_caller_restart(0)) return 2;
    if (masked_guard_restart(-1, 0) || masked_guard_restart(-1, -17)
        || masked_guard_restart(0, 3)) return 3;
    return 0;
}
