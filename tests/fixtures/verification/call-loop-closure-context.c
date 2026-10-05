/* Remaining constructor combinations in the existing nonrecursive core profile. */
#include "call-loop.c"

unsigned initialized_loop_call(unsigned x, unsigned n)
{
    unsigned y = loop_call(x, n);
    return y;
}

int and_loop_call(unsigned x, unsigned n)
{
    return loop_call(x, n) && x;
}

int or_loop_call(unsigned x, unsigned n)
{
    return loop_call(x, n) || n;
}

unsigned branch_loop_call(unsigned x, unsigned n)
{
    if (x)
        return loop_call(x, n);
    return n;
}
