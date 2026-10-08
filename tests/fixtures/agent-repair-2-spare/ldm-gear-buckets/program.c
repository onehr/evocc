/* Derived and modified from Zstandard 1.5.7.
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * BSD license selected; see origin.md and ../LICENSE.zstd.
 */
/* Long-distance matching over one caller-owned buffer.
 * A gear rolling hash selects split points. A split point names the
 * minMatchLength bytes that end at it. A Rabin-Karp hash of those bytes
 * gives a bucket index (low hBits bits) and a checksum (high 32 bits).
 * A bucket has 2^bucketSizeLog entries, and new entries overwrite old ones
 * in rotation. Positions are byte indexes from the buffer start. An entry
 * with offset 0 is empty, so the first byte never starts a match.
 * Only evaluate seeds 0..11 are supported.
 */
typedef unsigned char BYTE;
typedef unsigned U32;
typedef unsigned long long U64;
typedef unsigned long size_t;

enum {
    LDM_MIN_MATCH_LENGTH = 8, LDM_BUCKET_SIZE_LOG = 1, LDM_BUCKETSIZELOG_MAX = 3,
    LDM_HASHLOG_MIN = 4, LDM_HASHLOG_MAX = 6, LDM_BATCH_SIZE = 16, HASH_READ_SIZE = 8,
    ZSTD_btultra = 8, ROLL_HASH_CHAR_OFFSET = 10, SEQ_CAPACITY = 24
};
static const U64 prime8bytes = 0xCF1BBCDCB7A56463ULL;

typedef struct { U32 windowLog, hashLog, bucketSizeLog, minMatchLength, hashRateLog; } ldmParams_t;
typedef struct { U32 offset; U32 checksum; } ldmEntry_t;
typedef struct { U64 rolling; U64 stopMask; } ldmRollingHashState_t;
typedef struct {
    const BYTE *base;
    ldmEntry_t hashTable[64];
    BYTE bucketOffsets[32];
    size_t splitIndices[LDM_BATCH_SIZE];
} ldmState_t;
typedef struct { U32 litLength; U32 matchLength; U32 offset; } rawSeq;
typedef struct { rawSeq seq[SEQ_CAPACITY]; size_t size; size_t capacity; } rawSeqStore_t;

static U64 ZSTD_ldm_gearTab[256];

/* SplitMix64 output stands in for the fixed gear table. */
static void ldm_init_gear_table(void)
{
    U64 x = 0x243F6A8885A308D3ULL, z;
    U32 i;
    for (i = 0; i < 256; i++) {
        z = x += 0x9E3779B97F4A7C15ULL;
        z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
        z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
        ZSTD_ldm_gearTab[i] = z ^ (z >> 31);
    }
}

/* Rabin-Karp hash of size bytes; it replaces XXH64 for split points. */
static U64 ZSTD_rollingHash_compute(const BYTE *buf, size_t size)
{
    U64 hash = 0;
    size_t pos;
    for (pos = 0; pos < size; ++pos)
        hash = hash * prime8bytes + buf[pos] + ROLL_HASH_CHAR_OFFSET;
    return hash;
}

/* The stop mask has hashRateLog bits set, so a split occurs every
 * 2^hashRateLog bytes on average. The bits sit as high as possible
 * below bit minMatchLength (bit n depends on the last n bytes). When
 * hashRateLog is 0 or larger than that width, the mask is simply the
 * low hashRateLog bits. */
static void ZSTD_ldm_gear_init(ldmRollingHashState_t *state, const ldmParams_t *params)
{
    unsigned maxBitsInMask = params->minMatchLength < 64 ? params->minMatchLength : 64;
    unsigned hashRateLog = params->hashRateLog;

    state->rolling = ~(U32)0;
    if (hashRateLog > 0) {
        state->stopMask = (((U64)1 << hashRateLog) - 1) << (maxBitsInMask - hashRateLog);
    } else {
        state->stopMask = ((U64)1 << hashRateLog) - 1;
    }
}

/* Feed minMatchLength bytes into the rolling state without splitting. */
static void ZSTD_ldm_gear_reset(ldmRollingHashState_t *state, const BYTE *data, size_t minMatchLength)
{
    U64 hash = state->rolling;
    size_t n = 0;
    while (n < minMatchLength)
        hash = (hash << 1) + ZSTD_ldm_gearTab[data[n++] & 0xff];
    state->rolling = hash;
}

/* Record the position after each byte whose hash has all mask bits clear.
 * Stop after a full batch; return the number of bytes consumed. */
