/* L05 probe: operator admission on GNU floating vectors in C (compile
 * and, when accepted, run). A build failure is the measured answer.
 *  0 v % w    1 ~v     2 v & w    3 !v      4 v && w   5 v++
 *  6 &v[1]    7 v ? v : w with a vector condition
 *  8 long double lanes (vector_size(32)) with +
 *  9 (f2) of a double scalar (equal-size cast)   10 (double) of an f2
 * 11 equal-size f4 <-> i4 reinterpretation and back
 * 12 v[i] read and v[i] = x write with a runtime index
 * Exit status 0 means the accepted expression produced the expected value. */
#ifndef CASE
#define CASE 0
#endif
typedef float f4 __attribute__((vector_size(16)));
typedef float f2 __attribute__((vector_size(8)));
typedef int i4 __attribute__((vector_size(16)));
typedef long double ld2 __attribute__((vector_size(32)));
static volatile int index1 = 1;

int main(void)
{
    f4 v = {1.5f, -2, 3, 0}, w = {2, 2, 2, 2};
#if CASE == 0
    f4 r = v % w;
    return r[0] != 1.5f;
#elif CASE == 1
    f4 r = ~v;
    return r[0] == 1.5f;
#elif CASE == 2
    f4 r = v & w;
    return r[0] == 1.5f;
#elif CASE == 3
    __typeof__(!v) r = !v;
    return r[3] != -1 || r[0] != 0;
#elif CASE == 4
    __typeof__(v && w) r = v && w;
    return r[3] != 0 || r[0] != -1;
#elif CASE == 5
    v++;
    return v[0] != 2.5f;
#elif CASE == 6
    float *p = &v[1];
    return *p != -2;
#elif CASE == 7
    f4 r = v ? v : w;
    return r[3] != 2;
#elif CASE == 8
    ld2 a = {1.0L, 2.0L}, b = {0.5L, 0.25L};
    ld2 r = a + b;
    return r[0] != 1.5L || r[1] != 2.25L;
#elif CASE == 9
    volatile double d = 2.0;
    f2 r = (f2)d;
    return r[1] != 2.0f || r[0] != 0.0f;
#elif CASE == 10
    f2 a = {1, 2};
    double d = (double)a;
    union { double d; unsigned long u; } x = {.d = d};
    return x.u != 0x400000003f800000ul;
#elif CASE == 11
    i4 bits = (i4)v;
    f4 back = (f4)bits;
    return bits[0] != 0x3fc00000 || back[1] != -2;
#elif CASE == 12
    float x = v[index1];
    v[index1 + 1] = x;
    return v[2] != -2;
#else
#error unsupported CASE
#endif
}
