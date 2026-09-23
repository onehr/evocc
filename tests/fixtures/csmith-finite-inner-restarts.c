/* B03 reached-path controls, not replacements for the unchanged Csmith seeds.
 * Two transitions bound these diagnostic functions only. The original
 * programs have no such fuel and retain their oracle-unusable verdicts. */
typedef signed char i8;
typedef unsigned char u8;
typedef short i16;
typedef unsigned short u16;
typedef int i32;
typedef unsigned u32;
typedef long i64;
typedef unsigned long u64;
_Static_assert(sizeof(i64) == 8, "LP64 controls");

/* base/33: ten XORs restore the object; the extra high-bit XOR disappears
 * on assignment to i32. The safe helper returns its numerator on divisor 0. */
static int xor_restart(i32 initial)
{
    i32 object = initial, value = 0, parameter;
    unsigned visits = 0;
restart:
    parameter = 0;
    for (u8 outer = 0; outer <= 1; ++outer) {
        i8 inner;
        for (inner = 9; inner >= 0; --inner) {
            value = (object ^= 0x5cf8a057);
            value ^= (i64)(inner & 1) << 60;
        }
        u32 divisor = (4U < 17U) & parameter;
        u32 quotient = divisor ? (u32)value / divisor : (u32)value;
        if (object != initial || inner != -1)
            return 1;
        if (quotient && inner) {
            if (++visits == 2)
                return outer != 0;
            goto restart;
        }
    }
    return initial != 0 || visits != 0;
}

/* base/93: the shifted value is a Boolean, not an arbitrary u16. */
static int boolean_shift_restart(u16 count, i16 guard)
{
    i32 shared = 0x5a, local;
    unsigned visits = 0;
restart:
    for (unsigned outer = 0; outer <= 3; ++outer) {
        for (unsigned inner = 0; inner <= 3; ++inner) {
            local = (u16)(inner & 1) >> count;
            if (local)
                return count != 0;
            for (unsigned n = 0; n <= 3; ++n)
                local |= shared;
            shared |= local;
        }
        if (guard) {
            if (++visits == 2)
                return outer != 0 || shared != 0x5a;
            goto restart;
        }
    }
    return visits != 0;
}

/* base/223: aliases denote the same zeroed object, not the separate guard. */
static int zero_object_restart(u8 guard)
{
    i32 object = 0, *first = &object, *second = &object;
    unsigned visits = 0;
restart:
    if ((u32)(1 <= *first) - (u32)*second)
        return 1;
    u16 counter;
    for (counter = 0; counter != 31; ++counter) {
        for (i64 skipped = 0; skipped == -11; --skipped)
            guard = 0;
        *first = 0;
    }
    if (guard) {
        if (++visits == 2)
            return counter != 31 || object != 0;
        goto restart;
    }
    return visits != 0;
}

/* base/442: for any i32 callee result, (result - 1) || result is true;
 * the subtraction is i64 and cannot overflow. The reached guard is unchanged
 * by these callees; its only main-body updates follow this restart edge. */
static int return_test_restart(i32 result, u32 guard)
{
    unsigned visits = 0;
restart:
    if (((i64)result - 1) || result) {
        if (guard) {
            if (++visits == 2)
                return 0;
            goto restart;
        }
        return visits != 0;
    }
    return 1;
}

/* bitfields/21: finite, signed counter loops precede a separate byte guard. */
static int finite_nested_restart(i8 guard)
{
    unsigned visits = 0;
    unsigned steps;
restart:
    steps = 0;
    for (i32 outer = 0; outer != -24; outer -= 8)
        for (u8 middle = 0; middle <= 29; middle += 7) {
            for (i8 inner = 5; inner <= 26; ++inner) {
                for (i32 once = 0; once >= 0; --once)
                    ++steps;
            }
            if (outer)
                break;
        }
    if (steps != 154)
        return 1;
    for (unsigned masked = 0; masked < 256; ++masked)
        if ((u8)(255U - (masked & 0xf9U)) == 0)
            return 2;
    if (guard) {
        if (++visits == 2)
            return 0;
        goto restart;
    }
    return visits != 0;
}

/* bitfields/24: a finite callee resets the counter before the caller's goto. */
static i32 reset_counter;
static u8 reset_callee(void)
{
    u64 n;
    for (n = 0; n != 50; n += 5)
        for (reset_counter = 0; reset_counter <= -26; --reset_counter)
            return 0;
    return n;
}

static int reset_call_restart(u8 parameter)
{
    unsigned visits = 0;
restart:
    reset_counter = 1;
    if (reset_callee()) {
        for (reset_counter = 0; reset_counter >= -17;
             reset_counter = (u32)reset_counter - 7U) {
            i64 multiplier = (i64)0xe450bb101eb64d07UL;
            if ((1L + parameter) * multiplier && parameter) {
                if (++visits == 2)
                    return reset_counter != 0;
                goto restart;
            }
        }
        return parameter != 0 || reset_counter != -21;
    }
    return 1;
}

/* bitfields/239: signed-to-u64 conversion selects the arm. The i8 helper
 * receives 11, not the full 0xad5e930b word. Neither operation changes guard. */
static int converted_guard_restart(i32 guard)
{
    u64 left = 0x7b1c000000000000UL;
    i16 right = (i16)0xc247;
    i32 value = (i32)0xf39e677aU;
    unsigned visits = 0;
restart:
    if (left < right) {
        i8 a = (value &= 6) >= (u8)0xf2;
        i8 b = (i8)0xad5e930bU;
        if (a - b != -11 || value != 2)
            return 1;
        if (guard) {
            if (++visits == 2)
                return 0;
            goto restart;
        }
        return visits != 0;
    }
    return 2;
}

