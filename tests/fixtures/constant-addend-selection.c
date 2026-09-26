/* Small increments/decrements from the KVM address and loop hot paths. */
unsigned long advance(unsigned long x);
unsigned long retreat(unsigned long x);
unsigned int advance32(unsigned int x);
unsigned int retreat32(unsigned int x);
unsigned long left_constant(unsigned long x);
unsigned long constant_minus(unsigned long x);
unsigned long dynamic_add(unsigned long x, unsigned long n);

#ifndef DRIVER_ONLY
unsigned long advance(unsigned long x) { return x + 4095UL; }
unsigned long retreat(unsigned long x) { return x - 19UL; }
unsigned int advance32(unsigned int x) { return x + 4095U; }
unsigned int retreat32(unsigned int x) { return x - 19U; }
unsigned long left_constant(unsigned long x) { return 31UL + x; }
unsigned long constant_minus(unsigned long x) { return 31UL - x; }
unsigned long dynamic_add(unsigned long x, unsigned long n) { return x + n; }
#endif

#ifndef FUNCTIONS_ONLY
int main(void) {
    volatile unsigned long values[] = {0UL, 1UL, ~0UL, 0x8000000000000000UL,
                                      0x123456789abcdef0UL};
    const unsigned long expected[] = {4095UL, 4096UL, 4094UL,
        0x8000000000000fffUL, 0x123456789abceeefUL};
    for (unsigned i = 0; i < 5; ++i) {
        unsigned long x = values[i];
        if (advance(x) != expected[i] || dynamic_add(x, 4095UL) != expected[i]) return 1;
        if (retreat(x) + 19UL != x || left_constant(x) - 31UL != x) return 2;
        if (constant_minus(x) + x != 31UL) return 3;
        if (advance32((unsigned int)x) != (unsigned int)expected[i]) return 4;
        if (retreat32((unsigned int)x) + 19U != (unsigned int)x) return 5;
    }
    return 0;
}
#endif
