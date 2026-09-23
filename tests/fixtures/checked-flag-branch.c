/* A guard may use flags directly; it may not drop checks or observations. */
int sum(int a, int b) { return a + b; }
long difference(long a, long b) { return a - b; }
int product(int a, int b) { return a * b; }
unsigned shift(unsigned a, int count) { return a << count; }
int quotient(int a, int b) { return a / b; }
int observed(volatile int *a, volatile int *b) { return *a + *b; }

int main(void)
{
    volatile int hi = 2147483647, lo = -2147483647 - 1;
    volatile long wide = -9223372036854775807L - 1;
#if defined(TRAP_ADD)
    return sum(hi, 1);
#elif defined(TRAP_SUB)
    return (int)difference(wide, 1);
#elif defined(TRAP_MUL)
    return product(hi, 2);
#elif defined(TRAP_SHIFT)
    return (int)shift(1, 32);
#elif defined(TRAP_DIV)
    return quotient(lo, -1);
#else
    if (sum(hi, -1) != 2147483646 || sum(lo, 1) != -2147483647) return 1;
    if (difference(wide, -1) != wide + 1) return 2;
    if (product(-37, 11) != -407) return 3;
    if (shift(1, 31) != 2147483648U || shift(3, 0) != 3) return 4;
    if (quotient(-13, 3) != -4 || quotient(lo, 1) != lo) return 5;
    if (observed(&hi, &lo) != -1) return 6;
    int total = 0;
    for (int i = -7; i < 8; ++i) total = sum(total, product(i, 3));
    return total != 0;
#endif
}
