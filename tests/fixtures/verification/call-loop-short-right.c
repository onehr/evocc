/* A pending logical operand must retain the selected branch's premise. */
#include "call-loop.c"

int and_right_loop_call(unsigned x, unsigned n)
{
    return x && loop_call(x, n);
}

int or_right_loop_call(unsigned x, unsigned n)
{
    return x || loop_call(x, n);
}