static size_t ZSTD_ldm_gear_feed(ldmRollingHashState_t *state, const BYTE *data, size_t size,
                                 size_t *splits, unsigned *numSplits)
{
    size_t n = 0;
    U64 hash = state->rolling;
    U64 mask = state->stopMask;
    while (n < size) {
        hash = (hash << 1) + ZSTD_ldm_gearTab[data[n] & 0xff];
        n += 1;
        if ((hash & mask) == 0) {
            splits[*numSplits] = n;
            *numSplits += 1;
            if (*numSplits == LDM_BATCH_SIZE)
                break;
        }
    }
    state->rolling = hash;
    return n;
}

static U32 bounded(U32 low, U32 value, U32 high)
{
    return value < low ? low : (value > high ? high : value);
}

/* Fill parameters left at 0. An explicit hashLog sets the rate to
 * windowLog - hashLog; the btultra strategies halve the minimum match. */
static void ZSTD_ldm_adjustParameters(ldmParams_t *params, U32 strategy)
{
    if (params->hashRateLog == 0) {
        if (params->hashLog > 0) {
            if (params->windowLog > params->hashLog)
                params->hashRateLog = params->windowLog - params->hashLog;
        } else {
            params->hashRateLog = 7 - (strategy / 3);
        }
    }
    if (params->hashLog == 0)
        params->hashLog = bounded(LDM_HASHLOG_MIN, params->windowLog - params->hashRateLog, LDM_HASHLOG_MAX);
    if (params->minMatchLength == 0) {
        params->minMatchLength = LDM_MIN_MATCH_LENGTH;
        if (strategy >= ZSTD_btultra)
            params->minMatchLength /= 2;
    }
    if (params->bucketSizeLog == 0)
        params->bucketSizeLog = bounded(LDM_BUCKET_SIZE_LOG, strategy, LDM_BUCKETSIZELOG_MAX);
    if (params->bucketSizeLog > params->hashLog)
        params->bucketSizeLog = params->hashLog;
}

static ldmEntry_t *ZSTD_ldm_getBucket(ldmState_t *ldmState, size_t hash, U32 bucketSizeLog)
{
    return ldmState->hashTable + (hash << bucketSizeLog);
}

static void ZSTD_ldm_insertEntry(ldmState_t *ldmState, size_t hash, ldmEntry_t entry, U32 bucketSizeLog)
{
    BYTE *pOffset = ldmState->bucketOffsets + hash;
    unsigned offset = *pOffset;
    *(ZSTD_ldm_getBucket(ldmState, hash, bucketSizeLog) + offset) = entry;
    *pOffset = (BYTE)((offset + 1) & ((1u << bucketSizeLog) - 1));
}

static size_t ZSTD_count(const BYTE *pIn, const BYTE *pMatch, const BYTE *pInLimit)
{
    const BYTE *pStart = pIn;
    while (pIn < pInLimit && *pMatch == *pIn)
        pIn++, pMatch++;
    return (size_t)(pIn - pStart);
}

/* Count equal bytes before pIn and pMatch, stopping at pAnchor and pMatchBase. */
static size_t ZSTD_ldm_countBackwardsMatch(const BYTE *pIn, const BYTE *pAnchor,
                                           const BYTE *pMatch, const BYTE *pMatchBase)
{
    size_t matchLength = 0;
    while (pIn > pAnchor && pMatch > pMatchBase && pIn[-1] == pMatch[-1])
        pIn--, pMatch--, matchLength++;
    return matchLength;
}

static void ZSTD_ldm_fillHashTable(ldmState_t *ldmState, const BYTE *ip, const BYTE *iend,
                                   const ldmParams_t *params)
{
    U32 minMatchLength = params->minMatchLength;
    U32 hBits = params->hashLog - params->bucketSizeLog;
    const BYTE *istart = ip;
    ldmRollingHashState_t hashState;
    size_t *splits = ldmState->splitIndices;
    unsigned numSplits;
    ZSTD_ldm_gear_init(&hashState, params);
    while (ip < iend) {
        size_t hashed;
        unsigned n;
        numSplits = 0;
        hashed = ZSTD_ldm_gear_feed(&hashState, ip, (size_t)(iend - ip), splits, &numSplits);
        for (n = 0; n < numSplits; n++) {
            if (ip + splits[n] >= istart + minMatchLength) {
                const BYTE *split = ip + splits[n] - minMatchLength;
                U64 xxhash = ZSTD_rollingHash_compute(split, minMatchLength);
                ldmEntry_t entry;
                entry.offset = (U32)(split - ldmState->base);
                entry.checksum = (U32)(xxhash >> 32);
                ZSTD_ldm_insertEntry(ldmState, (U32)(xxhash & (((U32)1 << hBits) - 1)), entry,
                                     params->bucketSizeLog);
            }
        }
        ip += hashed;
    }
}

