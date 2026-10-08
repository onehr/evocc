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
typedef unsigned long long XXH64_hash_t;
typedef struct {
    unsigned long long total_len;
    unsigned large_len;
    unsigned long long v[4];
    unsigned long long mem64[4];
    unsigned memsize;
} XXH64_state_t;
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
static unsigned long long XXH_rotl64(unsigned long long value, unsigned count)
{
    count &= 63;
    if (count == 0)
        return value;
    return (value << count) | (value >> (64 - count));
}
static unsigned long long XXH_readLE64(const void *src)
{
    const unsigned char *p = src;
    return (unsigned long long)XXH_readLE32(p) |
           (unsigned long long)XXH_readLE32(p + 4) << 32;
}
static xxh_u64 XXH64_round(xxh_u64 acc, xxh_u64 input)
{
    acc += input * 0xC2B2AE3D27D4EB4FULL;
    acc  = XXH_rotl64(acc, 31);
    acc *= 0x9E3779B185EBCA87ULL;
    return acc;
}
static xxh_u64 XXH64_mergeRound(xxh_u64 acc, xxh_u64 val)
{
    val  = XXH64_round(0, val);
    acc ^= val;
    acc  = acc * 0x9E3779B185EBCA87ULL + 0x85EBCA77C2B2AE63ULL;
    return acc;
}
static xxh_u64 XXH64_avalanche(xxh_u64 hash)
{
    hash ^= hash >> 33;
    hash *= 0xC2B2AE3D27D4EB4FULL;
    hash ^= hash >> 29;
    hash *= 0x165667B19E3779F9ULL;
    hash ^= hash >> 32;
    return hash;
}
static xxh_u64
XXH64_finalize(xxh_u64 hash, const xxh_u8* ptr, size_t len, XXH_alignment align)
{
    len &= 31;
    while (len >= 8) {
        xxh_u64 const k1 = XXH64_round(0, XXH_readLE64(ptr));
        ptr += 8;
        hash ^= k1;
        hash  = XXH_rotl64(hash,27) * 0x9E3779B185EBCA87ULL + 0x85EBCA77C2B2AE63ULL;
        len -= 8;
    }
    if (len >= 4) {
        hash ^= (xxh_u64)(XXH_readLE32(ptr)) * 0x9E3779B185EBCA87ULL;
        ptr += 4;
        hash = XXH_rotl64(hash, 23) * 0xC2B2AE3D27D4EB4FULL + 0x165667B19E3779F9ULL;
        len -= 4;
    }
    while (len > 0) {
        hash ^= (*ptr++) * 0x27D4EB2F165667C5ULL;
        hash = XXH_rotl64(hash, 11) * 0x9E3779B185EBCA87ULL;
        --len;
    }
    return  XXH64_avalanche(hash);
}
XXH_errorcode XXH64_reset(XXH64_state_t* statePtr, XXH64_hash_t seed)
{
    clear_bytes(statePtr, 0, sizeof(*statePtr));
    statePtr->v[0] = seed + 0x9E3779B185EBCA87ULL + 0xC2B2AE3D27D4EB4FULL;
    statePtr->v[1] = seed + 0xC2B2AE3D27D4EB4FULL;
    statePtr->v[2] = seed + 0;
    statePtr->v[3] = seed - 0x9E3779B185EBCA87ULL;
    return XXH_OK;
}
XXH_errorcode
XXH64_update (XXH64_state_t* state, const void* input, size_t len)
{
    if (input==0) {
        return XXH_OK;
    }
    {   const xxh_u8* p = (const xxh_u8*)input;
        const xxh_u8* const bEnd = p + len;
        state->total_len += len;
        if (state->memsize + len < 32) {
            copy_bytes(((xxh_u8*)state->mem64) + state->memsize, input, len);
            state->memsize += (xxh_u32)len;
            return XXH_OK;
        }
        if (state->memsize) {
            copy_bytes(((xxh_u8*)state->mem64) + state->memsize, input, 32-state->memsize);
            state->v[0] = XXH64_round(state->v[0], XXH_readLE64(state->mem64+0));
            state->v[1] = XXH64_round(state->v[1], XXH_readLE64(state->mem64+1));
            state->v[2] = XXH64_round(state->v[2], XXH_readLE64(state->mem64+2));
            state->v[3] = XXH64_round(state->v[3], XXH_readLE64(state->mem64+3));
            p += 32 - state->memsize;
            state->memsize = 0;
        }
        if ((size_t)(bEnd-p) >= 32) {
            const xxh_u8* const limit = bEnd - 32;
            do {
                state->v[0] = XXH64_round(state->v[0], XXH_readLE64(p)); p+=8;
                state->v[1] = XXH64_round(state->v[1], XXH_readLE64(p)); p+=8;
                state->v[2] = XXH64_round(state->v[2], XXH_readLE64(p)); p+=8;
                state->v[3] = XXH64_round(state->v[3], XXH_readLE64(p)); p+=8;
            } while (p<=limit);
        }
        if (p < bEnd) {
            copy_bytes(state->mem64, p, (size_t)(bEnd-p));
            state->memsize = (unsigned)(bEnd-p);
        }
    }
    return XXH_OK;
}
XXH64_hash_t XXH64_digest(const XXH64_state_t* state)
{
    xxh_u64 h64;
    if (state->total_len >= 32) {
        h64 = XXH_rotl64(state->v[0], 1) + XXH_rotl64(state->v[1], 7) + XXH_rotl64(state->v[2], 12) + XXH_rotl64(state->v[3], 18);
        h64 = XXH64_mergeRound(h64, state->v[0]);
        h64 = XXH64_mergeRound(h64, state->v[1]);
        h64 = XXH64_mergeRound(h64, state->v[2]);
        h64 = XXH64_mergeRound(h64, state->v[3]);
    } else {
        h64  = state->v[2]  + 0x27D4EB2F165667C5ULL;
    }
    h64 += (xxh_u64) state->total_len;
    return XXH64_finalize(h64, (const xxh_u8*)state->mem64, (size_t)state->total_len, XXH_aligned);
}
static unsigned long long stream_hash(const unsigned char *data, unsigned size, unsigned chunk, unsigned long long seed)
{
    XXH64_state_t state;
    unsigned position = 0;
    XXH64_reset(&state, seed);
    while (position < size) {
        unsigned length = size - position;
        if (length > chunk)
            length = chunk;
        XXH64_update(&state, data + position, length);
        position += length;
    }
    return XXH64_digest(&state);
}
unsigned long long evaluate(unsigned seed)
{
    unsigned char data[160];
    XXH64_state_t probe;
    unsigned size = 13 + seed * 11;
    unsigned i;
    unsigned long long a;
    unsigned long long b;
    unsigned long long mixed;
    for (i = 0; i < size; ++i)
        data[i] = (unsigned char)(i * 23U + seed * 41U);
    XXH64_reset(&probe, seed);
    XXH64_update(&probe, seed == 11 ? 0 : data, seed + 1);
    a = stream_hash(data, size, 1 + seed % 9, seed * 17U);
    b = stream_hash(data, size, size, seed * 17U);
    mixed = XXH_rotl64(a, seed == 11 ? 0 : seed + 1);
    return (unsigned long long)mixed ^ b ^ size ^ XXH64_digest(&probe);
}
int main(void)
{
    unsigned long long checksum = evaluate(0);
    checksum ^= evaluate(1);
    checksum += evaluate(2);
    checksum ^= evaluate(11);
    return checksum == 0xffffffffffffffffULL;
}
