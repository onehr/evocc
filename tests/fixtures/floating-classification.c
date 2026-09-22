static int calls;
static double constant_inf = __builtin_inf();
static double constant_abs = __builtin_fabs(-1.25);
static double constant_nan_arithmetic = __builtin_nan("0x123") + 1.0;
static double next(double x) { calls++; return x; }
static unsigned long bits(double x) {
    unsigned long b;
    __builtin_memcpy(&b, &x, sizeof b);
    return b;
}
int main(void) {
    volatile double zero = 0.0;
    double nan = __builtin_nan("0x123");
    double inf = __builtin_inf();
    double tiny = 0x1p-1074;
    if (!__builtin_isnan(next(nan)) || calls != 1) return 1;
    if (__builtin_isnan(inf) || __builtin_isnan(zero)) return 2;
    if (!__builtin_isinf(next(-inf)) || calls != 2) return 3;
    if (__builtin_isinf(nan) || __builtin_isinf(tiny)) return 4;
    if (__builtin_isinf_sign(-inf) != -1 || __builtin_isinf_sign(inf) != 1) return 5;
    if (!__builtin_isfinite(tiny) || __builtin_isfinite(inf) || __builtin_isfinite(nan)) return 6;
    if (__builtin_isnormal(tiny) || __builtin_isnormal(zero) || !__builtin_isnormal(1.0)) return 7;
    if (!__builtin_signbit(-zero) || __builtin_signbit(zero)) return 8;
    if (bits(__builtin_fabs(next(-nan))) != bits(nan) || calls != 3) return 9;
    if (bits(__builtin_copysign(next(nan), next(-zero))) != (bits(nan) | (1UL << 63)) || calls != 5) return 10;
    float f = __builtin_inff();
    if (!__builtin_isinf(f) || __builtin_fabsf(-1.25f) != 1.25f) return 11;
    if (__builtin_copysignf(1.25f, -0.0f) != -1.25f) return 12;
    if (__builtin_huge_val() != inf || __builtin_huge_valf() != f) return 13;
    if (constant_inf != inf || constant_abs != 1.25 || bits(constant_nan_arithmetic) != bits(nan)) return 14;
    unsigned long signaling = 0xfff0000000001234UL;
    double snan;
    __builtin_memcpy(&snan, &signaling, sizeof snan);
    if (!__builtin_isnan(snan) || bits(__builtin_fabs(snan)) != 0x7ff0000000001234UL) return 15;
    return 0;
}
