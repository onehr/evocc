/* Derived and modified from Zstandard 1.5.7.
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * BSD license selected; see origin.md and ../LICENSE.zstd.
 */
/* Sequence coding without entropy tables. A sequence has a literal length,
 * a match length of at least 3 and an offset. The encoder replaces recent
 * offsets by repeat codes, maps each value to a code plus extra bits, and
 * packs the codes (6, 5 and 6 bits) and then the offset, match and literal
 * extra bits, least significant bit first. The decoder reverses the steps
 * with its own copy of the offset history. Only evaluate seeds 0..11 are
 * supported.
 */
typedef unsigned char BYTE;
typedef unsigned U32;
typedef unsigned long long U64;

enum { ZSTD_REP_NUM = 3, MINMATCH = 3, MAX_SEQ = 16, STREAM_CAPACITY = 96 };

static const BYTE LL_bits[36] = {
     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
     1, 1, 1, 1, 2, 2, 3, 3, 4, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15, 16 };
static const BYTE ML_bits[53] = {
     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
     1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 7, 8, 9, 10, 11,
    12, 13, 14, 15, 16 };
static const U32 LL_base[36] = {
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    16, 18, 20, 22, 24, 28, 32, 40, 48, 64, 0x80, 0x100, 0x200, 0x400, 0x800, 0x1000,
    0x2000, 0x4000, 0x8000, 0x10000 };
static const U32 ML_base[53] = {
     3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
    19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34,
    35, 37, 39, 41, 43, 47, 51, 59, 67, 83, 99, 0x83, 0x103, 0x203, 0x403, 0x803,
    0x1003, 0x2003, 0x4003, 0x8003, 0x10003 };
static const U32 OF_base[32] = {
    0, 1, 1, 5, 0xD, 0x1D, 0x3D, 0x7D, 0xFD, 0x1FD, 0x3FD, 0x7FD, 0xFFD, 0x1FFD, 0x3FFD, 0x7FFD,
    0xFFFD, 0x1FFFD, 0x3FFFD, 0x7FFFD, 0xFFFFD, 0x1FFFFD, 0x3FFFFD, 0x7FFFFD,
    0xFFFFFD, 0x1FFFFFD, 0x3FFFFFD, 0x7FFFFFD, 0xFFFFFFD, 0x1FFFFFFD, 0x3FFFFFFD, 0x7FFFFFFD };

typedef struct { U32 litLength; U32 matchLength; U32 offset; } rawSeq;
/* offBase 1..3 is a repeat code; offBase = offset + 3 is a full offset. */
typedef struct { U32 litLength; U32 offBase; U32 mlBase; } SeqDef;
typedef struct {
    SeqDef sequences[MAX_SEQ];
    BYTE llCode[MAX_SEQ];
    BYTE ofCode[MAX_SEQ];
    BYTE mlCode[MAX_SEQ];
    U32 nbSeq;
} SeqStore_t;
typedef struct { BYTE *data; U32 pos; U32 size; U64 container; U32 bits; } bitStream_t;

static U32 ZSTD_highbit32(U32 val)
{
    U32 r = 0;
    while (val >>= 1)
        r++;
    return r;
}

/* Literal lengths 0..63 use the table; longer ones use highbit + 19. */
static U32 ZSTD_LLcode(U32 litLength)
{
    static const BYTE LL_Code[64] = {  0,  1,  2,  3,  4,  5,  6,  7,
                                       8,  9, 10, 11, 12, 13, 14, 15,
                                      16, 16, 17, 17, 18, 18, 19, 19,
                                      20, 20, 20, 20, 21, 21, 21, 21,
                                      22, 22, 22, 22, 22, 22, 22, 22,
                                      23, 23, 23, 23, 23, 23, 23, 23,
                                      24, 24, 24, 24, 24, 24, 24, 24,
                                      24, 24, 24, 24, 24, 24, 24, 24 };
    static const U32 LL_deltaCode = 19;
    return (litLength > 64) ? ZSTD_highbit32(litLength) + LL_deltaCode : LL_Code[litLength];
}

