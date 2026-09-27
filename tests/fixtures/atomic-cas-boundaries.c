/* L11b/c: failure writeback and pointer-bearing values. CASE=0..2.
 * Generic record padding starts initialized; this is not an authorization
 * to compare uninitialized padding or discard pointer provenance. */
#include <string.h>
#ifndef CASE
#define CASE 0
#endif
static int first, second;
int main(void)
{
#if CASE == 0
    unsigned value = 3, expected = 4;
    if (__atomic_compare_exchange_n(&value, &expected, 7, 0, 5, 2)) return 1;
    if (expected != 3 || value != 3) return 2;
    if (!__atomic_compare_exchange_n(&value, &expected, 7, 0, 5, 2)) return 3;
    return value != 7 || expected != 3;
#elif CASE == 1
    int *value = &first, *expected = &second;
    if (__atomic_compare_exchange_n(&value, &expected, &second, 0, 5, 2)) return 1;
    if (expected != &first || value != &first) return 2;
    if (!__atomic_compare_exchange_n(&value, &expected, &second, 0, 5, 2)) return 3;
    return value != &second;
#elif CASE == 2
    struct __attribute__((aligned(16))) record { unsigned count; int *pointer; };
    struct record value, expected, desired;
    memset(&value, 0, sizeof value);
    memset(&expected, 0, sizeof expected);
    memset(&desired, 0, sizeof desired);
    value.count = 3; value.pointer = &first;
    expected.count = 4; expected.pointer = &second;
    desired.count = 7; desired.pointer = &second;
    if (__atomic_compare_exchange(&value, &expected, &desired, 0, 5, 2)) return 1;
    if (expected.count != 3 || expected.pointer != &first) return 2;
    if (!__atomic_compare_exchange(&value, &expected, &desired, 0, 5, 2)) return 3;
    return value.count != 7 || value.pointer != &second;
#else
#error unsupported CASE
#endif
}
