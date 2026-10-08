/* Derived and modified from Zstandard 1.5.7.
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * BSD license selected; see origin.md and ../LICENSE.zstd.
 */
/* Encoding-type selection for the three sequence code streams of a block
 * (literal length, offset and match length codes). Costs are in bits,
 * except the normalized-count header cost, which is in bytes. A stream
 * whose codes are all equal is RLE. A table built for an RLE stream or a
 * basic (default) stream is never repeated. Only evaluate seeds 0..11
 * are supported.
 */
typedef unsigned char BYTE;
typedef unsigned U32;
typedef unsigned long long U64;
typedef unsigned long size_t;

enum { set_basic, set_rle, set_compressed, set_repeat };
enum { FSE_repeat_none, FSE_repeat_check, FSE_repeat_valid };
enum { ZSTD_lazy = 4, FSE_MIN_TABLELOG = 5, FSE_MAX_TABLELOG = 12, FSE_DEFAULT_TABLELOG = 11 };
static const size_t ERROR_GENERIC = (size_t)-1;

static unsigned const kInverseProbabilityLog256[256] = {
    0,    2048, 1792, 1642, 1536, 1453, 1386, 1329, 1280, 1236, 1197, 1162,
    1130, 1100, 1073, 1047, 1024, 1001, 980,  960,  941,  923,  906,  889,
    874,  859,  844,  830,  817,  804,  791,  779,  768,  756,  745,  734,
    724,  714,  704,  694,  685,  676,  667,  658,  650,  642,  633,  626,
    618,  610,  603,  595,  588,  581,  574,  567,  561,  554,  548,  542,
    535,  529,  523,  517,  512,  506,  500,  495,  489,  484,  478,  473,
    468,  463,  458,  453,  448,  443,  438,  434,  429,  424,  420,  415,
    411,  407,  402,  398,  394,  390,  386,  382,  377,  373,  370,  366,
    362,  358,  354,  350,  347,  343,  339,  336,  332,  329,  325,  322,
    318,  315,  311,  308,  305,  302,  298,  295,  292,  289,  286,  282,
    279,  276,  273,  270,  267,  264,  261,  258,  256,  253,  250,  247,
    244,  241,  239,  236,  233,  230,  228,  225,  222,  220,  217,  215,
    212,  209,  207,  204,  202,  199,  197,  194,  192,  190,  187,  185,
    182,  180,  178,  175,  173,  171,  168,  166,  164,  162,  159,  157,
    155,  153,  151,  149,  146,  144,  142,  140,  138,  136,  134,  132,
    130,  128,  126,  123,  121,  119,  117,  115,  114,  112,  110,  108,
    106,  104,  102,  100,  98,   96,   94,   93,   91,   89,   87,   85,
    83,   82,   80,   78,   76,   74,   73,   71,   69,   67,   66,   64,
    62,   61,   59,   57,   55,   54,   52,   50,   49,   47,   46,   44,
    42,   41,   39,   37,   36,   34,   33,   31,   30,   28,   26,   25,
    23,   22,   20,   19,   17,   16,   14,   13,   11,   10,   8,    7,
    5,    4,    2,    1,
};
static const short LL_defaultNorm[36] = {
     4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2,
     2, 2, 2, 2, 2, 2, 2, 3, 2, 1, 1, 1, 1, 1, -1, -1, -1, -1 };
static const short ML_defaultNorm[53] = {
     1, 4, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1,
     1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
     1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1, -1, -1 };
static const short OF_defaultNorm[29] = {
     1, 1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1,
     1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1 };

/* A previous table: normalized counts that sum to 2^tableLog. */
typedef struct {
    short norm[53];
    unsigned tableLog;
    unsigned maxSymbolValue;
} prevTable_t;
typedef struct { unsigned repeatMode; prevTable_t table; } streamState_t;

static unsigned ZSTD_highbit32(U32 val)
{
    unsigned r = 0;
    while (val >>= 1)
        r++;
    return r;
}

