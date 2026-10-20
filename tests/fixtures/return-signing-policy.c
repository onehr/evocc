/* Return signing follows the allocated machine function, not a C leaf guess. */
extern unsigned long external_call(unsigned long);
extern void consume(unsigned long *);

__attribute__((noinline)) unsigned long plain_leaf(unsigned long x)
{
    return x ^ 7;
}

__attribute__((noinline)) unsigned long calling(unsigned long x)
{
    return external_call(x) + 1;
}

__attribute__((noinline)) unsigned long storage(unsigned long x)
{
    unsigned long a[2] = {x, 3};
    consume(a);
    return a[0] + a[1];
}

__attribute__((noinline)) unsigned long opaque(unsigned long x)
{
    __asm__ volatile("nop");
    return x;
}

__attribute__((noinline)) unsigned long stack_argument(
    unsigned long a, unsigned long b, unsigned long c, unsigned long d,
    unsigned long e, unsigned long f, unsigned long g, unsigned long h,
    unsigned long i)
{
    return a + i;
}

__attribute__((noinline)) void *frame_address(void)
{
    return __builtin_frame_address(0);
}
