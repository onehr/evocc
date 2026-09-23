/* Reuse the value already computed by a checked add/sub, not its guard. */
int add32(int a, int b) { return a + b; }
long sub64(long a, long b) { return a - b; }
long twice(long a, long b) { return (a + b) - b; }
int read_once(volatile int *p, volatile int *q) { return *p + *q; }

int main(void)
{
    volatile int lo = -2147483647 - 1, hi = 2147483647;
    volatile long big = 9223372036854775807L, small = -9223372036854775807L - 1;
    if (add32(hi, -1) != 2147483646 || add32(lo, 1) != -2147483647) return 1;
    if (sub64(small, -1) != -9223372036854775807L || sub64(big, 1) != big - 1) return 2;
    if (twice(17, -29) != 17 || twice(small, big) != small) return 3;
    if (read_once(&lo, &hi) != -1) return 4;
    long sum = 0;
    for (int i = -9; i < 10; ++i) sum += twice(i, 37);
    return sum != 0;
}
