/* L04 operation matrix. KIND=32|64|128 selects float, double or long
 * double _Complex (KIND=128 is binary128 on Linux only). CASE=0..9 isolates
 * one family so that each first EvoCC diagnostic maps to one construct.
 * Exit 0 means every assertion held; otherwise the status is the failed
 * check. Expectations are GCC 14 on aarch64 Linux; Darwin differences are
 * marked. Build: CC -std=gnu11 -O0|-O2 -frounding-math -ffp-contract=off
 * -DKIND=K -DCASE=C FILE -lm. No libc output is used, so both EvoCC
 * interpreters can run it once the operations are admitted. */
#include <complex.h>
#include <fenv.h>
#include <float.h>
#include <stdarg.h>
#ifndef KIND
#define KIND 64
#endif
#ifndef CASE
#define CASE 0
#endif
#if KIND == 32
#define R float
#define BUILD(x, y) __builtin_complex((float)(x), (float)(y))
#define MAKE CMPLXF
#define CREAL __builtin_crealf
#define CIMAG __builtin_cimagf
#define CONJ __builtin_conjf
#define HUGE FLT_MAX
#elif KIND == 64
#define R double
#define BUILD(x, y) __builtin_complex((double)(x), (double)(y))
#define MAKE CMPLX
#define CREAL __builtin_creal
#define CIMAG __builtin_cimag
#define CONJ __builtin_conj
#define HUGE DBL_MAX
#else
#define R long double
#define BUILD(x, y) __builtin_complex((long double)(x), (long double)(y))
#define MAKE CMPLXL
#define CREAL __builtin_creall
#define CIMAG __builtin_cimagl
#define CONJ __builtin_conjl
#define HUGE LDBL_MAX
#endif
typedef R _Complex Z;
#define NEG(x) __builtin_signbit(x)
#define ISNAN(x) __builtin_isnan(x)
#define ISINF(x) __builtin_isinf(x)
#define CHECK(n, cond) do { if (!(cond)) return (n); } while (0)

static volatile R zero = 0, one = 1, inf = __builtin_inf(), nan = __builtin_nan("");

#if CASE == 7
static Z s_lit = 1.0 + 14.0 * (1.0fi);          /* torture complex-5 form */
static Z s_build = BUILD(-0.0, 2.5);
static Z s_mul = (1.0 + 2.0i) * (3.0 + 4.0i);    /* exact: -5+10i */
static Z s_div = (9.0 + 7.0i) / (2.0 + 1.0i);    /* exact: 5+1i */
static R s_real = 3.0 + 4.0i;                   /* imaginary part discarded */
#if KIND == 64
static Z s_round = (1.0 + 1.0i) / (0.2 + 0.5i); /* rounded; see packet */
#endif
#endif

#if CASE == 9
static Z va_sum(int n, ...)
{
    va_list ap;
    Z s = 0;
    va_start(ap, n);
    while (n--) s += va_arg(ap, Z);
    va_end(ap);
    return s;
}
static Z pass(Z z) { return z; }
#endif