/* mlBase is matchLength - 3; values 0..127 use the table. */
static U32 ZSTD_MLcode(U32 mlBase)
{
    static const BYTE ML_Code[128] = { 0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15,
                                      16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
                                      32, 32, 33, 33, 34, 34, 35, 35, 36, 36, 36, 36, 37, 37, 37, 37,
                                      38, 38, 38, 38, 38, 38, 38, 38, 39, 39, 39, 39, 39, 39, 39, 39,
                                      40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40, 40,
                                      41, 41, 41, 41, 41, 41, 41, 41, 41, 41, 41, 41, 41, 41, 41, 41,
                                      42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42,
                                      42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42, 42 };
    static const U32 ML_deltaCode = 36;
    return (mlBase > 127) ? ZSTD_highbit32(mlBase) + ML_deltaCode : ML_Code[mlBase];
}

/* Offset history update. A full offset is pushed to the front. With
 * litLength 0 the repeat codes shift by one: code 1 selects entry 1,
 * code 2 entry 2, and code 3 the new value entry 0 minus 1. Otherwise code
 * 1 selects entry 0 and changes nothing. A selected entry moves to the
 * front; only the entries in front of it shift down. A new value shifts all. */
static void ZSTD_updateRep(U32 rep[ZSTD_REP_NUM], U32 const offBase, U32 const ll0)
{
    if (offBase > ZSTD_REP_NUM) {
        rep[2] = rep[1];
        rep[1] = rep[0];
        rep[0] = offBase - ZSTD_REP_NUM;
    } else {
        U32 const repCode = offBase - 1 + ll0;
        if (repCode > 0) {
            U32 const currentOffset = (repCode == ZSTD_REP_NUM) ? (rep[0] - 1) : rep[repCode];
            rep[2] = rep[1];
            rep[1] = rep[0];
            rep[0] = currentOffset;
        }
    }
}

/* Choose the repeat code for rawOffset, or the full offset form. */
static U32 ZSTD_finalizeOffBase(U32 rawOffset, const U32 rep[ZSTD_REP_NUM], U32 ll0)
{
    U32 offBase = rawOffset + ZSTD_REP_NUM;
    if (!ll0 && rawOffset == rep[0]) {
        offBase = 1;
    } else if (rawOffset == rep[1]) {
        offBase = 2 - ll0;
    } else if (rawOffset == rep[2]) {
        offBase = 3 - ll0;
    } else if (ll0 && rawOffset == rep[0] - 1) {
        offBase = 3;
    }
    return offBase;
}

static void ZSTD_seqToCodes(SeqStore_t *seqStorePtr)
{
    U32 u;
    for (u = 0; u < seqStorePtr->nbSeq; u++) {
        const SeqDef *seq = seqStorePtr->sequences + u;
        seqStorePtr->llCode[u] = (BYTE)ZSTD_LLcode(seq->litLength);
        seqStorePtr->ofCode[u] = (BYTE)ZSTD_highbit32(seq->offBase);
        seqStorePtr->mlCode[u] = (BYTE)ZSTD_MLcode(seq->mlBase);
    }
}

/* Packed size of one sequence in bits: three codes plus extra bits. */
static U32 ZSTD_sequenceBits(U32 litLength, U32 offBase, U32 mlBase)
{
    return 6 + 5 + 6 + LL_bits[ZSTD_LLcode(litLength)] + ZSTD_highbit32(offBase) + ML_bits[ZSTD_MLcode(mlBase)];
}

static void BIT_addBits(bitStream_t *bs, U32 value, U32 nbBits)
{
    bs->container |= (U64)(value & (((U32)1 << nbBits) - 1)) << bs->bits;
    bs->bits += nbBits;
    while (bs->bits >= 8) {
        bs->data[bs->pos++] = (BYTE)bs->container;
        bs->container >>= 8;
        bs->bits -= 8;
    }
}

