#include <arm_neon.h>

static unsigned char carryless(unsigned char a, unsigned char b) {
    unsigned int result = 0;
    for (unsigned int bit = 0; bit < 8; ++bit)
        if (b & (1u << bit)) result ^= (unsigned int)a << bit;
    return (unsigned char)result;
}

int main(void) {
    unsigned char a[16], b[16], result[16];
    for (unsigned int x = 0; x < 256; ++x) {
        for (unsigned int y = 0; y < 256; y += 16) {
            for (unsigned int lane = 0; lane < 16; ++lane) {
                a[lane] = (unsigned char)x;
                b[lane] = (unsigned char)(y + lane);
            }
            poly8x16_t av = vreinterpretq_p8_u8(vld1q_u8(a));
            poly8x16_t bv = vreinterpretq_p8_u8(vld1q_u8(b));
            vst1q_u8(result, vreinterpretq_u8_p8(vmulq_p8(av, bv)));
            for (unsigned int lane = 0; lane < 16; ++lane)
                if (result[lane] != carryless(a[lane], b[lane])) return 1;
        }
    }
    return 0;
}
