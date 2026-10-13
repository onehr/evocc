/* L03 probe: evaluation format and expression typing for _Float16 (KIND=0),
 * __fp16 (KIND=1) and __bf16 (KIND=2). Build with one CASE. The exit value
 * is an observation class, not a pass/fail verdict:
 *   CASE 0  a*b+c assigned to the half type: 1 per-operation rounding,
 *           2 one rounding of a binary32 evaluation, 3 other bits.
 *   CASE 1  (max*2)/2: 1 infinity (per-operation), 2 max (wider evaluation).
 *   CASE 2  a*b == (T)(product rounded once): 1 equal (per-operation),
 *           2 unequal (wider evaluation).
 *   CASE 3  1/3: 1 correctly rounded bits, 3 other bits.
 *   CASE 4  x++ at the first integer that the format cannot hold: 1 x stays,
 *           3 other bits.
 *   CASE 5  typing mask: bit 0 sizeof(+x)==2, bit 1 sizeof(x+x)==2,
 *           bit 2 sizeof(x+1)==2, bit 3 sizeof(x*1.0f)==4.
 *   CASE 6  unary minus of a signaling NaN: 1 sign flip only,
 *           2 sign flip and quieted, 3 other bits.
 *   CASE 7  _Float16 operand mixed with __bf16: 1 float-sized result equal
 *           to 2, 2 half-sized result equal to 2, 3 other (compile-time
 *           observation; some compilers refuse or crash).
 *   CASE 8  t = a*b; r = t + c: 1 per-operation bits, 2 one-rounding bits.
 *   CASE 9  (T)(a*b) + c: classes as CASE 8.
 *   CASE 10 r = a; r *= b; r += c: classes as CASE 8.
 *   CASE 11 r = mul(a, b) + c through a call: classes as CASE 8.
 *   CASE 12 (z + 2049) - 2048 with z = 0 and int operands: 1 result 0
 *           (operand rounded to the format first), 2 result 1 (operand
 *           converted to binary32), 3 other bits.
 *   CASE 13 float f = 0.1 written as a constant of the half type (suffix
 *           f16 or bf16; __fp16 has none): 1 f holds the constant rounded
 *           to the half format, 2 f holds the binary32 rounding, 3 other.
 *   CASE 14 T x = 1 + 2^-11 + 2^-30 written as a decimal constant of the
 *           half type: 1 one rounding to the half format (just above the
 *           tie), 2 rounding through binary32 first (tie to even), 3 other.
 * -DPRINT prints the bits; use it only with reference compilers. */
#include <string.h>
#ifdef PRINT
#include <stdio.h>
#endif
#ifndef KIND
#define KIND 0
#endif
#ifndef CASE
#define CASE 0
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
#define BF (KIND == 2)
#define WANT(half, bf) (BF ? (bf) : (half))

/* Representation moves are macros, so only CASE 11 passes the half type
 * through a function interface. */
#define hbits(value) ({ H hb_ = (value); unsigned short ub_; \
    memcpy(&ub_, &hb_, sizeof ub_); (unsigned)ub_; })
#define hvalue(bits) ({ unsigned short ub_ = (bits); H hb_; \
    memcpy(&hb_, &ub_, sizeof hb_); hb_; })

#if CASE == 11
__attribute__((noinline)) static H mul(H a, H b)
{
    return a * b;
}
#endif

static int report(unsigned got, int observed)
{
#ifdef PRINT
    printf("case %d: bits %04x class %d\n", CASE, got, observed);
#else
    (void)got;
#endif
    return observed;
}