/* Read nbBits; a read past the end yields zero bits and sets pos past size. */
static U32 BIT_readBits(bitStream_t *bs, U32 nbBits)
{
    U32 value;
    while (bs->bits < nbBits) {
        if (bs->pos < bs->size)
            bs->container |= (U64)bs->data[bs->pos] << bs->bits;
        bs->pos++;
        bs->bits += 8;
    }
    value = (U32)(bs->container & (((U64)1 << nbBits) - 1));
    bs->container >>= nbBits;
    bs->bits -= nbBits;
    return value;
}

/* Store the sequences with repeat codes. Returns the packed size in bits,
 * or 0 when the block does not fit in the stream. */
static U32 ZSTD_storeBlock(SeqStore_t *store, const rawSeq *seqs, U32 nbSeq, U32 rep[ZSTD_REP_NUM])
{
    U32 n, totalBits = 0;
    store->nbSeq = 0;
    for (n = 0; n < nbSeq; n++) {
        U32 const ll0 = seqs[n].litLength == 0;
        U32 const offBase = ZSTD_finalizeOffBase(seqs[n].offset, rep, ll0);
        SeqDef *seq = store->sequences + n;
        totalBits += ZSTD_sequenceBits(seqs[n].litLength, offBase, seqs[n].matchLength - MINMATCH);
        if (totalBits > STREAM_CAPACITY * 8)
            return 0;
        seq->litLength = seqs[n].litLength;
        seq->offBase = offBase;
        seq->mlBase = seqs[n].matchLength - MINMATCH;
        ZSTD_updateRep(rep, offBase, ll0);
        store->nbSeq++;
    }
    return totalBits;
}

static U32 ZSTD_encodeBlock(BYTE *dst, const SeqStore_t *store)
{
    bitStream_t bs;
    U32 n;
    bs.data = dst;
    bs.pos = 0;
    bs.container = 0;
    bs.bits = 0;
    for (n = 0; n < store->nbSeq; n++) {
        const SeqDef *seq = store->sequences + n;
        BIT_addBits(&bs, store->llCode[n], 6);
        BIT_addBits(&bs, store->ofCode[n], 5);
        BIT_addBits(&bs, store->mlCode[n], 6);
        BIT_addBits(&bs, seq->offBase, store->ofCode[n]);
        BIT_addBits(&bs, seq->mlBase, ML_bits[store->mlCode[n]]);
        BIT_addBits(&bs, seq->litLength, LL_bits[store->llCode[n]]);
    }
    if (bs.bits > 0)
        BIT_addBits(&bs, 0, 8 - bs.bits);
    return bs.pos;
}

/* Decode nbSeq sequences; returns the number of sequences that differ from
 * expected, or nbSeq + 1 when the stream is malformed. */
static U32 ZSTD_decodeBlock(const BYTE *src, U32 srcSize, U32 nbSeq, U32 prevOffset[ZSTD_REP_NUM],
                            const rawSeq *expected)
{
    bitStream_t bs;
    U32 n, wrong = 0;
    bs.data = (BYTE *)src;
    bs.pos = 0;
    bs.size = srcSize;
    bs.container = 0;
    bs.bits = 0;
    for (n = 0; n < nbSeq; n++) {
        U32 const llCode = BIT_readBits(&bs, 6);
        U32 const ofBits = BIT_readBits(&bs, 5);
        U32 const mlCode = BIT_readBits(&bs, 6);
        U32 offset, litLength, matchLength;
        if (llCode > 35 || mlCode > 52)
            return nbSeq + 1;
        if (ofBits > 1) {
            offset = OF_base[ofBits] + BIT_readBits(&bs, ofBits);
            prevOffset[2] = prevOffset[1];
            prevOffset[1] = prevOffset[0];
            prevOffset[0] = offset;
        } else {
            U32 const ll0 = (LL_base[llCode] == 0);
            if (ofBits == 0) {
                offset = prevOffset[ll0];
                prevOffset[1] = prevOffset[!ll0];
                prevOffset[0] = offset;
            } else {
                U32 temp;
                offset = OF_base[ofBits] + ll0 + BIT_readBits(&bs, 1);
                temp = (offset == 3) ? prevOffset[0] - 1 : prevOffset[offset];
                temp -= !temp;
                if (offset != 1)
                    prevOffset[2] = prevOffset[1];
                prevOffset[1] = prevOffset[0];
                prevOffset[0] = offset = temp;
            }
        }
        matchLength = ML_base[mlCode] + BIT_readBits(&bs, ML_bits[mlCode]);
        litLength = LL_base[llCode] + BIT_readBits(&bs, LL_bits[llCode]);
        wrong += litLength != expected[n].litLength || matchLength != expected[n].matchLength ||
                 offset != expected[n].offset;
    }
    return bs.pos > srcSize ? nbSeq + 1 : wrong;
}

