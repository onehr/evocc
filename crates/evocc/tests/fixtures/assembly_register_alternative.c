static volatile unsigned calls;

static unsigned long next(unsigned long n) { calls++; return n; }

static unsigned long mask(unsigned long index, unsigned long size) {
    unsigned long result;
    __asm__ volatile("cmp %1, %2\n\tsbc %0, xzr, xzr"
                     : "=r"(result) : "r"(index), "Ir"(size) : "cc");
    return result;
}

static unsigned compare32(unsigned index, unsigned size) {
    unsigned result;
    __asm__ volatile("cmp %w1, %w2\n\tcset %w0, lo"
                     : "=r"(result) : "r"(index), "Ir"(size) : "cc");
    return result;
}

#ifndef ORDINARY_ASSEMBLY_ONLY
static int branch(unsigned size) {
    int result = 3;
    __asm__ goto volatile("cmp %w[out], %w[bound]\n\tb.ne %l[taken]\n\tadd %w[out], %w[out], #2"
                          : [out] "+r"(result) : [bound] "Ir"(size)
                          : "cc", "memory" : taken);
    return result != 5;
taken:
    return result != 3;
}

static int output_branch(unsigned size) {
    unsigned result;
    __asm__ goto volatile("mov %w[out], #7\n\tcmp %w[out], %w[bound]\n\tb.ne %l[b0]\n\tadd %w[out], %w[out], #2"
                          : [out] "=&r"(result) : [bound] "Ir"(size)
                          : "cc" : b0);
    return result != 9;
b0:
    return result != 7;
}
#endif

int main(void) {
    if (mask(0, 0) != 0 || mask(0, 1) != ~0UL) return 1;
    if (mask(4094, 4095) != ~0UL || mask(4095, 4095) != 0) return 2;
    if (mask(4095, 4096) != ~0UL || mask(4096, 4097) != ~0UL) return 3;
    if (mask(3, 0x12345678UL) != ~0UL || mask(3, ~0UL) != ~0UL) return 4;
    if (mask(3, next(9)) != ~0UL || calls != 1) return 5;
    if (compare32(3, 9) != 1 || compare32(9, 3) != 0) return 6;
    if (compare32(3, ~0U) != 1 || compare32(~0U, 3) != 0) return 7;
#ifndef ORDINARY_ASSEMBLY_ONLY
    if (branch(3) || branch(1)) return 8;
    if (output_branch(7) || output_branch(1)) return 9;
#endif
    return 0;
}
