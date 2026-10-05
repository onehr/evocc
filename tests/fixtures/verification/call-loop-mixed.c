/* Finite and cyclic calls share one checked, nonrecursive closure. */
#include "call-loop.c"

unsigned finite_before_loop(unsigned x, unsigned n)
{
    return call_tick(x) + loop_call(x, n);
}

unsigned finite_loop_argument(unsigned x, unsigned n)
{
    return loop_call(call_tick(x), n);
}

unsigned loop_then_finite(unsigned x, unsigned n)
{
    return call_tick(loop_call(x, n));
}
