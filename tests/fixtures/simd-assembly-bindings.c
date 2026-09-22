typedef unsigned long V __attribute__((vector_size(16)));
typedef unsigned int D __attribute__((vector_size(8)));
#ifndef SIMD_CLOBBER
#define SIMD_CLOBBER "v2"
#endif

static int inputs, places;
static V values[2];
static V input(V a) { inputs++; return a; }

__attribute__((noinline)) V simd_asm_transform(V a, V b, int taken) {
    V r;
    /* The first write must not destroy b before its last read. */
    __asm__ volatile ("mov %0.16b, %1.16b\n\teor %0.16b, %0.16b, %2.16b"
                      : "=&w"(r) : "w"(a), "w"(b) : SIMD_CLOBBER, "v3");
    __asm__ goto ("mvn %0.16b, %0.16b\n\tcbnz %w1, %l[done]"
                  : "+w"(r) : "r"(taken) : : done);
    __asm__ volatile ("mvn %0.16b, %0.16b" : "+w"(r));
done:
    return r;
}

#ifndef SIMD_ASM_WORKER
int main(void) {
    V a = {0x0123456789abcdefUL, 0xfedcba9876543210UL};
    V b = {0xff00ff00ff00ff00UL, 0x00ff00ff00ff00ffUL};
    V r = simd_asm_transform(a, b, 0);
    if (r[0] != (a[0] ^ b[0]) || r[1] != (a[1] ^ b[1])) return 1;
    r = simd_asm_transform(a, b, 1);
    if (r[0] != ~(a[0] ^ b[0]) || r[1] != ~(a[1] ^ b[1])) return 2;

    /* Tied early input is legal; other inputs still cannot overlap. */
    __asm__ volatile ("eor %0.16b, %0.16b, %2.16b"
                      : "=&w"(r) : "0"(a), "w"(b));
    if (r[0] != (a[0] ^ b[0]) || r[1] != (a[1] ^ b[1])) return 3;
    __asm__ volatile ("// %b0 %h0 %s0 %d0 %q0" : "+&w"(r));
    if (r[0] != (a[0] ^ b[0]) || r[1] != (a[1] ^ b[1])) return 4;
    if (a[0] != 0x0123456789abcdefUL || a[1] != 0xfedcba9876543210UL) return 5;

    __asm__ volatile ("mov %[out].16b, %[in].16b"
                      : [out] "=w"(values[places++]) : [in] "w"(input(b)));
    if (places != 1 || inputs != 1 || values[0][0] != b[0] || values[0][1] != b[1]) return 6;
    D low = {0xdeadbeefU, 0x12345678U}, high = {0xfeedfaceU, 0xa5a5a5a5U};
    D x, y;
    __asm__ volatile ("mov %0.8b, %2.8b\n\tmov %1.8b, %3.8b"
                      : "=&w"(x), "=&w"(y) : "w"(high), "w"(low));
    if (x[0] != high[0] || x[1] != high[1] || y[0] != low[0] || y[1] != low[1]) return 7;
    register V fixed __asm__("v19") = a;
    __asm__ volatile ("mvn %0.16b, %0.16b" : "+w"(fixed));
#ifdef PROBE_FIXED_LANE_READ
    /* Retained exploration: GCC14 O2 folds this check to failure, unlike
       GCC O0, both Clang controls, and EvoCC. Not a passing oracle control. */
    if (fixed[0] != ~a[0] || fixed[1] != ~a[1]) return 8;
#else
    __asm__ volatile ("mov %0.16b, %1.16b" : "=w"(r) : "w"(fixed));
    if (r[0] != ~a[0] || r[1] != ~a[1]) return 8;
#endif
    return 0;
}
#endif
