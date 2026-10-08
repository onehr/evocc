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
typedef enum { trustInput, checkMaxSymbolValue } HIST_checkInput_e;
void HIST_add(unsigned* count, const void* src, size_t srcSize)
{
    const BYTE* ip = (const BYTE*)src;
    const BYTE* const end = ip + srcSize;
    unsigned local[256];
    unsigned i;
    for (i = 0; i < 256; ++i)
        local[i] = count[i];
    while (ip<end) {
        local[*ip++]++;
    }
    for (i = 0; i < 256; ++i)
        count[i] = local[i];
}
unsigned HIST_count_simple(unsigned* count, unsigned* maxSymbolValuePtr,
                           const void* src, size_t srcSize)
{
    const BYTE* ip = (const BYTE*)src;
    const BYTE* const end = ip + srcSize;
    unsigned maxSymbolValue = *maxSymbolValuePtr;
    unsigned largestCount=0;
    clear_bytes(count, 0, (maxSymbolValue+1) * sizeof(*count));
    if (srcSize==0) { *maxSymbolValuePtr = 0; return 0; }
    while (ip<end) {
        count[*ip++]++;
    }
    while (!count[maxSymbolValue]) maxSymbolValue--;
    *maxSymbolValuePtr = maxSymbolValue;
    {   U32 s;
        for (s=0; s<=maxSymbolValue; s++)
            if (count[s] > largestCount) largestCount = count[s];
    }
    return largestCount;
}
static size_t HIST_count_parallel_wksp(
                                unsigned* count, unsigned* maxSymbolValuePtr,
                                const void* source, size_t sourceSize,
                                HIST_checkInput_e check,
                                U32* const workSpace)
{
    const BYTE* ip = (const BYTE*)source;
    const BYTE* const iend = ip+sourceSize;
    size_t const countSize = (*maxSymbolValuePtr + 1) * sizeof(*count);
    unsigned max=0;
    U32* const Counting1 = workSpace;
    U32* const Counting2 = Counting1 + 256;
    U32* const Counting3 = Counting2 + 256;
    U32* const Counting4 = Counting3 + 256;
    if (!sourceSize) {
        clear_bytes(count, 0, countSize);
        *maxSymbolValuePtr = 0;
        return 0;
    }
    clear_bytes(workSpace, 0, 4*256*sizeof(unsigned));
    {   U32 cached = read32(ip); ip += 4;
        while (ip < iend-15) {
            U32 c = cached; cached = read32(ip); ip += 4;
            Counting1[(BYTE) c     ]++;
            Counting2[(BYTE)(c>>8) ]++;
            Counting3[(BYTE)(c>>16)]++;
            Counting4[       c>>24 ]++;
            c = cached; cached = read32(ip); ip += 4;
            Counting1[(BYTE) c     ]++;
            Counting2[(BYTE)(c>>8) ]++;
            Counting3[(BYTE)(c>>16)]++;
            Counting4[       c>>24 ]++;
            c = cached; cached = read32(ip); ip += 4;
            Counting1[(BYTE) c     ]++;
            Counting2[(BYTE)(c>>8) ]++;
            Counting3[(BYTE)(c>>16)]++;
            Counting4[       c>>24 ]++;
            c = cached; cached = read32(ip); ip += 4;
            Counting1[(BYTE) c     ]++;
            Counting2[(BYTE)(c>>8) ]++;
            Counting3[(BYTE)(c>>16)]++;
            Counting4[       c>>24 ]++;
        }
        ip-=4;
    }
    while (ip<iend) Counting1[*ip++]++;
    {   U32 s;
        for (s=0; s<256; s++) {
            Counting1[s] += Counting2[s] + Counting3[s] + Counting4[s];
            if (Counting1[s] > max) max = Counting1[s];
    }   }
    {   unsigned maxSymbolValue = 255;
        while (!Counting1[maxSymbolValue]) maxSymbolValue--;
        if (check && maxSymbolValue > *maxSymbolValuePtr) return ((size_t)-1);
        *maxSymbolValuePtr = maxSymbolValue;
        copy_bytes(count, Counting1, countSize);
    }
    return (size_t)max;
}
static unsigned count_pipeline(unsigned *count, const BYTE *data, unsigned length)
{
    unsigned maxSymbol = 255;
    unsigned workspace[1024];
    unsigned result;
    if (length < 20)
        result = HIST_count_simple(count, &maxSymbol, data, length);
    else
        result = (unsigned)HIST_count_parallel_wksp(count, &maxSymbol, data,
                  length, checkMaxSymbolValue, workspace);
    HIST_add(count, data, length);
    return result + maxSymbol;
}
static unsigned fingerprint(const unsigned *count)
{
    unsigned i;
    unsigned value = 2166136261U;
    for (i = 0; i < 256; ++i)
        value = (value ^ count[i]) * 16777619U;
    return value;
}
unsigned long long evaluate(unsigned seed)
{
    BYTE data[128];
    unsigned count[256];
    unsigned reversed[256];
    unsigned length = 17 + seed * 9;
    unsigned i;
    unsigned peak;
    unsigned other;
    for (i = 0; i < length; ++i)
        data[i] = (BYTE)((i * 13 + seed * 7) & 127);
    if (seed == 11)
        data[length - 1] = 255;
    for (i = 0; i < 256; ++i) {
        count[i] = 0;
        reversed[i] = 0;
    }
    peak = count_pipeline(count, data, length);
    for (i = 0; i < length / 2; ++i) {
        BYTE tmp = data[i];
        data[i] = data[length - i - 1];
        data[length - i - 1] = tmp;
    }
    other = count_pipeline(reversed, data, length);
    return ((unsigned long long)fingerprint(count) << 32) ^
           fingerprint(reversed) ^ peak ^ (other << 16);
}
int main(void)
{
    unsigned counts[256];
    BYTE ordinary[4] = {1, 3, 7, 15};
    BYTE extended[4] = {1, 3, 7, 255};
    unsigned i;
    for (i = 0; i < 256; ++i)
        counts[i] = 0;
    HIST_add(counts, ordinary, 2);
    HIST_add(counts, ordinary, 4);
    HIST_add(counts, extended, 4);
    return counts[255] != 1;
}