/* Count each code, trim maxSymbolValue to the largest present code and
 * return the largest count. */
static unsigned HIST_count_simple(unsigned *count, unsigned *maxSymbolValuePtr,
                                  const BYTE *src, size_t srcSize)
{
    const BYTE *ip = src;
    const BYTE *const end = ip + srcSize;
    unsigned maxSymbolValue = *maxSymbolValuePtr;
    unsigned largestCount = 0;
    unsigned s;
    for (s = 0; s <= maxSymbolValue; s++)
        count[s] = 0;
    if (srcSize == 0) {
        *maxSymbolValuePtr = 0;
        return 0;
    }
    while (ip < end)
        count[*ip++]++;
    while (!count[maxSymbolValue])
        maxSymbolValue--;
    *maxSymbolValuePtr = maxSymbolValue;
    for (s = 0; s < maxSymbolValue; s++)
        if (count[s] > largestCount)
            largestCount = count[s];
    return largestCount;
}

static unsigned FSE_minTableLog(size_t srcSize, unsigned maxSymbolValue)
{
    U32 minBitsSrc = ZSTD_highbit32((U32)srcSize) + 1;
    U32 minBitsSymbols = ZSTD_highbit32(maxSymbolValue) + 2;
    return minBitsSrc < minBitsSymbols ? minBitsSrc : minBitsSymbols;
}

static unsigned FSE_optimalTableLog(unsigned maxTableLog, size_t srcSize, unsigned maxSymbolValue)
{
    U32 maxBitsSrc = ZSTD_highbit32((U32)(srcSize - 1)) - 2;
    U32 tableLog = maxTableLog;
    U32 minBits = FSE_minTableLog(srcSize, maxSymbolValue);
    if (tableLog == 0) tableLog = FSE_DEFAULT_TABLELOG;
    if (maxBitsSrc < tableLog) tableLog = maxBitsSrc;
    if (minBits > tableLog) tableLog = minBits;
    if (tableLog < FSE_MIN_TABLELOG) tableLog = FSE_MIN_TABLELOG;
    if (tableLog > FSE_MAX_TABLELOG) tableLog = FSE_MAX_TABLELOG;
    return tableLog;
}

/* Header size bound in bytes; it stands in for the written header size. */
static size_t FSE_NCountWriteBound(unsigned maxSymbolValue, unsigned tableLog)
{
    size_t maxHeaderSize = (((maxSymbolValue + 1) * tableLog + 4 + 2) / 8) + 1 + 2;
    return maxSymbolValue ? maxHeaderSize : 512;
}

/* Scale counts to sum 2^tableLog. A present code keeps at least 1; the most
 * frequent code takes the rounding remainder. */
static void normalize_counts(short *norm, unsigned tableLog, const unsigned *count,
                             size_t total, unsigned max)
{
    unsigned s, largest = 0;
    int stillToDistribute = 1 << tableLog;
    for (s = 0; s <= max; s++) {
        short proba = 0;
        if (count[s] != 0) {
            proba = (short)(((size_t)count[s] << tableLog) / total);
            if (proba == 0) proba = 1;
        }
        if (count[s] > count[largest]) largest = s;
        norm[s] = proba;
        stillToDistribute -= proba;
    }
    norm[largest] = (short)(norm[largest] + stillToDistribute);
}

/* Bits for count under the entropy bound; needs count[s] < total. */
static size_t ZSTD_entropyCost(unsigned const *count, unsigned const max, size_t const total)
{
    unsigned cost = 0;
    unsigned s;
    for (s = 0; s <= max; ++s) {
        unsigned norm = (unsigned)((256 * count[s]) / total);
        if (count[s] != 0 && norm == 0)
            norm = 1;
        cost += count[s] * kInverseProbabilityLog256[norm];
    }
    return cost >> 8;
}

/* Bits for count under a normalized table with accuracyLog <= 8. A norm
 * of -1 counts as 1. */
