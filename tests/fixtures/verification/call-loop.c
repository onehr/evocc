/* The same loop must compose with direct calls on either side of its boundary. */
unsigned call_tick(unsigned x)
{
    return x * 3U + 1U;
}

unsigned loop_call(unsigned x, unsigned n)
{
    while (n) {
        x = call_tick(x);
        --n;
    }
    return x;
}

unsigned call_loop(unsigned x, unsigned n)
{
    return loop_call(x, n) + 1U;
}
