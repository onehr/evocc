/* Suspended caller values and continuations in the existing core-v1 profile. */
#include "call-loop.c"

unsigned right_call(unsigned x, unsigned n)
{
    return x + loop_call(x, n);
}

unsigned conditional_call(unsigned x, unsigned n)
{
    return loop_call(x, n) ? x : n;
}

unsigned assigned_call(unsigned x, unsigned n)
{
    x = loop_call(x, n);
    return x;
}

unsigned nested_call(unsigned x, unsigned n)
{
    return loop_call(loop_call(x, n), 1U);
}