/* Emit one sequence per accepted match. A match extends forward from the
 * split and backward down to the anchor. litLength counts the bytes from
 * the anchor to the start of the backward-extended match. Returns the
 * number of trailing literals, or (size_t)-1 when the store is full. */
static size_t ZSTD_ldm_generateSequences_internal(ldmState_t *ldmState, rawSeqStore_t *rawSeqStore,
                                                  const ldmParams_t *params, const BYTE *src, size_t srcSize)
{
    U32 minMatchLength = params->minMatchLength;
    U32 entsPerBucket = 1U << params->bucketSizeLog;
    U32 hBits = params->hashLog - params->bucketSizeLog;
    const BYTE *base = ldmState->base;
    const BYTE *iend = src + srcSize;
    const BYTE *ilimit = iend - HASH_READ_SIZE;
    const BYTE *anchor = src;
    const BYTE *ip = src;
    ldmRollingHashState_t hashState;
    size_t *splits = ldmState->splitIndices;
    unsigned numSplits;
    if (srcSize < minMatchLength + HASH_READ_SIZE)
        return (size_t)(iend - anchor);
    ZSTD_ldm_gear_init(&hashState, params);
    ZSTD_ldm_gear_reset(&hashState, ip, minMatchLength);
    ip += minMatchLength;
    while (ip < ilimit) {
        size_t hashed;
        unsigned n;
        numSplits = 0;
        hashed = ZSTD_ldm_gear_feed(&hashState, ip, (size_t)(ilimit - ip), splits, &numSplits);
        for (n = 0; n < numSplits; n++) {
            size_t forwardMatchLength = 0, backwardMatchLength = 0, bestMatchLength = 0;
            const BYTE *split = ip + splits[n] - minMatchLength;
            U64 xxhash = ZSTD_rollingHash_compute(split, minMatchLength);
            U32 hash = (U32)(xxhash & (((U32)1 << hBits) - 1));
            ldmEntry_t *bucket = ZSTD_ldm_getBucket(ldmState, hash, params->bucketSizeLog);
            const ldmEntry_t *cur;
            const ldmEntry_t *bestEntry = 0;
            ldmEntry_t newEntry;
            newEntry.offset = (U32)(split - base);
            newEntry.checksum = (U32)(xxhash >> 32);
            if (split < anchor) {
                ZSTD_ldm_insertEntry(ldmState, hash, newEntry, params->bucketSizeLog);
                continue;
            }
            for (cur = bucket; cur < bucket + entsPerBucket; cur++) {
                size_t curForward, curBackward;
                if (cur->checksum != newEntry.checksum || cur->offset == 0)
                    continue;
                curForward = ZSTD_count(split, base + cur->offset, iend);
                if (curForward < minMatchLength)
                    continue;
                curBackward = ZSTD_ldm_countBackwardsMatch(split, anchor, base + cur->offset, base);
                if (curForward + curBackward > bestMatchLength) {
                    bestMatchLength = curForward + curBackward;
                    forwardMatchLength = curForward;
                    backwardMatchLength = curBackward;
                    bestEntry = cur;
                }
            }
            if (bestEntry != 0) {
                rawSeq *seq = rawSeqStore->seq + rawSeqStore->size;
                if (rawSeqStore->size == rawSeqStore->capacity)
                    return (size_t)-1;
                seq->litLength = (U32)(split - anchor);
                seq->matchLength = (U32)(forwardMatchLength + backwardMatchLength);
                seq->offset = (U32)(split - base) - bestEntry->offset;
                rawSeqStore->size++;
            }
            ZSTD_ldm_insertEntry(ldmState, hash, newEntry, params->bucketSizeLog);
            if (bestEntry == 0)
                continue;
            anchor = split + forwardMatchLength;
            /* A match past the hashed bytes restarts the gear state from
             * the minMatchLength bytes that end at the anchor. */
            if (anchor > ip + hashed) {
                ZSTD_ldm_gear_reset(&hashState, anchor - minMatchLength, minMatchLength);
                ip = anchor - hashed;
                break;
            }
        }
        ip += hashed;
    }
    return (size_t)(iend - anchor);
}

