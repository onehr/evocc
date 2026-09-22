#include <arm_neon.h>

static int evaluations;
static unsigned char bytes[34];

static const unsigned char *input(void) {
    ++evaluations;
    return bytes + 1;
}

int main(void) {
    for (int i = 0; i != 34; ++i) bytes[i] = (unsigned char)i;
    uint8x16_t a = vld1q_u8(input());
    unsigned char out[18] = { 0 };
    vst1q_u8(out + 1, a);
    if (evaluations != 1 || out[0] != 0 || out[17] != 0) return 1;
    for (int i = 0; i != 16; ++i)
        if (out[i + 1] != i + 1) return 2;

    unsigned long left[2] = { 0x123456789abcdef0UL, 0xfedcba9876543210UL };
    unsigned long right[2] = { 0xff00ff00ff00ff00UL, 0x00ff00ff00ff00ffUL };
    unsigned long result[2];
    vst1q_u64(result, veorq_u64(vld1q_u64(left), vld1q_u64(right)));
    if (result[0] != (left[0] ^ right[0]) || result[1] != (left[1] ^ right[1]))
        return 3;

    const unsigned char indices[16] = {
        0, 15, 16, 255, 3, 2, 1, 0, 7, 8, 9, 10, 11, 12, 13, 14
    };
    uint8x16_t selected = vqtbl1q_u8(a, vld1q_u8(indices));
    vst1q_u8(out, selected);
    for (int i = 0; i != 16; ++i)
        if (out[i] != (indices[i] < 16 ? indices[i] + 1 : 0)) return 4;

    vst1q_u8(out, vandq_u8(vshrq_n_u8(a, 4), vdupq_n_u8(15)));
    for (int i = 0; i != 16; ++i)
        if (out[i] != ((i + 1) >> 4)) return 5;
    vst1q_u8(out, vshrq_n_u8(a, 8));
    for (int i = 0; i != 16; ++i)
        if (out[i] != 0) return 6;

    const short first[8] = { 0, 1, 2, 3, 4, 5, 6, 7 };
    const short second[8] = { 7, 6, 5, 4, 3, 2, 1, 0 };
    int16x8x2_t zipped = vzipq_s16(vld1q_s16(first), vld1q_s16(second));
    int16x8x2_t unzipped = vuzpq_s16(zipped.val[0], zipped.val[1]);
    short lanes[8];
    vst1q_s16(lanes, unzipped.val[0]);
    for (int i = 0; i != 8; ++i)
        if (lanes[i] != first[i]) return 7;
    vst1q_s16(lanes, unzipped.val[1]);
    for (int i = 0; i != 8; ++i)
        if (lanes[i] != second[i]) return 8;
    return 0;
}
