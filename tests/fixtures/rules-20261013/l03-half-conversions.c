/* L03 probe: exact conversion bits for _Float16 (KIND=0), __fp16 (KIND=1)
 * and __bf16 (KIND=2). CASE=N builds one case; no CASE builds all of them.
 * Each case compares the observed bits with the IEEE single-rounding,
 * round-to-nearest-even value. main sets bit (N mod 8) of its result for
 * each mismatching case N, so 0 means every selected case matches. Case 21
 * assumes the Linux binary128 long double. -DPRINT prints the bits; use it
 * only with reference compilers. */
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
#define BF (KIND == 2)
#define WANT(half, bf) (BF ? (bf) : (half))
#ifndef CASE
#define CASE (-1)
#endif
#define ON(n) (CASE < 0 || CASE == (n))

/* Representation moves are macros, so no function interface carries the
 * half type and each case reaches its own conversion. */
#define HBITS(value) ({ H hb_ = (value); unsigned short ub_; \
    memcpy(&ub_, &hb_, sizeof ub_); (unsigned long long)ub_; })
#define HVALUE(bits) ({ unsigned short ub_ = (bits); H hb_; \
    memcpy(&hb_, &ub_, sizeof hb_); hb_; })

static float f32(unsigned bits)
{
    float value;
    memcpy(&value, &bits, sizeof value);
    return value;
}

static unsigned long long f32bits(float value)
{
    unsigned bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

static double f64(unsigned long long bits)
{
    double value;
    memcpy(&value, &bits, sizeof value);
    return value;
}

static unsigned long long f64bits(double value)
{
    unsigned long long bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

static unsigned mismatches;

static void check(int number, unsigned long long got, unsigned long long want)
{
#ifdef PRINT
    printf("case %2d: got %llx want %llx%s\n", number, got, want,
           got == want ? "" : " MISMATCH");
#endif
    if (got != want)
        mismatches |= 1u << (number % 8);
}

int main(void)
{
#if ON(0) /* binary32 1.5 */
    { volatile float in = f32(0x3fc00000u); check(0, HBITS((H)in), WANT(0x3e00, 0x3fc0)); }
#endif
#if ON(1) /* binary32 exactly halfway, even neighbour below */
    { volatile float in = f32(WANT(0x3f801000u, 0x3f808000u)); check(1, HBITS((H)in), WANT(0x3c00, 0x3f80)); }
#endif
#if ON(2) /* binary32 exactly halfway, even neighbour above */
    { volatile float in = f32(WANT(0x3f803000u, 0x3f818000u)); check(2, HBITS((H)in), WANT(0x3c02, 0x3f82)); }
#endif
#if ON(3) /* binary64 just above halfway; a binary32 step would round to the tie */
    { volatile double in = f64(WANT(0x3ff0020000001000ull, 0x3ff0100000001000ull)); check(3, HBITS((H)in), WANT(0x3c01, 0x3f81)); }
#endif
#if ON(4) /* binary32 overflow to infinity */
    { volatile float in = f32(WANT(0x477ff000u, 0x7f7fffffu)); check(4, HBITS((H)in), WANT(0x7c00, 0x7f80)); }
#endif
#if ON(5) /* binary32 just below the overflow threshold */
    { volatile float in = f32(WANT(0x477fefffu, 0x7f7f7fffu)); check(5, HBITS((H)in), WANT(0x7bff, 0x7f7f)); }
#endif
#if ON(6) /* subnormal halfway between zero and the least subnormal */
    { volatile float in = f32(WANT(0x33000000u, 0x00008000u)); check(6, HBITS((H)in), WANT(0x0000, 0x0000)); }
#endif
#if ON(7) /* subnormal halfway, even neighbour above */
    { volatile float in = f32(WANT(0x33c00000u, 0x00018000u)); check(7, HBITS((H)in), WANT(0x0002, 0x0002)); }
#endif
#if ON(8) /* binary32 signaling NaN is quieted */
    { volatile float in = f32(0x7f800001u); check(8, HBITS((H)in), WANT(0x7e00, 0x7fc0)); }
#endif
#if ON(9) /* negative quiet NaN keeps sign and leading payload bits */
    { volatile float in = f32(0xffc12345u); check(9, HBITS((H)in), WANT(0xfe09, 0xffc1)); }
#endif
#if ON(10) /* int halfway case */
    { volatile int in = WANT(2049, 257); check(10, HBITS((H)in), WANT(0x6800, 0x4380)); }
#endif
#if ON(11) /* int outside the binary16 range; IEEE overflow result */
    { volatile int in = WANT(65520, 2147483647); check(11, HBITS((H)in), WANT(0x7c00, 0x4f00)); }
#endif
#if ON(12) /* long long; a binary64 step rounds bf16 to the tie */
    { volatile long long in = WANT(0x7fffffffffffffffll, 0x1010000000000001ll); check(12, HBITS((H)in), WANT(0x7c00, 0x5d81)); }
#endif
#if ON(13) /* signaling NaN widened to binary32 */
    { volatile H in = HVALUE(WANT(0x7d00, 0x7f81)); check(13, f32bits((float)in), WANT(0x7fe00000u, 0x7fc10000u)); }
#endif
#if ON(14) /* least subnormal widened to binary32 */
    { volatile H in = HVALUE(0x0001); check(14, f32bits((float)in), WANT(0x33800000u, 0x00010000u)); }
#endif
#if ON(15) /* conversion to int truncates */
    { volatile H in = HVALUE(WANT(0xbf00, 0xbfe0)); check(15, (unsigned)(int)in, (unsigned)-1); }
#endif
#if ON(16) /* binary64 overflow to infinity */
    { volatile double in = 1e300; check(16, HBITS((H)in), WANT(0x7c00, 0x7f80)); }
#endif
#if ON(17) /* binary64 quiet NaN with an all-ones payload */
    { volatile double in = f64(0x7fffffffffffffffull); check(17, HBITS((H)in), WANT(0x7fff, 0x7fff)); }
#endif
#if ON(18) /* unsigned __int128 maximum overflows both formats */
    { volatile unsigned __int128 in = ~(unsigned __int128)0; check(18, HBITS((H)in), WANT(0x7c00, 0x7f80)); }
#endif
#if ON(19) /* translation-time conversion of the case-3 constant */
    { static const H value = WANT(0x1.0020000001p0, 0x1.0100000001p0); check(19, HBITS(value), WANT(0x3c01, 0x3f81)); }
#endif
#if ON(20) /* widening to binary64 is exact */
    { volatile H in = HVALUE(WANT(0x7bff, 0x7f7f)); check(20, f64bits((double)in), WANT(0x40effc0000000000ull, 0x47efe00000000000ull)); }
#endif
#if ON(21) /* long double (binary128 on Linux) just above halfway */
    { volatile long double in = 1.0L + WANT(0x1p-11L, 0x1p-8L) + 0x1p-100L; check(21, HBITS((H)in), WANT(0x3c01, 0x3f81)); }
#endif
    return (int)mismatches;
}
