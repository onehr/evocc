/* Derived and modified from Zstandard 1.5.7.
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * BSD license selected; see origin.md and ../LICENSE.zstd.
 */
/* The Zstandard frame layer with raw and RLE blocks only.
 * A frame is: magic 0xFD2FB528 (little endian), a frame header, blocks,
 * and an optional 4-byte content checksum. A skippable frame is: magic
 * 0x184D2A50..0x184D2A5F, a 4-byte payload size and the payload; the
 * decoder skips it. Decoded bytes of all frames go back to back into the
 * file-scope array g_regen (REGEN_CAPACITY bytes). A block that does not fit
 * there is an error, and nothing is written for it. Errors are size_t values
 * above (size_t)-20. Only evaluate seeds 0..11 are supported.
 */
typedef unsigned char BYTE;
typedef unsigned U32;
typedef unsigned long long U64;
typedef unsigned long size_t;

enum { bt_raw, bt_rle, bt_compressed, bt_reserved };
enum {
    ZSTD_FRAMEIDSIZE = 4, ZSTD_blockHeaderSize = 3, ZSTD_SKIPPABLEHEADERSIZE = 8,
    ZSTD_FRAMEHEADERSIZE_MAX = 18, ZSTD_WINDOWLOG_ABSOLUTEMIN = 10, ZSTD_WINDOWLOG_MAX = 31,
    REGEN_CAPACITY = 128, STREAM_CAPACITY = 320
};
static const U32 ZSTD_MAGICNUMBER = 0xFD2FB528U;
static const U32 ZSTD_MAGIC_SKIPPABLE_START = 0x184D2A50U;
static const U32 ZSTD_MAGIC_SKIPPABLE_MASK = 0xFFFFFFF0U;
static const U64 ZSTD_CONTENTSIZE_UNKNOWN = (U64)-1;
static const size_t ZSTD_fcs_fieldSize[4] = { 0, 2, 4, 8 };
static const size_t ZSTD_did_fieldSize[4] = { 0, 1, 2, 4 };
static const size_t ERR_srcSize_wrong = (size_t)-1, ERR_prefix_unknown = (size_t)-2;
static const size_t ERR_corruption_detected = (size_t)-3, ERR_dstSize_tooSmall = (size_t)-4;
static const size_t ERR_checksum_wrong = (size_t)-5, ERR_frameParameter = (size_t)-6;

static BYTE g_regen[REGEN_CAPACITY];

typedef struct { U64 frameContentSize, windowSize; U32 headerSize, dictID, checksumFlag; } ZSTD_FrameHeader;
typedef struct { U32 lastBlock; U32 blockType; U32 origSize; } blockProperties_t;
typedef struct { U32 contentSizeFlag; U32 checksumFlag; U32 windowLog; U32 dictID; } frameParams_t;

static int ZSTD_isError(size_t code) { return code > (size_t)-20; }
static U32 MEM_readLE16(const BYTE *p) { return (U32)p[0] | (U32)p[1] << 8; }
static U32 MEM_readLE24(const BYTE *p) { return MEM_readLE16(p) | (U32)p[2] << 16; }
static U32 MEM_readLE32(const BYTE *p) { return MEM_readLE16(p) | MEM_readLE16(p + 2) << 16; }
static U64 MEM_readLE64(const BYTE *p) { return MEM_readLE32(p) | (U64)MEM_readLE32(p + 4) << 32; }
static void MEM_writeLE16(BYTE *p, U32 v) { p[0] = (BYTE)v; p[1] = (BYTE)(v >> 8); }
static void MEM_writeLE24(BYTE *p, U32 v) { MEM_writeLE16(p, v); p[2] = (BYTE)(v >> 16); }
static void MEM_writeLE32(BYTE *p, U32 v) { MEM_writeLE16(p, v); MEM_writeLE16(p + 2, v >> 16); }

/* FNV-1a of the frame content; it stands in for the XXH64 low 32 bits. */
static U32 content_checksum(const BYTE *p, size_t size)
{
    U32 h = 2166136261U;
    while (size--)
        h = (h ^ *p++) * 16777619U;
    return h;
}

