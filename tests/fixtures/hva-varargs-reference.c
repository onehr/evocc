/* B04: reduce integer-vector-varargs-worker.c without changing its ABI.
 * Compile HVA_DRIVER separately so a self-control still crosses a real call.
 * These are two 8-byte vectors, not the unresolved sub-8-byte vector ABI. */
typedef short lanes __attribute__((vector_size(8)));
struct pair { lanes a, b; };

long read_pairs(int count, ...);
long read_stacked(lanes, lanes, lanes, lanes, lanes, lanes, lanes, lanes, int, ...);

#ifndef HVA_DRIVER
long read_pairs(int count, ...)
{
    __builtin_va_list ap;
    __builtin_va_start(ap, count);
    long sum = 0;
    for (int i = 0; i < count; ++i) {
        struct pair p = __builtin_va_arg(ap, struct pair);
        sum += p.a[0] + p.b[3];
    }
    __builtin_va_end(ap);
    return sum;
}

long read_stacked(lanes a, lanes b, lanes c, lanes d, lanes e, lanes f, lanes g, lanes h,
                  int count, ...)
{
    __builtin_va_list ap;
    __builtin_va_start(ap, count);
    struct pair p = __builtin_va_arg(ap, struct pair);
    __builtin_va_end(ap);
    return p.a[0] + p.b[3] + a[0] + b[0] + c[0] + d[0] + e[0] + f[0] + g[0] + h[0];
}
#else
#ifdef HVA_TRACE
extern int printf(const char *, ...);
#endif
int main(void)
{
    struct pair p = {{2, 3, 4, 5}, {6, 7, 8, 9}};
    lanes z = {0, 0, 0, 0};
    long one = read_pairs(1, p);
    long four = read_pairs(4, p, p, p, p);
    long five = read_pairs(5, p, p, p, p, p);
    long stacked = read_stacked(z, z, z, z, z, z, z, z, 1, p);
#ifdef HVA_TRACE
    printf("HVA one=%ld four=%ld five=%ld stacked=%ld\n", one, four, five, stacked);
#endif
    return one != 11 || four != 44 || five != 55 || stacked != 11;
}
#endif
