#include <stdio.h>

typedef unsigned __int128 u128;
typedef __int128 i128;
extern int add_ok(i128, i128), sub_ok(i128, i128), mul_ok(i128, i128);
extern int div_ok(i128, i128), nonzero(u128);
extern int count_unsigned(u128, u128), count_signed(u128, i128);
extern int old_shift_ok(i128, u128);

static unsigned checks, failures;
static void check(const char *name, int got, int expected, u128 a, u128 b) {
    ++checks;
    if (got == expected) return;
    if (failures++ < 8)
        printf("%s a=%016lx%016lx b=%016lx%016lx got=%d expected=%d\n", name,
               (unsigned long)(a >> 64), (unsigned long)a,
               (unsigned long)(b >> 64), (unsigned long)b, got, expected);
}

/* This reference uses unsigned division, not the backend's limb product.
   Clang's overflow builtin needs __muloti4, absent in the sealed image. */
static int product_fits(i128 a, i128 b) {
    u128 aa = a < 0 ? -(u128)a : (u128)a;
    u128 bb = b < 0 ? -(u128)b : (u128)b;
    u128 limit = ((u128)1 << 127) - ((a < 0) == (b < 0));
    return aa == 0 || bb <= limit / aa;
}

int main(void) {
    static const unsigned long limbs[] = {
        0, 1, 2, 0x7fffffffffffffffUL, 0x8000000000000000UL,
        0xfffffffffffffffeUL, 0xffffffffffffffffUL, 0x123456789abcdef0UL
    };
    const u128 minimum = (u128)1 << 127;
    for (unsigned ah = 0; ah < 8; ++ah)
    for (unsigned al = 0; al < 8; ++al)
    for (unsigned bh = 0; bh < 8; ++bh)
    for (unsigned bl = 0; bl < 8; ++bl) {
        u128 a = ((u128)limbs[ah] << 64) | limbs[al];
        u128 b = ((u128)limbs[bh] << 64) | limbs[bl];
        i128 result;
        check("add", add_ok(a, b), !__builtin_add_overflow((i128)a, (i128)b, &result), a, b);
        check("sub", sub_ok(a, b), !__builtin_sub_overflow((i128)a, (i128)b, &result), a, b);
        check("mul", mul_ok(a, b), product_fits(a, b), a, b);
        check("div", div_ok(a, b), !(a == minimum && b == (u128)-1), a, b);
        check("nonzero", nonzero(b), b != 0, a, b);
    }
    const u128 counts[] = {0, 1, 63, 64, 127, 128, 129, (u128)1 << 64, (u128)-1};
    for (unsigned i = 0; i < sizeof(counts) / sizeof(counts[0]); ++i) {
        u128 s = counts[i];
        check("count unsigned", count_unsigned(1, s), s < 128, 1, s);
        check("count signed", count_signed(1, s), (i128)s >= 0 && s < 128, 1, s);
        for (unsigned h = 0; h < 8; ++h)
        for (unsigned l = 0; l < 8; ++l) {
            u128 x = ((u128)limbs[h] << 64) | limbs[l];
            int fits = (i128)x >= 0 && s < 128 && x <= ((minimum - 1) >> (unsigned)s);
            check("old shift", old_shift_ok(x, s), fits, x, s);
        }
    }
    printf("checks=%u failures=%u\n", checks, failures);
    return failures != 0;
}
