#include "call-loop.c"

int call_constant_tail(unsigned x, unsigned n)
{
    return loop_call(x, n) != 0 ||
           (unsigned long)-1 != 0xffffffffffffffffUL ||
           (long)0xffffffffU != 4294967295L ||
           -7 / 3 != -2 || -7 % 3 != -1 ||
           (unsigned)(2147483647 + 1) != 0x80000000U;
}
