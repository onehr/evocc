/* A separate patched-dependency lane. No host FP supplies expected results.
 * The binary128 oracle rounds a <=128-bit hexadecimal significand with integer
 * quotient/remainder arithmetic, independently of floatscan and APFloat. */
#pragma STDC FENV_ACCESS ON
#include <fenv.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <math.h>

typedef unsigned __int128 U;
static int checks, errors;
static const int modes[] = {FE_TONEAREST, FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO};

static U expected(U m, int exponent, int negative, int mode)
{
    int length = 0;
    for (U n = m; n; n >>= 1) ++length;
    int top = exponent + length - 125;
    int shift = length - 113;
    int subshift = 124 - exponent - 16494;
    if (shift < subshift) shift = subshift;
    U q = m >> shift, rem = m - (q << shift);
    U half = (U)1 << (shift - 1);
    int up = mode == FE_TONEAREST ? rem > half || (rem == half && (q & 1))
           : rem && ((mode == FE_UPWARD && !negative) ||
                     (mode == FE_DOWNWARD && negative));
    q += up;
    if (q == (U)1 << 113) { q >>= 1; ++top; }
    U sign = (U)negative << 127;
    if (top > 16383) {
        int infinity = mode == FE_TONEAREST ||
                       (mode == FE_UPWARD && !negative) ||
                       (mode == FE_DOWNWARD && negative);
        return sign | (infinity ? (U)0x7fff << 112 : ((U)0x7fff << 112) - 1);
    }
    if (top < -16382) return sign | q;
    return sign | (U)(top + 16383) << 112 | (q & (((U)1 << 112) - 1));
}

static U representation(long double x)
{
    union { long double x; uint64_t word[2]; } u = {x};
    return (U)u.word[1] << 64 | u.word[0];
}

static void check_hex(const char *s, U want)
{
    char *end;
    long double x = strtold(s, &end);
    ++checks;
    if (*end || representation(x) != want) {
        if (errors++ < 12) printf("strtold mismatch %s mode=%d want=%016lx%016lx got=%016lx%016lx\n",
            s, fegetround(), (unsigned long)(want >> 64), (unsigned long)want,
            (unsigned long)(representation(x) >> 64), (unsigned long)representation(x));
    }
    int n = -1;
    int ret = sscanf(s, "%La%n", &x, &n);
    ++checks;
    if (ret != 1 || n < 0 || s[n] || representation(x) != want) {
        if (errors++ < 12) printf("scanf mismatch %s mode=%d\n", s, fegetround());
    }
}

static void hex_matrix(int mode)
{
    const char digits[] = "0123456789abcdef";
    const int leading[] = {1, 2, 4, 8, 15};
    const unsigned tails[] = {0, 1, 0x7fff, 0x8000, 0x8001, 0xffff};
    const int exponents[] = {0, -16382, -16494, -16464, -16462, 16380};
    char s[80];
    for (int neg = 0; neg < 2; ++neg)
    for (int l = 0; l < 5; ++l)
    for (int parity = 1; parity <= 2; ++parity)
    for (int t = 0; t < 6; ++t)
    for (int e = 0; e < 6; ++e) {
        U m = leading[l];
        for (int i = 1; i < 28; ++i) m = (m << 4) | (i == 27 ? parity : 1);
        m = (m << 16) | tails[t];
        int n = 0;
        if (neg) s[n++] = '-';
        s[n++] = '0'; s[n++] = 'x';
        for (int i = 31; i >= 0; --i) {
            s[n++] = digits[(unsigned)(m >> (i*4)) & 15];
            if (i == 31) s[n++] = '.';
        }
        snprintf(s+n, sizeof s-n, "p%d", exponents[e]);
        check_hex(s, expected(m, exponents[e], neg, mode));
    }
}

static void power_identities(void)
{
    static const uint32_t values[] = {
        0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x807fffff,
        0x00800000, 0x80800000, 0x3f800000, 0xbf800000,
        0x3eaaaaab, 0xbeaaaaab, 0x7f7fffff, 0xff7fffff,
        0x7f800000, 0xff800000, 0x7fc12345, 0xffc12345,
        0x7f812345, 0xff812345
    };
    float (*volatile call)(float, float) = powf;
    for (unsigned i = 0; i < sizeof values/sizeof *values; ++i) {
        union {float f; uint32_t u;} x, y;
        x.u = values[i];
        uint32_t absolute = x.u & 0x7fffffff;
        int signaling = absolute > 0x7f800000 && !(absolute & 0x400000);
        uint32_t want = x.u | (signaling ? 0x400000 : 0);
        for (int sticky = 0; sticky < 2; ++sticky) {
            feclearexcept(FE_ALL_EXCEPT);
            if (sticky) feraiseexcept(FE_DIVBYZERO);
            y.f = call(x.f, 1.0f);
            int flags = fetestexcept(FE_ALL_EXCEPT);
            ++checks;
            /* powf's library contract does not specify a NaN sign/payload.
             * Exact identity applies to non-NaNs; NaNs must be quiet, with
             * INVALID for a signaling argument, without losing sticky flags. */
            int value_ok = absolute > 0x7f800000
                ? (y.u & 0x7fc00000) == 0x7fc00000 : y.u == want;
            if (!value_ok || flags != ((sticky ? FE_DIVBYZERO : 0) |
                                         (signaling ? FE_INVALID : 0))) {
                if (errors++ < 12) printf("powf mismatch %08x -> %08x flags=%x\n", x.u, y.u, flags);
            }
        }
    }
}

int main(void)
{
    if (sizeof(long double) != 16) return 2;
    for (unsigned r = 0; r < sizeof modes/sizeof *modes; ++r) {
        if (fesetround(modes[r])) return 3;
        hex_matrix(modes[r]);
        power_identities();
    }
    fesetround(FE_TONEAREST);
    printf("checks=%d failures=%d\n", checks, errors);
    return errors != 0;
}
