/* Neither caller nor callee has a statically bounded iteration count. */
#include "call-loop.c"

unsigned loop_calls_loop(unsigned x, unsigned n, unsigned m)
{
    while (n) {
        x = loop_call(x, m);
        --n;
    }
    return x;
}
