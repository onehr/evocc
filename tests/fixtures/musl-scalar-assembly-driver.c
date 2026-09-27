#include <fenv.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
static uint64_t bits(double x) { uint64_t u; memcpy(&u, &x, 8); return u; }
int main(void) {
    const int modes[] = {FE_TONEAREST, FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO};
    const double negative[] = {-2.0, -1.0, -2.0, -1.0};
    int checks = 0;
    for (int i = 0; i < 4; ++i) {
        if (fesetround(modes[i])) return 1;
        feclearexcept(FE_ALL_EXCEPT);
        if (ceil(1.5) != 2.0 || ceil(-1.5) != -1.0) return 2;
        if (floor(1.5) != 1.0 || floor(-1.5) != -2.0) return 3;
        if (fetestexcept(FE_ALL_EXCEPT)) return 4;
        checks += 5;
        if (feraiseexcept(FE_DIVBYZERO)) return 5;
        if (rint(1.5) != (i < 2 ? 2.0 : 1.0) || rint(-1.5) != negative[i]) return 6;
        if (rintf(1.5f) != (i < 2 ? 2.0f : 1.0f) || rintf(-1.5f) != (float)negative[i]) return 7;
        if (fetestexcept(FE_ALL_EXCEPT) != (FE_DIVBYZERO | FE_INEXACT)) return 8;
        if (fegetround() != modes[i]) return 9;
        checks += 6;
        if (bits(ceil(-0.0)) != UINT64_C(0x8000000000000000)) return 10;
        if (bits(floor(-0.0)) != UINT64_C(0x8000000000000000)) return 11;
        if (bits(rint(-0.0)) != UINT64_C(0x8000000000000000)) return 12;
        checks += 3;
    }
    fesetround(FE_TONEAREST);
    printf("original-musl-scalar-asm: %d PASS\n", checks);
    return 0;
}
