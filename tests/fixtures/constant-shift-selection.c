/* Same input widths and count rules as the KVM page-index helpers. */
unsigned long right(unsigned long x);
unsigned long left(unsigned long x);
long arithmetic(long x);
unsigned int narrow(unsigned int x);
unsigned long dynamic(unsigned long x, unsigned long n);

#ifndef DRIVER_ONLY
unsigned long right(unsigned long x) { return x >> 12; }
unsigned long left(unsigned long x) { return x << 13; }
long arithmetic(long x) { return x >> 7; }
unsigned int narrow(unsigned int x) { return x >> 31; }
unsigned long dynamic(unsigned long x, unsigned long n) { return x >> n; }
#endif

#ifndef FUNCTIONS_ONLY
int main(void) {
    volatile unsigned long values[] = {0UL, 1UL, ~0UL, 0x8000000000000000UL,
                                      0x123456789abcdef0UL};
    const unsigned long answers[] = {0UL, 0UL, 0xfffffffffffffUL,
                                    0x8000000000000UL, 0x123456789abcdUL};
    for (unsigned i = 0; i < 5; ++i) {
        unsigned long x = values[i];
        if (right(x) != answers[i] || dynamic(x, 12) != answers[i]) return 1;
        if (left(x) != (x * 8192UL)) return 2;
    }
    if (arithmetic(-1) != -1 || arithmetic(-129) != -2 || arithmetic(256) != 2) return 3;
    if (narrow(0xffffffffU) != 1 || narrow(0x7fffffffU) != 0) return 4;
    return 0;
}
#endif