static U64 mix(U64 acc, U64 value)
{
    return (acc ^ value) * 0x100000001B3ULL;
}

/* Draw sequences that often reuse one of the last three distinct offsets. */
static U32 build_sequences(rawSeq *seqs, unsigned seed)
{
    U32 recent[3] = { 1, 4, 8 };
    U32 x = seed * 2246822519u + 374761393u;
    U32 n, nbSeq = 10 + seed % 6;
    for (n = 0; n < nbSeq; n++) {
        U32 pick, offset;
        x = x * 1103515245u + 12345u;
        pick = (x >> 16) % 8;
        seqs[n].litLength = pick < 2 ? 0 : (pick < 7 ? (x >> 8) % 24 : 36 + (x >> 4) % 29);
        seqs[n].matchLength = MINMATCH + ((x >> 20) % 5 == 0 ? 120 + (x >> 3) % 200 : (x >> 12) % 30);
        x = x * 1103515245u + 12345u;
        pick = (x >> 16) % 6;
        offset = pick < 3 ? recent[pick] : (pick == 3 && recent[0] > 1 ? recent[0] - 1 : 1 + (x >> 4) % 900);
        seqs[n].offset = offset;
        if (offset != recent[0]) {
            if (offset != recent[1])
                recent[2] = recent[1];
            recent[1] = recent[0];
            recent[0] = offset;
        }
    }
    return nbSeq;
}

static U64 code_block(const rawSeq *seqs, U32 nbSeq, U64 acc)
{
    SeqStore_t store;
    BYTE stream[STREAM_CAPACITY];
    U32 rep[ZSTD_REP_NUM] = { 1, 4, 8 };
    U32 prevOffset[ZSTD_REP_NUM] = { 1, 4, 8 };
    U32 totalBits = ZSTD_storeBlock(&store, seqs, nbSeq, rep);
    U32 size, n;
    acc = mix(acc, totalBits);
    if (totalBits == 0)
        return mix(acc, store.nbSeq);
    ZSTD_seqToCodes(&store);
    size = ZSTD_encodeBlock(stream, &store);
    for (n = 0; n < size; n++)
        acc = mix(acc, stream[n]);
    acc = mix(acc, ZSTD_decodeBlock(stream, size, nbSeq, prevOffset, seqs));
    for (n = 0; n < ZSTD_REP_NUM; n++)
        acc = mix(mix(acc, rep[n]), prevOffset[n]);
    return acc;
}

unsigned long long evaluate(unsigned seed)
{
    rawSeq seqs[MAX_SEQ];
    U32 nbSeq = build_sequences(seqs, seed);
    U64 acc = mix(0xCBF29CE484222325ULL, nbSeq);
    acc = code_block(seqs, nbSeq, acc);
    /* The second pass drops the first sequence, so the history differs. */
    return code_block(seqs + 1, nbSeq - 1, acc);
}

int main(void)
{
    U64 checksum = evaluate(0);
    checksum ^= evaluate(1) * 3;
    return (int)(checksum & 127);
}