static size_t ZSTD_writeFrameHeader(BYTE *op, size_t dstCapacity, const frameParams_t *params,
                                    U64 pledgedSrcSize)
{
    U32 const dictID = params->dictID;
    U32 const dictIDSizeCode = (dictID > 0) + (dictID >= 256) + (dictID >= 65536);
    U32 const checksumFlag = params->checksumFlag > 0;
    U32 const windowSize = (U32)1 << params->windowLog;
    U32 const singleSegment = params->contentSizeFlag && (windowSize >= pledgedSrcSize);
    BYTE const windowLogByte = (BYTE)((params->windowLog - ZSTD_WINDOWLOG_ABSOLUTEMIN) << 3);
    U32 const fcsCode = params->contentSizeFlag ?
        (pledgedSrcSize >= 256) + (pledgedSrcSize >= 65536 + 256) + (pledgedSrcSize >= 0xFFFFFFFFU) : 0;
    BYTE const frameHeaderDescriptionByte =
        (BYTE)(dictIDSizeCode + (checksumFlag << 2) + (singleSegment << 5) + (fcsCode << 6));
    size_t pos = 4;
    if (dstCapacity < ZSTD_FRAMEHEADERSIZE_MAX)
        return ERR_dstSize_tooSmall;
    MEM_writeLE32(op, ZSTD_MAGICNUMBER);
    op[pos++] = frameHeaderDescriptionByte;
    if (!singleSegment) op[pos++] = windowLogByte;
    switch (dictIDSizeCode) {
    case 1: op[pos] = (BYTE)dictID; pos++; break;
    case 2: MEM_writeLE16(op + pos, dictID); pos += 2; break;
    case 3: MEM_writeLE32(op + pos, dictID); pos += 4; break;
    default: break;
    }
    switch (fcsCode) {
    case 0: if (singleSegment) op[pos++] = (BYTE)pledgedSrcSize; break;
    case 1: MEM_writeLE16(op + pos, (U32)(pledgedSrcSize - 256)); pos += 2; break;
    default: MEM_writeLE32(op + pos, (U32)pledgedSrcSize); pos += 4; break;
    }
    return pos;
}

/* A block header is 3 bytes: bit 0 last block, bits 1-2 type, bits 3-23 size.
 * A raw block carries its bytes; an RLE block carries one byte. */
static size_t ZSTD_writeBlock(BYTE *op, size_t dstCapacity, const BYTE *src, size_t srcSize,
                              U32 isRle, U32 lastBlock)
{
    U32 const cBlockHeader = lastBlock + ((isRle ? (U32)bt_rle : (U32)bt_raw) << 1) + (U32)(srcSize << 3);
    size_t const cSize = isRle ? 1 : srcSize;
    size_t i;
    if (ZSTD_blockHeaderSize + cSize > dstCapacity)
        return ERR_dstSize_tooSmall;
    MEM_writeLE24(op, cBlockHeader);
    for (i = 0; i < cSize; i++)
        op[ZSTD_blockHeaderSize + i] = src[i];
    return ZSTD_blockHeaderSize + cSize;
}

static size_t ZSTD_writeSkippableFrame(BYTE *op, size_t dstCapacity, const BYTE *src, size_t srcSize,
                                       unsigned magicVariant)
{
    size_t i;
    if (dstCapacity < srcSize + ZSTD_SKIPPABLEHEADERSIZE || magicVariant > 15)
        return ERR_dstSize_tooSmall;
    MEM_writeLE32(op, ZSTD_MAGIC_SKIPPABLE_START + magicVariant);
    MEM_writeLE32(op + 4, (U32)srcSize);
    for (i = 0; i < srcSize; i++)
        op[ZSTD_SKIPPABLEHEADERSIZE + i] = src[i];
    return srcSize + ZSTD_SKIPPABLEHEADERSIZE;
}

/* Header size: magic, descriptor, window byte unless single segment,
 * dictionary ID field, and content-size field. A single-segment frame
 * with content-size code 0 still has a 1-byte content size. */
static size_t ZSTD_frameHeaderSize_internal(const BYTE *src, size_t srcSize)
{
    size_t const minInputSize = 5;
    if (srcSize < minInputSize) return ERR_srcSize_wrong;
    {
        BYTE const fhd = src[minInputSize - 1];
        U32 const dictID = fhd & 3;
        U32 const singleSegment = (fhd >> 5) & 1;
        U32 const fcsId = fhd >> 6;
        return minInputSize + !singleSegment + ZSTD_did_fieldSize[dictID] + ZSTD_fcs_fieldSize[fcsId]
             + (singleSegment && fcsId);
    }
}

