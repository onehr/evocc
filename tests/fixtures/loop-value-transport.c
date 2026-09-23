/* Stable values must survive loop edges without confusing them with updates. */
static volatile unsigned visits;

__attribute__((noinline))
static unsigned fold(int n, int m, const unsigned *row, unsigned bias)
{
    unsigned sum = 0;
    for (int i = 0; i < n; ++i)
        for (int j = 0; j < m; ++j) {
            sum += row[i * m + j] + bias;
            ++visits;
        }
    return sum;
}

__attribute__((noinline))
static unsigned exchanged(unsigned a, unsigned b, unsigned n)
{
    unsigned result = 0;
    for (unsigned i = 0; i < n; ++i) {
        result += a;
        unsigned old = a;
        a = b;
        b = old;
    }
    return result;
}

__attribute__((noinline))
static unsigned selected(unsigned a, unsigned b, unsigned n)
{
    unsigned result = 0;
    for (unsigned i = 0; i < n; ++i)
        result += (i & 1) ? a : b;
    return result;
}

int main(void)
{
    const unsigned values[] = {1, 2, 3, 4, 5, 6};
    if (fold(2, 3, values, 7) != 63 || visits != 6)
        return 1;
    /* Zero-trip loops must not acquire an object access. */
    if (fold(0, 3, (const unsigned *)0, 7) != 0 ||
        fold(2, 0, (const unsigned *)0, 7) != 0 || visits != 6)
        return 2;
    if (exchanged(2, 9, 5) != 24 || selected(2, 9, 5) != 31)
        return 3;
    return 0;
}
