/* L04 helper qualification probe. Reference compilers only.
 *
 * It compares the linked complex multiply/divide helpers with C models of
 * the libgcc 14.2 and compiler-rt 19.1.7 source algorithms. Each model runs
 * unfused and with fma() where a fused build would contract. It also prints
 * the status flags of the (1+1i)/(0+0i) helper calls.
 *
 * Build: CC -std=gnu11 -O2 -frounding-math -ffp-contract=off FILE -lm
 * The exit status is 0 when the probe ran; the printed counts are the data.
 * On Darwin, long double is binary64, so the f128 rows are skipped there.
 */
#include <fenv.h>
#include <float.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef float _Complex cf;
typedef double _Complex cd;
cf __mulsc3(float, float, float, float);
cf __divsc3(float, float, float, float);
cd __muldc3(double, double, double, double);
cd __divdc3(double, double, double, double);
#if LDBL_MANT_DIG == 113
typedef long double _Complex cq;
cq __multc3(long double, long double, long double, long double);
cq __divtc3(long double, long double, long double, long double);
#endif

/* ---- libgcc2.c (GCC 14.2) DIVC3, binary64 Smith path; F selects fma. ---- */
/* x*y + z, x*y - z and z - x*y, fused when F is set; otherwise exactly the
 * source expression, so directed rounding sees the same intermediate. */
#define MADD(F, x, y, z) ((F) ? fma((x), (y), (z)) : (x) * (y) + (z))
#define MSUB(F, x, y, z) ((F) ? fma((x), (y), -(z)) : (x) * (y) - (z))
#define RSUB(F, x, y, z) ((F) ? fma(-(x), (y), (z)) : (z) - (x) * (y))
static cd gcc_divdc3(double a, double b, double c, double d, int F)
{
    const double RBIG = DBL_MAX / 2, RMIN = DBL_MIN, RMIN2 = DBL_EPSILON;
    const double RMINSCAL = 1 / DBL_EPSILON, RMAX2 = RBIG * RMIN2;
    double denom, ratio, x, y;
    if (fabs(c) < fabs(d)) {
        if (fabs(d) >= RBIG) { a /= 2; b /= 2; c /= 2; d /= 2; }
        if (fabs(d) < RMIN2) { a *= RMINSCAL; b *= RMINSCAL; c *= RMINSCAL; d *= RMINSCAL; }
        else if ((fabs(a) < RMIN && fabs(b) < RMAX2 && fabs(d) < RMAX2)
                 || (fabs(b) < RMIN && fabs(a) < RMAX2 && fabs(d) < RMAX2)) {
            a *= RMINSCAL; b *= RMINSCAL; c *= RMINSCAL; d *= RMINSCAL;
        }
        ratio = c / d;
        denom = MADD(F, c, ratio, d);
        if (fabs(ratio) > RMIN) {
            x = MADD(F, a, ratio, b) / denom;
            y = MSUB(F, b, ratio, a) / denom;
        } else {
            x = MADD(F, a / d, c, b) / denom;
            y = MSUB(F, b / d, c, a) / denom;
        }
    } else {
        if (fabs(c) >= RBIG) { a /= 2; b /= 2; c /= 2; d /= 2; }
        if (fabs(c) < RMIN2) { a *= RMINSCAL; b *= RMINSCAL; c *= RMINSCAL; d *= RMINSCAL; }
        else if ((fabs(a) < RMIN && fabs(b) < RMAX2 && fabs(c) < RMAX2)
                 || (fabs(b) < RMIN && fabs(a) < RMAX2 && fabs(c) < RMAX2)) {
            a *= RMINSCAL; b *= RMINSCAL; c *= RMINSCAL; d *= RMINSCAL;
        }
        ratio = d / c;
        denom = MADD(F, d, ratio, c);
        if (fabs(ratio) > RMIN) {
            x = MADD(F, b, ratio, a) / denom;
            y = RSUB(F, a, ratio, b) / denom;
        } else {
            x = MADD(F, b / c, d, a) / denom;
            y = RSUB(F, a / c, d, b) / denom;
        }
    }
    if (isnan(x) && isnan(y)) {
        if (c == 0.0 && d == 0.0 && (!isnan(a) || !isnan(b))) {
            x = copysign(INFINITY, c) * a;
            y = copysign(INFINITY, c) * b;
        } else if ((isinf(a) || isinf(b)) && isfinite(c) && isfinite(d)) {
            a = copysign(isinf(a) ? 1 : 0, a);
            b = copysign(isinf(b) ? 1 : 0, b);
            x = INFINITY * (a * c + b * d);
            y = INFINITY * (b * c - a * d);
        } else if ((isinf(c) || isinf(d)) && isfinite(a) && isfinite(b)) {
            c = copysign(isinf(c) ? 1 : 0, c);
            d = copysign(isinf(d) ? 1 : 0, d);
            x = 0.0 * (a * c + b * d);
            y = 0.0 * (b * c - a * d);
        }
    }
    return __builtin_complex(x, y);
}

