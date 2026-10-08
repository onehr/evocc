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
static void clear_bytes(void *dst, int value, size_t size)
{
    BYTE *out = dst;
    size_t i;
    for (i = 0; i < size; ++i)
        out[i] = (BYTE)value;
}
typedef U64 BitContainerType;
typedef struct {
    U64 bitContainer;
    unsigned bitPos;
    char *startPtr;
    char *ptr;
    char *endPtr;
} BIT_CStream_t;
typedef struct {
    U64 bitContainer;
    unsigned bitsConsumed;
    const char *start;
    const char *ptr;
    const char *limitPtr;
} BIT_DStream_t;
typedef enum {
    BIT_DStream_unfinished,
    BIT_DStream_endOfBuffer,
    BIT_DStream_completed,
    BIT_DStream_overflow
} BIT_DStream_status;
static void write64(void *dst, U64 value)
{
    BYTE *p = dst;
    unsigned i;
    for (i = 0; i < 8; ++i)
        p[i] = (BYTE)(value >> (8 * i));
}
static U64 read64(const void *src)
{
    const BYTE *p = src;
    U64 value = 0;
    unsigned i;
    for (i = 0; i < 8; ++i)
        value |= (U64)p[i] << (8 * i);
    return value;
}
static U64 BIT_getLowerBits(U64 value, unsigned bits)
{
    return value & ((1ULL << bits) - 1);
}
static size_t BIT_initCStream(BIT_CStream_t* bitC,
                                  void* startPtr, size_t dstCapacity)
{
    bitC->bitContainer = 0;
    bitC->bitPos = 0;
    bitC->startPtr = (char*)startPtr;
    bitC->ptr = bitC->startPtr;
    bitC->endPtr = bitC->startPtr + dstCapacity - sizeof(bitC->bitContainer);
    if (dstCapacity <= sizeof(bitC->bitContainer)) return ((size_t)-1);
    return 0;
}
static void BIT_addBits(BIT_CStream_t* bitC,
                            BitContainerType value, unsigned nbBits)
{
    bitC->bitContainer |= BIT_getLowerBits(value, nbBits) << bitC->bitPos;
    bitC->bitPos += nbBits;
}
static void BIT_addBitsFast(BIT_CStream_t* bitC,
                                BitContainerType value, unsigned nbBits)
{
    bitC->bitContainer |= value << bitC->bitPos;
    bitC->bitPos += nbBits;
}
static void BIT_flushBits(BIT_CStream_t* bitC)
{
    size_t const nbBytes = bitC->bitPos >> 3;
    write64(bitC->ptr, bitC->bitContainer);
    bitC->ptr += nbBytes;
    if (bitC->ptr > bitC->endPtr) bitC->ptr = bitC->endPtr;
    bitC->bitPos &= 7;
    bitC->bitContainer >>= nbBytes*8;
}
static size_t BIT_closeCStream(BIT_CStream_t* bitC)
{
    BIT_addBitsFast(bitC, 0, 1);
    BIT_flushBits(bitC);
    if (bitC->ptr >= bitC->endPtr) return 0;
    return (size_t)(bitC->ptr - bitC->startPtr) + (bitC->bitPos > 0);
}
static size_t BIT_initDStream(BIT_DStream_t* bitD, const void* srcBuffer, size_t srcSize)
{
    if (srcSize < 1) { clear_bytes(bitD, 0, sizeof(*bitD)); return ((size_t)-1); }
    bitD->start = (const char*)srcBuffer;
    bitD->limitPtr = bitD->start + sizeof(bitD->bitContainer);
    if (srcSize >=  sizeof(bitD->bitContainer)) {
        bitD->ptr   = (const char*)srcBuffer + srcSize - sizeof(bitD->bitContainer);
        bitD->bitContainer = read64(bitD->ptr);
        { BYTE const lastByte = ((const BYTE*)srcBuffer)[srcSize-1];
          bitD->bitsConsumed = lastByte ? 8 - highbit(lastByte) : 0;
          if (lastByte == 0) return ((size_t)-1);  }
    } else {
        bitD->ptr   = bitD->start;
        bitD->bitContainer = *(const BYTE*)(bitD->start);
        switch(srcSize)
        {
        case 7: bitD->bitContainer += (BitContainerType)(((const BYTE*)(srcBuffer))[6]) << (sizeof(bitD->bitContainer)*8 - 16);
        case 6: bitD->bitContainer += (BitContainerType)(((const BYTE*)(srcBuffer))[5]) << (sizeof(bitD->bitContainer)*8 - 24);
        case 5: bitD->bitContainer += (BitContainerType)(((const BYTE*)(srcBuffer))[4]) << (sizeof(bitD->bitContainer)*8 - 32);
        case 4: bitD->bitContainer += (BitContainerType)(((const BYTE*)(srcBuffer))[3]) << 24;
        case 3: bitD->bitContainer += (BitContainerType)(((const BYTE*)(srcBuffer))[2]) << 16;
        case 2: bitD->bitContainer += (BitContainerType)(((const BYTE*)(srcBuffer))[1]) <<  8;
        default: break;
        }
        {   BYTE const lastByte = ((const BYTE*)srcBuffer)[srcSize-1];
            bitD->bitsConsumed = lastByte ? 8 - highbit(lastByte) : 0;
            if (lastByte == 0) return ((size_t)-1);
        }
        bitD->bitsConsumed += (U32)(sizeof(bitD->bitContainer) - srcSize)*8;
    }
    return srcSize;
}
static U64 BIT_lookBits(const BIT_DStream_t *bitD, U32 nbBits)
{
    U32 start = 64 - bitD->bitsConsumed - nbBits;
    return (bitD->bitContainer >> (start & 63)) & ((1ULL << nbBits) - 1);
}
static void BIT_skipBits(BIT_DStream_t* bitD, U32 nbBits)
{
    bitD->bitsConsumed += nbBits;
}
static BitContainerType BIT_readBits(BIT_DStream_t* bitD, unsigned nbBits)
{
    BitContainerType const value = BIT_lookBits(bitD, nbBits);
    BIT_skipBits(bitD, nbBits);
    return value;
}
static BIT_DStream_status BIT_reloadDStream_internal(BIT_DStream_t* bitD)
{
    bitD->ptr -= bitD->bitsConsumed >> 3;
    bitD->bitsConsumed &= 7;
    bitD->bitContainer = read64(bitD->ptr);
    return BIT_DStream_unfinished;
}
static BIT_DStream_status BIT_reloadDStream(BIT_DStream_t* bitD)
{
    if ((bitD->bitsConsumed > (sizeof(bitD->bitContainer)*8))) {
        static const BitContainerType zeroFilled = 0;
        bitD->ptr = (const char*)&zeroFilled;
        return BIT_DStream_overflow;
    }
    if (bitD->ptr >= bitD->limitPtr) {
        return BIT_reloadDStream_internal(bitD);
    }
    if (bitD->ptr == bitD->start) {
        if (bitD->bitsConsumed < sizeof(bitD->bitContainer)*8) return BIT_DStream_endOfBuffer;
        return BIT_DStream_completed;
    }
    {   U32 nbBytes = bitD->bitsConsumed >> 3;
        BIT_DStream_status result = BIT_DStream_unfinished;
        if (bitD->ptr - nbBytes < bitD->start) {
            nbBytes = (U32)(bitD->ptr - bitD->start);
            result = BIT_DStream_endOfBuffer;
        }
        bitD->ptr -= nbBytes;
        bitD->bitsConsumed -= nbBytes*8;
        bitD->bitContainer = read64(bitD->ptr);
        return result;
    }
}
static unsigned BIT_endOfDStream(const BIT_DStream_t* DStream)
{
    return ((DStream->ptr == DStream->start) && (DStream->bitsConsumed == sizeof(DStream->bitContainer)*8));
}
static void append_value(BIT_CStream_t *stream, U64 value, unsigned width)
{
    if (stream->bitPos + width > 64)
        BIT_flushBits(stream);
    BIT_addBits(stream, value, width);
}
unsigned long long evaluate(unsigned seed)
{
    BYTE data[128];
    unsigned values[24];
    BIT_CStream_t writer;
    BIT_DStream_t reader;
    unsigned width = seed == 11 ? 16 : 5 + seed;
    unsigned count = 12 + seed;
    unsigned i;
    unsigned hash = 2166136261U;
    unsigned decoded = 0;
    size_t size;
    for (i = 0; i < 128; ++i)
        data[i] = 0;
    BIT_initCStream(&writer, data, 128);
    for (i = 0; i < count; ++i) {
        values[i] = (i * 79 + seed * 17) & ((1U << width) - 1);
        append_value(&writer, values[i], width);
    }
    size = BIT_closeCStream(&writer);
    for (i = 0; i < size; ++i)
        hash = (hash ^ data[i]) * 16777619U;
    if (BIT_initDStream(&reader, data, size) == (size_t)-1)
        return ((U64)hash << 32) ^ 0xbad00000U;
    for (i = count; i != 0; --i) {
        unsigned value;
        BIT_reloadDStream(&reader);
        value = (unsigned)BIT_readBits(&reader, width);
        decoded = decoded * 33 + value;
    }
    BIT_reloadDStream(&reader);
    return ((U64)hash << 32) ^ decoded ^ (size << 16) ^
           BIT_endOfDStream(&reader);
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
