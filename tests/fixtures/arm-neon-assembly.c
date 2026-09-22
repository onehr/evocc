#include <arm_neon.h>

static uint64x2_t xor_vectors(uint64x2_t a, uint64x2_t b) {
    uint64x2_t result;
    __asm__("eor %0.16b, %1.16b, %2.16b"
            : "=w"(result) : "w"(a), "w"(b));
    return result;
}

int main(void) {
    uint64x2_t a = { 0x0123456789abcdefUL, 0xfedcba9876543210UL };
    uint64x2_t b = { 0xff00ff00ff00ff00UL, 0x00ff00ff00ff00ffUL };
    uint64x2_t result = xor_vectors(a, b);
    return result[0] != (a[0] ^ b[0]) || result[1] != (a[1] ^ b[1]);
}
