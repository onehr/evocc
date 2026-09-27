/* Shared source RED for executable binary128, not a binary64 substitute. */
typedef unsigned __int128 U;
union quad_bits { long double f; U u; };
struct quad_pair { long double a, b; };
static long double static_quad = 0x1.0000000000000000000000000001p0L;
static long double static_quad_zero = -0.0L;

long double quad_from_signed(__int128 x) { return x; }
long double quad_from_unsigned(U x) { return x; }
__int128 quad_to_signed(long double x) { return x; }
U quad_to_unsigned(long double x) { return x; }
long double quad_mix(int a, long double b, double c, float d) {
    return (a + b) * c - d;
}
struct quad_pair quad_pair_flip(struct quad_pair x) {
    struct quad_pair r = {x.b, x.a};
    return r;
}
int quad_core(void) {
    volatile long double one = 1.0L, step = 0x1p-112L;
    long double x = one + step;
    if (x != static_quad || x == one) return 1;
    if (x - one != step) return 2;
    if ((one + step) * (one - step) != one) return 3;
    if (x / one != x || -(-x) != x) return 4;
    union quad_bits bits = {.f=static_quad_zero};
    if (bits.u != (U)1 << 127) return 5;
    bits.f = one / static_quad_zero;
    if (bits.u != ((U)0xffff000000000000UL << 64)) return 6;
    if (quad_mix(2, 3.25L, 2.0, 0.5f) != 10.0L) return 7;
    struct quad_pair pair = {one, x};
    pair = quad_pair_flip(pair);
    if (pair.a != x || pair.b != one) return 8;
    U integer = ((U)1 << 112) + 1;
    if (quad_to_unsigned(quad_from_unsigned(integer)) != integer) return 9;
    if (quad_to_signed(-1.5L) != -1) return 10;
    if ((float)0x1.0000010000000000000000000001p0L != 0x1.000002p0f) return 11;
    return 0;
}
int quad_nan(void) {
    volatile union quad_bits a = {.u=((U)0x7fff800000000000UL << 64) | 0x11};
    volatile union quad_bits b = {.u=((U)0xffff000000000000UL << 64) | 0x22};
    union quad_bits r = {.f=a.f + b.f};
    if (r.u != (b.u | (U)1 << 111)) return 1;
    if (a.f == a.f || !(a.f != a.f)) return 2;
    if (a.f < 1 || a.f > 1 || a.f <= 1 || a.f >= 1) return 3;
    if (!__builtin_isnan(a.f) || __builtin_isfinite(a.f)) return 4;
    if (!__builtin_signbit(b.f)) return 5;
    r.f = __builtin_fabsl(b.f);
    if (r.u != (b.u & ~((U)1 << 127))) return 6;
    return 0;
}
extern int fesetround(int), feclearexcept(int), fetestexcept(int);
int quad_dynamic(void) {
#pragma STDC FENV_ACCESS ON
    volatile long double one = 1.0L, step = 0x1p-113L;
    union quad_bits r;
    if (fesetround(0x400000)) return 1;
    feclearexcept(31);
    r.f = one + step;
    if (r.u != (((U)0x3fff000000000000UL << 64) | 1)) return 2;
    if (fetestexcept(31) != 16) return 3;
    return fesetround(0);
}
int quad_uninitialized(void) {
    long double x;
    return x == 0.0L;
}
int quad_invalid_conversion(void) {
    volatile long double x = __builtin_nanl("3");
    return (int)x;
}
static long double quad_variadic(int count, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, count);
    long double sum = 0;
    for (int i=0; i<count; ++i) sum += __builtin_va_arg(ap, long double);
    __builtin_va_end(ap);
    return sum;
}
int quad_varargs(void) {
    return quad_variadic(9, 1.L, 2.L, 3.L, 4.L, 5.L, 6.L, 7.L, 8.L,
                         0x1.0000000000000000000000000001p0L)
        != 0x1.2800000000000000000000000000p5L;
}
