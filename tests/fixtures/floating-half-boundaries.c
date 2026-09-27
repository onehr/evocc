/* L03: distinguish storage/conversion, arithmetic promotion and call ABI.
 * KIND=0 (_Float16), 1 (__fp16), 2 (__bf16); CASE=0 or 1.
 * A reference refusal is a compatibility boundary, not an EvoCC PASS. */
#include <stdio.h>
#include <string.h>
#ifndef KIND
#define KIND 0
#endif
#ifndef CASE
#define CASE 0
#endif
#if KIND == 0
typedef _Float16 half;
#elif KIND == 1
typedef __fp16 half;
#elif KIND == 2
typedef __bf16 half;
#else
#error unsupported KIND
#endif
#if CASE == 1
__attribute__((noinline)) half step(half value) { return value + (half)0.5f; }
#endif
int main(void)
{
    volatile half input = (half)1.5f;
    half input_copy = input;
    unsigned char input_bytes[sizeof input_copy];
    memcpy(input_bytes, &input_copy, sizeof input_copy);
#if CASE == 0
    half result = input + (half)0.5f;
#else
    half result = step(input);
#endif
    unsigned char bytes[sizeof result];
    memcpy(bytes, &result, sizeof result);
    printf("size=%zu align=%zu unary=%zu bytes=", sizeof(half), _Alignof(half),
           sizeof(+input));
    for (unsigned i = sizeof result; i; --i) printf("%02x", bytes[i - 1]);
    printf(" input=");
    for (unsigned i = sizeof input_copy; i; --i) printf("%02x", input_bytes[i - 1]);
    printf("\n");
    return (double)result != 2.0;
}