/* libgcc2.c DIVC3 for SFmode with hardware double: plain formula in double. */
static cf gcc_divsc3(float a, float b, float c, float d, int F)
{
    double aa = a, bb = b, cc = c, dd = d;
    double denom = F ? fma(cc, cc, dd * dd) : cc * cc + dd * dd;
    float x = (F ? fma(aa, cc, bb * dd) : aa * cc + bb * dd) / denom;
    float y = (F ? fma(bb, cc, -(aa * dd)) : bb * cc - aa * dd) / denom;
    if (isnan(x) && isnan(y)) {
        if (c == 0.0 && d == 0.0 && (!isnan(a) || !isnan(b))) {
            x = copysignf(INFINITY, c) * a;
            y = copysignf(INFINITY, c) * b;
        } else if ((isinf(a) || isinf(b)) && isfinite(c) && isfinite(d)) {
            a = copysignf(isinf(a) ? 1 : 0, a);
            b = copysignf(isinf(b) ? 1 : 0, b);
            x = INFINITY * (a * c + b * d);
            y = INFINITY * (b * c - a * d);
        } else if ((isinf(c) || isinf(d)) && isfinite(a) && isfinite(b)) {
            c = copysignf(isinf(c) ? 1 : 0, c);
            d = copysignf(isinf(d) ? 1 : 0, d);
            x = 0.0f * (a * c + b * d);
            y = 0.0f * (b * c - a * d);
        }
    }
    return __builtin_complex(x, y);
}

/* libgcc2.c MULC3 and compiler-rt muldc3.c share this algorithm. */
static cd gcc_muldc3(double a, double b, double c, double d)
{
    double ac = a * c, bd = b * d, ad = a * d, bc = b * c;
    double x = ac - bd, y = ad + bc;
    if (isnan(x) && isnan(y)) {
        int recalc = 0;
        if (isinf(a) || isinf(b)) {
            a = copysign(isinf(a) ? 1 : 0, a);
            b = copysign(isinf(b) ? 1 : 0, b);
            if (isnan(c)) c = copysign(0, c);
            if (isnan(d)) d = copysign(0, d);
            recalc = 1;
        }
        if (isinf(c) || isinf(d)) {
            c = copysign(isinf(c) ? 1 : 0, c);
            d = copysign(isinf(d) ? 1 : 0, d);
            if (isnan(a)) a = copysign(0, a);
            if (isnan(b)) b = copysign(0, b);
            recalc = 1;
        }
        if (!recalc && (isinf(ac) || isinf(bd) || isinf(ad) || isinf(bc))) {
            if (isnan(a)) a = copysign(0, a);
            if (isnan(b)) b = copysign(0, b);
            if (isnan(c)) c = copysign(0, c);
            if (isnan(d)) d = copysign(0, d);
            recalc = 1;
        }
        if (recalc) {
            x = INFINITY * (a * c - b * d);
            y = INFINITY * (a * d + b * c);
        }
    }
    return __builtin_complex(x, y);
}

