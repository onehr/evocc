static int inputs, places;
static double values[2];
static double input(double a) { inputs++; return a; }

__attribute__((noinline)) void scalar_asm_clobber(void) {
    __asm__ volatile (
        "movi v0.16b, #0\n\tmovi v1.16b, #0\n\tmovi v2.16b, #0\n\tmovi v3.16b, #0\n\t"
        "movi v4.16b, #0\n\tmovi v5.16b, #0\n\tmovi v6.16b, #0\n\tmovi v7.16b, #0\n\t"
        "movi v16.16b, #0\n\tmovi v17.16b, #0\n\tmovi v18.16b, #0\n\tmovi v19.16b, #0\n\t"
        "movi v20.16b, #0\n\tmovi v21.16b, #0\n\tmovi v22.16b, #0\n\tmovi v23.16b, #0\n\t"
        "movi v24.16b, #0\n\tmovi v25.16b, #0\n\tmovi v26.16b, #0\n\tmovi v27.16b, #0\n\t"
        "movi v28.16b, #0\n\tmovi v29.16b, #0\n\tmovi v30.16b, #0\n\tmovi v31.16b, #0"
        : : : "v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7",
        "v16", "v17", "v18", "v19", "v20", "v21", "v22", "v23",
        "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31");
}

#define SPILL(FN, TYPE, VIEW) \
__attribute__((noinline)) TYPE FN(int taken) { \
    TYPE a0,a1,a2,a3,a4,a5,a6,a7,a8,a9,a10,a11; \
    __asm__ goto ( \
        "fmov %" VIEW "[a0], #1.0\n\tfmov %" VIEW "[a1], #2.0\n\t" \
        "fmov %" VIEW "[a2], #3.0\n\tfmov %" VIEW "[a3], #4.0\n\t" \
        "fmov %" VIEW "[a4], #5.0\n\tfmov %" VIEW "[a5], #6.0\n\t" \
        "fmov %" VIEW "[a6], #7.0\n\tfmov %" VIEW "[a7], #8.0\n\t" \
        "fmov %" VIEW "[a8], #9.0\n\tfmov %" VIEW "[a9], #10.0\n\t" \
        "fmov %" VIEW "[a10], #11.0\n\tfmov %" VIEW "[a11], #12.0\n\t" \
        "cbnz %w[t], %l[done]" \
        : [a0] "=w"(a0), [a1] "=w"(a1), [a2] "=w"(a2), [a3] "=w"(a3), \
          [a4] "=w"(a4), [a5] "=w"(a5), [a6] "=w"(a6), [a7] "=w"(a7), \
          [a8] "=w"(a8), [a9] "=w"(a9), [a10] "=w"(a10), [a11] "=w"(a11) \
        : [t] "r"(taken) : : done); \
    a0 = -a0; \
done: \
    scalar_asm_clobber(); \
    return a0+a1+a2+a3+a4+a5+a6+a7+a8+a9+a10+a11; \
}
SPILL(scalar_asm_spill, double, "d")
SPILL(scalar_asm_spillf, float, "s")

__attribute__((noinline)) double scalar_asm_transform(double a, double b, int taken) {
    double r;
    /* The early first write must not destroy b before its last read. */
    __asm__ volatile ("fmov %d0, %d1\n\tfadd %d0, %d0, %d2"
                      : "=&w"(r) : "w"(a), "w"(b) : "s2", "d3");
    __asm__ goto ("fneg %d0, %d0\n\tcbnz %w1, %l[done]"
                  : "+w"(r) : "r"(taken) : : done);
    __asm__ volatile ("fneg %d0, %d0" : "+w"(r));
done:
    return r;
}

__attribute__((noinline)) float scalar_asm_float(float a, float b) {
    float r;
    __asm__ volatile ("fadd %s0, %s0, %s2"
                      : "=&w"(r) : "0"(a), "w"(b));
    __asm__ volatile ("fneg %s0, %s0" : "+&w"(r));
    __asm__ volatile ("// scalar-default %0; views %b0 %h0 %s0 %d0 %q0"
                      : "+w"(r));
    return r;
}

#ifndef SCALAR_ASM_WORKER
int main(void) {
    if (scalar_asm_transform(1.25, 2.5, 0) != 3.75) return 1;
    if (scalar_asm_transform(1.25, 2.5, 1) != -3.75) return 2;
    if (scalar_asm_float(1.25f, 2.5f) != -3.75f) return 3;
    __asm__ volatile ("fmov %d[out], %d[in]"
                      : [out] "=w"(values[places++]) : [in] "w"(input(7.5)));
    if (inputs != 1 || places != 1 || values[0] != 7.5) return 4;
    float x, y;
    float a = 1.25f, b = 2.5f;
    __asm__ volatile ("fmov %s0, %s2\n\tfmov %s1, %s3"
                      : "=&w"(x), "=&w"(y) : "w"(b), "w"(a));
    if (x != b || y != a) return 5;
    register double fixed __asm__("v19") = 6.25;
    double result;
    __asm__ volatile ("fneg %d0, %d0" : "+w"(fixed));
    __asm__ volatile ("fmov %d0, %d1" : "=w"(result) : "w"(fixed));
    if (result != -6.25) return 6;
    if (scalar_asm_spill(0) != 76.0 || scalar_asm_spill(1) != 78.0) return 7;
    if (scalar_asm_spillf(0) != 76.0f || scalar_asm_spillf(1) != 78.0f) return 8;
    return 0;
}
#endif