int main(void)
{
#if CASE == 0
    volatile H a = hvalue(WANT(0x3c01, 0x3f81)); /* 1 + one ulp */
    volatile H b = hvalue(WANT(0x3c03, 0x3f83)); /* 1 + three ulps */
    volatile H c = hvalue(WANT(0xbc00, 0xbf80)); /* -1 */
    H r = a * b + c;
    unsigned got = hbits(r);
    return report(got, got == WANT(0x1c00u, 0x3d00u) ? 1 : got == WANT(0x1c01u, 0x3d01u) ? 2 : 3);
#elif CASE == 1
    volatile H a = hvalue(WANT(0x7bff, 0x7f7f)); /* largest finite */
    volatile H two = hvalue(WANT(0x4000, 0x4000));
    H r = (a * two) / two;
    unsigned got = hbits(r);
    return report(got, got == WANT(0x7c00u, 0x7f80u) ? 1 : got == WANT(0x7bffu, 0x7f7fu) ? 2 : 3);
#elif CASE == 2
    volatile H a = hvalue(WANT(0x3c01, 0x3f81));
    volatile H b = hvalue(WANT(0x3c03, 0x3f83));
    volatile H d = hvalue(WANT(0x3c04, 0x3f84)); /* a*b rounded once to the format */
    int equal = a * b == d;
    return report(equal, equal ? 1 : 2);
#elif CASE == 3
    volatile H one = hvalue(WANT(0x3c00, 0x3f80));
    volatile H three = hvalue(WANT(0x4200, 0x4040));
    H r = one / three;
    unsigned got = hbits(r);
    return report(got, got == WANT(0x3555u, 0x3eabu) ? 1 : 3);
#elif CASE == 4
    volatile H x = hvalue(WANT(0x6800, 0x4380)); /* 2048 or 256 */
    x++;
    unsigned got = hbits(x);
    return report(got, got == WANT(0x6800u, 0x4380u) ? 1 : 3);
#elif CASE == 5
    volatile H x = hvalue(0x3c00);
    int mask = (sizeof(+x) == 2) | (sizeof(x + x) == 2) << 1
             | (sizeof(x + 1) == 2) << 2 | (sizeof(x * 1.0f) == 4) << 3;
    return report((unsigned)mask, mask);
#elif CASE == 6
    volatile H x = hvalue(WANT(0x7d00, 0x7f81)); /* signaling NaN */
    H r = -x;
    unsigned got = hbits(r);
    return report(got, got == WANT(0xfd00u, 0xff81u) ? 1 : got == WANT(0xff00u, 0xffc1u) ? 2 : 3);
#elif CASE == 7
    volatile _Float16 h = 1.0f;
    volatile __bf16 g = 1.0f;
    int size = (int)sizeof(h + g);
    float r = h + g;
    return report((unsigned)size, r != 2.0f ? 3 : size == 4 ? 1 : size == 2 ? 2 : 3);
#elif CASE >= 8 && CASE <= 11
    volatile H a = hvalue(WANT(0x3c01, 0x3f81));
    volatile H b = hvalue(WANT(0x3c03, 0x3f83));
    volatile H c = hvalue(WANT(0xbc00, 0xbf80));
#if CASE == 8
    H t = a * b;
    H r = t + c;
#elif CASE == 9
    H r = (H)(a * b) + c;
#elif CASE == 10
    H r = a;
    r *= b;
    r += c;
#else
    H r = mul(a, b) + c;
#endif
    unsigned got = hbits(r);
    return report(got, got == WANT(0x1c00u, 0x3d00u) ? 1 : got == WANT(0x1c01u, 0x3d01u) ? 2 : 3);
#elif CASE == 12
    volatile H z = hvalue(0x0000);
    volatile int odd = WANT(2049, 257), even = WANT(2048, 256);
    H r = (z + odd) - even;
    unsigned got = hbits(r);
    return report(got, got == 0 ? 1 : got == WANT(0x3c00u, 0x3f80u) ? 2 : 3);
#elif CASE == 13
#if KIND == 0
    float f = 0.1f16;
#elif KIND == 2
    float f = 0.1bf16;
#else
#error __fp16 has no constant suffix
#endif
    unsigned bits;
    memcpy(&bits, &f, sizeof bits);
    return report(bits, bits == WANT(0x3dccc000u, 0x3dcd0000u) ? 1 : bits == 0x3dcccccdu ? 2 : 3);
#elif CASE == 14
#if KIND == 0
    volatile H x = 1.000488282181322574615478515625f16;
#elif KIND == 2
    volatile H x = 1.003906250931322574615478515625bf16; /* 1 + 2^-8 + 2^-30 */
#else
#error __fp16 has no constant suffix
#endif
    unsigned got = hbits(x);
    return report(got, got == WANT(0x3c01u, 0x3f81u) ? 1 : got == WANT(0x3c00u, 0x3f80u) ? 2 : 3);
#else
#error unsupported CASE
#endif
}
