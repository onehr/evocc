/* Full-width shift counts are a no-op control for extension selection. */
unsigned long repeated_count(unsigned long x, unsigned long y, unsigned n) {
    n &= 63;
    return (x << n) ^ (y >> n);
}

unsigned long repeated_count_three(unsigned long x, unsigned long y,
                                  unsigned long z, unsigned n) {
    n &= 63;
    return (x << n) ^ (y >> n) ^ (z << n);
}

unsigned long distinct_counts(unsigned long x, unsigned long y,
                              unsigned n, unsigned m) {
    return (x << (n & 63)) ^ (y >> (m & 63));
}

extern int consume_signed(signed char, signed char, signed char);
extern unsigned consume_unsigned(unsigned short, unsigned short);

int repeated_signed_abi(signed char x) {
    return consume_signed(x, x, x);
}

unsigned repeated_unsigned_abi(unsigned short x) {
    return consume_unsigned(x, x);
}
