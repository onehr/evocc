/* Source-bound diagnostic: include the pinned, unchanged musl powf algorithm
 * under a private name. This does not replace a benchmark or installed object.
 * Add musl's src/math and original private include directories to the command.
 */
#include <fenv.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#define powf evocc_probe_powf
#include "powf.c"
#undef powf

/* Instrument the unchanged non-intrinsic exp2 polynomial immediately before
 * its final float narrowing. Original: Arm Limited, 2017-2018, MIT (powf.c).
 * The real exp2_inline and library powf remain separate comparison controls.
 */
#if TOINT_INTRINSICS
#error This reduction covers the pinned non-intrinsic powf configuration only
#endif
static double_t before_narrowing(double_t xd)
{
    double_t kd = eval_as_double(xd + SHIFT);
    uint64_t ki = asuint64(kd);
    kd -= SHIFT;
    double_t r = xd - kd;
    uint64_t t = T[ki % N];
    t += ki << (52 - EXP2F_TABLE_BITS);
    double_t s = asdouble(t);
    double_t z = C[0] * r + C[1];
    double_t r2 = r * r;
    double_t y = C[2] * r + 1;
    y = z * r2 + y;
    return y * s;
}

static uint32_t bits(float value)
{
    uint32_t raw;
    memcpy(&raw, &value, sizeof raw);
    return raw;
}

static __attribute__((noinline)) float observed_expansion(double_t value)
{
    return exp2_inline(value, 0);
}

int main(void)
{
    const int modes[] = {FE_TONEAREST, FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO};
    volatile float x = 0x1.fffffep127f;
    volatile float one = 1.0f;
    float (*volatile original)(float, float) = powf;
    float (*volatile expand)(double_t) = observed_expansion;
    int different = 0;
    for (unsigned i = 0; i < 4; ++i) {
        if (fesetround(modes[i])) return 2;
        feclearexcept(FE_ALL_EXCEPT);
        double_t logarithm = log2_inline(asuint(x));
        double_t product = one * logarithm;
        int log_flags = fetestexcept(FE_ALL_EXCEPT);
        int overflow_branch = product > 0x1.fffffffd1d571p+6 * POWF_SCALE;
        double_t approximant = before_narrowing(product);
        feclearexcept(FE_ALL_EXCEPT);
        float narrowed = eval_as_float(approximant);
        int narrow_flags = fetestexcept(FE_ALL_EXCEPT);
        double_t exact = x;
        feclearexcept(FE_ALL_EXCEPT);
        volatile float identity = eval_as_float(exact);
        int identity_flags = fetestexcept(FE_ALL_EXCEPT);
        feclearexcept(FE_ALL_EXCEPT);
        float expanded = expand(product);
        int exp_flags = fetestexcept(FE_ALL_EXCEPT);
        feclearexcept(FE_ALL_EXCEPT);
        float probe = evocc_probe_powf(x, one);
        int probe_flags = fetestexcept(FE_ALL_EXCEPT);
        feclearexcept(FE_ALL_EXCEPT);
        float library = original(x, one);
        int library_flags = fetestexcept(FE_ALL_EXCEPT);
        different |= bits(probe) != bits(library) || probe_flags != library_flags
            || bits(narrowed) != bits(expanded) || bits(identity) != bits(x)
            || identity_flags != 0;
        /* Rendering happens after every flag snapshot; printf cannot supply
         * or clear an observed arithmetic exception in this diagnostic. */
        printf("mode=%u log=%016llx product=%016llx cutoff=%016llx branch=%d "
               "log_flags=%x approximant=%016llx exact=%016llx "
               "narrowed=%08x/%x identity=%08x/%x "
               "expanded=%08x exp_flags=%x probe=%08x/%x library=%08x/%x\n",
               i, (unsigned long long)asuint64(logarithm),
               (unsigned long long)asuint64(product),
               (unsigned long long)asuint64(0x1.fffffffd1d571p+6 * POWF_SCALE),
               overflow_branch, log_flags, (unsigned long long)asuint64(approximant),
               (unsigned long long)asuint64(exact), bits(narrowed), narrow_flags,
               bits(identity), identity_flags, bits(expanded), exp_flags,
               bits(probe), probe_flags, bits(library), library_flags);
    }
    return different;
}