/* Returns 0 when zfhPtr is filled, the wanted input size when srcSize is
 * too small, or an error. */
static size_t ZSTD_getFrameHeader(ZSTD_FrameHeader *zfhPtr, const BYTE *ip, size_t srcSize)
{
    size_t const minInputSize = 5;
    if (srcSize < minInputSize) return minInputSize;
    if (MEM_readLE32(ip) != ZSTD_MAGICNUMBER) return ERR_prefix_unknown;
    {
        size_t const fhsize = ZSTD_frameHeaderSize_internal(ip, srcSize);
        if (srcSize < fhsize) return fhsize;
        zfhPtr->headerSize = (U32)fhsize;
    }
    {
        BYTE const fhdByte = ip[minInputSize - 1];
        size_t pos = minInputSize;
        U32 const dictIDSizeCode = fhdByte & 3;
        U32 const singleSegment = (fhdByte >> 5) & 1;
        U32 const fcsID = fhdByte >> 6;
        U64 windowSize = 0;
        U32 dictID = 0;
        U64 frameContentSize = ZSTD_CONTENTSIZE_UNKNOWN;
        if ((fhdByte & 0x08) != 0)
            return ERR_frameParameter;
        if (!singleSegment) {
            BYTE const wlByte = ip[pos++];
            U32 const windowLog = (wlByte >> 3) + ZSTD_WINDOWLOG_ABSOLUTEMIN;
            if (windowLog > ZSTD_WINDOWLOG_MAX)
                return ERR_frameParameter;
            windowSize = (1ULL << windowLog);
            windowSize += (windowSize >> 3) * (wlByte & 7);
        }
        switch (dictIDSizeCode) {
        case 1: dictID = ip[pos]; pos++; break;
        case 2: dictID = MEM_readLE16(ip + pos); pos += 2; break;
        case 3: dictID = MEM_readLE32(ip + pos); pos += 4; break;
        default: break;
        }
        switch (fcsID) {
        case 0: if (singleSegment) frameContentSize = ip[pos]; break;
        case 1: frameContentSize = MEM_readLE16(ip + pos) + 256; break;
        case 2: frameContentSize = MEM_readLE32(ip + pos); break;
        default: frameContentSize = MEM_readLE64(ip + pos); break;
        }
        if (singleSegment) windowSize = frameContentSize;
        zfhPtr->frameContentSize = frameContentSize;
        zfhPtr->windowSize = windowSize;
        zfhPtr->dictID = dictID;
        zfhPtr->checksumFlag = (fhdByte >> 2) & 1;
    }
    return 0;
}

static size_t ZSTD_getcBlockSize(const BYTE *src, size_t srcSize, blockProperties_t *bpPtr)
{
    U32 cBlockHeader, cSize;
    if (srcSize < ZSTD_blockHeaderSize) return ERR_srcSize_wrong;
    cBlockHeader = MEM_readLE24(src);
    cSize = cBlockHeader >> 3;
    bpPtr->lastBlock = cBlockHeader & 1;
    bpPtr->blockType = (cBlockHeader >> 1) & 3;
    bpPtr->origSize = cSize;
    if (bpPtr->blockType == bt_rle) return 1;
    if (bpPtr->blockType == bt_reserved) return ERR_corruption_detected;
    return cSize;
}

/* Both block writers check the remaining capacity of g_regen first. */
static size_t ZSTD_copyRawBlock(size_t op, size_t dstCapacity, const BYTE *src, size_t srcSize)
{
    size_t i;
    if (srcSize > dstCapacity)
        return ERR_dstSize_tooSmall;
    for (i = 0; i < srcSize; i++)
        g_regen[op + i] = src[i];
    return srcSize;
}

static size_t ZSTD_setRleBlock(size_t op, size_t dstCapacity, BYTE b, size_t regenSize)
{
    size_t i;
    if (regenSize > dstCapacity)
        return ERR_dstSize_tooSmall;
    for (i = 0; i < regenSize; i++)
        g_regen[op + i] = b;
    return regenSize;
}