/* bitfields/240: -10 becomes a large u64, but -20 becomes a large u32.
 * The decrement of the independent guard belongs to a skipped callee body. */
static int unsigned_entry_restart(u32 guard)
{
    unsigned visits = 0;
restart:
    for (u8 skipped = 0; skipped >= 57; ++skipped)
        --guard;
    u64 outer = -10;
    if (outer >= 30) {
        for (u32 skipped = -20; skipped <= 52; skipped += 7)
            guard = 0;
        if (guard) {
            if (++visits == 2)
                return outer != (u64)-10;
            goto restart;
        }
        /* Zero-guard control stops here, not after an invented huge loop. */
        return visits != 0;
    }
    return 1;
}

/* bitfields/292: self-copy and another field's update preserve f5. */
struct record {
    signed f0 : 1;
    signed f1 : 23;
    signed f2 : 10;
    unsigned f3 : 9;
    signed f4 : 26;
    unsigned f5 : 30;
};
static int bitfield_guard_restart(unsigned guard)
{
    struct record object = {0, 2791, -29, 10, -15, 5446};
    int distinct[2] = {0, 0};
    u8 wrapped = 6;
    unsigned visits = 0;
    object.f5 = guard;
restart:
    for (i8 outer = 0; outer >= -19; --outer) {
        --wrapped;
        for (i16 inner = 2; inner >= 0; --inner)
            object.f4 ^= 1;
        struct record *alias = &object;
        *alias = object;
    }
    if (&distinct[0] != &distinct[1] && object.f5) {
        if (++visits == 2)
            return object.f5 != guard || wrapped != (u8)(6 - 40);
        goto restart;
    }
    return visits != 0 || object.f5 != guard;
}

/* bitfields/455: even if the two addresses alias, toggling the low bit
 * cannot change the zero quotient. Invalid safe-helper counts return the
 * first operand; they are not C shifts by 88. */
static i8 safe_byte_shift(i8 value, u8 count)
{
    return value < 0 || count >= 32 || value > (127 >> count)
        ? value : (i8)(value << count);
}

static int small_quotient_restart(i32 initial, int alias, u64 guard)
{
    i32 object = initial, other = -10;
    i32 *value = &object, *changed = alias ? &object : &other;
    unsigned visits = 0;
restart:
    if (safe_byte_shift(*value / 0x336adbc7, 4))
        return 1;
    for (i16 counter = -25; counter <= -17; ++counter) {
        *value = *value;
        *changed ^= safe_byte_shift(1, (u8)guard);
    }
    if (guard) {
        if (++visits == 2)
            return object != initial || other != -10;
        goto restart;
    }
    return visits != 0;
}

/* bitfields/481: the parameter is a copied 21-bit value; successive helper
 * conversions to i16, u8 and i8 matter independently. */
union field { signed value : 21; };
static int copied_field_restart(int initial)
{
    union field parameter = {initial};
    i32 object = 1, *alias = &object;
    unsigned visits = 0;
    for (unsigned byte = 0; byte < 256; ++byte)
        if (((u16)(byte & 0x7c5e9217) / 0xc374) >= 0xe0df)
            return 2;
restart:
    for (u16 counter = 0; counter <= 20; ++counter)
        *alias = ((u16)((u8)91 & 0x7c5e9217) / 0xc374) >= 0xe0df;
    i16 partial = (i16)parameter.value + (i16)(object % (i16)0xf432);
    u8 wrapped = (u8)((u8)partial + (u8)-1);
    i8 difference = (i8)wrapped - (i8)4875;
    if (object != 0)
        return 1;
    if (difference && parameter.value) {
        if (++visits == 2)
            return parameter.value != initial || difference != -4;
        goto restart;
    }
    return visits != 0 || (initial != 0 && initial != 12);
}

int main(void)
{
    if (xor_restart(0x3a5fcfa7) || xor_restart(0x66a76ff0) || xor_restart(0)) return 1;
    if (boolean_shift_restart(1, -10) || boolean_shift_restart(1, 0)
        || boolean_shift_restart(0, -10)) return 2;
    if (zero_object_restart(0x3a) || zero_object_restart(0)) return 3;
    if (return_test_restart(-2147483647 - 1, 0x21fbfd46)
        || return_test_restart(0, 0x21fbfd46) || return_test_restart(1, 0x21fbfd46)
        || return_test_restart(2147483647, 0x21fbfd46) || return_test_restart(0, 0)) return 4;
    if (finite_nested_restart(2) || finite_nested_restart(0)) return 5;
    if (reset_call_restart(0) || reset_call_restart(1)) return 6;
    if (converted_guard_restart(4) || converted_guard_restart(0)) return 7;
    if (unsigned_entry_restart(0x04b9b014) || unsigned_entry_restart(0)) return 8;
    if (bitfield_guard_restart(5446) || bitfield_guard_restart(0)) return 9;
    if (small_quotient_restart(-68, 0, 0xe7085cf5d29a8c58UL)
        || small_quotient_restart(-10, 1, 0xe7085cf5d29a8c58UL)
        || small_quotient_restart(-9, 1, 0xe7085cf5d29a8c58UL)
        || small_quotient_restart(0, 0, 0)) return 10;
    if (copied_field_restart(72712) || copied_field_restart(0)
        || copied_field_restart(12)) return 11;
    return 0;
}
