extern void invalid(void) __attribute__((error("constant query was not proved")));

__attribute__((noinline)) unsigned masks(unsigned value)
{
    unsigned long mask;
    mask = 240;
    value &= ~mask;
    if (!__builtin_constant_p(mask))
        invalid();
    mask = 15;
    value |= mask;
    if (!__builtin_constant_p(mask))
        invalid();
    return value;
}

__attribute__((noinline)) int positive(int selector)
{
    int n = selector ? 16 : 255;
    if (!__builtin_constant_p((long long)n >= 0) || (long long)n < 0)
        invalid();
    return n;
}

int main(void)
{
    return masks(85) != 15 || positive(0) != 255 || positive(1) != 16;
}
