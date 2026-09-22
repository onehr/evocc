#ifdef TEST_PRIVATE
typedef __Uint8x8_t uint8x8_t;
typedef __Uint8x16_t uint8x16_t;
typedef __Int16x4_t int16x4_t;
typedef __Int16x8_t int16x8_t;
typedef __Uint32x2_t uint32x2_t;
typedef __Uint32x4_t uint32x4_t;
typedef __Uint64x1_t uint64x1_t;
typedef __Uint64x2_t uint64x2_t;
typedef __Poly16x4_t poly16x4_t;
typedef __Poly16x8_t poly16x8_t;
#define vcombine_u8 __builtin_aarch64_combinev8qi_uuu
#define vcombine_s16 __builtin_aarch64_combinev4hi
#define vcombine_u32 __builtin_aarch64_combinev2si_uuu
#define vcombine_u64(a,b) __builtin_aarch64_combinedi_uuu((a)[0],(b)[0])
#define vcombine_p16 __builtin_aarch64_combinev4hi_ppp
#else
#include <arm_neon.h>
#endif

static int evaluations;
static uint32x2_t next(void) { ++evaluations; return (uint32x2_t){3, 4}; }

int main(void) {
    uint8x16_t a = vcombine_u8((uint8x8_t){1, 2, 3, 4, 5, 6, 7, 8},
                              (uint8x8_t){9, 10, 11, 12, 13, 14, 15, 16});
    for (int i = 0; i < 16; ++i) if (a[i] != i + 1) return 1;
    int16x8_t b = vcombine_s16((int16x4_t){-1, -2, -3, -4},
                              (int16x4_t){5, 6, 7, 8});
    if (b[0] != -1 || b[3] != -4 || b[4] != 5 || b[7] != 8) return 2;
    uint32x4_t c = vcombine_u32((uint32x2_t){1, 2}, next());
    if (c[0] != 1 || c[1] != 2 || c[2] != 3 || c[3] != 4 || evaluations != 1) return 3;
    uint64x2_t d = vcombine_u64((uint64x1_t){~0UL}, (uint64x1_t){0x123456789UL});
    if (d[0] != ~0UL || d[1] != 0x123456789UL) return 4;
    poly16x8_t e = vcombine_p16((poly16x4_t){1, 2, 3, 4}, (poly16x4_t){5, 6, 7, 8});
    if (e[0] != 1 || e[7] != 8) return 5;
    return 0;
}
