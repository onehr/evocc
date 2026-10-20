/* Exercise authenticated returns, including activation-sensitive callees. */
__attribute__((noinline)) unsigned long leaf(unsigned long x)
{
    return x ^ 7;
}

__attribute__((noinline)) unsigned long multiple_returns(unsigned long x)
{
    if (x == 0) return 9;
    if (x == 1) return 17;
    return x + 3;
}

__attribute__((noinline)) unsigned long ninth_argument(
    unsigned long a, unsigned long b, unsigned long c, unsigned long d,
    unsigned long e, unsigned long f, unsigned long g, unsigned long h,
    unsigned long i)
{
    return a + b + c + d + e + f + g + h + i;
}

__attribute__((noinline)) unsigned long variadic(int count, ...)
{
    __builtin_va_list args;
    __builtin_va_start(args, count);
    unsigned long total = 0;
    for (int i = 0; i < count; ++i)
        total += __builtin_va_arg(args, unsigned long);
    __builtin_va_end(args);
    return total;
}

__attribute__((noinline)) unsigned long dynamic_storage(unsigned long n)
{
    volatile unsigned char bytes[n];
    bytes[0] = 3;
    bytes[n - 1] = 5;
    return bytes[0] + bytes[n - 1];
}

__attribute__((noinline)) int activation(void)
{
    return __builtin_frame_address(0) != (void *)0
        && __builtin_return_address(0) != (void *)0;
}

__attribute__((noinline)) unsigned long caller(unsigned long x)
{
    return leaf(x) + multiple_returns(x) + (unsigned long)activation();
}

int main(void)
{
    for (unsigned long i = 0; i < 1000; ++i) {
        unsigned long expected = i == 0 ? 9 : i == 1 ? 17 : i + 3;
        if (caller(i) != (i ^ 7) + expected + 1) return 1;
    }
    if (ninth_argument(1,2,3,4,5,6,7,8,9) != 45) return 2;
    if (variadic(3, 5UL, 7UL, 11UL) != 23) return 3;
    if (dynamic_storage(17) != 8) return 4;
    return 0;
}
