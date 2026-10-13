/* L05 probe: __builtin_convertvector between floating and integer lanes.
 * Each lane must equal the scalar C conversion of the same value.
 * CASE=0 float->int truncation; 1 int->float rounding (RNE);
 * 2 double->float narrowing with NaN payloads; 3 float->double widening
 * with a signaling NaN; 4 unsigned->float; 5 float->unsigned;
 * 6 double<->long both ways. Exit status counts differing lanes.
 * CASE=7 is an observation: out-of-range float->int lanes, which are
 * undefined for scalar C conversions (C11 6.3.1.4p1). It returns
 * 0 when the lanes saturate like FCVTZS, else 1. */
#ifndef CASE
#define CASE 0
#endif
typedef float f4 __attribute__((vector_size(16)));
typedef float f2 __attribute__((vector_size(8)));
typedef double d2 __attribute__((vector_size(16)));
typedef int i4 __attribute__((vector_size(16)));
typedef unsigned u4 __attribute__((vector_size(16)));
typedef long l2 __attribute__((vector_size(16)));
typedef union { float f; unsigned u; } F;
typedef union { double d; unsigned long u; } D;
static volatile int one = 1;

int main(void)
{
    int bad = 0;
#if CASE == 0
    f4 a = {1.5f, -2.5f, 3.99f, -0.0f};
    a *= (float)one;
    i4 r = __builtin_convertvector(a, i4);
    for (int k = 0; k < 4; ++k)
        bad += r[k] != (int)a[k];
    bad += r[0] != 1 || r[1] != -2 || r[2] != 3 || r[3] != 0;
#elif CASE == 1
    i4 a = {16777217, -16777217, 3, 2147483647};
    a *= one;
    f4 r = __builtin_convertvector(a, f4);
    for (int k = 0; k < 4; ++k) {
        F x = {.f = r[k]}, y = {.f = (float)a[k]};
        bad += x.u != y.u;
    }
    F e = {.f = r[3]};
    bad += r[0] != 16777216.0f || r[1] != -16777216.0f || e.u != 0x4f000000u;
#elif CASE == 2
    D in[2] = {{.u = 0x3ff0000010000000ul}, {.u = 0x7ff8000000000123ul}};
    d2 a = {in[0].d, in[1].d};
    f2 r = __builtin_convertvector(a, f2);
    for (int k = 0; k < 2; ++k) {
        F x = {.f = r[k]}, y = {.f = (float)a[k]};
        bad += x.u != y.u;
    }
    F e0 = {.f = r[0]};
    bad += e0.u != 0x3f800000u;
#elif CASE == 3
    F in[2] = {{.u = 0x3f800001u}, {.u = 0x7f800011u}};
    f2 a = {in[0].f, in[1].f};
    d2 r = __builtin_convertvector(a, d2);
    for (int k = 0; k < 2; ++k) {
        D x = {.d = r[k]}, y = {.d = (double)a[k]};
        bad += x.u != y.u;
    }
    D e1 = {.d = r[1]};
    bad += e1.u != 0x7ff8000220000000ul;
#elif CASE == 4
    u4 a = {4294967295u, 16777217u, 0, 2147483649u};
    a *= (unsigned)one;
    f4 r = __builtin_convertvector(a, f4);
    for (int k = 0; k < 4; ++k) {
        F x = {.f = r[k]}, y = {.f = (float)a[k]};
        bad += x.u != y.u;
    }
#elif CASE == 5
    f4 a = {4294967040.0f, 0.75f, 2147483648.0f, 1.0f};
    a *= (float)one;
    u4 r = __builtin_convertvector(a, u4);
    for (int k = 0; k < 4; ++k)
        bad += r[k] != (unsigned)a[k];
#elif CASE == 6
    d2 a = {-9007199254740993.0, 1e18};
    l2 b = {9007199254740993L, -3};
    a *= (double)one;
    b *= one;
    l2 r = __builtin_convertvector(a, l2);
    d2 s = __builtin_convertvector(b, d2);
    for (int k = 0; k < 2; ++k) {
        D x = {.d = s[k]}, y = {.d = (double)b[k]};
        bad += r[k] != (long)a[k] || x.u != y.u;
    }
#elif CASE == 7
    F nan = {.u = 0x7fc00000u};
    f4 a = {nan.f, 3e9f, -3e9f, __builtin_inff()};
    a *= (float)one;
    i4 r = __builtin_convertvector(a, i4);
    bad = !(r[0] == 0 && r[1] == 2147483647 && r[2] == -2147483647 - 1
            && r[3] == 2147483647);
#else
#error unsupported CASE
#endif
    return bad;
}
