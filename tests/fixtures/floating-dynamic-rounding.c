/* Each named entry is also executable by the zero-argument C-HIR runner. */
union Bits { double value; unsigned long bits; };
static const double translated = 1.0 + 0x1p-53;

unsigned long runtime_constant(void) {
    union Bits result = {.value = 1.0 + 0x1p-53};
    return result.bits;
}

unsigned long runtime_volatile(void) {
    volatile double one = 1.0;
    volatile double half = 0x1p-53;
    union Bits result = {.value = one + half};
    return result.bits;
}

unsigned long translation_constant(void) {
    union Bits result = {.value = translated};
    return result.bits;
}

unsigned long runtime_integer_conversion(void) {
    long value = 9007199254740993L;
    union Bits result = {.value = (double)value};
    return result.bits;
}

unsigned long runtime_narrowing(void) {
    union { float value; unsigned bits; } result = {.value = (float)16777217.0};
    return result.bits;
}

unsigned long runtime_branch_conversion(void) {
    volatile int selected = 1;
    long value = 9007199254740993L;
    union Bits result = {.value = selected ? value : 0.0};
    return result.bits;
}

unsigned long runtime_condition(void) {
    return 1.0 + 0x1p-53 > 1.0;
}

unsigned long quiet_nan_comparison(void) {
    union Bits input = {.bits = 0x7ff8000000000011UL};
    volatile double value = input.value;
    return __builtin_isless(value, 1.0);
}

unsigned long signaling_nan_comparison(void) {
    union Bits input = {.bits = 0x7ff0000000000011UL};
    volatile double value = input.value;
    return __builtin_isless(value, 1.0);
}

unsigned long ordered_nan_comparison(void) {
    union Bits input = {.bits = 0x7ff8000000000011UL};
    volatile double value = input.value;
    return value < 1.0;
}
