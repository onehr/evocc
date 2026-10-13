/* L05 probe: which scalar operands broadcast into floating-vector
 * arithmetic. Each CASE is one expression; a build failure is the
 * measured answer for that compiler. When it builds, exit status 0
 * means every lane holds the expected value.
 *  0 f4 + 2.0        (double constant, exact in float)
 *  1 f4 + 0.1        (double constant, inexact in float)
 *  2 f4 + d          (double variable)
 *  3 f4 + 16777217   (int constant, inexact in float)
 *  4 f4 + i          (int variable)
 *  5 d2 + f          (float variable)
 *  6 d2 + i          (int variable)
 *  7 d2 + l          (long variable)
 *  8 f4 + 2.0L       (long double constant, exact)
 *  9 f4 + 1e40       (double constant, outside float range)
 * 10 f4 < 2.0f       (scalar comparison, lane mask)
 * 11 f4 + c          (char variable)
 * 12 d2 + 9007199254740993L (long constant, inexact in double)
 * 13 f4 + 16777216   (int constant, exact in float) */
#ifndef CASE
#define CASE 0
#endif
typedef float f4 __attribute__((vector_size(16)));
typedef double d2 __attribute__((vector_size(16)));
static volatile double d = 2.0;
static volatile float f = 2.0f;
static volatile int i = 2;
static volatile long l = 2;
static volatile char c = 2;

int main(void)
{
    f4 a = {1, 2, 3, 4};
    d2 b = {1, 2};
#if CASE == 0
    f4 r = a + 2.0;
    return r[0] != 3 || r[3] != 6;
#elif CASE == 1
    f4 r = a + 0.1;
    return r[0] != 1.1f;
#elif CASE == 2
    f4 r = a + d;
    return r[0] != 3;
#elif CASE == 3
    f4 r = a + 16777217;
    return r[0] != 16777218.0f;
#elif CASE == 4
    f4 r = a + i;
    return r[0] != 3;
#elif CASE == 5
    d2 r = b + f;
    return r[0] != 3 || r[1] != 4;
#elif CASE == 6
    d2 r = b + i;
    return r[0] != 3 || r[1] != 4;
#elif CASE == 7
    d2 r = b + l;
    return r[0] != 3 || r[1] != 4;
#elif CASE == 8
    f4 r = a + 2.0L;
    return r[0] != 3;
#elif CASE == 9
    f4 r = a + 1e40;
    return r[0] != __builtin_inff();
#elif CASE == 10
    __typeof__(a < 2.0f) m = a < 2.0f;
    return m[0] != -1 || m[1] != 0;
#elif CASE == 11
    f4 r = a + c;
    return r[0] != 3;
#elif CASE == 12
    d2 r = b + 9007199254740993L;
    return r[0] != 9007199254740994.0;
#elif CASE == 13
    f4 r = a + 16777216;
    return r[0] != 16777216.0f;
#else
#error unsupported CASE
#endif
}