static size_t ZSTD_crossEntropyCost(short const *norm, unsigned accuracyLog,
                                    unsigned const *count, unsigned const max)
{
    unsigned const shift = 8 - accuracyLog;
    size_t cost = 0;
    unsigned s;
    for (s = 0; s <= max; ++s) {
        unsigned const normAcc = (norm[s] != -1) ? (unsigned)norm[s] : 1;
        unsigned const norm256 = normAcc << shift;
        cost += count[s] * kInverseProbabilityLog256[norm256];
    }
    return cost >> 8;
}

/* Bits for count under the previous table, or an error when that table
 * has no probability for a present code. */
static size_t ZSTD_fseBitCost(const prevTable_t *prev, unsigned const *count, unsigned const max)
{
    unsigned s;
    if (prev->maxSymbolValue < max)
        return ERROR_GENERIC;
    for (s = 0; s <= max; ++s)
        if (count[s] != 0 && prev->norm[s] == 0)
            return ERROR_GENERIC;
    return ZSTD_crossEntropyCost(prev->norm, prev->tableLog, count, max);
}

/* costs receives the basic, repeat and compressed estimates in bits when
 * the strategy compares costs; it is left unchanged otherwise. */
static unsigned ZSTD_selectEncodingType(unsigned *repeatMode, unsigned const *count, unsigned const max,
                                        size_t const mostFrequent, size_t nbSeq, unsigned const FSELog,
                                        const prevTable_t *prevCTable, short const *defaultNorm,
                                        U32 defaultNormLog, int isDefaultAllowed, unsigned strategy,
                                        size_t *costs)
{
    if (mostFrequent == nbSeq) {
        *repeatMode = FSE_repeat_none;
        if (isDefaultAllowed && nbSeq <= 2)
            return set_basic;
        return set_rle;
    }
    if (strategy < ZSTD_lazy) {
        if (isDefaultAllowed) {
            size_t const staticFse_nbSeq_max = 1000;
            size_t const mult = 10 - strategy;
            size_t const baseLog = 3;
            size_t const dynamicFse_nbSeq_min = (((size_t)1 << defaultNormLog) * mult) >> baseLog;
            if ((*repeatMode == FSE_repeat_valid) && (nbSeq < staticFse_nbSeq_max))
                return set_repeat;
            if ((nbSeq < dynamicFse_nbSeq_min) || (mostFrequent < (nbSeq >> (defaultNormLog - 1)))) {
                *repeatMode = FSE_repeat_none;
                return set_basic;
            }
        }
    } else {
        size_t const basicCost = isDefaultAllowed ? ZSTD_crossEntropyCost(defaultNorm, defaultNormLog, count, max) : ERROR_GENERIC;
        size_t const repeatCost = *repeatMode != FSE_repeat_none ? ZSTD_fseBitCost(prevCTable, count, max) : ERROR_GENERIC;
        size_t const NCountCost = FSE_NCountWriteBound(max, FSE_optimalTableLog(FSELog, nbSeq, max));
        size_t const compressedCost = NCountCost + ZSTD_entropyCost(count, max, nbSeq);
        costs[0] = basicCost;
        costs[1] = repeatCost;
        costs[2] = compressedCost;
        if (basicCost <= repeatCost && basicCost <= compressedCost) {
            *repeatMode = FSE_repeat_none;
            return set_basic;
        }
        if (repeatCost <= compressedCost)
            return set_repeat;
    }
    *repeatMode = FSE_repeat_check;
    return set_compressed;
}

static U64 mix(U64 acc, U64 value)
{
    return (acc ^ value) * 0x100000001B3ULL;
}

/* Select a type for one stream and keep the table that the next block may
 * repeat. Offsets above defaultMax rule out the default table. */
