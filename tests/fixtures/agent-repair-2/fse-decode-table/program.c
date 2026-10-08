/* Derived and modified from Zstandard 1.5.7.
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * BSD license selected; see origin.md and ../LICENSE.zstd.
 */
typedef unsigned char BYTE;
typedef unsigned U32;
typedef unsigned short U16;
typedef short S16;
typedef unsigned long long U64;
typedef unsigned long size_t;
static unsigned highbit(unsigned value)
{
    unsigned result = 0;
    while (value > 1) {
        value >>= 1;
        ++result;
    }
    return result;
}
typedef struct {
    U16 newState;
    BYTE symbol;
    BYTE nbBits;
} Entry;
typedef struct {
    U16 tableLog;
    U16 fastMode;
} FSE_DTableHeader;
static void write64(BYTE *out, U64 value)
{
    unsigned i;
    for (i = 0; i < 8; ++i)
        out[i] = (BYTE)(value >> (i * 8));
}
static size_t FSE_buildDTable_internal(Entry *tableDecode, const short* normalizedCounter, unsigned maxSymbolValue, unsigned tableLog)
{
    U16 symbolNext[16];
    BYTE spread[72];
    U32 const maxSV1 = maxSymbolValue + 1;
    U32 const tableSize = 1 << tableLog;
    U32 highThreshold = tableSize-1;
    if (maxSymbolValue > 15) return ((size_t)-1);
    if (tableLog > 6) return ((size_t)-1);
    {   FSE_DTableHeader DTableH;
        DTableH.tableLog = (U16)tableLog;
        DTableH.fastMode = 1;
        {   S16 const largeLimit= (S16)(1 << (tableLog-1));
            U32 s;
            for (s=0; s<maxSV1; s++) {
                if (normalizedCounter[s]==-1) {
                    tableDecode[highThreshold--].symbol = (BYTE)s;
                    symbolNext[s] = 1;
                } else {
                    if (normalizedCounter[s] >= largeLimit) DTableH.fastMode=0;
                    symbolNext[s] = (U16)normalizedCounter[s];
        }   }   }
        (void)DTableH;
    }
    if (highThreshold == tableSize - 1) {
        size_t const tableMask = tableSize-1;
        size_t const step = ((tableSize >> 1) + (tableSize >> 3) + 3);
        {   U64 const add = 0x0101010101010101ull;
            size_t pos = 0;
            U64 sv = 0;
            U32 s;
            for (s=0; s<maxSV1; ++s, sv += add) {
                int i;
                int const n = normalizedCounter[s];
                write64(spread + pos, sv);
                for (i = 8; i < n; i += 8) {
                    write64(spread + pos + i, sv);
                }
                pos += (size_t)n;
        }   }
        {
            size_t position = 0;
            size_t s;
            size_t const unroll = 2;
            for (s = 0; s < (size_t)tableSize; s += unroll) {
                size_t u;
                for (u = 0; u < unroll; ++u) {
                    size_t const uPosition = (position + (u * step)) & tableMask;
                    tableDecode[uPosition].symbol = spread[s + u];
                }
                position = (position + (unroll * step)) & tableMask;
            }
        }
    } else {
        U32 const tableMask = tableSize-1;
        U32 const step = ((tableSize >> 1) + (tableSize >> 3) + 3);
        U32 s, position = 0;
        for (s=0; s<maxSV1; s++) {
            int i;
            for (i=0; i<normalizedCounter[s]; i++) {
                tableDecode[position].symbol = (BYTE)s;
                position = (position + step) & tableMask;
                while (position > highThreshold) position = (position + step) & tableMask;
        }   }
        if (position!=0) return ((size_t)-1);
    }
    {   U32 u;
        for (u=0; u<tableSize; u++) {
            BYTE const symbol = (BYTE)(tableDecode[u].symbol);
            U32 const nextState = symbolNext[symbol]++;
            tableDecode[u].nbBits = (BYTE) (tableLog - highbit(nextState) );
            tableDecode[u].newState = (U16) ( (nextState << tableDecode[u].nbBits) - (tableSize - 1));
    }   }
    return 0;
}
static unsigned inspect_table(const Entry *table, unsigned tableSize)
{
    unsigned i;
    unsigned value = 2166136261U;
    for (i = 0; i < tableSize; ++i) {
        value = (value ^ table[i].symbol) * 16777619U;
        value = (value ^ table[i].nbBits) * 16777619U;
        value = (value ^ table[i].newState) * 16777619U;
    }
    return value;
}
static unsigned walk_table(const Entry *table, unsigned size, unsigned start)
{
    unsigned state = start % size;
    unsigned step;
    unsigned value = 0;
    for (step = 0; step < 24; ++step) {
        unsigned mask;
        unsigned bits;
        if (state >= size)
            return value ^ 0xbad00000U;
        value = value * 33 + table[state].symbol;
        mask = (1U << table[state].nbBits) - 1;
        bits = (step * 13 + start) & mask;
        state = table[state].newState + bits;
    }
    return value ^ state;
}
unsigned long long evaluate(unsigned seed)
{
    Entry table[63];
    short normalized[4];
    unsigned tableLog = seed == 11 ? 6 : 5;
    unsigned size = 1U << tableLog;
    unsigned i;
    size_t status;
    normalized[0] = seed & 1 ? -1 : 1;
    normalized[1] = (short)(2 + seed % 3);
    normalized[2] = (short)(3 + seed % 5);
    normalized[3] = (short)(size - 1 - normalized[1] - normalized[2]);
    for (i = 0; i < size; ++i) {
        table[i].newState = 0;
        table[i].symbol = 0;
        table[i].nbBits = 0;
    }
    status = FSE_buildDTable_internal(table, normalized, 3, tableLog);
    if (size > sizeof(table) / sizeof(table[0]))
        return 0xbad00000U + seed;
    return ((unsigned long long)inspect_table(table, size) << 32) ^
           walk_table(table, size, seed * 7) ^ status;
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
