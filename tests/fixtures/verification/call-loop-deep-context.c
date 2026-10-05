/* Reuse the same cyclic closure under saved values, short circuit and loops. */
#include "call-loop-deep.c"

unsigned deep_saved(unsigned x, unsigned n, unsigned m)
{
    return x + call_nested_loop(x, n, m);
}

int deep_short(unsigned x, unsigned n, unsigned m)
{
    return x && call_nested_loop(x, n, m);
}

unsigned deep_outer_loop(unsigned x, unsigned n, unsigned m, unsigned k)
{
    while (k) {
        x = call_nested_loop(x, n, m);
        --k;
    }
    return x;
}
