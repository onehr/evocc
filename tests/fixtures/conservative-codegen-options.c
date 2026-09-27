/* Disable-only optimization controls must preserve the existing native model. */
__attribute__((noinline)) int plain_leaf(int x) { return x + 1; }

__attribute__((noinline, aligned(64))) int source_aligned(int x)
{
    return plain_leaf(x) + 3;
}

__attribute__((noinline)) void zero_values(int *values, unsigned n)
{
    for (unsigned i = 0; i < n; ++i) values[i] = 0;
}

int main(void)
{
    int values[16];
    for (unsigned i = 0; i < 16; ++i) values[i] = (int)i + 1;
    zero_values(values, 16);
    for (unsigned i = 0; i < 16; ++i)
        if (values[i]) return 1;
    return source_aligned(5) != 9;
}
