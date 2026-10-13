/* L03 ABI probe, caller side. Pair with l03-half-abi-worker.c (same KIND).
 * main returns a mask of failed call shapes (0 = every shape agrees):
 *   1 named argument/result, 2 mixed SIMD/GPR cursor, 4 stack arguments,
 *   8 HFA of three, 16 five-member record, 32 mixed record,
 *   64 anonymous arguments read as the half type (not __fp16).
 * The double reading of anonymous half arguments is an observation, not a
 * verdict: the printed value reports which reads matched. -DNO_HFA removes
 * the HFA shape, as in the worker. */
#include <string.h>
#ifdef PRINT
#include <stdio.h>
#endif
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

struct H3 { H a, b, c; };
struct H5 { H a, b, c, d, e; };
struct HF { H a; float f; };

H w_swap_second(H a, H b);
unsigned w_mixed(float f0, H a, double d1, H b, int i, H c);
unsigned w_stack(H a0, H a1, H a2, H a3, H a4, H a5, H a6, H a7, H a8, H a9);
#ifndef NO_HFA
struct H3 w_h3(struct H3 in);
#endif
struct H5 w_h5(struct H5 in);
struct HF w_hf(struct HF in);
unsigned w_va_double(int n, ...);
unsigned w_va_half(int n, ...);

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

int main(void)
{
    unsigned failed = 0;
    if (bits(w_swap_second(make(0x3c00), make(0x3c05))) != 0x3c05)
        failed |= 1;
    if (w_mixed(2.0f, make(0x3c01), 3.0, make(0x3c02), 7, make(0x3c03)) != 0x3f)
        failed |= 2;
    if (w_stack(make(0x3c00), make(0x3c01), make(0x3c02), make(0x3c03), make(0x3c04),
                make(0x3c05), make(0x3c06), make(0x3c07), make(0x3c08), make(0x3c09)) != 0x3ff)
        failed |= 4;
#ifndef NO_HFA
    struct H3 h3 = w_h3((struct H3){ make(0x3c01), make(0x3c02), make(0x3c03) });
    if (bits(h3.a) != 0x3c03 || bits(h3.b) != 0x3c01 || bits(h3.c) != 0x3c02)
        failed |= 8;
#endif
    struct H5 h5 = w_h5((struct H5){ make(1), make(2), make(3), make(4), make(5) });
    if (bits(h5.a) != 5 || bits(h5.b) != 4 || bits(h5.c) != 3 || bits(h5.d) != 2 || bits(h5.e) != 1)
        failed |= 16;
    struct HF hf = w_hf((struct HF){ make(0x3c00), 2.0f });
    if (bits(hf.a) != 0x3c01 || hf.f != 3.0f)
        failed |= 32;
#if KIND != 1
    if (w_va_half(3, make(0x3c00), make(0x3c01), make(0x3c02)) != 7)
        failed |= 64;
#endif
    H one = make(KIND == 2 ? 0x3f80 : 0x3c00);
    H two = make(0x4000);
    H three = make(KIND == 2 ? 0x4040 : 0x4200);
    unsigned as_double = w_va_double(3, one, two, three);
#ifdef PRINT
    printf("failed=%u anonymous-as-double=%u\n", failed, as_double);
#else
    (void)as_double;
#endif
    return (int)failed;
}
