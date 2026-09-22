/* The private branch exercises the same frozen prototypes without a sysroot. */
#ifdef TEST_PRIVATE
#pragma GCC aarch64 "arm_neon.h"
typedef __Uint8x8_t uint8x8_t;
typedef __Uint8x16_t uint8x16_t;
typedef __Uint16x8_t uint16x8_t;
typedef __Uint32x4_t uint32x4_t;
typedef __Uint64x2_t uint64x2_t;
typedef __Int16x8_t int16x8_t;
typedef __Int32x4_t int32x4_t;
typedef __Int64x2_t int64x2_t;
#define vshrn_n_u16 __builtin_aarch64_shrn_nv8hi_uus
#define vshrn_n_u32 __builtin_aarch64_shrn_nv4si_uus
#define vshrn_n_u64 __builtin_aarch64_shrn_nv2di_uus
#define vshrn_n_s16 __builtin_aarch64_shrn_nv8hi
#define vshrn_n_s32 __builtin_aarch64_shrn_nv4si
#define vshrn_n_s64 __builtin_aarch64_shrn_nv2di
#define vsli_n_u8 __builtin_aarch64_usli_nv8qi_uuus
#define vsriq_n_u8 __builtin_aarch64_usri_nv16qi_uuus
#define vsliq_n_s16 __builtin_aarch64_ssli_nv8hi
#define vsriq_n_u64 __builtin_aarch64_usri_nv2di_uuus
#define vld2q_u16(p) __builtin_aarch64_ld2v8hi_us((const short *)(p))
#define vld3q_u32(p) __builtin_aarch64_ld3v4si_us((const int *)(p))
#define vld4q_u8(p) __builtin_aarch64_ld4v16qi_us((const signed char *)(p))
#else
#include <arm_neon.h>
#endif

static int calls;
static uint16x8_t input(void) {
    ++calls;
    return (uint16x8_t){65535, 32768, 4095, 256, 255, 128, 1, 0};
}

static int narrow(void) {
    uint16x8_t a = input();
    uint8x8_t lo = vshrn_n_u16(input(), 1);
    uint8x8_t hi = vshrn_n_u16(a, 8);
    uint8x8_t converted = vshrn_n_u16(a, 0x100000008ULL);
    if (calls != 2) return 1;
    for (int i=0; i<8; ++i)
        if (lo[i] != (unsigned char)(a[i] >> 1) ||
            hi[i] != (unsigned char)(a[i] >> 8) || converted[i] != hi[i])
            return 2;
    uint32x4_t b = {0xffffffffU, 0x80000000U, 65536, 65535};
    uint64x2_t c = {~0UL, 0x8000000000000001UL};
    __typeof__(vshrn_n_u32(b,16)) rb = vshrn_n_u32(b,16);
    __typeof__(vshrn_n_u64(c,32)) rc = vshrn_n_u64(c,32);
    for (int i=0; i<4; ++i)
        if (rb[i] != (unsigned short)(b[i] >> 16)) return 3;
    for (int i=0; i<2; ++i)
        if (rc[i] != (unsigned int)(c[i] >> 32)) return 4;
    int16x8_t s = {-1, -32768, -129, 32767, 255, 8, 7, 0};
    int32x4_t t = {-1, (-2147483647-1), 65536, 2147483647};
    int64x2_t u = {-1, (-9223372036854775807L-1)};
    __typeof__(vshrn_n_s16(s,3)) rs = vshrn_n_s16(s,3);
    __typeof__(vshrn_n_s32(t,16)) rt = vshrn_n_s32(t,16);
    __typeof__(vshrn_n_s64(u,1)) ru = vshrn_n_s64(u,1);
    for (int i=0; i<8; ++i)
        if ((unsigned char)rs[i] != (unsigned char)((unsigned short)s[i] >> 3)) return 5;
    for (int i=0; i<4; ++i)
        if ((unsigned short)rt[i] != (unsigned short)((unsigned int)t[i] >> 16)) return 6;
    for (int i=0; i<2; ++i)
        if ((unsigned int)ru[i] != (unsigned int)((unsigned long)u[i] >> 1)) return 7;
    return 0;
}

static int insert(void) {
    uint8x8_t a = {0, 255, 170, 85, 128, 127, 1, 2};
    uint8x8_t b = {255, 0, 85, 170, 1, 2, 128, 127};
    uint8x8_t l0 = vsli_n_u8(a,b,0), l4 = vsli_n_u8(a,b,4), l7 = vsli_n_u8(a,b,7);
    uint8x16_t c = {0, 255, 170, 85, 128, 127, 1, 2};
    uint8x16_t d = {255, 0, 85, 170, 1, 2, 128, 127};
    uint8x16_t r1 = vsriq_n_u8(c,d,1), r4 = vsriq_n_u8(c,d,4), r8 = vsriq_n_u8(c,d,8);
    for (int i=0; i<8; ++i)
        if (l0[i] != b[i] || l4[i] != (unsigned char)((a[i]&15)|(b[i]<<4)) ||
            l7[i] != (unsigned char)((a[i]&127)|(b[i]<<7))) return 11;
    for (int i=0; i<16; ++i)
        if (r1[i] != ((c[i]&128)|(d[i]>>1)) ||
            r4[i] != ((c[i]&240)|(d[i]>>4)) || r8[i] != c[i]) return 12;
    int16x8_t s = {-1, -32768, 1, 3, 5, 7, 9, 11};
    int16x8_t t = {2, -1, 3, 5, 7, 9, 11, 13};
    int16x8_t l = vsliq_n_s16(s,t,15);
    for (int i=0; i<8; ++i)
        if ((unsigned short)l[i] != (unsigned short)(((unsigned short)s[i]&32767)|((unsigned short)t[i]<<15))) return 13;
    uint64x2_t x={0x8123456789abcdefUL, 3}, y={~0UL, 5};
    uint64x2_t r=vsriq_n_u64(x,y,64);
    if (r[0]!=x[0] || r[1]!=x[1]) return 14;
    return 0;
}

static int interleave(void) {
    unsigned short a[17];
    unsigned int b[12];
    unsigned char c[65];
    for (int i=0; i<17; ++i) a[i] = (unsigned short)(257*i+1);
    for (int i=0; i<12; ++i) b[i] = 0x80000000U+(unsigned)i;
    for (int i=0; i<65; ++i) c[i] = (unsigned char)(3*i+1);
    unsigned short *p=a;
    uint16x8x2_t x=vld2q_u16(++p);
    uint32x4x3_t y=vld3q_u32(b);
    uint8x16x4_t z=vld4q_u8(c+1);
    if (p != a+1) return 21;
    for (int i=0; i<8; ++i)
        for (int j=0; j<2; ++j)
            if (x.val[j][i] != a[1+2*i+j]) return 22;
    for (int i=0; i<4; ++i)
        for (int j=0; j<3; ++j)
            if (y.val[j][i] != b[3*i+j]) return 23;
    for (int i=0; i<16; ++i)
        for (int j=0; j<4; ++j)
            if (z.val[j][i] != c[1+4*i+j]) return 24;
    return 0;
}

int main(void) {
#if defined(TEST_NARROW)
    return narrow();
#elif defined(TEST_INSERT)
    return insert();
#elif defined(TEST_INTERLEAVE)
    return interleave();
#else
    int n=narrow();
    if (n) return n;
    n=insert();
    return n ? n : interleave();
#endif
}
