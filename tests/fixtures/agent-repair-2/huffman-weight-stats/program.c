/* Derived and modified from Zstandard 1.5.7.
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * BSD license selected; see origin.md and ../LICENSE.zstd.
 */
/* Only uncompressed-nibble weight headers are supported; FSE-compressed
 * headers are refused. Weight corruption returns (size_t)-1; an excessive
 * derived table log returns (size_t)-2. Only evaluate seeds 0..11 are supported.
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
static void clear_bytes(void *dst, int value, size_t size)
{
    BYTE *out = dst;
    size_t i;
    for (i = 0; i < size; ++i)
        out[i] = (BYTE)value;
}
static size_t HUF_readStats_body(BYTE* huffWeight, size_t hwSize, U32* outputRanks,
                   U32* nbSymbolsPtr, U32* tableLogPtr,
                   const void* src, size_t srcSize,
                   void* workSpace, size_t wkspSize,
                   int bmi2)
{
    U32 rankStats[13];
    U32 weightTotal;
    const BYTE* ip = (const BYTE*) src;
    size_t iSize;
    size_t oSize;
    if (!srcSize) return ((size_t)-1);
    iSize = ip[0];
    if (iSize >= 128) {
        oSize = iSize - 127;
        iSize = ((oSize+1)/2);
        if (iSize+1 > srcSize) return ((size_t)-1);
        if (oSize >= hwSize) return ((size_t)-1);
        ip += 1;
        {   U32 n;
            for (n=0; n<oSize; n+=2) {
                huffWeight[n]   = ip[n/2] >> 4;
                huffWeight[n+1] = ip[n/2] & 15;
    }   }   }
    else {
        return (size_t)-1;
    }
    clear_bytes(rankStats, 0, (12 + 1) * sizeof(U32));
    weightTotal = 0;
    {   U32 n; for (n=0; n<oSize; n++) {
            if (huffWeight[n] > 15) return ((size_t)-1);
            rankStats[huffWeight[n]]++;
            weightTotal += (1 << huffWeight[n]) >> 1;
    }   }
    if (weightTotal == 0) return ((size_t)-1);
    {   U32 const tableLog = highbit(weightTotal) + 1;
        if (tableLog > 12)
            return (size_t)-2;
        *tableLogPtr = tableLog;
        {   U32 const total = 1 << tableLog;
            U32 const rest = total - weightTotal;
            U32 const verif = 1 << highbit(rest);
            U32 const lastWeight = highbit(rest);
            if (verif != rest) return ((size_t)-1);
            huffWeight[oSize] = (BYTE)lastWeight;
            rankStats[lastWeight]++;
    }   }
    if ((rankStats[1] < 2) || (rankStats[1] & 1)) return ((size_t)-1);
    for (weightTotal = 0; weightTotal < 13; ++weightTotal)
        outputRanks[weightTotal] = rankStats[weightTotal];
    *nbSymbolsPtr = (U32)(oSize+1);
    return iSize+1;
}
static unsigned pack_weights(BYTE *out, const BYTE *weight, unsigned symbols)
{
    unsigned i;
    unsigned explicitCount = symbols - 1;
    out[0] = (BYTE)(127 + explicitCount);
    for (i = 0; i < explicitCount; i += 2) {
        unsigned high = weight[i];
        unsigned low = i + 1 < explicitCount ? weight[i + 1] : 0;
        out[1 + i / 2] = (BYTE)((high << 4) | low);
    }
    return 1 + (explicitCount + 1) / 2;
}
static unsigned make_lengths(BYTE *lengths, const BYTE *weights,
                             unsigned symbols, unsigned tableLog)
{
    unsigned i;
    unsigned total = 0;
    for (i = 0; i < symbols; ++i) {
        if (weights[i] == 0)
            lengths[i] = 0;
        else {
            lengths[i] = (BYTE)(tableLog + 1 - weights[i]);
            total += 1U << (tableLog - lengths[i]);
        }
    }
    return total;
}
static unsigned canonical_codes(const BYTE *lengths, unsigned symbols,
                                 unsigned *codes)
{
    unsigned count[13];
    unsigned next[13];
    unsigned i;
    unsigned code = 0;
    unsigned hash = 17;
    for (i = 0; i < 13; ++i) {
        count[i] = 0;
        next[i] = 0;
    }
    for (i = 0; i < symbols; ++i)
        ++count[lengths[i]];
    count[0] = 0;
    for (i = 1; i < 13; ++i) {
        code = (code + count[i - 1]) << 1;
        next[i] = code;
    }
    for (i = 0; i < symbols; ++i) {
        codes[i] = lengths[i] ? next[lengths[i]]++ : 0;
        hash = hash * 33 + codes[i] + lengths[i] * 257;
    }
    return hash;
}
unsigned long long evaluate(unsigned seed)
{
    BYTE bytes[16];
    BYTE weights[16];
    BYTE decoded[16];
    BYTE lengths[16];
    unsigned ranks[13];
    unsigned codes[16];
    unsigned symbols = seed + 2;
    unsigned decodedSymbols = 0;
    unsigned tableLog = 0;
    unsigned i;
    unsigned size;
    size_t status;
    for (i = 0; i < 16; ++i) {
        bytes[i] = 0;
        decoded[i] = 0;
        weights[i] = (BYTE)(i + 2 < symbols ? symbols - i - 1 : 1);
    }
    size = pack_weights(bytes, weights, symbols);
    if (seed == 11)
        bytes[1] = 0xf1;
    status = HUF_readStats_body(decoded, 16, ranks, &decodedSymbols,
                                &tableLog, bytes, size, 0, 0, 0);
    if (status > 16)
        return 0xbad00000U + seed + (status != (size_t)-1);
    i = make_lengths(lengths, decoded, decodedSymbols, tableLog);
    return ((unsigned long long)canonical_codes(lengths, decodedSymbols, codes) << 32) ^
           i ^ (tableLog << 16) ^ (ranks[1] << 8) ^ status;
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