/* compiler-rt fp_lib.h logb/scalbn/fmax ports: no libm, so no libm status. */
#define CRT_HELPERS(T, U, SIG, EXPW, BIAS, SUF)                                     \
static T crt_logb##SUF(T x)                                                     \
{                                                                               \
    U rep; memcpy(&rep, &x, sizeof rep);                                        \
    int e = (int)((rep >> SIG) & ((1u << EXPW) - 1));                          \
    if (e == (1 << EXPW) - 1) return (rep >> (SIG + EXPW)) == 0 || x != x ? x : -x; \
    if (x == 0) { U inf = ((U)((1u << EXPW) - 1) << SIG) | ((U)1 << (SIG + EXPW)); \
                  T r; memcpy(&r, &inf, sizeof r); return r; }                  \
    if (e != 0) return (T)(e - BIAS);                                           \
    U m = rep & (((U)1 << SIG) - 1); int shift = 0;                             \
    while (!(m & ((U)1 << SIG))) { m <<= 1; ++shift; }                          \
    return (T)(1 - BIAS - shift);                                               \
}                                                                               \
static T crt_scalbn##SUF(T x, int y)                                            \
{                                                                               \
    U rep; memcpy(&rep, &x, sizeof rep);                                        \
    int e = (int)((rep >> SIG) & ((1u << EXPW) - 1));                          \
    if (x == 0 || e == (1 << EXPW) - 1) return x;                               \
    U sig = rep & (((U)1 << SIG) - 1), sign = rep & ((U)1 << (SIG + EXPW));     \
    if (e == 0) { int sh = 0; while (!(sig & ((U)1 << SIG))) { sig <<= 1; ++sh; } \
                  e = 1 - sh; sig &= ((U)1 << SIG) - 1; }                       \
    if (__builtin_sadd_overflow(e, y, &e)) e = y >= 0 ? 1 << 30 : -(1 << 30);   \
    if (e >= (1 << EXPW) - 1) { U big = sign | ((U)((1 << EXPW) - 2) << SIG);   \
                                T r; memcpy(&r, &big, sizeof r); return r * 2; } \
    if (e <= 0) { U t = sign | ((U)1 << SIG) | sig; T tmp; memcpy(&tmp, &t, sizeof tmp); \
                  e += BIAS - 1; if (e < 1) e = 1;                              \
                  U f = (U)e << SIG; T sc; memcpy(&sc, &f, sizeof sc); return tmp * sc; } \
    U out = sign | ((U)e << SIG) | sig; T r; memcpy(&r, &out, sizeof r); return r; \
}                                                                               \
static T crt_fmax##SUF(T x, T y) { return (x != x || x < y) ? y : x; }
CRT_HELPERS(double, uint64_t, 52, 11, 1023, d)
CRT_HELPERS(float, uint32_t, 23, 8, 127, f)

/* compiler-rt 19.1.7 divdc3.c (logb/scalbn scaling, no Smith). */
static cd crt_divdc3(double a, double b, double c, double d, int F)
{
    int ilogbw = 0;
    double logbw = crt_logbd(crt_fmaxd(fabs(c), fabs(d)));
    if (isfinite(logbw)) {
        ilogbw = (int)logbw;
        c = crt_scalbnd(c, -ilogbw);
        d = crt_scalbnd(d, -ilogbw);
    }
    double denom = F ? fma(c, c, d * d) : c * c + d * d;
    double x = crt_scalbnd((F ? fma(a, c, b * d) : a * c + b * d) / denom, -ilogbw);
    double y = crt_scalbnd((F ? fma(b, c, -(a * d)) : b * c - a * d) / denom, -ilogbw);
    if (isnan(x) && isnan(y)) {
        if (denom == 0.0 && (!isnan(a) || !isnan(b))) {
            x = copysign(INFINITY, c) * a;
            y = copysign(INFINITY, c) * b;
        } else if ((isinf(a) || isinf(b)) && isfinite(c) && isfinite(d)) {
            a = copysign(isinf(a) ? 1.0 : 0.0, a);
            b = copysign(isinf(b) ? 1.0 : 0.0, b);
            x = INFINITY * (a * c + b * d);
            y = INFINITY * (b * c - a * d);
        } else if (isinf(logbw) && logbw > 0.0 && isfinite(a) && isfinite(b)) {
            c = copysign(isinf(c) ? 1.0 : 0.0, c);
            d = copysign(isinf(d) ? 1.0 : 0.0, d);
            x = 0.0 * (a * c + b * d);
            y = 0.0 * (b * c - a * d);
        }
    }
    return __builtin_complex(x, y);
}

