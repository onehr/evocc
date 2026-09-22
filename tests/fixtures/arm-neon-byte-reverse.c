#ifdef TEST_PRIVATE
typedef __Uint8x8_t uint8x8_t;
typedef __Uint8x16_t uint8x16_t;
#define vrbit_u8(x) ((uint8x8_t)__builtin_aarch64_rbitv8qi((__Int8x8_t)(x)))
#define vrbitq_u8(x) ((uint8x16_t)__builtin_aarch64_rbitv16qi((__Int8x16_t)(x)))
#else
#include <arm_neon.h>
#endif
#ifndef TEST_START
#define TEST_START 0
#define TEST_END 256
#endif
static int calls;
static uint8x16_t once(uint8x16_t x) { ++calls; return x; }
static unsigned char reverse(unsigned char x) {
    unsigned char out=0;
    for (int i=0; i<8; ++i) { out=(out<<1)|(x&1); x>>=1; }
    return out;
}
int main(void) {
    for (int start=TEST_START; start<TEST_END; start+=16) {
        uint8x16_t a;
        uint8x8_t b;
        for (int i=0; i<16; ++i) a[i]=start+i;
        for (int i=0; i<8; ++i) b[i]=start+2*i;
        uint8x16_t x=vrbitq_u8(once(a));
        uint8x8_t y=vrbit_u8(b);
        if (calls!=(start-TEST_START)/16+1) return 1;
        for (int i=0; i<16; ++i) if (x[i]!=reverse(start+i)) return 2;
        for (int i=0; i<8; ++i) if (y[i]!=reverse(start+2*i)) return 3;
    }
    return 0;
}