static U64 encode_stream(streamState_t *st, const BYTE *codes, size_t nbSeq, unsigned maxCode,
                         unsigned FSELog, const short *defaultNorm, unsigned defaultMax,
                         U32 defaultNormLog, unsigned strategy, U64 acc)
{
    unsigned count[53];
    unsigned max = maxCode;
    size_t costs[3] = { 0, 0, 0 };
    size_t mostFrequent = HIST_count_simple(count, &max, codes, nbSeq);
    unsigned type = ZSTD_selectEncodingType(&st->repeatMode, count, max, mostFrequent, nbSeq, FSELog,
                                            &st->table, defaultNorm, defaultNormLog,
                                            max <= defaultMax, strategy, costs);
    if (type == set_compressed) {
        unsigned s;
        st->table.tableLog = FSE_optimalTableLog(FSELog, nbSeq, max);
        st->table.maxSymbolValue = max;
        normalize_counts(st->table.norm, st->table.tableLog, count, nbSeq, max);
        for (s = max + 1; s < 53; s++)
            st->table.norm[s] = 0;
    }
    acc = mix(acc, type * 4 + st->repeatMode);
    acc = mix(mix(mix(acc, costs[0]), costs[1]), costs[2]);
    return mix(acc, max * 1024 + mostFrequent);
}

/* Codes of one stream: a skewed draw over [0, span); a span of 1 makes
 * every code equal to base. */
static void draw_codes(BYTE *codes, size_t nbSeq, U32 *x, unsigned base, unsigned span)
{
    size_t i;
    for (i = 0; i < nbSeq; i++) {
        unsigned r;
        *x = *x * 1103515245u + 12345u;
        r = (*x >> 16) % span;
        if (((*x >> 8) & 3) != 0)
            r = r / 3;
        codes[i] = (BYTE)(base + r);
    }
}

static U64 encode_block(streamState_t *streams, U32 seed, unsigned block, unsigned strategy, U64 acc)
{
    BYTE ll[160], of[160], ml[160];
    U32 x = seed * 2654435761u + block * 40503u + 7u;
    size_t nbSeq = 3 + (seed * 37 + block * 61) % 90;
    unsigned shape = (seed * 3 + block * 5) % 7;
    draw_codes(ll, nbSeq, &x, shape == 1 ? 9 : 0, shape == 1 ? 1 : 30);
    draw_codes(of, nbSeq, &x, shape == 4 ? 6 : 1, shape == 4 ? 1 : 4 + seed * 5 / 2);
    draw_codes(ml, nbSeq, &x, 0, shape == 6 ? 53 : 24);
    acc = encode_stream(streams + 0, ll, nbSeq, 35, 8, LL_defaultNorm, 35, 6, strategy, acc);
    acc = encode_stream(streams + 1, of, nbSeq, 31, 8, OF_defaultNorm, 28, 5, strategy, acc);
    return encode_stream(streams + 2, ml, nbSeq, 52, 8, ML_defaultNorm, 52, 6, strategy, acc);
}

unsigned long long evaluate(unsigned seed)
{
    static const unsigned dictCount[8] = { 9, 7, 5, 4, 3, 2, 2, 1 };
    streamState_t streams[3];
    unsigned strategy = 1 + (seed * 5) % 9;
    unsigned i, s, block;
    U64 acc = 0xCBF29CE484222325ULL;
    for (i = 0; i < 3; i++) {
        streams[i].repeatMode = seed % 3 == 0 ? FSE_repeat_valid : FSE_repeat_none;
        streams[i].table.maxSymbolValue = 7;
        streams[i].table.tableLog = 5;
        normalize_counts(streams[i].table.norm, 5, dictCount, 33, 7);
        for (s = 8; s < 53; s++)
            streams[i].table.norm[s] = 0;
    }
    for (block = 0; block < 3; block++)
        acc = encode_block(streams, seed, block, strategy, acc);
    return acc;
}

int main(void)
{
    U64 checksum = evaluate(1);
    checksum ^= evaluate(4) * 5;
    return (int)(checksum & 127);
}
