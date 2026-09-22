#ifdef TEST_PRIVATE
typedef __Int8x8_t int8x8_t;
typedef __Int8x16_t int8x16_t;
typedef __Int16x4_t int16x4_t;
typedef __Int16x8_t int16x8_t;
typedef __Int32x2_t int32x2_t;
typedef __Int32x4_t int32x4_t;
typedef __Poly64x1_t poly64x1_t;
typedef __Poly64x2_t poly64x2_t;
#define vget_low_s8 __builtin_aarch64_get_lowv16qi
#define vget_high_s8 __builtin_aarch64_get_highv16qi
#define vget_low_s16 __builtin_aarch64_get_lowv8hi
#define vget_high_s16 __builtin_aarch64_get_highv8hi
#define vget_low_s32 __builtin_aarch64_get_lowv4si
#define vget_high_s32 __builtin_aarch64_get_highv4si
#define vget_low_p64(x) ((poly64x1_t){__builtin_aarch64_get_lowv2di((__Int64x2_t)(x))})
#define vget_high_p64(x) ((poly64x1_t){__builtin_aarch64_get_highv2di((__Int64x2_t)(x))})
#else
#include <arm_neon.h>
#endif
static int calls;
static int8x16_t once(int8x16_t x) { ++calls; return x; }
int main(void) {
    int8x16_t a;
    for(int i=0;i<16;++i) a[i]=i*17;
    int8x8_t al=vget_low_s8(once(a)), ah=vget_high_s8(once(a));
    for(int i=0;i<8;++i) if(al[i]!=a[i] || ah[i]!=a[i+8]) return 1;
    if(calls!=2) return 2;
    int16x8_t b={-32768,-17,-1,0,1,17,999,32767};
    int16x4_t bl=vget_low_s16(b), bh=vget_high_s16(b);
    for(int i=0;i<4;++i) if(bl[i]!=b[i] || bh[i]!=b[i+4]) return 3;
    int32x4_t c={-2147483647-1,-1,17,2147483647};
    int32x2_t cl=vget_low_s32(c), ch=vget_high_s32(c);
    for(int i=0;i<2;++i) if(cl[i]!=c[i] || ch[i]!=c[i+2]) return 4;
    poly64x2_t d={0x123456789abcdef0UL,~0UL};
    poly64x1_t dl=vget_low_p64(d), dh=vget_high_p64(d);
    return dl[0]!=d[0] || dh[0]!=d[1];
}
