unsigned long shifted_add(unsigned long x, unsigned long y) { return x + (y << 3); }
unsigned shifted_xor(unsigned x, unsigned y) { return x ^ (y >> 7); }
unsigned long multiply_add(unsigned long x, unsigned long y, unsigned long z) { return x + y * z; }
unsigned long multiply_sub(unsigned long x, unsigned long y, unsigned long z) { return x - y * z; }
long extended_add(long x, signed char y) { return x + y; }
int extended_compare(long x, signed char y) { return x < y; }
unsigned long arithmetic_shift(unsigned long x, long y) { return x - (unsigned long)(y >> 5); }
unsigned multiply_word(unsigned x, unsigned y, unsigned z) { return x + y * z; }
int shifted_compare(unsigned long x, unsigned long y) { return x < (y >> 3); }
unsigned long indexed_load(const unsigned long *p, unsigned long i) { return p[i]; }
void indexed_store(unsigned long *p, unsigned long i, unsigned long value) { p[i] = value; }
unsigned long shared_shift(unsigned long x, unsigned long y) {
    unsigned long shifted = y << 3;
    return (x + shifted) ^ shifted;
}
unsigned long volatile_index(const volatile unsigned long *p, unsigned long i) { return p[i]; }
