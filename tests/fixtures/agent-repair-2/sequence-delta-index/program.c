/* Written for this corpus; see origin.md for the algorithm contract. */
typedef struct {
    unsigned char bytes[192];
    unsigned offsets[8];
    long long bases[8];
    unsigned byteCount;
    unsigned count;
    unsigned blocks;
} DeltaIndex;
static unsigned long long zigzag(long long delta)
{
    if (delta < 0)
        return (unsigned long long)(-delta) * 2 - 1;
    return (unsigned long long)delta * 2;
}
static long long unzigzag(unsigned long long code)
{
    if (code & 1)
        return -(long long)(code / 2) - 1;
    return (long long)(code / 2);
}
static unsigned write_varint(unsigned char *out, unsigned capacity,
                             unsigned long long value)
{
    unsigned count = 0;
    do {
        unsigned char byte = (unsigned char)(value & 127);
        value >>= 7;
        if (value != 0)
            byte |= 128;
        if (count == capacity)
            return 0;
        out[count++] = byte;
    } while (value != 0);
    return count;
}
static int read_varint(const unsigned char *bytes, unsigned count,
                       unsigned *position, unsigned long long *value)
{
    unsigned shift = 0;
    unsigned long long result = 0;
    while (*position < count && shift < 35) {
        unsigned byte = bytes[(*position)++];
        result |= (unsigned long long)(byte & 127) << shift;
        if ((byte & 128) == 0) {
            *value = result;
            return 1;
        }
        shift += 7;
    }
    return 0;
}
static int index_build(DeltaIndex *index, const int *values, unsigned count)
{
    unsigned i;
    int previous = 0;
    if (count > 32)
        return 0;
    index->count = count;
    index->byteCount = 0;
    index->blocks = 0;
    for (i = 0; i < count; ++i) {
        int current = values[i];
        long long delta = (long long)(current - previous);
        unsigned written;
        if (i % 4 == 0) {
            unsigned block = index->blocks++;
            index->offsets[block] = index->byteCount;
            index->bases[block] = current;
        }
        written = write_varint(index->bytes + index->byteCount,
                               192 - index->byteCount, zigzag(delta));
        if (written == 0)
            return 0;
        index->byteCount += written;
        previous = current;
    }
    return 1;
}
static int index_get(const DeltaIndex *index, unsigned item, long long *result)
{
    unsigned block;
    unsigned position;
    unsigned current;
    long long value;
    if (item >= index->count)
        return 0;
    block = item / 4;
    position = index->offsets[block];
    value = index->bases[block];
    for (current = block * 4; current <= item; ++current) {
        unsigned long long code;
        if (!read_varint(index->bytes, index->byteCount, &position, &code))
            return 0;
        value += unzigzag(code);
    }
    *result = value;
    return 1;
}
static long long index_sum(const DeltaIndex *index, unsigned begin, unsigned end)
{
    unsigned i;
    long long total = 0;
    if (begin > end || end > index->count)
        return 0;
    for (i = begin; i < end; ++i) {
        long long value;
        if (!index_get(index, i, &value))
            return 0;
        total += value;
    }
    return total;
}
static unsigned index_find(const DeltaIndex *index, long long threshold)
{
    unsigned i;
    for (i = 0; i < index->count; ++i) {
        long long value;
        if (!index_get(index, i, &value))
            return index->count;
        if (value >= threshold)
            return i;
    }
    return index->count;
}
static unsigned long long index_digest(const DeltaIndex *index)
{
    unsigned i;
    unsigned long long digest = index->byteCount;
    for (i = 0; i < index->count; ++i) {
        long long value = 0;
        index_get(index, i, &value);
        digest = digest * 65599 + (unsigned long long)value;
    }
    for (i = 0; i < index->byteCount; ++i)
        digest = digest * 33 ^ index->bytes[i];
    return digest;
}
unsigned long long evaluate(unsigned seed)
{
    DeltaIndex index;
    int values[32];
    unsigned count = 24 + seed % 9;
    unsigned i;
    unsigned long long result;
    for (i = 0; i < count; ++i)
        values[i] = (int)(seed * 97 + i * i * 3) - (int)(i % 4) * 131;
    if (seed == 11) {
        values[12] = 2147483640;
        values[13] = -2147483600;
    }
    index_build(&index, values, count);
    result = index_digest(&index);
    result ^= (unsigned long long)index_sum(&index, seed % 5, count - 2);
    result += index_find(&index, 400 + seed * 73);
    result *= 33;
    result ^= (unsigned long long)index_sum(&index, 0, count);
    return result;
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
