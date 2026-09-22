/* Only object bytes are observed; no floating arithmetic or value load. */
#ifndef PAYLOAD
#define PAYLOAD "0x1"
#endif
static float f = __builtin_nanf(PAYLOAD);
static double d = __builtin_nan(PAYLOAD);
static long double l = __builtin_nanl(PAYLOAD);
static double negative = -__builtin_nan(PAYLOAD);
#ifdef PROBE_PRINT
extern int printf(const char *, ...);
static void show(const void *object, unsigned size) {
    const unsigned char *bytes = object;
    for (unsigned i = size; i; --i) printf("%02x", bytes[i - 1]);
    printf("\n");
}
#endif
int main(void) {
#ifdef PROBE_PRINT
    show(&f, sizeof f); show(&d, sizeof d);
    show(&l, sizeof l); show(&negative, sizeof negative);
#else
    unsigned a;
    unsigned long b, c;
    __uint128_t wide;
    __builtin_memcpy(&a, &f, sizeof a);
    __builtin_memcpy(&b, &d, sizeof b);
    __builtin_memcpy(&wide, &l, sizeof wide);
    __builtin_memcpy(&c, &negative, sizeof c);
    if (a != 0x7fc00001U || b != 0x7ff8000000000001UL) return 1;
    if (wide != (((__uint128_t)0x7fff800000000000UL << 64) | 1)) return 2;
    if (c != 0xfff8000000000001UL) return 3;
    double same = __builtin_nan("0X1");
    if (__builtin_memcmp(&same, &d, sizeof d)) return 4;
    union { double d; unsigned long u; } v;
    v.d = __builtin_nan("01");
    if (v.u != b) return 5;
    static float largest_f = __builtin_nanf("0xffffffffffffffffffffffffffffffff");
    static double largest_d = __builtin_nan("0xffffffffffffffffffffffffffffffff");
    static long double largest_l = __builtin_nanl("0xffffffffffffffffffffffffffffffff");
    __builtin_memcpy(&a, &largest_f, sizeof a);
    __builtin_memcpy(&b, &largest_d, sizeof b);
    __builtin_memcpy(&wide, &largest_l, sizeof wide);
    if (a != 0x7fffffffU || b != 0x7fffffffffffffffUL) return 6;
    if (wide != (((__uint128_t)0x7fffffffffffffffUL << 64) | ~0UL)) return 7;
    v.d = __builtin_nan("");
    if (v.u != 0x7ff8000000000000UL) return 8;
    v.d = __builtin_nan("010");
    if (v.u != 0x7ff8000000000008UL) return 9;
    v.d = __builtin_nan("10");
    if (v.u != 0x7ff800000000000aUL) return 10;
#endif
    return 0;
}