/* Bytes [0, dictSize) only seed the table; matching covers the rest. */
static size_t ldm_process(ldmState_t *state, rawSeqStore_t *store, const ldmParams_t *params,
                          const BYTE *data, size_t dictSize, size_t size)
{
    U32 i;
    for (i = 0; i < 64; i++)
        state->hashTable[i].offset = state->hashTable[i].checksum = 0;
    for (i = 0; i < 32; i++)
        state->bucketOffsets[i] = 0;
    state->base = data;
    store->size = 0;
    store->capacity = SEQ_CAPACITY;
    ZSTD_ldm_fillHashTable(state, data, data + dictSize, params);
    return ZSTD_ldm_generateSequences_internal(state, store, params, data + dictSize, size - dictSize);
}

/* Rebuild bytes [dictSize, size) from the sequences; count wrong bytes.
 * Returns size + 1 when a sequence leaves the buffer. */
static U32 ldm_replay(const BYTE *data, size_t dictSize, size_t size, const rawSeqStore_t *store,
                      size_t lastLiterals)
{
    BYTE out[160];
    size_t pos = dictSize;
    size_t i, k;
    U32 wrong = 0;
    for (i = 0; i < dictSize; i++)
        out[i] = data[i];
    for (i = 0; i < store->size; i++) {
        const rawSeq *seq = store->seq + i;
        if (seq->litLength > size - pos || seq->matchLength > size - pos - seq->litLength ||
            seq->offset == 0 || seq->offset > pos + seq->litLength)
            return (U32)size + 1;
        for (k = 0; k < seq->litLength; k++, pos++)
            out[pos] = data[pos];
        for (k = 0; k < seq->matchLength; k++, pos++)
            out[pos] = out[pos - seq->offset];
    }
    if (lastLiterals != size - pos)
        return (U32)size + 1;
    for (; pos < size; pos++)
        out[pos] = data[pos];
    for (i = dictSize; i < size; i++)
        wrong += out[i] != data[i];
    return wrong;
}

static U64 mix(U64 acc, U64 value)
{
    return (acc ^ value) * 0x100000001B3ULL;
}

static size_t build_input(BYTE *data, U32 seed)
{
    static const U32 distances[4] = { 0, 40, 29, 17 };
    size_t size = 120 + (seed % 4) * 12;
    U32 x = seed * 2654435761u + 0x6B43A9B5u;
    U32 mode = 0;
    size_t i;
    for (i = 0; i < size; i++) {
        x = x * 1103515245u + 12345u;
        if (i % 20 == 0)
            mode = (x >> 16) % 4;
        if (i >= 40 && mode != 0)
            data[i] = data[i - distances[mode]];
        else
            data[i] = (BYTE)('a' + ((x >> 16) % 16));
        if (i >= 40 && (x >> 24) == 7)
            data[i] = (BYTE)'z';
    }
    return size;
}

unsigned long long evaluate(unsigned seed)
{
    BYTE data[160];
    ldmState_t state;
    rawSeqStore_t store;
    ldmParams_t params;
    U32 strategy = 1 + (seed * 4) % 9;
    size_t size, last, i;
    U64 acc = 0xCBF29CE484222325ULL;
    ldm_init_gear_table();
    size = build_input(data, seed);
    params.windowLog = 8 + seed % 3;
    params.hashLog = 5 + seed % 2;
    params.bucketSizeLog = params.minMatchLength = params.hashRateLog = 0;
    ZSTD_ldm_adjustParameters(&params, strategy);
    last = ldm_process(&state, &store, &params, data, 40, size);
    acc = mix(acc, params.hashRateLog * 256 + params.minMatchLength * 16 + params.bucketSizeLog);
    acc = mix(acc, store.size);
    for (i = 0; i < store.size; i++)
        acc = mix(mix(mix(acc, store.seq[i].litLength), store.seq[i].matchLength), store.seq[i].offset);
    acc = mix(acc, last);
    acc = mix(acc, last == (size_t)-1 ? 999 : ldm_replay(data, 40, size, &store, last));
    for (i = 0; i < 64; i++)
        acc = mix(acc, (U64)state.hashTable[i].offset << 32 | state.hashTable[i].checksum);
    for (i = 0; i < 32; i++)
        acc = mix(acc, state.bucketOffsets[i]);
    return acc;
}

int main(void)
{
    U64 checksum = evaluate(7);
    checksum ^= evaluate(2) * 3;
    return (int)(checksum & 127);
}
