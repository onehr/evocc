/* L03 probe: rounding direction and exception flags of conversions into
 * _Float16 (KIND=0) and __bf16 (KIND=2) under #pragma STDC FENV_ACCESS ON.
 * Each line is "mode input -> bits flags"; flags use the glibc FE_* bits
 * (invalid 1, divide-by-zero 2, overflow 4, underflow 8, inexact 16).
 * Build with -frounding-math -ffp-contract=off and link -lm. This probe
 * prints, so it serves the reference compilers only. */
#include <fenv.h>
#include <stdio.h>
#include <string.h>
#ifndef KIND
#define KIND 0
#endif
#if KIND == 0
typedef _Float16 H;
#elif KIND == 2
typedef __bf16 H;
#else
#error unsupported KIND
#endif
#define WANT(half, bf) (KIND == 2 ? (bf) : (half))
#pragma STDC FENV_ACCESS ON

static float f32(unsigned bits)
{
    float value;
    memcpy(&value, &bits, sizeof value);
    return value;
}

int main(void)
{
    static const int modes[] = { FE_TONEAREST, FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO };
    static const char *names[] = { "nearest", "upward", "downward", "zero" };
    /* tie, overflow, tiny inexact, signaling NaN, exact, and a tiny value
     * that rounds to the least normal (tininess before or after rounding) */
    const unsigned inputs[] = { WANT(0x3f801000u, 0x3f808000u), WANT(0x477ff000u, 0x7f7fffffu),
                                WANT(0x33000001u, 0x00008001u), 0x7f800001u, 0x3fc00000u,
                                WANT(0x387ff000u, 0x007fc000u) };
    for (int m = 0; m < 4; ++m) {
        for (int i = 0; i < 6; ++i) {
            volatile float in = f32(inputs[i]);
            fesetround(modes[m]);
            feclearexcept(FE_ALL_EXCEPT);
            volatile H out = (H)in;
            int flags = fetestexcept(FE_ALL_EXCEPT);
            fesetround(FE_TONEAREST);
            H copy = out;
            unsigned short bits;
            memcpy(&bits, &copy, sizeof bits);
            printf("%s %08x -> %04x flags %d\n", names[m], inputs[i], bits, flags);
        }
    }
    return 0;
}
