/* Logical edges, effects, loop-carried values and label identity must survive
 * choosing either conditional successor as the machine fall-through. */
#ifndef DRIVER_ONLY
volatile unsigned effects;

__attribute__((noinline)) int choose(int n)
{
    if (n < 0) { effects += 1; return 17; }
    if (!n) { effects += 2; return 23; }
    effects += 4;
    return 41;
}

__attribute__((noinline)) int high_bit(unsigned long n)
{
    if (n & (1UL << 63)) return 7;
    return 9;
}

__attribute__((noinline)) int loop_values(unsigned n)
{
    int a = 1, b = 2;
    for (unsigned i = 0; i < n; ++i) {
        if (i & 1) a += b;
        else b += a;
    }
    return a * 100 + b;
}

__attribute__((noinline)) int addressed(int n)
{
    void *label = &&taken;
    if (n) goto *label;
    return 9;
taken:
    return 7;
}
#else
extern volatile unsigned effects;
int choose(int);
int high_bit(unsigned long);
int loop_values(unsigned);
int addressed(int);
#endif

#ifndef FUNCTIONS_ONLY
int main(void)
{
    static const int expected[] = {17, 17, 17, 23, 41, 41, 41};
    for (int i = 0; i < 7; ++i)
        if (choose(i - 3) != expected[i]) return 1;
    if (effects != 17) return 2;
    if (high_bit(0) != 9 || high_bit(1UL << 63) != 7) return 3;
    if (loop_values(0) != 102 || loop_values(6) != 2918) return 4;
    if (addressed(0) != 9 || addressed(1) != 7) return 5;
    return 0;
}
#endif
