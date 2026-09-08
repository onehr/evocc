_Static_assert(__builtin_clrsb(0) == 31, "zero int");
_Static_assert(__builtin_clrsb(-1) == 31, "negative one int");
_Static_assert(__builtin_clrsbl(0L) == 63, "zero long");
_Static_assert(__builtin_clrsbll(-1LL) == 63, "negative one long long");
_Static_assert(__builtin_parity(0xffffffffU) == 0, "even parity");
_Static_assert(__builtin_parityl(1UL << 63) == 1, "top bit long");
_Static_assert(__builtin_parityll(0x8000000000000001ULL) == 0, "two bits");
_Static_assert(sizeof(__builtin_clrsbl(0)) == 4, "count is int");
_Static_assert(sizeof(__builtin_parityll(0)) == 4, "parity is int");

static unsigned long samples[] = {
    0, 1, 2, 3, 0x7fffffffUL, 0x80000000UL, 0xffffffffUL,
    0x100000000UL, 0x7fffffffffffffffUL, 0x8000000000000000UL,
    0xffffffffffffffffUL, 0xfffffffffffffffeUL, 0xa5a5a5a55a5a5a5aUL
};
static int calls;
static unsigned long next(unsigned long x) { calls++; return x; }

static int redundant(unsigned long x, unsigned width) {
    unsigned long mask = width == 32 ? 0xffffffffUL : ~0UL;
    x &= mask;
    if ((x >> (width - 1)) != 0) x = (~x) & mask;
    if (!x) return width - 1;
    int top = 0;
    if (x >> 32) { x >>= 32; top += 32; }
    if (x >> 16) { x >>= 16; top += 16; }
    if (x >> 8) { x >>= 8; top += 8; }
    if (x >> 4) { x >>= 4; top += 4; }
    if (x >> 2) { x >>= 2; top += 2; }
    if (x >> 1) top++;
    return width - 2 - top;
}
static int parity(unsigned long x) {
    x ^= x >> 32;
    x ^= x >> 16;
    x ^= x >> 8;
    x ^= x >> 4;
    x ^= x >> 2;
    x ^= x >> 1;
    return x & 1;
}
int main(void) {
    for (unsigned i = 0; i < sizeof(samples) / sizeof(samples[0]); ++i) {
        unsigned long x = samples[i];
        if (__builtin_clrsb(next(x)) != redundant(x, 32)) return 1;
        if (__builtin_clrsbl(next(x)) != redundant(x, 64)) return 2;
        if (__builtin_clrsbll(next(x)) != redundant(x, 64)) return 3;
        if (__builtin_parity(next(x)) != parity((unsigned)x)) return 4;
        if (__builtin_parityl(next(x)) != parity(x)) return 5;
        if (__builtin_parityll(next(x)) != parity(x)) return 6;
        if (calls != (int)(6 * (i + 1))) return 7;
    }
    for (unsigned x = 0; x != 256; ++x) {
        if (__builtin_clrsb(x) != redundant(x, 32)) return 8;
        if (__builtin_clrsb(~x) != redundant(~x, 32)) return 9;
        if (__builtin_parity(x) != parity(x)) return 10;
    }
    return 0;
}
