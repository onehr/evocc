#include <stdio.h>

__attribute__((noinline)) _Complex short short_pair(_Complex short x) {
    return x + (2 + 3i);
}
__attribute__((noinline)) _Complex int int_pair(_Complex int x) {
    return x + (2 + 3i);
}
__attribute__((noinline)) _Complex long long_pair(_Complex long x) {
    return x + (2 + 3i);
}
int main(void) {
    _Complex short a = 3 + 4i;
    _Complex int b = 3 + 4i;
    _Complex long c = 3 + 4i;
    a = short_pair(a); b = int_pair(b); c = long_pair(c);
    printf("layout=%zu/%zu,%zu/%zu,%zu/%zu promotion=%zu/%zu\n",
           sizeof(a), _Alignof(__typeof__(a)), sizeof(b), _Alignof(__typeof__(b)),
           sizeof(c), _Alignof(__typeof__(c)), sizeof(+a), sizeof(a + a));
    printf("abi=%d/%d,%d/%d,%ld/%ld\n", __real__ a, __imag__ a,
           __real__ b, __imag__ b, __real__ c, __imag__ c);
    _Complex int d = (9 + 7i) / (2 + 1i);
    _Complex int e = ~(3 + 4i);
    printf("divide=%d/%d conjugate=%d/%d\n", __real__ d, __imag__ d,
           __real__ e, __imag__ e);
    return 0;
}