static size_t readSkippableFrameSize(const BYTE *src, size_t srcSize)
{
    U32 sizeU32;
    if (srcSize < ZSTD_SKIPPABLEHEADERSIZE) return ERR_srcSize_wrong;
    sizeU32 = MEM_readLE32(src + ZSTD_FRAMEIDSIZE);
    if ((U32)(sizeU32 + ZSTD_SKIPPABLEHEADERSIZE) < sizeU32)
        return ERR_frameParameter;
    if (ZSTD_SKIPPABLEHEADERSIZE + (size_t)sizeU32 > srcSize)
        return ERR_srcSize_wrong;
    return ZSTD_SKIPPABLEHEADERSIZE + (size_t)sizeU32;
}

/* Decode one frame at *srcPtr into g_regen from *opPtr; advance both. */
static size_t ZSTD_decompressFrame(size_t *opPtr, const BYTE **srcPtr, size_t *srcSizePtr, U64 *info)
{
    const BYTE *ip = *srcPtr;
    size_t const ostart = *opPtr;
    size_t op = ostart;
    size_t remainingSrcSize = *srcSizePtr;
    ZSTD_FrameHeader zfh;
    if (remainingSrcSize < 6 + ZSTD_blockHeaderSize)
        return ERR_srcSize_wrong;
    {
        size_t const frameHeaderSize = ZSTD_frameHeaderSize_internal(ip, 5);
        size_t const result = ZSTD_getFrameHeader(&zfh, ip, remainingSrcSize);
        if (ZSTD_isError(frameHeaderSize)) return frameHeaderSize;
        if (remainingSrcSize < frameHeaderSize + ZSTD_blockHeaderSize) return ERR_srcSize_wrong;
        if (result != 0) return ZSTD_isError(result) ? result : ERR_srcSize_wrong;
        ip += frameHeaderSize;
        remainingSrcSize -= frameHeaderSize;
    }
    *info = (*info ^ zfh.windowSize ^ (U64)zfh.dictID << 40) * 0x100000001B3ULL;
    while (1) {
        size_t decodedSize;
        blockProperties_t blockProperties;
        size_t const cBlockSize = ZSTD_getcBlockSize(ip, remainingSrcSize, &blockProperties);
        if (ZSTD_isError(cBlockSize)) return cBlockSize;
        ip += ZSTD_blockHeaderSize;
        remainingSrcSize -= ZSTD_blockHeaderSize;
        if (cBlockSize > remainingSrcSize) return ERR_srcSize_wrong;
        if (blockProperties.blockType == bt_raw)
            decodedSize = ZSTD_copyRawBlock(op, REGEN_CAPACITY - op, ip, cBlockSize);
        else if (blockProperties.blockType == bt_rle)
            decodedSize = ZSTD_setRleBlock(op, REGEN_CAPACITY, *ip, blockProperties.origSize);
        else
            return ERR_corruption_detected;
        if (ZSTD_isError(decodedSize)) return decodedSize;
        op += decodedSize;
        ip += cBlockSize;
        remainingSrcSize -= cBlockSize;
        if (blockProperties.lastBlock) break;
    }
    if (zfh.frameContentSize != ZSTD_CONTENTSIZE_UNKNOWN && (U64)(op - ostart) != zfh.frameContentSize)
        return ERR_corruption_detected;
    if (zfh.checksumFlag) {
        if (remainingSrcSize < 4) return ERR_checksum_wrong;
        if (MEM_readLE32(ip) != content_checksum(g_regen + ostart, op - ostart)) return ERR_checksum_wrong;
        ip += 4;
        remainingSrcSize -= 4;
    }
    *srcPtr = ip;
    *srcSizePtr = remainingSrcSize;
    *opPtr = op;
    return op - ostart;
}

/* Decode all frames; skippable frames only add their variant to info.
 * Returns the total decoded size or the first error. */
