/* Derived and modified from zlib 1.3.2.
 * Copyright (C) 1995-2026 Mark Adler.
 * Altered source; see origin.md and ../LICENSE.zlib.
 */
static unsigned x2n_table[32];
static unsigned multmodp(unsigned a, unsigned b) {
    unsigned m, p;
    m = (unsigned)1 << 31;
    p = 0;
    for (;;) {
        if (a & m) {
            p ^= b;
            if ((a & (m - 1)) == 0)
                break;
        }
        m >>= 1;
        b = b & 1 ? (b >> 1) ^ 0xedb88320U : b >> 1;
    }
    return p;
}
static unsigned x2nmodp(unsigned long long n, unsigned k) {
    unsigned p;
    p = (unsigned)1 << 31;
    while (n) {
        if (n & 1)
            p = multmodp(x2n_table[k], p);
        n >>= 1;
        k++;
    }
    return p;
}
static void prepare_powers(void)
{
    unsigned i;
    unsigned p = 1U << 30;
    x2n_table[0] = p;
    for (i = 1; i < 32; ++i) {
        p = multmodp(p, p);
        x2n_table[i] = p;
    }
}
static unsigned crc_byte(unsigned crc, unsigned byte)
{
    unsigned bit;
    crc ^= byte;
    for (bit = 0; bit < 8; ++bit) {
        if (crc & 1U)
            crc = (crc >> 1) ^ 0xedb88320U;
        else
            crc >>= 1;
    }
    return crc;
}
static unsigned crc_bytes(unsigned crc, const unsigned char *data, unsigned count)
{
    unsigned i;
    crc = ~crc;
    for (i = 0; i < count; ++i)
        crc = crc_byte(crc, data[i]);
    return ~crc;
}
static unsigned combine(unsigned left, unsigned right, unsigned long long length)
{
    unsigned op = x2nmodp(length, 3);
    return multmodp(op, left) | right;
}
static unsigned crc_chunks(const unsigned char *data, unsigned count, unsigned chunk)
{
    unsigned position = 0;
    unsigned crc = 0;
    while (position < count) {
        unsigned size = count - position;
        unsigned next;
        if (size > chunk)
            size = chunk;
        next = crc_bytes(0, data + position, size);
        crc = combine(crc, next, size);
        position += size;
    }
    return crc;
}
static unsigned zero_crc(unsigned long long length)
{
    unsigned crc = ~0U;
    unsigned p = x2nmodp(length, 3);
    crc = multmodp(p, crc);
    return ~crc;
}
static unsigned sparse_crc(unsigned first, unsigned last, unsigned long long gap)
{
    unsigned char a[1];
    unsigned char b[1];
    unsigned left;
    unsigned zeros;
    unsigned right;
    a[0] = (unsigned char)first;
    b[0] = (unsigned char)last;
    left = crc_bytes(0, a, 1);
    right = crc_bytes(0, b, 1);
    zeros = zero_crc(gap);
    left = combine(left, zeros, gap);
    return combine(left, right, 1);
}
static unsigned inspect_splits(const unsigned char *data, unsigned count)
{
    unsigned i;
    unsigned digest = 0;
    for (i = 0; i <= count; ++i) {
        unsigned left = crc_bytes(0, data, i);
        unsigned right = crc_bytes(0, data + i, count - i);
        digest = digest * 33U + combine(left, right, count - i);
    }
    return digest;
}
static unsigned crc_reversed(const unsigned char *data, unsigned count)
{
    unsigned crc = 0;
    while (count != 0) {
        --count;
        crc = crc_bytes(crc, data + count, 1);
    }
    return crc;
}
unsigned long long evaluate(unsigned seed)
{
    unsigned char data[80];
    unsigned size = 17 + seed * 3;
    unsigned i;
    unsigned whole;
    unsigned chunked;
    unsigned streamed = 0;
    unsigned sparse;
    unsigned long long gap = seed == 11 ? 1073741825ULL : seed + 3;
    prepare_powers();
    for (i = 0; i < size; ++i)
        data[i] = (unsigned char)(i * 29U + seed * 17U);
    whole = crc_bytes(0, data, size);
    chunked = crc_chunks(data, size, seed % 7 + 1);
    for (i = 0; i < size; ++i)
        streamed = crc_bytes(streamed, data + i, 1);
    sparse = sparse_crc(seed + 7, seed + 19, gap);
    return ((unsigned long long)whole << 32) ^ chunked ^ streamed ^
           sparse ^ inspect_splits(data, size) ^ crc_reversed(data, size);
}
int main(void)
{
    unsigned left;
    unsigned ordinary;
    unsigned sparse;
    prepare_powers();
    left = crc_byte(0, 17);
    ordinary = combine(left, crc_byte(0, 29), 1);
    ordinary ^= combine(left, crc_byte(0, 31), 3);
    sparse = sparse_crc(7, 19, 1073741825ULL);
    return (ordinary ^ sparse) == 0xffffffffU;
}