static int run(void)
{
#if CASE == 0
    /* Representation, parts and lvalues (C11 6.2.5p13, GNU __real__/__imag__). */
    union { Z z; R p[2]; } u;
    CHECK(1, sizeof(Z) == 2 * sizeof(R) && _Alignof(Z) == _Alignof(R));
    u.p[0] = 3; u.p[1] = -4;
    CHECK(2, __real__ u.z == 3 && __imag__ u.z == -4);
    Z z = u.z;
    __real__ z = 5;
    __imag__ z = 6;
    u.z = z;
    CHECK(3, u.p[0] == 5 && u.p[1] == 6);
    CHECK(4, CREAL(z) == 5 && CIMAG(z) == 6);
    Z c = ~BUILD(1, zero);
    CHECK(5, __real__ c == 1 && __imag__ c == 0 && NEG(__imag__ c));
    c = CONJ(BUILD(1, zero));
    CHECK(6, NEG(__imag__ c));
    c = -BUILD(zero, zero);
    CHECK(7, NEG(__real__ c) && NEG(__imag__ c));
    c = +BUILD(-zero, 2);
    CHECK(8, NEG(__real__ c) && __imag__ c == 2);
#elif CASE == 1
    /* Construction: __builtin_complex and CMPLX keep -0 and infinity;
     * x + y*I is arithmetic and turns 0*inf into NaN. */
    feclearexcept(FE_ALL_EXCEPT);
    Z a = BUILD(-zero, inf);
    CHECK(1, NEG(__real__ a) && __real__ a == 0 && ISINF(__imag__ a));
    CHECK(2, fetestexcept(FE_ALL_EXCEPT) == 0);
    Z b = MAKE(-zero, inf);
    CHECK(3, NEG(__real__ b) && ISINF(__imag__ b));
    Z c = (R)-zero + (R)inf * I;
    CHECK(4, ISNAN(__real__ c) && ISINF(__imag__ c));
    CHECK(5, fetestexcept(FE_INVALID) != 0);
    Z d = 2.5i;
    CHECK(6, __real__ d == 0 && !NEG(__real__ d) && __imag__ d == 2.5);
    _Complex int e = 3i;
    CHECK(7, sizeof e == 2 * sizeof(int) && __imag__ e == 3);
#elif CASE == 2
    /* Conversions (C11 6.3.1.6, 6.3.1.7, 6.3.1.2). */
    feclearexcept(FE_ALL_EXCEPT);
    Z a = (R)-1.5;
    CHECK(1, __real__ a == -1.5 && __imag__ a == 0 && !NEG(__imag__ a));
    R r = BUILD(7, nan);
    CHECK(2, r == 7 && fetestexcept(FE_ALL_EXCEPT) == 0);
    CHECK(3, !(_Bool)BUILD(zero, zero) && !(_Bool)BUILD(-zero, -zero));
    CHECK(4, (_Bool)BUILD(zero, nan) && (_Bool)BUILD(zero, 0x1p-100));
    CHECK(5, (int)BUILD(3.75, 100.5) == 3);
    Z i = 7;
    CHECK(6, __real__ i == 7 && __imag__ i == 0 && !NEG(__imag__ i));
    feclearexcept(FE_ALL_EXCEPT);
    volatile double _Complex wide = __builtin_complex(0.1, 0.2);
    float _Complex narrow = wide;
    CHECK(7, __real__ narrow == 0.1f && __imag__ narrow == 0.2f);
    CHECK(8, fetestexcept(FE_INEXACT) != 0);
    double _Complex back = narrow;
    CHECK(9, __real__ back == (double)0.1f && __imag__ back == (double)0.2f);
#elif CASE == 3
    /* Equality compares both parts; truth tests both parts (C11 6.5.9p4). */
    CHECK(1, BUILD(1, zero) == BUILD(1, -zero));
    CHECK(2, !(BUILD(nan, 0) == BUILD(nan, 0)) && BUILD(nan, 0) != BUILD(nan, 0));
    CHECK(3, BUILD(1, zero) == 1.0 && !(BUILD(1, 0x1p-60) == 1.0));
    CHECK(4, !BUILD(zero, zero) && !!BUILD(zero, nan));
    Z z = BUILD(zero, -zero);
    if (z) return 5;
    (void)z;
    CHECK(6, (BUILD(zero, 1) && one) == 1);
    CHECK(7, (BUILD(zero, zero) || zero) == 0);
#elif CASE == 4
    /* Mixed real and complex operands are not converted to complex
     * (C11 6.3.1.8 and G.5.1); GCC and Clang compute per component. */
    Z a = BUILD(1, -zero);
    Z s = a + (R)1;
    CHECK(1, __real__ s == 2 && NEG(__imag__ s));
    Z t = (R)1 - BUILD(1, zero);
    CHECK(2, __real__ t == 0 && NEG(__imag__ t));
    Z m = a * (R)1;
    CHECK(3, __real__ m == 1 && NEG(__imag__ m));
    Z q = a / (R)1;
    CHECK(4, __real__ q == 1 && NEG(__imag__ q));
    Z k = BUILD(1.5, -2) * 2;
    CHECK(5, k == BUILD(3, -4));
    Z v = (R)1 / BUILD(zero, zero);
    CHECK(6, ISINF(__real__ v) && !NEG(__real__ v) && ISNAN(__imag__ v));
#elif CASE == 5
    /* Multiplication: inline formula, Annex G recovery when both parts are NaN. */
    feclearexcept(FE_ALL_EXCEPT);
    CHECK(1, BUILD(1, 2) * BUILD(3, 4) == BUILD(-5, 10));
    CHECK(2, fetestexcept(FE_ALL_EXCEPT) == 0);
    Z g = BUILD(inf, nan) * BUILD(1, 1);
    CHECK(3, ISINF(__real__ g) && ISINF(__imag__ g) && !NEG(__real__ g) && !NEG(__imag__ g));
    Z h = BUILD(zero, zero) * BUILD(inf, zero);
    CHECK(4, ISNAN(__real__ h) && ISNAN(__imag__ h));
    Z o = BUILD(HUGE, HUGE) * BUILD(HUGE, HUGE);
    CHECK(5, ISNAN(__real__ o) && ISINF(__imag__ o));
    Z n = BUILD(-zero, zero) * BUILD(zero, zero);
    CHECK(6, NEG(__real__ n) && __real__ n == 0 && !NEG(__imag__ n) && __imag__ n == 0);
    volatile Z x = BUILD(one / 3, one / 7), y = BUILD(one / 11, one / 13);
    feclearexcept(FE_ALL_EXCEPT);
    Z p = x * y;
    CHECK(7, fetestexcept(FE_INEXACT) != 0 && __real__ p > 0 && __imag__ p > 0);
#elif CASE == 6
    /* Division: the target helper algorithm, including its status flags. */
    CHECK(1, BUILD(9, 7) / BUILD(2, 1) == BUILD(5, 1));
    feclearexcept(FE_ALL_EXCEPT);
    Z z = BUILD(one, one) / BUILD(zero, zero);
    int flags = fetestexcept(FE_ALL_EXCEPT);
    CHECK(2, ISINF(__real__ z) && ISINF(__imag__ z) && !NEG(__real__ z) && !NEG(__imag__ z));
#if defined(__APPLE__) || KIND == 32
    CHECK(3, flags == FE_INVALID);
#else
    CHECK(3, flags == (FE_INVALID | FE_DIVBYZERO));
#endif
    Z i = BUILD(inf, zero) / BUILD(1, 1);
    CHECK(4, ISINF(__real__ i) && ISINF(__imag__ i) && NEG(__imag__ i));
    Z f = BUILD(1, 1) / BUILD(inf, zero);
    CHECK(5, __real__ f == 0 && __imag__ f == 0);
#if KIND == 64 && !defined(__APPLE__)
    /* The qualified libgcc __divdc3 fuses its Smith products. */
    Z q = BUILD(one, one) / BUILD(0.2, 0.5);
    CHECK(6, __real__ q == 0x1.34f72c234f72cp+1 && __imag__ q == -0x1.08d3dcb08d3ddp+0);
#endif
#if KIND == 32 && !defined(__APPLE__)
    /* libgcc __divsc3 computes in binary64: exactly 10, not 0x1.3ffffep+3. */
    Z w = BUILD(one, one) / BUILD(0.1f, 0.1f);
    CHECK(7, __real__ w == 10 && __imag__ w == 0);
#endif
#elif CASE == 7
    /* Static initialization is translation-time arithmetic. */
    CHECK(1, s_lit == BUILD(1, 14));
    CHECK(2, NEG(__real__ s_build) && __imag__ s_build == 2.5);
    CHECK(3, s_mul == BUILD(-5, 10) && s_div == BUILD(5, 1));
    CHECK(4, s_real == 3);
#if KIND == 64
    /* GCC folds with MPC (correctly rounded); the runtime helper differs. */
    CHECK(5, __real__ s_round == 0x1.34f72c234f72cp+1
             && __imag__ s_round == -0x1.08d3dcb08d3dcp+0);
#endif
#elif CASE == 8
    /* Compound assignment, GNU increment and volatile complex objects. */
    Z z = BUILD(1, 2);
    z *= BUILD(3, 4);
    CHECK(1, z == BUILD(-5, 10));
    z /= BUILD(1, 2);
    CHECK(2, z == BUILD(3, 4));
    z += (R)1;
    z -= BUILD(0, 1);
    CHECK(3, z == BUILD(4, 3));
    R r = 10;
    r += BUILD(1, 99);
    CHECK(4, r == 11);
    z++;
    CHECK(5, z == BUILD(5, 3));
    volatile Z v = BUILD(1, 1);
    v += BUILD(2, 2);
    CHECK(6, v == BUILD(3, 3));
#elif CASE == 9
    /* Same-unit calls, returns and variadic access. */
    CHECK(1, pass(BUILD(-zero, inf)) == BUILD(0, inf));
    CHECK(2, NEG(__real__ pass(BUILD(-zero, 1))));
    CHECK(3, va_sum(3, BUILD(1, 2), BUILD(3, 4), BUILD(5, 6)) == BUILD(9, 12));
#endif
    return 0;
}

int main(void)
{
    return run();
}
