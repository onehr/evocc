/* L03 ABI probe, callee side. Pair with l03-half-abi-driver.c and build the
 * two files with the same KIND (0 _Float16, 1 __fp16, 2 __bf16), possibly
 * with different compilers. The callees only move and compare bits, so the
 * evaluation-format question of the arithmetic probe does not enter.
 * -DNO_HFA removes the three-member HFA shape (GCC 14 and AAPCS64 disagree
 * on it for __bf16). */
#include <stdarg.h>
#include <string.h>
#ifndef KIND
#define KIND 0
#endif
#if KIND == 0
typedef _Float16 H;
#elif KIND == 1
typedef __fp16 H;
#elif KIND == 2
typedef __bf16 H;
#else
#error unsupported KIND
#endif

struct H3 { H a, b, c; };          /* HFA of three members */
struct H5 { H a, b, c, d, e; };    /* five members: not an HFA */
struct HF { H a; float f; };       /* mixed members: not an HFA */

static unsigned short bits(H value)
{
    unsigned short out;
    memcpy(&out, &value, sizeof out);
    return out;
}

static H make(unsigned short in)
{
    H out;
    memcpy(&out, &in, sizeof out);
    return out;
}

/* Two named arguments in v0 and v1; the result returns in v0. */
H w_swap_second(H a, H b)
{
    (void)a;
    return b;
}

/* Shared SIMD cursor: f0 v0, a v1, d1 v2, b v3, i x0, c v4. */
unsigned w_mixed(float f0, H a, double d1, H b, int i, H c)
{
    return (f0 == 2.0f) | (bits(a) == 0x3c01) << 1 | (d1 == 3.0) << 2
         | (bits(b) == 0x3c02) << 3 | (i == 7) << 4 | (bits(c) == 0x3c03) << 5;
}

/* Ten half arguments: the last two use the stack under Linux. */
unsigned w_stack(H a0, H a1, H a2, H a3, H a4, H a5, H a6, H a7, H a8, H a9)
{
    H all[10] = { a0, a1, a2, a3, a4, a5, a6, a7, a8, a9 };
    unsigned ok = 0;
    for (int k = 0; k < 10; ++k)
        ok |= (unsigned)(bits(all[k]) == 0x3c00 + k) << k;
    return ok;
}

#ifndef NO_HFA
struct H3 w_h3(struct H3 in)
{
    struct H3 out = { in.c, in.a, in.b };
    return out;
}
#endif

struct H5 w_h5(struct H5 in)
{
    struct H5 out = { in.e, in.d, in.c, in.b, in.a };
    return out;
}

struct HF w_hf(struct HF in)
{
    struct HF out = { make((unsigned short)(bits(in.a) + 1)), in.f + 1.0f };
    return out;
}

/* Reads three anonymous arguments as double: correct only when the caller
 * applied a promotion to double. */
unsigned w_va_double(int n, ...)
{
    va_list ap;
    unsigned ok = 0;
    va_start(ap, n);
    for (int k = 0; k < n; ++k)
        ok |= (unsigned)(va_arg(ap, double) == 1.0 + k) << k;
    va_end(ap);
    return ok;
}

#if KIND != 1
/* Reads three anonymous arguments in the half type itself. */
unsigned w_va_half(int n, ...)
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