/* compiler-rt 19.1.7 divsc3.c, float throughout. */
static cf crt_divsc3(float a, float b, float c, float d, int F)
{
    int ilogbw = 0;
    float logbw = crt_logbf(crt_fmaxf(fabsf(c), fabsf(d)));
    if (isfinite(logbw)) {
        ilogbw = (int)logbw;
        c = crt_scalbnf(c, -ilogbw);
        d = crt_scalbnf(d, -ilogbw);
    }
    float denom = F ? fmaf(c, c, d * d) : c * c + d * d;
    float x = crt_scalbnf((F ? fmaf(a, c, b * d) : a * c + b * d) / denom, -ilogbw);
    float y = crt_scalbnf((F ? fmaf(b, c, -(a * d)) : b * c - a * d) / denom, -ilogbw);
    if (isnan(x) && isnan(y)) {
        if (denom == 0.0f && (!isnan(a) || !isnan(b))) {
            x = copysignf(INFINITY, c) * a;
            y = copysignf(INFINITY, c) * b;
        } else if ((isinf(a) || isinf(b)) && isfinite(c) && isfinite(d)) {
            a = copysignf(isinf(a) ? 1.0f : 0.0f, a);
            b = copysignf(isinf(b) ? 1.0f : 0.0f, b);
            x = INFINITY * (a * c + b * d);
            y = INFINITY * (b * c - a * d);
        } else if (isinf(logbw) && logbw > 0.0f && isfinite(a) && isfinite(b)) {
            c = copysignf(isinf(c) ? 1.0f : 0.0f, c);
            d = copysignf(isinf(d) ? 1.0f : 0.0f, d);
            x = 0.0f * (a * c + b * d);
            y = 0.0f * (b * c - a * d);
        }
    }
    return __builtin_complex(x, y);
}

