unsigned loop_join(unsigned x, unsigned n)
{
    unsigned total = 0;
    for (unsigned i = 0; i < n; ++i) {
        if ((i & 3U) == 0U)
            continue;
        if (i == 17U)
            break;
        total += (x & 1U) ? i : (i ^ x);
        x = (x >> 1) | (x << 31);
    }
    while (n > 20U)
        n -= 7U;
    do {
        total += n;
        n >>= 1;
    } while (n != 0U);
    return total;
}

int short_circuit(int x)
{
    int y = 0;
    if (x && (y = 3))
        y += 1;
    if (!x || (y = 9))
        y += 2;
    return y;
}