static size_t ZSTD_decompressMultiFrame(const BYTE *src, size_t srcSize, U64 *info)
{
    size_t op = 0;
    while (srcSize >= 5) {
        U32 const magicNumber = MEM_readLE32(src);
        if ((magicNumber & ZSTD_MAGIC_SKIPPABLE_MASK) == ZSTD_MAGIC_SKIPPABLE_START) {
            size_t const skippableSize = readSkippableFrameSize(src, srcSize);
            if (ZSTD_isError(skippableSize)) return skippableSize;
            *info = (*info ^ (magicNumber - ZSTD_MAGIC_SKIPPABLE_START)) * 0x100000001B3ULL;
            src += skippableSize;
            srcSize -= skippableSize;
            continue;
        }
        {
            size_t const res = ZSTD_decompressFrame(&op, &src, &srcSize, info);
            if (ZSTD_isError(res)) return res;
        }
    }
    return srcSize == 0 ? op : ERR_srcSize_wrong;
}

/* Content is a list of segments; a run segment becomes an RLE block and
 * any other segment a raw block. */
static size_t write_frame(BYTE *op, size_t cap, const frameParams_t *params, U32 *x, size_t *total)
{
    BYTE content[128];
    size_t size = 0, pos, seg, nbSegments, starts[4];
    U32 runs[4];
    *x = *x * 1103515245U + 12345U;
    nbSegments = 1 + (*x >> 16) % 3;
    for (seg = 0; seg < nbSegments; seg++) {
        size_t len, i;
        *x = *x * 1103515245U + 12345U;
        runs[seg] = (*x >> 13) % 3 == 0;
        len = 3 + (*x >> 17) % (runs[seg] ? 40 : 18);
        starts[seg] = size;
        for (i = 0; i < len; i++)
            content[size + i] = (BYTE)(runs[seg] ? 'a' + (*x >> 25) % 8 : 'A' + ((*x >> 9) + i * 7) % 26);
        size += len;
    }
    pos = ZSTD_writeFrameHeader(op, cap, params, params->contentSizeFlag ? size : ZSTD_CONTENTSIZE_UNKNOWN);
    if (ZSTD_isError(pos)) return pos;
    for (seg = 0; seg < nbSegments; seg++) {
        size_t const end = seg + 1 < nbSegments ? starts[seg + 1] : size;
        size_t const written = ZSTD_writeBlock(op + pos, cap - pos, content + starts[seg], end - starts[seg],
                                               runs[seg], seg + 1 == nbSegments);
        if (ZSTD_isError(written)) return written;
        pos += written;
    }
    if (params->checksumFlag) {
        if (cap - pos < 4) return ERR_dstSize_tooSmall;
        MEM_writeLE32(op + pos, content_checksum(content, size));
        pos += 4;
    }
    *total += size;
    return pos;
}

unsigned long long evaluate(unsigned seed)
{
    BYTE stream[STREAM_CAPACITY];
    BYTE payload[12];
    U32 x = seed * 2654435761U + 0x9E3779B9U;
    size_t pos = 0, total = 0, frame, nbFrames = 2 + seed % 3, decoded, i;
    U64 info = 0xCBF29CE484222325ULL;
    for (i = 0; i < sizeof(payload); i++)
        payload[i] = (BYTE)(seed * 31 + i);
    for (frame = 0; frame < nbFrames; frame++) {
        frameParams_t params;
        size_t written;
        params.contentSizeFlag = (frame + seed) % 5 > 2;
        params.checksumFlag = (frame + seed) % 2;
        params.windowLog = 10 + (seed + frame) % 4;
        params.dictID = frame == 1 ? (seed * 97) % 300 : 0;
        if (frame == seed % nbFrames) {
            written = ZSTD_writeSkippableFrame(stream + pos, STREAM_CAPACITY - pos, payload, seed % 5 * 3, seed);
            pos += ZSTD_isError(written) ? 0 : written;
        }
        written = write_frame(stream + pos, STREAM_CAPACITY - pos, &params, &x, &total);
        if (ZSTD_isError(written)) return 0xE000 + frame;
        pos += written;
    }
    decoded = ZSTD_decompressMultiFrame(stream, pos, &info);
    info = (info ^ pos ^ (U64)total << 16) * 0x100000001B3ULL;
    if (ZSTD_isError(decoded))
        return (info ^ (0 - decoded)) * 0x100000001B3ULL;
    return (info ^ content_checksum(g_regen, decoded) ^ (U64)decoded << 32) * 0x100000001B3ULL;
}

int main(void)
{
    U64 checksum = evaluate(4);
    checksum ^= evaluate(10) * 3;
    return (int)(checksum & 127);
}
