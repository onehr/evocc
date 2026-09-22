typedef unsigned long V __attribute__((vector_size(16)));
extern V simd_asm_transform(V, V, int);
int main(void) {
    V a = {0x0123456789abcdefUL, 0xfedcba9876543210UL};
    V b = {0xff00ff00ff00ff00UL, 0x00ff00ff00ff00ffUL};
    for (int take = 0; take != 2; ++take) {
        V r = simd_asm_transform(a, b, take);
        for (int lane = 0; lane != 2; ++lane) {
            unsigned long want = a[lane] ^ b[lane];
            if (r[lane] != (take ? ~want : want)) return 1;
        }
    }
    return 0;
}
