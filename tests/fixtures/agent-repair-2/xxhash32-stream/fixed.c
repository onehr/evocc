/* Derived and modified from Zstandard 1.5.7.
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * xxHash portions: Copyright (c) Yann Collet - Meta Platforms, Inc.
 * BSD license selected; see origin.md and ../LICENSE.zstd.
 */
typedef unsigned char xxh_u8;
typedef unsigned xxh_u32;
typedef unsigned long long xxh_u64;
typedef unsigned long size_t;
typedef int XXH_alignment;
typedef int XXH_errorcode;
enum { XXH_OK = 0, XXH_aligned = 0 };
typedef unsigned XXH32_hash_t;
typedef struct {
    unsigned total_len_32;
    unsigned large_len;
    unsigned v[4];
    unsigned mem32[4];
    unsigned memsize;
} XXH32_state_t;
static void copy_bytes(void *dst, const void *src, size_t size)
{
    unsigned char *d = dst;
    const unsigned char *s = src;
    size_t i;
    for (i = 0; i < size; ++i)
        d[i] = s[i];
}
static void clear_bytes(void *dst, int value, size_t size)
{
    unsigned char *d = dst;
    size_t i;
    for (i = 0; i < size; ++i)
        d[i] = (unsigned char)value;
}
static unsigned XXH_readLE32(const void *src)
{
    const unsigned char *p = src;
    return (unsigned)p[0] | (unsigned)p[1] << 8 |
           (unsigned)p[2] << 16 | (unsigned)p[3] << 24;
}
static unsigned XXH_rotl32(unsigned value, unsigned count)
{
    count &= 31;
    if (count == 0)
        return value;
    return (value << count) | (value >> (32 - count));
}
static xxh_u32 XXH32_round(xxh_u32 acc, xxh_u32 input)
{
    acc += input * 0x85EBCA77U;
    acc  = XXH_rotl32(acc, 13);
    acc *= 0x9E3779B1U;
    return acc;
}
static xxh_u32 XXH32_avalanche(xxh_u32 hash)
{
    hash ^= hash >> 15;
    hash *= 0x85EBCA77U;
    hash ^= hash >> 13;
    hash *= 0xC2B2AE3DU;
    hash ^= hash >> 16;
    return hash;
}
static unsigned XXH32_finalize(unsigned hash, const unsigned char *ptr, size_t len, XXH_alignment align)
{
    (void)align;
    len &= 15;
    while (len >= 4) {
        hash += XXH_readLE32(ptr) * 0xC2B2AE3DU;
        ptr += 4;
        hash = XXH_rotl32(hash, 17) * 0x27D4EB2FU;
        len -= 4;
    }
    while (len > 0) {
        hash += *ptr++ * 0x165667B1U;
        hash = XXH_rotl32(hash, 11) * 0x9E3779B1U;
        --len;
    }
    return XXH32_avalanche(hash);
}
XXH_errorcode XXH32_reset(XXH32_state_t* statePtr, XXH32_hash_t seed)
{
    clear_bytes(statePtr, 0, sizeof(*statePtr));
    statePtr->v[0] = seed + 0x9E3779B1U + 0x85EBCA77U;
    statePtr->v[1] = seed + 0x85EBCA77U;
    statePtr->v[2] = seed + 0;
    statePtr->v[3] = seed - 0x9E3779B1U;
    return XXH_OK;
}
XXH_errorcode
XXH32_update(XXH32_state_t* state, const void* input, size_t len)
{
    if (input==0) {
        return XXH_OK;
    }
    {   const xxh_u8* p = (const xxh_u8*)input;
        const xxh_u8* const bEnd = p + len;
        state->total_len_32 += (XXH32_hash_t)len;
        state->large_len |= (XXH32_hash_t)((len>=16) | (state->total_len_32>=16));
        if (state->memsize + len < 16)  {
            copy_bytes((xxh_u8*)(state->mem32) + state->memsize, input, len);
            state->memsize += (XXH32_hash_t)len;
            return XXH_OK;
        }
        if (state->memsize) {
            copy_bytes((xxh_u8*)(state->mem32) + state->memsize, input, 16-state->memsize);
            {   const xxh_u32* p32 = state->mem32;
                state->v[0] = XXH32_round(state->v[0], XXH_readLE32(p32)); p32++;
                state->v[1] = XXH32_round(state->v[1], XXH_readLE32(p32)); p32++;
                state->v[2] = XXH32_round(state->v[2], XXH_readLE32(p32)); p32++;
                state->v[3] = XXH32_round(state->v[3], XXH_readLE32(p32));
            }
            p += 16-state->memsize;
            state->memsize = 0;
        }
        if ((size_t)(bEnd-p) >= 16) {
            const xxh_u8* const limit = bEnd - 16;
            do {
                state->v[0] = XXH32_round(state->v[0], XXH_readLE32(p)); p+=4;
                state->v[1] = XXH32_round(state->v[1], XXH_readLE32(p)); p+=4;
                state->v[2] = XXH32_round(state->v[2], XXH_readLE32(p)); p+=4;
                state->v[3] = XXH32_round(state->v[3], XXH_readLE32(p)); p+=4;
            } while (p<=limit);
        }
        if (p < bEnd) {
            copy_bytes(state->mem32, p, (size_t)(bEnd-p));
            state->memsize = (unsigned)(bEnd-p);
        }
    }
    return XXH_OK;
}
XXH32_hash_t XXH32_digest(const XXH32_state_t* state)
{
    xxh_u32 h32;
    if (state->large_len) {
        h32 = XXH_rotl32(state->v[0], 1)
            + XXH_rotl32(state->v[1], 7)
            + XXH_rotl32(state->v[2], 12)
            + XXH_rotl32(state->v[3], 18);
    } else {
        h32 = state->v[2]  + 0x165667B1U;
    }
    h32 += state->total_len_32;
    return XXH32_finalize(h32, (const xxh_u8*)state->mem32, state->memsize, XXH_aligned);
}
static unsigned stream_hash(const unsigned char *data, unsigned size, unsigned chunk, unsigned seed)
{
    XXH32_state_t state;
    unsigned position = 0;
    XXH32_reset(&state, seed);
    while (position < size) {
        unsigned length = size - position;
        if (length > chunk)
            length = chunk;
        XXH32_update(&state, data + position, length);
        position += length;
    }
    return XXH32_digest(&state);
}
unsigned long long evaluate(unsigned seed)
{
    unsigned char data[160];
    XXH32_state_t probe;
    unsigned size = 13 + seed * 11;
    unsigned i;
    unsigned a;
    unsigned b;
    unsigned mixed;
    for (i = 0; i < size; ++i)
        data[i] = (unsigned char)(i * 23U + seed * 41U);
    XXH32_reset(&probe, seed);
    XXH32_update(&probe, seed == 11 ? 0 : data, seed + 1);
    a = stream_hash(data, size, 1 + seed % 9, seed * 17U);
    b = stream_hash(data, size, size, seed * 17U);
    mixed = XXH_rotl32(a, seed == 11 ? 0 : seed + 1);
    return (unsigned long long)mixed ^ b ^ size ^ XXH32_digest(&probe);
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
