typedef unsigned long vector64x2 __attribute__((vector_size(16)));

int main(void) {
    vector64x2 r = { 0x0123456789abcdefUL, 0xfedcba9876543210UL };
    __asm__ volatile ("" : "+w" (r));
    return r[0] != 0x0123456789abcdefUL || r[1] != 0xfedcba9876543210UL;
}
