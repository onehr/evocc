#include <arm_neon.h>

int main(void) {
    uint8x16_t a = { 128 };
    uint8x16_t b = vshrq_n_u8(a, 0x100000001ULL);
    int8x16_t c = { 1 };
    c = vshlq_n_s8(c, 0x100000007ULL);
    return b[0] != 64 || (unsigned char)c[0] != 128;
}