#if LDBL_MANT_DIG == 113
/* libgcc2.c DIVC3 for TFmode (soft-fp operations, no contraction). */
static cq gcc_divtc3(long double a, long double b, long double c, long double d)
{
    const long double RBIG = LDBL_MAX / 2, RMIN = LDBL_MIN, RMIN2 = LDBL_EPSILON;
    const long double RMINSCAL = 1 / LDBL_EPSILON, RMAX2 = RBIG * RMIN2;
    long double denom, ratio, x, y;
    if (fabsl(c) < fabsl(d)) {
        if (fabsl(d) >= RBIG) { a /= 2; b /= 2; c /= 2; d /= 2; }
        if (fabsl(d) < RMIN2) { a *= RMINSCAL; b *= RMINSCAL; c *= RMINSCAL; d *= RMINSCAL; }
        else if ((fabsl(a) < RMIN && fabsl(b) < RMAX2 && fabsl(d) < RMAX2)
                 || (fabsl(b) < RMIN && fabsl(a) < RMAX2 && fabsl(d) < RMAX2)) {
            a *= RMINSCAL; b *= RMINSCAL; c *= RMINSCAL; d *= RMINSCAL;
        }
        ratio = c / d; denom = (c * ratio) + d;
        if (fabsl(ratio) > RMIN) { x = ((a * ratio) + b) / denom; y = ((b * ratio) - a) / denom; }
        else { x = ((c * (a / d)) + b) / denom; y = ((c * (b / d)) - a) / denom; }
    } else {
        if (fabsl(c) >= RBIG) { a /= 2; b /= 2; c /= 2; d /= 2; }
        if (fabsl(c) < RMIN2) { a *= RMINSCAL; b *= RMINSCAL; c *= RMINSCAL; d *= RMINSCAL; }
        else if ((fabsl(a) < RMIN && fabsl(b) < RMAX2 && fabsl(c) < RMAX2)
                 || (fabsl(b) < RMIN && fabsl(a) < RMAX2 && fabsl(c) < RMAX2)) {
            a *= RMINSCAL; b *= RMINSCAL; c *= RMINSCAL; d *= RMINSCAL;
        }
        ratio = d / c; denom = (d * ratio) + c;
        if (fabsl(ratio) > RMIN) { x = ((b * ratio) + a) / denom; y = (b - (a * ratio)) / denom; }
        else { x = (a + (d * (b / c))) / denom; y = (b - (d * (a / c))) / denom; }
    }
    if (isnan(x) && isnan(y)) {
        if (c == 0.0L && d == 0.0L && (!isnan(a) || !isnan(b))) {
            x = copysignl(INFINITY, c) * a; y = copysignl(INFINITY, c) * b;
        } else if ((isinf(a) || isinf(b)) && isfinite(c) && isfinite(d)) {
            a = copysignl(isinf(a) ? 1 : 0, a); b = copysignl(isinf(b) ? 1 : 0, b);
            x = INFINITY * (a * c + b * d); y = INFINITY * (b * c - a * d);
        } else if ((isinf(c) || isinf(d)) && isfinite(a) && isfinite(b)) {
            c = copysignl(isinf(c) ? 1 : 0, c); d = copysignl(isinf(d) ? 1 : 0, d);
            x = 0.0L * (a * c + b * d); y = 0.0L * (b * c - a * d);
        }
    }
    return __builtin_complex(x, y);
}
#endif

static uint64_t state = 0x9e3779b97f4a7c15u;
static uint64_t next(void)
{
    state ^= state << 13; state ^= state >> 7; state ^= state << 17;
    return state;
}
/* A finite double with a random sign, significand and exponent. */
static double rnd(int wide)
{
    uint64_t r = next();
    int e = wide ? (int)(r >> 53) % 2046 + 1 : 1023 + (int)(r >> 53) % 41 - 20;
    uint64_t bits = (r & 0x000fffffffffffffu) | ((uint64_t)e << 52) | (r & 0x8000000000000000u);
    double v; memcpy(&v, &bits, 8); return v;
}
static int b64(double x, double y) { return memcmp(&x, &y, 8) == 0; }
static int b32(float x, float y) { return memcmp(&x, &y, 4) == 0; }
static int v64(double x, double y) { return (isnan(x) && isnan(y)) || b64(x, y); }
static int v32(float x, float y) { return (isnan(x) && isnan(y)) || b32(x, y); }
/* Count value (NaN equals NaN) and exact-bit differences of two results. */
struct tally { long value, bits; };
static void dd(struct tally *t, cd x, cd y)
{
    t->value += !(v64(__real__ x, __real__ y) && v64(__imag__ x, __imag__ y));
    t->bits += !(b64(__real__ x, __real__ y) && b64(__imag__ x, __imag__ y));
}
static void ff(struct tally *t, cf x, cf y)
{
    t->value += !(v32(__real__ x, __real__ y) && v32(__imag__ x, __imag__ y));
    t->bits += !(b32(__real__ x, __real__ y) && b32(__imag__ x, __imag__ y));
}
#define T(t) (t).value, (t).bits

