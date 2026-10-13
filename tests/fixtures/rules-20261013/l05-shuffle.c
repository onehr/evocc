/* L05 probe: lane selection on floating vectors is a bit move.
 * CASE=0 __builtin_shufflevector, one and two sources, fewer lanes;
 * CASE=1 GCC __builtin_shuffle with a double vector and a 64-bit mask
 *        taken modulo the lane count (the pr85331 form; Clang rejects it);
 * CASE=2 GCC __builtin_shuffle with two float sources;
 * CASE=3 signaling-NaN bits survive shuffle, element read/write and copy.
 * Exit status 0 means every expected lane matched. */
#ifndef CASE
#define CASE 0
#endif
typedef float f4 __attribute__((vector_size(16)));
typedef float f2 __attribute__((vector_size(8)));
typedef double d2 __attribute__((vector_size(16)));
typedef int i4 __attribute__((vector_size(16)));
typedef long l2 __attribute__((vector_size(16)));
typedef union { float f; unsigned u; } F;
static volatile long big = 10000000001L;

int main(void)
{
#if CASE == 0
    f4 a = {1, -2, 3, -4}, b = {5, 6, 7, 8};
    f4 r = __builtin_shufflevector(a, a, 3, 2, 1, 0);
    f4 s = __builtin_shufflevector(a, b, 0, 4, 1, 5);
    f2 t = __builtin_shufflevector(a, b, 7, 2);
    return r[0] != -4 || r[3] != 1 || s[1] != 5 || s[2] != -2 || t[0] != 8
        || t[1] != 3;
#elif CASE == 1
    d2 y = {1.0, 2.0};
    l2 m = {big, 0};
    d2 r = __builtin_shuffle(y, m);
    return r[0] != 2.0 || r[1] != 1.0;
#elif CASE == 2
    f4 a = {1, 2, 3, 4}, b = {5, 6, 7, 8};
    i4 m = {7, 0, 13, -1};
    f4 r = __builtin_shuffle(a, b, m);
    return r[0] != 8 || r[1] != 1 || r[2] != 6 || r[3] != 8;
#elif CASE == 3
    F s = {.u = 0x7f800011u}, q = {.u = 0xffc00022u};
    f4 a = {s.f, 1.0f, q.f, -0.0f};
    f4 r = __builtin_shufflevector(a, a, 2, 0, 3, 1);
    f4 c = r;
    c[3] = s.f;
    F x0 = {.f = c[0]}, x1 = {.f = c[1]}, x2 = {.f = c[2]}, x3 = {.f = c[3]};
    return x0.u != 0xffc00022u || x1.u != 0x7f800011u || x2.u != 0x80000000u
        || x3.u != 0x7f800011u;
#else
#error unsupported CASE
#endif
}
