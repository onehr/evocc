/* Worker side of the half-format crossed ABI probe (L03). The functions
 * only move and compare representation bits, so the evaluation format of
 * an operation does not enter; w_va_promoted adds exact small values. */
#include <stdarg.h>
#include "floating-half-abi.h"

/* Two named arguments in v0 and v1; the result returns in v0. */
__attribute__((noinline)) H w_second(H a, H b)
{
    (void)a;
    return b;
}

/* The SIMD cursor is shared: f0 v0, a v1, d1 v2, b v3, i w0, c v4. */
__attribute__((noinline)) unsigned w_mixed(float f0, H a, double d1, H b, int i, H c)
{
    return (f0 == 2.0f) | (bits(a) == 0x3c01) << 1 | (d1 == 3.0) << 2
         | (bits(b) == 0x3c02) << 3 | (i == 7) << 4 | (bits(c) == 0x3c03) << 5;
}

/* Eight half arguments fill v0 to v7; the result is the last one. */
__attribute__((noinline)) H w_eight(H a0, H a1, H a2, H a3, H a4, H a5, H a6, H a7)
{
    unsigned ok = bits(a0) == 0x3c00 && bits(a1) == 0x3c01 && bits(a2) == 0x3c02
               && bits(a3) == 0x3c03 && bits(a4) == 0x3c04 && bits(a5) == 0x3c05
               && bits(a6) == 0x3c06;
    return ok ? a7 : make(0);
}

#if KIND != 2
/* AAPCS64 HFAs of one to four binary16 members, in consecutive registers. */
__attribute__((noinline)) struct H1 w_h1(struct H1 in)
{
    struct H1 out = {make((u16)(bits(in.a) + 1))};
    return out;
}

__attribute__((noinline)) struct H2 w_h2(struct H2 in)
{
    struct H2 out = {in.b, in.a};
    return out;
}

__attribute__((noinline)) struct H3 w_h3(struct H3 in)
{
    struct H3 out = {in.c, in.a, in.b};
    return out;
}

/* The HFA takes v0 to v3 and the scalar after it takes v4. */
__attribute__((noinline)) struct H4 w_h4(struct H4 in, H extra)
{
    struct H4 out = {{in.a[3], in.a[2], extra, in.a[0]}};
    return out;
}
#endif

/* Five members and mixed members are general composites. */
__attribute__((noinline)) struct H5 w_h5(struct H5 in)
{
    struct H5 out = {in.e, in.d, in.c, in.b, in.a};
    return out;
}

__attribute__((noinline)) struct HF w_hf(struct HF in)
{
    struct HF out = {make((u16)(bits(in.a) + 1)), in.f + 1.0f};
    return out;
}

__attribute__((noinline)) struct HI w_hi(struct HI in)
{
    struct HI out = {in.b, in.i + 1, in.a};
    return out;
}

/* Reads anonymous double arguments: an __fp16 argument promotes to double,
 * and a float argument does. */
__attribute__((noinline)) double w_va_promoted(int n, ...)
{
    va_list ap;
    double sum = 0;
    va_start(ap, n);
    for (int k = 0; k < n; ++k)
        sum = sum * 16 + va_arg(ap, double);
    va_end(ap);
    return sum;
}

#ifdef LINUX
/* Records of both half formats are general composites: four and eight
 * bytes in general registers. */
__attribute__((noinline)) struct HM w_hm(struct HM in)
{
    struct HM out = {f16_make((u16)(f16_bits(in.a) + 1)), bf16_make((u16)(bf16_bits(in.b) + 2))};
    return out;
}

__attribute__((noinline)) struct HN w_hn(struct HN in, struct HM extra)
{
    struct HN out = {in.c, in.d, extra.b, extra.a};
    return out;
}

/* Ten half arguments: the last two use 8-byte stack slots. */
__attribute__((noinline)) unsigned w_stack(H a0, H a1, H a2, H a3, H a4, H a5, H a6, H a7,
                                           H a8, H a9)
{
    H all[10] = {a0, a1, a2, a3, a4, a5, a6, a7, a8, a9};
    unsigned ok = 0;
    for (int k = 0; k < 10; ++k)
        ok |= (unsigned)(bits(all[k]) == 0x3c00 + k) << k;
    return ok;
}

#if KIND != 1
/* Anonymous _Float16 and __bf16 arguments stay unpromoted (C23 6.5.2.2):
 * the first eight take v registers, the rest stack slots. */
__attribute__((noinline)) unsigned w_va_half(int n, ...)
{
    va_list ap;
    unsigned ok = 0;
    va_start(ap, n);
    for (int k = 0; k < n; ++k)
        ok |= (unsigned)(bits(va_arg(ap, H)) == 0x3c00 + k) << k;
    va_end(ap);
    return ok;
}
#endif
#endif
