/* Full-width masks from address/flag manipulation, plus register controls. */
unsigned long page_base(unsigned long x);
unsigned long high_flag(unsigned long x);
unsigned long flip_nibbles(unsigned long x);
unsigned int narrow_mask(unsigned int x);
unsigned int left_mask(unsigned int x);
unsigned long nonencoding(unsigned long x);
unsigned long dynamic_mask(unsigned long x, unsigned long mask);

#ifndef DRIVER_ONLY
unsigned long page_base(unsigned long x) { return x & ~4095UL; }
unsigned long high_flag(unsigned long x) { return x | 0x8000000000000001UL; }
unsigned long flip_nibbles(unsigned long x) { return x ^ 0x0f0f0f0f0f0f0f0fUL; }
unsigned int narrow_mask(unsigned int x) { return x & 0x55555555U; }
unsigned int left_mask(unsigned int x) { return 0x80000001U ^ x; }
unsigned long nonencoding(unsigned long x) { return x & 0x123456789abcdef0UL; }
unsigned long dynamic_mask(unsigned long x, unsigned long mask) { return x & mask; }
#endif

#ifndef FUNCTIONS_ONLY
static int calls;
static unsigned long next(void) { ++calls; return 0x123456789abcdef0UL; }
int main(void)
{
    volatile unsigned long values[] = {0UL, 1UL, ~0UL, 0x8000000000000000UL,
                                       0x123456789abcdef0UL};
    for (unsigned i = 0; i < 5; ++i) {
        unsigned long x = values[i];
        if (page_base(x) != x / 4096UL * 4096UL) return 1;
        if (high_flag(x) != (x | 0x8000000000000001UL)) return 2;
        if ((flip_nibbles(x) ^ x) != 0x0f0f0f0f0f0f0f0fUL) return 3;
        if (narrow_mask((unsigned int)x) != ((unsigned int)x & 0x55555555U)) return 4;
        if (left_mask((unsigned int)x) != ((unsigned int)x ^ 0x80000001U)) return 5;
        if (nonencoding(x) != dynamic_mask(x, 0x123456789abcdef0UL)) return 6;
    }
    unsigned long result = page_base(next());
    if (calls != 1 || result != 0x123456789abcd000UL) return 7;
    result = dynamic_mask(next(), next());
    if (calls != 3 || result != 0x123456789abcdef0UL) return 8;
    return 0;
}
#endif
