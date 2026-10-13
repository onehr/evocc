/* L05 probe: sticky exception flags of one floating-vector operation are
 * the union of the flags of the same scalar operations on each lane.
 * Exit status: bits 0..4 = INVALID, DIVBYZERO, OVERFLOW, UNDERFLOW,
 * INEXACT raised by the vector operation; bit 6 set when that differs
 * from the union of the scalar lane operations.
 *  0 f4 / : {1/0, 0/0, 1/3, 3/1}        expect INVALID|DIVBYZERO|INEXACT
 *  1 f4 + : lane 3 overflows             expect OVERFLOW|INEXACT
 *  2 f4 < : one quiet-NaN lane           C relational: INVALID
 *  3 f4 ==: one quiet-NaN lane           quiet equality: none
 *  4 f4 ==: one signaling-NaN lane       INVALID
 *  5 f2 / : two exact 8-byte lanes       none (no hidden upper lanes)
 *  6 f8 * : wide vector, lane 7 underflows  UNDERFLOW|INEXACT
 *  7 f4 -> i4 conversion with a NaN lane (undefined for scalar C; observe)
 *  8 d2 / : {1/0, 1/1}                   DIVBYZERO
 *  9 unary minus of a signaling NaN lane none
 * 10 f4 !=: one quiet-NaN lane           none
 * 11 f4 >=: one quiet-NaN lane           INVALID
 * Build references with -ffp-contract=off -frounding-math. */
#include <fenv.h>
#pragma STDC FENV_ACCESS ON
#ifndef CASE
#define CASE 0
#endif
typedef float f4 __attribute__((vector_size(16)));
typedef float f2 __attribute__((vector_size(8)));
typedef float f8 __attribute__((vector_size(32)));
typedef double d2 __attribute__((vector_size(16)));
typedef int i4 __attribute__((vector_size(16)));
typedef union { float f; unsigned u; } F;

static int bits(void)
{
    int raised = fetestexcept(FE_ALL_EXCEPT), r = 0;
    r |= raised & FE_INVALID ? 1 : 0;
    r |= raised & FE_DIVBYZERO ? 2 : 0;
    r |= raised & FE_OVERFLOW ? 4 : 0;
    r |= raised & FE_UNDERFLOW ? 8 : 0;
    r |= raised & FE_INEXACT ? 16 : 0;
    return r;
}

volatile float vin[8];
volatile float win[8];
volatile double dvin[2], dwin[2];
volatile f4 vsink;
volatile f2 v2sink;
volatile f8 v8sink;
volatile d2 dsink;
volatile i4 isink;
volatile float ssink;
volatile int csink;

__attribute__((noinline)) static int scalar_union(int which, int lanes)
{
    int all = 0;
    for (int k = 0; k < lanes; ++k) {
        feclearexcept(FE_ALL_EXCEPT);
        float a = vin[k], b = win[k];
        switch (which) {
        case 0: case 5: ssink = a / b; break;
        case 1: ssink = a + b; break;
        case 2: csink = a < b; break;
        case 3: case 4: csink = a == b; break;
        case 6: ssink = a * b; break;
        case 9: ssink = -a; break;
        case 10: csink = a != b; break;
        case 11: csink = a >= b; break;
        default: break;
        }
        all |= bits();
    }
    return all;
}

int main(void)
{
    F qnan = {.u = 0x7fc00000u}, snan = {.u = 0x7f800011u};
    float in0[8] = {1, 0, 1, 3, 1, 1, 1, 1}, in1[8] = {0, 0, 3, 1, 1, 1, 1, 1};
    int lanes = 4, vector = 0, scalar;
#if CASE == 1
    in0[3] = 3.4e38f; in1[3] = 3.4e38f; in0[2] = 1; in1[2] = 1;
    in1[0] = 1; in1[1] = 1; in0[1] = 1;
#elif CASE == 2 || CASE == 3 || CASE == 10 || CASE == 11
    in0[1] = qnan.f; in1[0] = 1; in1[1] = 1;
#elif CASE == 4 || CASE == 9
    in0[1] = snan.f; in1[0] = 1; in1[1] = 1;
#elif CASE == 5
    in0[0] = 1; in1[0] = 1; in0[1] = 4; in1[1] = 2; lanes = 2;
#elif CASE == 6
    for (int k = 0; k < 8; ++k) { in0[k] = 2; in1[k] = 0.5f; }
    in0[7] = 1e-30f; in1[7] = 1e-30f; lanes = 8;
#elif CASE == 7
    in0[2] = qnan.f;
#endif
    for (int k = 0; k < 8; ++k) { vin[k] = in0[k]; win[k] = in1[k]; }
    f4 a = {vin[0], vin[1], vin[2], vin[3]}, b = {win[0], win[1], win[2], win[3]};
    feclearexcept(FE_ALL_EXCEPT);
#if CASE == 0
    vsink = a / b;
#elif CASE == 1
    vsink = a + b;
#elif CASE == 2
    isink = a < b;
#elif CASE == 3 || CASE == 4
    isink = a == b;
#elif CASE == 5
    f2 x = {vin[0], vin[1]}, y = {win[0], win[1]};
    feclearexcept(FE_ALL_EXCEPT);
    v2sink = x / y;
#elif CASE == 6
    f8 x = {vin[0], vin[1], vin[2], vin[3], vin[4], vin[5], vin[6], vin[7]};
    f8 y = {win[0], win[1], win[2], win[3], win[4], win[5], win[6], win[7]};
    feclearexcept(FE_ALL_EXCEPT);
    v8sink = x * y;
#elif CASE == 7
    isink = __builtin_convertvector(a, i4);
#elif CASE == 8
    dvin[0] = 1; dvin[1] = 1; dwin[0] = 0; dwin[1] = 1;
    d2 x = {dvin[0], dvin[1]}, y = {dwin[0], dwin[1]};
    feclearexcept(FE_ALL_EXCEPT);
    dsink = x / y;
#elif CASE == 9
    vsink = -a;
#elif CASE == 10
    isink = a != b;
#elif CASE == 11
    isink = a >= b;
#endif
    vector = bits();
#if CASE == 7 || CASE == 8
    scalar = vector;
#else
    scalar = scalar_union(CASE, lanes);
#endif
    return vector | (vector != scalar ? 64 : 0);
}
