unsigned long nine(unsigned long a, unsigned long b, unsigned long c,
                   unsigned long d, unsigned long e, unsigned long f,
                   unsigned long g, unsigned long h, unsigned long i)
{
    return a + b + c + d + e + f + g + h + i;
}

long packed(long a, long b, long c, long d, long e, long f, long g, long h,
            signed char i, short j, int k, ...)
{
    __builtin_va_list ap;
    __builtin_va_start(ap, k);
    long l = __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return a + b + c + d + e + f + g + h + i + j + k + l;
}
