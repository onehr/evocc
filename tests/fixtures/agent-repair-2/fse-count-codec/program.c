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
static void copy_bytes(void *dst, const void *src, size_t size)
{
    BYTE *out = dst;
    const BYTE *in = src;
    size_t i;
    for (i = 0; i < size; ++i)
        out[i] = in[i];
}
static unsigned read32(const void *src)
{
    const BYTE *p = src;
    return (unsigned)p[0] | (unsigned)p[1] << 8 |
           (unsigned)p[2] << 16 | (unsigned)p[3] << 24;
}
static unsigned is_error(size_t value)
{
    return value == (size_t)-1;
}
static unsigned trailing_zeros(unsigned value)
{
    unsigned count = 0;
    while ((value & 1) == 0 && count < 32) {
        value >>= 1;
        ++count;
    }
    return count;
}
size_t FSE_readNCount_body(short *, unsigned *, unsigned *, const void *, size_t);
static size_t
FSE_writeNCount_generic (void* header, size_t headerBufferSize,
                   const short* normalizedCounter, unsigned maxSymbolValue, unsigned tableLog,
                         unsigned writeIsSafe)
{
    BYTE* const ostart = (BYTE*) header;
    BYTE* out = ostart;
    BYTE* const oend = ostart + headerBufferSize;
    int nbBits;
    const int tableSize = 1 << tableLog;
    int remaining;
    int threshold;
    U32 bitStream = 0;
    int bitCount = 0;
    unsigned symbol = 0;
    unsigned const alphabetSize = maxSymbolValue + 1;
    int previousIs0 = 0;
    bitStream += (tableLog-5) << bitCount;
    bitCount  += 4;
    remaining = tableSize+1;
    threshold = tableSize;
    nbBits = (int)tableLog+1;
    while ((symbol < alphabetSize) && (remaining>1)) {
        if (previousIs0) {
            unsigned start = symbol;
            while ((symbol < alphabetSize) && !normalizedCounter[symbol]) symbol++;
            if (symbol == alphabetSize) break;
            while (symbol >= start+24) {
                start+=24;
                bitStream += 0xFFFFU << bitCount;
                if ((!writeIsSafe) && (out > oend-2))
                    return ((size_t)-1);
                out[0] = (BYTE) bitStream;
                out[1] = (BYTE)(bitStream>>8);
                out+=2;
                bitStream>>=16;
            }
            while (symbol >= start+3) {
                start+=3;
                bitStream += 3U << bitCount;
                bitCount += 2;
            }
            bitStream += (symbol-start) << bitCount;
            bitCount += 2;
            if (bitCount>16) {
                if ((!writeIsSafe) && (out > oend - 2))
                    return ((size_t)-1);
                out[0] = (BYTE)bitStream;
                out[1] = (BYTE)(bitStream>>8);
                out += 2;
                bitStream >>= 16;
                bitCount -= 16;
        }   }
        {   int count = normalizedCounter[symbol++];
            int const max = (2*threshold-1) - remaining;
            remaining -= count < 0 ? -count : count;
            count++;
            if (count>=threshold)
                count += max;
            bitStream += (U32)count << bitCount;
            bitCount  += nbBits;
            bitCount  -= (count<max);
            previousIs0  = (count==1);
            if (remaining<1) return ((size_t)-1);
            while (remaining<threshold) { nbBits--; threshold>>=1; }
        }
        if (bitCount>16) {
            if ((!writeIsSafe) && (out > oend - 2))
                return ((size_t)-1);
            out[0] = (BYTE)bitStream;
            out[1] = (BYTE)(bitStream>>8);
            out += 2;
            bitStream >>= 16;
            bitCount -= 16;
    }   }
    if (remaining != 1)
        return ((size_t)-1);
    if ((!writeIsSafe) && (out > oend - 2))
        return ((size_t)-1);
    out[0] = (BYTE)bitStream;
    out[1] = (BYTE)(bitStream>>8);
    out+= (bitCount+7) /8;
    return (size_t)(out-ostart) + 1;
}
size_t FSE_readNCount_body(short* normalizedCounter, unsigned* maxSVPtr, unsigned* tableLogPtr,
                           const void* headerBuffer, size_t hbSize)
{
    const BYTE* const istart = (const BYTE*) headerBuffer;
    const BYTE* const iend = istart + hbSize;
    const BYTE* ip = istart;
    int nbBits;
    int remaining;
    int threshold;
    U32 bitStream;
    int bitCount;
    unsigned charnum = 0;
    unsigned const maxSV1 = *maxSVPtr + 1;
    int previous0 = 0;
    if (hbSize < 8) {
        char buffer[8] = {0};
        copy_bytes(buffer, headerBuffer, hbSize);
        {   size_t const countSize = FSE_readNCount_body(normalizedCounter, maxSVPtr, tableLogPtr,
                                                    buffer, sizeof(buffer));
            if (is_error(countSize)) return countSize;
            if (countSize > hbSize) return ((size_t)-1);
            return countSize;
    }   }
    clear_bytes(normalizedCounter, 0, (*maxSVPtr+1) * sizeof(normalizedCounter[0]));
    bitStream = read32(ip);
    nbBits = (bitStream & 0xF) + 5;
    if (nbBits > 12) return ((size_t)-1);
    bitStream >>= 4;
    bitCount = 4;
    *tableLogPtr = nbBits;
    remaining = (1<<nbBits)+1;
    threshold = 1<<nbBits;
    nbBits++;
    for (;;) {
        if (previous0) {
            int repeats = trailing_zeros(~bitStream | 0x80000000) >> 1;
            while (repeats >= 12) {
                charnum += 3 * 12;
                if ((ip <= iend-7)) {
                    ip += 3;
                } else {
                    bitCount -= (int)(8 * (iend - 7 - ip));
                    bitCount &= 31;
                    ip = iend - 4;
                }
                bitStream = read32(ip) >> bitCount;
                repeats = trailing_zeros(~bitStream | 0x80000000) >> 1;
            }
            charnum += 3 * repeats;
            bitStream >>= 2 * repeats;
            bitCount += 2 * repeats;
            charnum += bitStream & 3;
            bitCount += 2;
            if (charnum >= maxSV1) break;
            if ((ip <= iend-7) || (ip + (bitCount>>3) <= iend-4)) {
                ip += bitCount>>3;
                bitCount &= 7;
            } else {
                bitCount -= (int)(8 * (iend - 4 - ip));
                bitCount &= 31;
                ip = iend - 4;
            }
            bitStream = read32(ip) >> bitCount;
        }
        {
            int const max = (2*threshold-1) - remaining;
            int count;
            if ((bitStream & (threshold-1)) < (U32)max) {
                count = bitStream & (threshold-1);
                bitCount += nbBits-1;
            } else {
                count = bitStream & (2*threshold-1);
                if (count >= threshold) count -= max;
                bitCount += nbBits;
            }
            count--;
            if (count >= 0) {
                remaining -= count;
            } else {
                remaining += count;
            }
            normalizedCounter[charnum++] = (short)count;
            previous0 = !count;
            if (remaining < threshold) {
                if (remaining <= 1) break;
                nbBits = highbit(remaining) + 1;
                threshold = 1 << (nbBits - 1);
            }
            if (charnum >= maxSV1) break;
            if ((ip <= iend-7) || (ip + (bitCount>>3) <= iend-4)) {
                ip += bitCount>>3;
                bitCount &= 7;
            } else {
                bitCount -= (int)(8 * (iend - 4 - ip));
                bitCount &= 31;
                ip = iend - 4;
            }
            bitStream = read32(ip) >> bitCount;
    }   }
    if (remaining != 1) return ((size_t)-1);
    if (charnum > maxSV1) return ((size_t)-1);
    if (bitCount > 32) return ((size_t)-1);
    *maxSVPtr = charnum-1;
    ip += (bitCount+7)>>3;
    return ip-istart;
}
unsigned long long evaluate(unsigned seed)
{
    short normalized[16];
    short decoded[15];
    BYTE bytes[64];
    unsigned maximum = seed == 11 ? 15 : 3;
    unsigned tableLog = 0;
    unsigned maxDecoded = maximum;
    unsigned i;
    unsigned sum = 0;
    unsigned digest = 2166136261U;
    size_t written;
    size_t read;
    for (i = 0; i < 64; ++i)
        bytes[i] = 0;
    for (i = 0; i <= maximum; ++i) {
        normalized[i] = seed == 11 ? 2 : (short)(i == 0 ? seed + 1 : 1 + (i + seed) % 4);
        sum += (unsigned)normalized[i];
    }
    normalized[maximum] += (short)(32 - sum);
    written = FSE_writeNCount_generic(bytes, 64, normalized, maximum, 5, 0);
    read = FSE_readNCount_body(decoded, &maxDecoded, &tableLog, bytes, 64);
    for (i = 0; i <= maximum; ++i)
        digest = (digest ^ (unsigned)(decoded[i] + 2)) * 16777619U;
    for (i = 0; i < 16; ++i)
        digest = (digest ^ bytes[i]) * 16777619U;
    if (maxDecoded >= sizeof(decoded) / sizeof(decoded[0]))
        return 0xbad00000U + seed;
    return ((unsigned long long)digest << 32) ^ written ^ (read << 8) ^
           (tableLog << 16) ^ (maxDecoded << 24);
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
