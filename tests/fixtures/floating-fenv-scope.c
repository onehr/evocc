/* Lexical policy is attached to code, not the caller's current source region. */
extern int fesetround(int);
extern int feclearexcept(int);
extern int fetestexcept(int);
union fp_scope_bits { double d; unsigned long u; };

#pragma STDC FENV_ACCESS ON
static double scope_static = 1.0 + 0x1p-53;
static inline __attribute__((always_inline)) double active_inline(double x) {
    return x + 0x1p-53;
}
double active_function(double x) { return x + 0x1p-53; }

#pragma STDC FENV_ACCESS OFF
static inline __attribute__((always_inline)) double fixed_inline(double x) {
    return x + 0x1p-53;
}
double fixed_function(double x) { return x + 0x1p-53; }

int scope_probe(void) {
#pragma STDC FENV_ACCESS ON
    union fp_scope_bits a, b, c;
    if (fesetround(0x400000)) return 1;
    feclearexcept(31);
    a.d = active_inline(1.0);
    if (a.u != 0x3ff0000000000001UL || !(fetestexcept(16))) return 2;
    {
#pragma STDC FENV_ACCESS OFF
        if (fesetround(0)) return 3;
        b.d = fixed_inline(1.0);
    }
    if (b.u != 0x3ff0000000000000UL) return 4;
    if (fesetround(0x400000)) return 5;
    c.d = 1.0 + 0x1p-53;
    if (c.u != 0x3ff0000000000001UL) return 6;
    if (fesetround(0)) return 7;
    a.d = scope_static;
    return a.u != 0x3ff0000000000000UL;
}

int scope_goto(void) {
    int count = 0;
    union fp_scope_bits value;
    goto inner;
    {
#pragma STDC FENV_ACCESS ON
inner:
        if (fesetround(0x400000)) return 1;
        value.d = 1.0 + 0x1p-53;
        if (value.u != 0x3ff0000000000001UL) return 2;
        if (fesetround(0)) return 3;
        if (++count < 2) goto inner;
    }
    value.d = 1.0 + 0x1p-53;
    return value.u != 0x3ff0000000000000UL;
}

static double scope_return_conversion(void) {
#pragma STDC FENV_ACCESS ON
    return 9007199254740993UL;
}

int scope_boundaries(void) {
    union fp_scope_bits value;
    fesetround(0x400000);
    feclearexcept(31);
    value.d = scope_return_conversion();
    fesetround(0);
    if (value.u != 0x4340000000000001UL || !(fetestexcept(16))) return 1;
    switch (1) {
#pragma STDC FENV_ACCESS ON
    case 1:
        fesetround(0x400000);
        value.d = 1.0 + 0x1p-53;
        fesetround(0);
        break;
    default: return 2;
    }
    if (value.u != 0x3ff0000000000001UL) return 3;
    value.d = ({
        _Pragma("STDC FENV_ACCESS ON")
        fesetround(0x400000);
        double result = 1.0 + 0x1p-53;
        fesetround(0);
        result;
    });
    if (value.u != 0x3ff0000000000001UL) return 4;
    for (int i=0; i<3; ++i) {
        {
            _Pragma("STDC FENV_ACCESS ON")
            fesetround(0x400000);
            value.d = 1.0 + 0x1p-53;
            fesetround(0);
            if (value.u != 0x3ff0000000000001UL) return 5;
            if (i==0) continue;
            break;
        }
    }
    value.d = 1.0 + 0x1p-53;
    return value.u != 0x3ff0000000000000UL;
}

int scope_computed(void) {
    union fp_scope_bits value;
    void *target = &&indirect;
    goto *target;
    {
        _Pragma("STDC FENV_ACCESS ON")
indirect:
        fesetround(0x400000);
        value.d = 1.0 + 0x1p-53;
        fesetround(0);
    }
    return value.u != 0x3ff0000000000001UL;
}

int scope_expression(void) {
    fesetround(0x400000);
    union fp_scope_bits value = { .d = ({
#pragma STDC FENV_ACCESS ON
        1.0 + 0x1p-53;
    }) };
    fesetround(0);
    return value.u != 0x3ff0000000000001UL;
}

#pragma STDC FENV_ACCESS ON
#include "floating-environment-library.c"

int scope_discarded(void) {
    fesetround(0);
    feclearexcept(31);
    (void)(0x1.fffffep127f * 2.0f);
    if (fetestexcept(31) != 20) return 1;
    (void)(0x1p-149f / 2.0f);
    if (fetestexcept(31) != 28) return 2;
    (void)(1.0 / 0.0);
    if (fetestexcept(31) != 30) return 3;
    (void)(0.0 / 0.0);
    if (fetestexcept(31) != 31) return 4;
    feclearexcept(31);
    (void)(int)1.5;
    if (fetestexcept(31) != 16) return 5;
    feclearexcept(31);
    (void)(float)0x1.000001p0;
    return fetestexcept(31) != 16;
}
