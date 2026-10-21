/* L03: translation-time half values and the binary32 value of half
 * operations. Each expected representation is the one that GCC 14 gives on
 * aarch64 Linux at -O0 and -O2. main returns 0 when every check agrees, or
 * the number of the first check that differs.
 *
 * A half constant and a half operation have a binary32 value. A static
 * object of a half type rounds that value once. A conversion of a half
 * expression to an integer truncates the binary32 value. A conditional,
 * comma or unary plus expression keeps the binary32 value of the selected
 * operand; a statement expression rounds it to its half type.
 *
 * The bf16 constant suffix and the __BFLT16_* macros are Linux only: GCC 14
 * has them, and Apple clang has neither, so on Darwin they are E0112 and
 * undefined. Their checks are built for Linux alone. */
#include <string.h>

static unsigned h16(_Float16 value)
{
    unsigned short bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

static unsigned b16(__bf16 value)
{
    unsigned short bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

static unsigned f32(float value)
{
    unsigned bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

static const _Float16 s1 = 2048.0f16 + 1.0f16;
static const float s2 = 2048.0f16 + 1.0f16;
static const float s3 = 0.1f16;
static const _Float16 s4 = 0.1f16;
static const _Float16 s5 = 1.000488282181322574615478515625f16;
static const double s6 = (_Float16)2048 + (_Float16)1;
static const _Float16 s7 = (_Float16)2048 + 1.0;
#ifdef __linux__
static const __bf16 b1 = 256.0bf16 + 1.0bf16;
static const float b2 = 256.0bf16 + 1.0bf16;
static const float b3 = 0.1bf16;
#endif
static const __bf16 b4 = 0.1;
static const __bf16 b5 = 0.1f;
static const _Float16 h6 = 0.1;
static const _Float16 h7 = 0.1f;
static const _Float16 h8 = 65520;
static const _Float16 h9 = 1e10;
static const __bf16 b9 = 1e300;
static const _Float16 h10 = -0.0;
static const _Float16 h11 = 1e-10;
static const _Float16 h12 = 0x1p-25;
static const _Float16 h13 = 0x1.8p-25;
static const _Float16 h14 = __builtin_nan("");
static const __bf16 b14 = __builtin_nan("0x12345");
static const _Float16 h15 = __builtin_inf();
static const __fp16 p1 = 2048.0f16 + 1.0f16;
static const __fp16 p2 = 65520.0f;
_Float16 g = 1.0;
static const _Float16 m1[] = {__FLT16_MAX__, __FLT16_MIN__, __FLT16_EPSILON__,
                               __FLT16_DENORM_MIN__, __FLT16_NORM_MAX__};
#ifdef __linux__
static const __bf16 m2[] = {__BFLT16_MAX__, __BFLT16_MIN__, __BFLT16_EPSILON__,
                            __BFLT16_DENORM_MIN__, __BFLT16_NORM_MAX__};
static char a3[(int)257.0bf16];
#endif
static char a2[(int)2049.0f16];
static char a8[(int)2049.5f16];

volatile _Float16 a = 2048, c = 1;
volatile __bf16 p = 256, q = 1;
volatile __fp16 r = 2048, s = 1;
volatile int t = 1, z = 0;
static const float k1 = 1 ? (_Float16)1 : 2049, k2 = 0 ? (_Float16)1 : 2049;
static const float k3 = 0 ? (__bf16)1 : 257, k4 = 0 ? (_Float16)1 : (_Float16)2049;

#define CHECK(number, condition)                                               \
    do {                                                                       \
        if (!(condition))                                                      \
            return number;                                                     \
    } while (0)

int main(void)
{
    CHECK(1, h16(s1) == 0x6800);
    CHECK(2, f32(s2) == 0x45001000);
    CHECK(3, f32(s3) == 0x3dcccccd);
    CHECK(4, h16(s4) == 0x2e66);
    CHECK(5, h16(s5) == 0x3c00);
    CHECK(6, s6 == 2049.0);
    CHECK(7, h16(s7) == 0x6800);
#ifdef __linux__
    CHECK(8, b16(b1) == 0x4380);
    CHECK(9, f32(b2) == 0x43808000);
    CHECK(10, f32(b3) == 0x3dcccccd);
#endif
    CHECK(11, b16(b4) == 0x3dcd);
    CHECK(12, b16(b5) == 0x3dcd);
    CHECK(13, h16(h6) == 0x2e66);
    CHECK(14, h16(h7) == 0x2e66);
    CHECK(15, h16(h8) == 0x7c00);
    CHECK(16, h16(h9) == 0x7c00);
    CHECK(17, b16(b9) == 0x7f80);
    CHECK(18, h16(h10) == 0x8000);
    CHECK(19, h16(h11) == 0x0000);
    CHECK(20, h16(h12) == 0x0000);
    CHECK(21, h16(h13) == 0x0001);
    CHECK(22, h16(h14) == 0x7e00);
    CHECK(23, b16(b14) == 0x7fc0);
    CHECK(24, h16(h15) == 0x7c00);
    CHECK(25, h16(p1) == 0x6800);
    CHECK(26, h16(p2) == 0x7c00);
    CHECK(27, h16(g) == 0x3c00);
    CHECK(28, sizeof a2 == 2049 && sizeof a8 == 2049);
    CHECK(29, sizeof(1.0f16) == 2 && sizeof(1.0F16) == 2);
#ifdef __linux__
    CHECK(55, sizeof a3 == 257 && sizeof(1.0bf16) == 2 && sizeof(1.0BF16) == 2);
#endif
    CHECK(30, _Generic((_Float16)1 + (__fp16)1, float: 1, default: 0)
                  && _Generic((__bf16)1 + (__fp16)1, float: 1, default: 0)
                  && _Generic((__bf16)1 + 1, __bf16: 1, default: 0)
                  && _Generic((__bf16)1 + 1.0, double: 1, default: 0)
                  && _Generic((_Float16)1 * 1.0f, float: 1, default: 0)
                  && _Generic((__fp16)1 + (__fp16)1, float: 1, default: 0));
    CHECK(31, f32(({ a + c; })) == 0x45000000);
    CHECK(32, f32((0, a + c)) == 0x45001000);
    CHECK(33, f32(t ? a + c : a) == 0x45001000);
    CHECK(34, f32(t ? a : a + c) == 0x45000000);
    CHECK(35, f32(!t ? a : a + c) == 0x45001000);
    CHECK(36, f32((a + c) ?: a) == 0x45001000);
    CHECK(37, f32(({ p + q; })) == 0x43800000);
    CHECK(38, f32(t ? p + q : p) == 0x43808000);
    CHECK(39, f32((p + q) ?: p) == 0x43808000);
    CHECK(40, f32(t ? r + s : r) == 0x45001000);
    CHECK(41, f32(t ? r : s) == 0x45000000);
    CHECK(42, f32(t ? (t ? a + c : a) : a) == 0x45001000);
    CHECK(43, f32(t ? +(a + c) : a) == 0x45001000);
    CHECK(44, f32(t ? -a - c : a) == 0xc5001000);
    CHECK(45, f32(t ? 2049.0f16 : a) == 0x45001000);
    CHECK(46, f32(t ? a : 2049.0f16) == 0x45000000);
    CHECK(47, sizeof(t ? a + c : a) == 2 && sizeof(t ? r : s) == 4
                  && sizeof((0, a + c)) == 2 && sizeof(({ a + c; })) == 2);
    CHECK(48, (int)(a + c + 0.5f16) == 2049 && (int)(p + q) == 257);
    CHECK(49, h16(m1[0]) == 0x7bff && h16(m1[1]) == 0x0400 && h16(m1[2]) == 0x1400
                  && h16(m1[3]) == 0x0001 && h16(m1[4]) == 0x7bff);
#ifdef __linux__
    CHECK(50, b16(m2[0]) == 0x7f7f && b16(m2[1]) == 0x0080 && b16(m2[2]) == 0x3c00
                  && b16(m2[3]) == 0x0001 && b16(m2[4]) == 0x7f7f && __BFLT16_MANT_DIG__ == 8);
#endif
    CHECK(51, __FLT16_MANT_DIG__ == 11 && __FLT_EVAL_METHOD__ == 0
                  && __FLT_EVAL_METHOD_TS_18661_3__ == 0);
    /* A conditional of half type evaluates in binary32: an integer arm
     * converts to binary32 directly, at run time and in a constant. */
    CHECK(52, f32(z ? a : 2049) == 0x45001000 && f32(t ? 2049 : a) == 0x45001000
                  && f32(z ? p : 257) == 0x43808000 && f32(z ? r : 2049) == 0x45001000
                  && f32(z ? a : (_Float16)2049) == 0x45000000 && f32(z ? p : 257u) == 0x43808000);
    CHECK(53, f32(t ? (z ? a : 2049) : a) == 0x45001000 && f32(z ? a : (t ? 2049 : a)) == 0x45001000);
    CHECK(54, f32(k1) == 0x3f800000 && f32(k2) == 0x45001000 && f32(k3) == 0x43808000
                  && f32(k4) == 0x45000000);
    return 0;
}
