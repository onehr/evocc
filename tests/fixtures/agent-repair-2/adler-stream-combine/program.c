/* Altered zlib source; license retained in ../LICENSE.zlib. */
/* Null data means checksum initialization (value 1) for every length.
 * Negative second-stream combination lengths return 0xffffffffU.
 * Only evaluate seeds 0..11 are supported.
 */
static unsigned adler_update(unsigned adler, const unsigned char *buf,
                              unsigned length)
{
    unsigned sum2 = (adler >> 16) & 65535u;
    unsigned n = 0;
    adler &= 65535u;
    if (buf == 0 && length != 1)
    {
        return 1;
    }
    if (length == 1)
    {
        adler += buf[0];
        if (adler >= 65521u)
        {
            adler -= 65521u;
        }
        sum2 += adler;
        if (sum2 >= 65521u)
        {
            sum2 -= 65521u;
        }
        return adler | (sum2 << 16);
    }
    if (length < 16)
    {
        while (length > 0)
        {
            adler += *buf++;
            sum2 += adler;
            length--;
        }
        if (adler >= 65521u)
        {
            adler -= 65521u;
        }
        sum2 %= 65521u;
        return adler | (sum2 << 16);
    }
    while (length >= 5552u)
    {
        length -= 5552u;
        n = 5552u / 16u;
        do
        {
            unsigned i = 0;
            for (i = 0; i < 16; i++)
            {
                adler += buf[i];
                sum2 += adler;
            }
            buf += 16;
            n--;
        } while (n > 0);
        adler %= 65521u;
        sum2 %= 65521u;
    }
    if (length != 0)
    {
        while (length >= 16)
        {
            unsigned i = 0;
            length -= 16;
            for (i = 0; i < 16; i++)
            {
                adler += buf[i];
                sum2 += adler;
            }
            buf += 16;
        }
        while (length > 0)
        {
            adler += *buf++;
            sum2 += adler;
            length--;
        }
        adler %= 65521u;
        sum2 %= 65521u;
    }
    return adler | (sum2 << 16);
}
static unsigned adler_combine(unsigned first, unsigned second, long long len2)
{
    unsigned sum1 = 0;
    unsigned sum2 = 0;
    unsigned rem = 0;
    if (len2 < 0)
    {
        return 0xffffffffu;
    }
    len2 %= 65521;
    rem = (unsigned)len2;
    sum1 = first & 65535u;
    sum2 = rem * sum1;
    sum2 %= 65521u;
    sum1 += (second & 65535u) + 65521u - 1u;
    sum2 += ((first >> 16) & 65535u) + ((second >> 16) & 65535u) + 65521u + rem;
    if (sum1 >= 65521u)
    {
        sum1 -= 65521u;
    }
    if (sum1 >= 65521u)
    {
        sum1 -= 65521u;
    }
    if (sum2 >= (65521u << 1))
    {
        sum2 -= 65521u << 1;
    }
    if (sum2 >= 65521u)
    {
        sum2 -= 65521u;
    }
    return sum1 | (sum2 << 16);
}
static unsigned fold_checksum(unsigned value)
{
    unsigned lower = value & 65535u;
    unsigned upper = value >> 16;
    return (lower * 31u + upper * 17u) & 65535u;
}
static unsigned stream_chunks(const unsigned char *data, unsigned length,
                               unsigned chunk)
{
    unsigned state = 1;
    unsigned offset = 0;
    if (chunk == 0)
    {
        return 1;
    }
    while (offset < length)
    {
        unsigned amount = length - offset;
        if (amount > chunk)
        {
            amount = chunk;
        }
        state = adler_update(state, data + offset, amount);
        offset += amount;
    }
    return state;
}
static unsigned combine_chunks(const unsigned char *data, unsigned length,
                                unsigned split)
{
    unsigned first = 0;
    unsigned second = 0;
    if (split > length)
    {
        return 0xffffffffu;
    }
    first = adler_update(1, data, split);
    second = adler_update(1, data + split, length - split);
    return adler_combine(first, second, (long long)(length - split));
}
unsigned evaluate(unsigned seed)
{
    unsigned char data[64];
    unsigned length = 17u + seed % 31u;
    unsigned split = 1u + seed % 15u;
    unsigned i = 0;
    unsigned streamed = 0;
    unsigned combined = 0;
    unsigned ordinary = 0;
    unsigned result = 0;
    for (i = 0; i < 64; i++)
    {
        data[i] = (unsigned char)((i * 29u + seed * 17u + (i >> 1)) & 255u);
    }
    streamed = stream_chunks(data, length, 1u + seed % 7u);
    combined = combine_chunks(data, length, split);
    ordinary = adler_update(1, data, length);
    result = fold_checksum(streamed) * 13u + fold_checksum(combined) * 17u;
    result += (unsigned)(streamed == ordinary) * 71u;
    result += (unsigned)(combined == ordinary) * 89u;
    result += fold_checksum(adler_combine(streamed, 1, 0)) * 7u;
    result += fold_checksum(adler_combine(streamed, combined, -1));
    result += adler_update(1, seed == 11u ? 0 : data, 1) & 65535u;
    return result & 65535u;
}
int main(void)
{
    unsigned result = evaluate(0);
    result ^= evaluate(1);
    result ^= evaluate(2);
    result ^= evaluate(11);
    return (int)(result & 255u);
}
