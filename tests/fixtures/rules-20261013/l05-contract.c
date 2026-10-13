/* L05 probe: is a*b+c contracted into one fused operation?
 * a = b = 1+2^-12 and c = -(1+2^-11) in binary32. The unfused product
 * rounds to 1+2^-11 (ties to even), so a*b+c is +0. A fused multiply-add
 * keeps the 2^-24 term. CASE=0 scalar float, CASE=1 float x4 vector,
 * CASE=2 double x2 vector with a = 1+2^-27, c = -(1+2^-26).
 * Exit status: 0 unfused, 1 fused, 2 any other value. */
#ifndef CASE
#define CASE 0
#endif
typedef float f4 __attribute__((vector_size(16)));
typedef double d2 __attribute__((vector_size(16)));
typedef union { float f; unsigned u; } F;
typedef union { double d; unsigned long u; } D;

int main(void)
{
#if CASE == 0
    volatile F a = {.u = 0x3f800800u}, c = {.u = 0xbf801000u};
    float x = a.f, z = c.f;
    float r = x * x + z;
    F out = {.f = r};
    return out.u == 0 ? 0 : out.u == 0x33800000u ? 1 : 2;
#elif CASE == 1
    volatile F a = {.u = 0x3f800800u}, c = {.u = 0xbf801000u};
    f4 x = {a.f, a.f, a.f, a.f}, z = {c.f, c.f, c.f, c.f};
    f4 r = x * x + z;
    F out = {.f = r[2]};
    return out.u == 0 ? 0 : out.u == 0x33800000u ? 1 : 2;
#elif CASE == 2
    volatile D a = {.u = 0x3ff0000002000000ul}, c = {.u = 0xbff0000004000000ul};
    d2 x = {a.d, a.d}, z = {c.d, c.d};
    d2 r = x * x + z;
    D out = {.d = r[1]};
    return out.u == 0 ? 0 : out.u == 0x3c90000000000000ul ? 1 : 2;
#else
#error unsupported CASE
#endif
}