/* Opaque helper pointers: no compiler may treat a call as a pure function. */
static cd (*volatile divdc)(double, double, double, double) = __divdc3;
static cd (*volatile muldc)(double, double, double, double) = __muldc3;
static cf (*volatile divsc)(float, float, float, float) = __divsc3;
static cf (*volatile mulsc)(float, float, float, float) = __mulsc3;
#if LDBL_MANT_DIG == 113
static cq (*volatile divtc)(long double, long double, long double, long double) = __divtc3;
#endif

static const double special[] = { 0.0, -0.0, 1.0, -1.0, 0.5, 3.0, 7.0, 0x1p-1022,
    0x1p-1074, 0x1.fffffffffffffp1023, INFINITY, -INFINITY, NAN };
#define NS (sizeof special / sizeof special[0])

int main(void)
{
    static const int modes[4] = { FE_TONEAREST, FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO };
    static const char *names[4] = { "RNE", "UP", "DOWN", "ZERO" };
    /* 1. Status of the helpers for (1+1i)/(0+0i) and one Annex G product. */
    feclearexcept(FE_ALL_EXCEPT);
    cf qf = divsc(1, 1, 0, 0);
    int fs = fetestexcept(FE_ALL_EXCEPT);
    feclearexcept(FE_ALL_EXCEPT);
    cd qd = divdc(1, 1, 0, 0);
    int fdd = fetestexcept(FE_ALL_EXCEPT);
    printf("(1+1i)/(0+0i): sc flags=%#x re=%a im=%a; dc flags=%#x re=%a im=%a\n",
           fs, (double)__real__ qf, (double)__imag__ qf, fdd, __real__ qd, __imag__ qd);
    printf("flag bits: invalid=%#x divbyzero=%#x overflow=%#x underflow=%#x inexact=%#x\n",
           FE_INVALID, FE_DIVBYZERO, FE_OVERFLOW, FE_UNDERFLOW, FE_INEXACT);
    feclearexcept(FE_ALL_EXCEPT);
    cd md = muldc(INFINITY, NAN, 1, 1);
    printf("muldc3(inf+nan i, 1+1i): re=%a im=%a flags=%#x\n", __real__ md, __imag__ md,
           fetestexcept(FE_ALL_EXCEPT));

    /* 2. Special-operand grid: values, bits and flags, helper versus models. */
    struct tally gf = {0}, gu = {0}, gc = {0}, gm = {0}, gs = {0}, gsf = {0}, gcs = {0};
    long grid = 0, flf = 0, flm = 0, fls = 0, flc = 0, flcs = 0;
    for (unsigned i = 0; i < NS; ++i) for (unsigned j = 0; j < NS; ++j)
    for (unsigned k = 0; k < NS; ++k) for (unsigned l = 0; l < NS; ++l) {
        double a = special[i], b = special[j], c = special[k], d = special[l];
        int f1, f2;
        ++grid;
        feclearexcept(FE_ALL_EXCEPT); cd h = divdc(a, b, c, d); f1 = fetestexcept(FE_ALL_EXCEPT);
        feclearexcept(FE_ALL_EXCEPT); cd m = gcc_divdc3(a, b, c, d, 1); f2 = fetestexcept(FE_ALL_EXCEPT);
        dd(&gf, h, m); flf += f1 != f2;
        dd(&gu, h, gcc_divdc3(a, b, c, d, 0));
        feclearexcept(FE_ALL_EXCEPT); cd r = crt_divdc3(a, b, c, d, 1); f2 = fetestexcept(FE_ALL_EXCEPT);
        dd(&gc, h, r); flc += f1 != f2;
        feclearexcept(FE_ALL_EXCEPT); cd p = muldc(a, b, c, d); f1 = fetestexcept(FE_ALL_EXCEPT);
        feclearexcept(FE_ALL_EXCEPT); cd q = gcc_muldc3(a, b, c, d); f2 = fetestexcept(FE_ALL_EXCEPT);
        dd(&gm, p, q); flm += f1 != f2;
        float fa = (float)a, fb = (float)b, fc = (float)c, fv = (float)d;
        feclearexcept(FE_ALL_EXCEPT); cf hs = divsc(fa, fb, fc, fv); f1 = fetestexcept(FE_ALL_EXCEPT);
        feclearexcept(FE_ALL_EXCEPT); cf ms = gcc_divsc3(fa, fb, fc, fv, 1); f2 = fetestexcept(FE_ALL_EXCEPT);
        ff(&gsf, hs, ms); fls += f1 != f2;
        ff(&gs, hs, gcc_divsc3(fa, fb, fc, fv, 0));
        feclearexcept(FE_ALL_EXCEPT); cf cs = crt_divsc3(fa, fb, fc, fv, 1); f2 = fetestexcept(FE_ALL_EXCEPT);
        ff(&gcs, hs, cs); flcs += f1 != f2;
    }
    printf("grid %ld (value/bits; flag-set differences):\n", grid);
    printf("  divdc3 vs libgcc model fused %ld/%ld (%ld), unfused %ld/%ld; vs crt model fused %ld/%ld (%ld)\n",
           T(gf), flf, T(gu), T(gc), flc);
    printf("  muldc3 vs model %ld/%ld (%ld)\n", T(gm), flm);
    printf("  divsc3 vs libgcc model fused %ld/%ld (%ld), unfused %ld/%ld; vs crt model fused %ld/%ld (%ld)\n",
           T(gsf), fls, T(gs), T(gcs), flcs);

    /* 3. Pseudo-random finite operands in four rounding modes. */
    for (int mi = 0; mi < 4; ++mi) {
        struct tally f = {0}, u = {0}, c0 = {0}, c1 = {0}, mu = {0}, s1 = {0}, cs = {0}, q = {0}, rn = {0};
        long n = 0;
        state = 0x9e3779b97f4a7c15u;
        for (int t = 0; t < 200000; ++t) {
            int wide = t & 1;
            double a = rnd(wide), b = rnd(wide), c = rnd(wide), d = rnd(wide);
            float fa = (float)a, fb = (float)b, fc = (float)c, fv = (float)d;
            fesetround(modes[mi]);
            cd h = divdc(a, b, c, d);
            cd m = gcc_divdc3(a, b, c, d, 1), um = gcc_divdc3(a, b, c, d, 0);
            cd r0 = crt_divdc3(a, b, c, d, 0), r1 = crt_divdc3(a, b, c, d, 1);
            cd p = muldc(a, b, c, d), pm = gcc_muldc3(a, b, c, d);
            cf hs = divsc(fa, fb, fc, fv), ms = gcc_divsc3(fa, fb, fc, fv, 1);
            cf crs = crt_divsc3(fa, fb, fc, fv, 1);
#if LDBL_MANT_DIG == 113
            cq hq = divtc(a, b, c, d), mq = gcc_divtc3(a, b, c, d);
            q.value += memcmp(&hq, &mq, sizeof hq) != 0;
            q.bits = q.value;
#endif
            fesetround(FE_TONEAREST);
            cd h0 = divdc(a, b, c, d);
            ++n;
            dd(&f, h, m); dd(&u, h, um); dd(&c0, h, r0); dd(&c1, h, r1); dd(&mu, p, pm);
            ff(&s1, hs, ms); ff(&cs, hs, crs); dd(&rn, h, h0);
        }
        printf("%s random %ld (value/bits): divdc3 vs libgcc fused %ld/%ld, unfused %ld/%ld;"
               " vs crt unfused %ld/%ld, fused %ld/%ld; muldc3 vs model %ld/%ld;"
               " divsc3 vs libgcc fused %ld/%ld, vs crt fused %ld/%ld; divtc3 vs model %ld;"
               " divdc3 differs from its RNE result %ld\n", names[mi], n, T(f), T(u), T(c0),
               T(c1), T(mu), T(s1), T(cs), q.value, rn.value);
    }
    (void)mulsc;
    return 0;
}
