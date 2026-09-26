/* Direct-edge identities must not move or weaken an assembly boundary. */
__attribute__((noinline))
unsigned long barrier_sum(unsigned n, unsigned long a, unsigned long b)
{
    unsigned long result = 0;
    for (unsigned i = 0; i < n; ++i) {
        unsigned long value;
        __asm__ volatile("add %0, %1, %2"
                         : "=&r"(value) : "r"(a), "r"(b)
                         : "x9", "cc", "memory");
        result += value;
    }
    return result;
}

__attribute__((noinline))
unsigned long memory_sum(unsigned n, unsigned long *p, unsigned long step)
{
    for (unsigned i = 0; i < n; ++i) {
        unsigned long value;
        __asm__ volatile("ldr %0, %1\n\tadd %0, %0, %2\n\tstr %0, %1"
                         : "=&r"(value), "+m"(*p) : "r"(step) : "memory");
    }
    return n ? *p : 0;
}

/* Two occurrences of one value can require DISTINCT fixed locations. */
__attribute__((noinline))
unsigned long fixed_pair(unsigned n, unsigned long value)
{
    unsigned long result = 0;
    for (unsigned i = 0; i < n; ++i) {
        register unsigned long left __asm__("x19") = value;
        register unsigned long right __asm__("x20") = value;
        unsigned long sum;
        __asm__ volatile("add %0, %1, %2"
                         : "=&r"(sum) : "r"(left), "r"(right));
        result += sum;
    }
    return result;
}

__attribute__((noinline))
unsigned long tied_counter(unsigned n, unsigned long value)
{
    for (unsigned i = 0; i < n; ++i)
        __asm__ volatile("add %0, %0, #1" : "+r"(value));
    return value;
}

int main(void)
{
    unsigned long values[] = {0, 1, 0x123456789abcdef0UL, ~0UL};
    if (memory_sum(0, (unsigned long *)0, 7) != 0)
        return 1;
    for (unsigned n = 0; n < 18; ++n) {
        for (unsigned i = 0; i < 4; ++i) {
            unsigned long a = values[i], b = values[(i + 1) % 4];
            unsigned long stored = a;
            if (barrier_sum(n, a, b) != n * (a + b))
                return 2;
            if (memory_sum(n, &stored, b) != (n ? a + n * b : 0))
                return 3;
            if (stored != a + n * b)
                return 4;
            if (fixed_pair(n, a) != n * (a + a))
                return 5;
            if (tied_counter(n, a) != a + n)
                return 6;
        }
    }
    return 0;
}
