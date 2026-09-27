#pragma STDC FENV_ACCESS ON
#include <fenv.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

extern double scalar_asm_round(double);
extern float scalar_asm_roundf(float);
extern double scalar_asm_ceil(double);
extern double scalar_asm_set_round(unsigned long, double, double);
extern double scalar_asm_flags(double, double);
extern double scalar_asm_copy(double);
extern float scalar_asm_copyf(float);

static uint64_t bits(double x) { uint64_t u; memcpy(&u, &x, 8); return u; }
static uint32_t bitsf(float x) { uint32_t u; memcpy(&u, &x, 4); return u; }
int main(void) {
    const int modes[] = {FE_TONEAREST, FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO};
    int checks = 0;
    for (int i = 0; i < 4; ++i) {
        if (fesetround(modes[i]) || feclearexcept(FE_ALL_EXCEPT)) return 1;
        if (feraiseexcept(FE_DIVBYZERO)) return 2;
        double r = scalar_asm_round(1.5);
        if (bits(r) != bits(i < 2 ? 2.0 : 1.0)) return 3;
        if (fetestexcept(FE_ALL_EXCEPT) != (FE_DIVBYZERO | FE_INEXACT)) return 4;
        if (fegetround() != modes[i]) return 5;
        checks += 3;
        feclearexcept(FE_ALL_EXCEPT);
        float s = scalar_asm_roundf(1.5f);
        if (bitsf(s) != bitsf(i < 2 ? 2.0f : 1.0f)) return 6;
        if (fetestexcept(FE_ALL_EXCEPT) != FE_INEXACT) return 7;
        checks += 2;
        feclearexcept(FE_ALL_EXCEPT);
        if (bits(scalar_asm_ceil(1.25)) != bits(2.0)) return 8;
        if (fetestexcept(FE_ALL_EXCEPT)) return 9;
        checks += 2;
        scalar_asm_flags(0.0, 0.0);
        if (fetestexcept(FE_ALL_EXCEPT) != FE_INVALID) return 10;
        checks++;
    }
    unsigned long original;
    __asm__ volatile("mrs %0, fpcr" : "=r"(original));
    for (unsigned long mode = 0; mode < 4; ++mode) {
        unsigned long control = (original & ~(3UL << 22)) | (mode << 22);
        feclearexcept(FE_ALL_EXCEPT);
        double r = scalar_asm_set_round(control, 1.0, 0x1p-53);
        if (bits(r) != (UINT64_C(0x3ff0000000000000) + (mode == 1))) return 11;
        if (fetestexcept(FE_ALL_EXCEPT) != FE_INEXACT) return 12;
        checks += 2;
    }
    __asm__ volatile("msr fpcr, %0" : : "r"(original));
    fesetround(FE_TONEAREST);
    const uint64_t representations[] = {0, UINT64_C(0x8000000000000000), 1,
        UINT64_C(0x7ff0000000000000), UINT64_C(0x7ff123456789abcd), UINT64_C(0xfff8abcdef123456)};
    const uint32_t representationsf[] = {0, 0x80000000U, 1, 0x7f800000U, 0x7f812345U, 0xffc54321U};
    for (int i = 0; i < 6; ++i) {
        double x; float y;
        memcpy(&x, &representations[i], 8);
        memcpy(&y, &representationsf[i], 4);
        feclearexcept(FE_ALL_EXCEPT);
        if (bits(scalar_asm_copy(x)) != representations[i]) return 13;
        if (bitsf(scalar_asm_copyf(y)) != representationsf[i]) return 14;
        if (fetestexcept(FE_ALL_EXCEPT)) return 15;
        checks += 3;
    }
    printf("scalar-asm-environment: %d PASS\n", checks);
    return 0;
}
