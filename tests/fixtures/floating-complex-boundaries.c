/* L04 batch probes. KIND=32/64/128, CASE=0..4.
 * CASE 0/1 have exact finite assertions. CASE 2..4 record special-value
 * bits/status for adjudication; exit zero is NOT numerical acceptance.
 * Capture status before printing. No fast-math or implicit contraction. */
#include <fenv.h>
#include <stdio.h>
#include <string.h>
#ifndef KIND
#define KIND 64
#endif
#ifndef CASE
#define CASE 0
#endif
#if KIND == 32
#define REAL float
#define LARGE 0x1p127f
#elif KIND == 64
#define REAL double
#define LARGE 0x1p1023
#elif KIND == 128
#define REAL long double
#define LARGE 0x1p16383L
#else
#error unsupported KIND
#endif
typedef REAL _Complex pair;

#if CASE == 0
__attribute__((noinline)) pair transport(pair value) { return value; }
#endif

static void dump(REAL value)
{
    unsigned char bytes[sizeof value];
    memcpy(bytes, &value, sizeof value);
    for (unsigned i = sizeof value; i; --i) printf("%02x", bytes[i - 1]);
}

int main(void)
{
    volatile REAL one = 1, zero = 0;
    pair result;
    if (feclearexcept(FE_ALL_EXCEPT)) return 100;
#if CASE == 0
    pair input;
    __real__ input = 3;
    __imag__ input = -4;
    result = transport(input);
#elif CASE == 1
    volatile pair a, b;
    __real__ a = 9; __imag__ a = 7;
    __real__ b = 2; __imag__ b = 1;
    result = a / b;
#elif CASE == 2
    /* Direct construction must not introduce 0 * infinity arithmetic. */
    result = __builtin_complex((REAL)-0.0, (REAL)__builtin_inf());
#elif CASE == 3
    volatile pair a;
    __real__ a = LARGE; __imag__ a = LARGE;
    volatile pair b = a;
    result = a / b;
#elif CASE == 4
    volatile pair a, b;
    __real__ a = one; __imag__ a = one;
    __real__ b = zero; __imag__ b = zero;
    result = a / b;
#else
#error unsupported CASE
#endif
    int flags = fetestexcept(FE_ALL_EXCEPT);
    printf("real="); dump(__real__ result);
    printf(" imag="); dump(__imag__ result);
    printf(" flags=%x size=%zu align=%zu\n", flags, sizeof(pair), _Alignof(pair));
#if CASE == 0
    return __real__ result != 3 || __imag__ result != -4;
#elif CASE == 1
    return __real__ result != 5 || __imag__ result != 1;
#else
    return 0;
#endif
}
