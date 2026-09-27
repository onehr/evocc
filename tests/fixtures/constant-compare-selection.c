/* Constants in scalar branch/predicate hot paths; dynamic reference controls. */
unsigned signed_right(long x);
unsigned signed_left(long x);
unsigned unsigned_right(unsigned long x);
unsigned unsigned_left(unsigned long x);
unsigned word_right(unsigned x);
unsigned maximum_bound(unsigned long x);

#ifndef DRIVER_ONLY
unsigned signed_right(long x) { return (x < 19L) | ((x <= 19L) << 1) | ((x == 19L) << 2); }
unsigned signed_left(long x) { return (19L < x) | ((19L <= x) << 1) | ((19L != x) << 2); }
unsigned unsigned_right(unsigned long x) { return (x < 19UL) | ((x <= 19UL) << 1); }
unsigned unsigned_left(unsigned long x) { return (19UL < x) | ((19UL <= x) << 1); }
unsigned word_right(unsigned x) { return (x < 19U) | ((x <= 19U) << 1); }
unsigned maximum_bound(unsigned long x) { return x <= 4095UL; }
#endif

#ifndef FUNCTIONS_ONLY
static int calls;
static long next(void) { ++calls; return -1; }
int main(void)
{
    volatile long values[] = {0, 1, 18, 19, 20, 4095, 4096, -1,
                              0x7fffffffffffffffL, -0x7fffffffffffffffL-1};
    volatile long sb = 19;
    volatile unsigned long ub = 19, upper = 4095;
    volatile unsigned wb = 19;
    for (unsigned i = 0; i < sizeof values / sizeof *values; ++i) {
        long s = values[i];
        unsigned long u = (unsigned long)s;
        unsigned w = (unsigned)s;
        if (signed_right(s) != (unsigned)((s < sb) | ((s <= sb) << 1) | ((s == sb) << 2))) return 1;
        if (signed_left(s) != (unsigned)((sb < s) | ((sb <= s) << 1) | ((sb != s) << 2))) return 2;
        if (unsigned_right(u) != (unsigned)((u < ub) | ((u <= ub) << 1))) return 3;
        if (unsigned_left(u) != (unsigned)((ub < u) | ((ub <= u) << 1))) return 4;
        if (word_right(w) != (unsigned)((w < wb) | ((w <= wb) << 1))) return 5;
        if (maximum_bound(u) != (u <= upper)) return 6;
    }
    if (signed_right(next()) != 3 || calls != 1) return 7;
    if (signed_left(next()) != 4 || calls != 2) return 8;
    return 0;
}
#endif
