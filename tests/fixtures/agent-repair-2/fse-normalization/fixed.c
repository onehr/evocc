/* Derived and modified from Zstandard 1.5.7.
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * BSD license selected; see origin.md and ../LICENSE.zstd.
 */
/* Empty histograms are supported adapter inputs and report UINT_MAX without
 * producing a normalization table. Only evaluate seeds 0..11 are supported.
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
static U64 divide64(U64 value, U32 divisor)
{
    return value / divisor;
}
static unsigned is_error(size_t value)
{
    return value == (size_t)-1;
}
static unsigned FSE_minTableLog(size_t srcSize, unsigned maxSymbolValue)
{
    U32 minBitsSrc = highbit((U32)(srcSize)) + 1;
    U32 minBitsSymbols = highbit(maxSymbolValue) + 2;
    U32 minBits = minBitsSrc < minBitsSymbols ? minBitsSrc : minBitsSymbols;
    return minBits;
}
static size_t FSE_normalizeM2(short* norm, U32 tableLog, const unsigned* count, size_t total, U32 maxSymbolValue, short lowProbCount)
{
    short const NOT_YET_ASSIGNED = -2;
    U32 s;
    U32 distributed = 0;
    U32 ToDistribute;
    U32 const lowThreshold = (U32)(total >> tableLog);
    U32 lowOne = (U32)((total * 3) >> (tableLog + 1));
    for (s=0; s<=maxSymbolValue; s++) {
        if (count[s] == 0) {
            norm[s]=0;
            continue;
        }
        if (count[s] <= lowThreshold) {
            norm[s] = lowProbCount;
            distributed++;
            total -= count[s];
            continue;
        }
        if (count[s] <= lowOne) {
            norm[s] = 1;
            distributed++;
            total -= count[s];
            continue;
        }
        norm[s]=NOT_YET_ASSIGNED;
    }
    ToDistribute = (1 << tableLog) - distributed;
    if (ToDistribute == 0)
        return 0;
    if ((total / ToDistribute) > lowOne) {
        lowOne = (U32)((total * 3) / (ToDistribute * 2));
        for (s=0; s<=maxSymbolValue; s++) {
            if ((norm[s] == NOT_YET_ASSIGNED) && (count[s] <= lowOne)) {
                norm[s] = 1;
                distributed++;
                total -= count[s];
                continue;
        }   }
        ToDistribute = (1 << tableLog) - distributed;
    }
    if (distributed == maxSymbolValue+1) {
        U32 maxV = 0, maxC = 0;
        for (s=0; s<=maxSymbolValue; s++)
            if (count[s] > maxC) { maxV=s; maxC=count[s]; }
        norm[maxV] += (short)ToDistribute;
        return 0;
    }
    if (total == 0) {
        for (s=0; ToDistribute > 0; s = (s+1)%(maxSymbolValue+1))
            if (norm[s] > 0) { ToDistribute--; norm[s]++; }
        return 0;
    }
    {   U64 const vStepLog = 62 - tableLog;
        U64 const mid = (1ULL << (vStepLog-1)) - 1;
        U64 const rStep = divide64((((U64)1<<vStepLog) * ToDistribute) + mid, (U32)total);
        U64 tmpTotal = mid;
        for (s=0; s<=maxSymbolValue; s++) {
            if (norm[s]==NOT_YET_ASSIGNED) {
                U64 const end = tmpTotal + (count[s] * rStep);
                U32 const sStart = (U32)(tmpTotal >> vStepLog);
                U32 const sEnd = (U32)(end >> vStepLog);
                U32 const weight = sEnd - sStart;
                if (weight < 1)
                    return ((size_t)-1);
                norm[s] = (short)weight;
                tmpTotal = end;
    }   }   }
    return 0;
}
size_t FSE_normalizeCount (short* normalizedCounter, unsigned tableLog,
                           const unsigned* count, size_t total,
                           unsigned maxSymbolValue, unsigned useLowProbCount)
{
    if (tableLog==0) tableLog = 6;
    if (tableLog < 5) return ((size_t)-1);
    if (tableLog > 12) return ((size_t)-1);
    if (tableLog < FSE_minTableLog(total, maxSymbolValue)) return ((size_t)-1);
    {   static U32 const rtbTable[] = {     0, 473195, 504333, 520860, 550000, 700000, 750000, 830000 };
        short const lowProbCount = useLowProbCount ? -1 : 1;
        U64 const scale = 62 - tableLog;
        U64 const step = divide64((U64)1<<62, (U32)total);
        U64 const vStep = 1ULL<<(scale-20);
        int stillToDistribute = 1<<tableLog;
        unsigned s;
        unsigned largest=0;
        short largestP=0;
        U32 lowThreshold = (U32)(total >> tableLog);
        for (s=0; s<=maxSymbolValue; s++) {
            if (count[s] == total) return 0;
            if (count[s] == 0) { normalizedCounter[s]=0; continue; }
            if (count[s] <= lowThreshold) {
                normalizedCounter[s] = lowProbCount;
                stillToDistribute--;
            } else {
                short proba = (short)((count[s]*step) >> scale);
                if (proba<8) {
                    U64 restToBeat = vStep * rtbTable[proba];
                    proba += (count[s]*step) - ((U64)proba<<scale) > restToBeat;
                }
                if (proba > largestP) { largestP=proba; largest=s; }
                normalizedCounter[s] = proba;
                stillToDistribute -= proba;
        }   }
        if (-stillToDistribute >= (normalizedCounter[largest] >> 1)) {
            size_t const errorCode = FSE_normalizeM2(normalizedCounter, tableLog, count, total, maxSymbolValue, lowProbCount);
            if (is_error(errorCode)) return errorCode;
        }
        else normalizedCounter[largest] += (short)stillToDistribute;
    }
    return tableLog;
}
static unsigned normalize_histogram(short *norm, const unsigned *count,
                                    unsigned maximum, unsigned tableLog)
{
    unsigned i;
    unsigned total = 0;
    for (i = 0; i <= maximum; ++i) {
        total += count[i];
        norm[i] = 0;
    }
    if (total == 0)
        return (unsigned)-1;
    return (unsigned)FSE_normalizeCount(norm, tableLog, count, total, maximum, 1);
}
static unsigned inspect_normalization(const short *norm, unsigned size)
{
    unsigned i;
    unsigned sum = 0;
    unsigned digest = 17;
    for (i = 0; i < size; ++i) {
        unsigned magnitude = norm[i] < 0 ? (unsigned)-norm[i] : (unsigned)norm[i];
        sum += magnitude;
        digest = digest * 65599U + (unsigned)(norm[i] + 2);
    }
    return digest ^ (sum << 16);
}
unsigned long long evaluate(unsigned seed)
{
    unsigned count[16];
    short first[16];
    short second[16];
    unsigned i;
    unsigned status;
    unsigned other;
    for (i = 0; i < 16; ++i)
        count[i] = seed == 11 ? 0 : 1 + (i * i + seed * 13) % 59;
    status = normalize_histogram(first, count, 15, 6);
    for (i = 0; i < 16; ++i)
        count[i] *= 3;
    other = normalize_histogram(second, count, 15, 7);
    return ((unsigned long long)inspect_normalization(first, 16) << 32) ^
           inspect_normalization(second, 16) ^ status ^ (other << 8);
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
