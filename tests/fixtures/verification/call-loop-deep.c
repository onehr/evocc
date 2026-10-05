/* Preserve the full nonrecursive closure, including both cyclic callees. */
#include "call-loop-nested.c"

unsigned call_nested_loop(unsigned x, unsigned n, unsigned m)
{
    return loop_calls_loop(x, n, m) + 1U;
}
