/* Oracle-only: instruction order, result bits and FPSR, not C optimization.
 * Emit a deterministic stream; the numeric adapter checks every record.
 * Caller FPCR/FPSR are restored. No traps or nonstandard controls are enabled.
 */
#include <stdint.h>
#include <stdio.h>

static const uint64_t values64[] = {
    0, 0x8000000000000000, 1, 0x8000000000000001,
    0x000fffffffffffff, 0x0010000000000000, 0x0010000000000001,
    0x3fefffffffffffff, 0x3ff0000000000000, 0x3ff0000000000001,
    0x3ca0000000000000, 0x4000000000000000, 0x4008000000000000,
    0x7fefffffffffffff, 0x7ff0000000000000, 0xfff0000000000000,
    0x7ff8000000000011, 0xfff8000000000022,
    0x7ff0000000000033, 0xfff0000000000044
};
static const uint32_t values32[] = {
    0, 0x80000000, 1, 0x80000001,
    0x007fffff, 0x00800000, 0x00800001,
    0x3f7fffff, 0x3f800000, 0x3f800001,
    0x33800000, 0x40000000, 0x40400000,
    0x7f7fffff, 0x7f800000, 0xff800000,
    0x7fc00011, 0xffc00022, 0x7f800033, 0xff800044
};

#define RUN(W, T, U, VIEW, HEX) \
static void run##W(unsigned mode, U a, U b, U c) { \
    union { U u; T f; } x = {.u=a}, y = {.u=b}, z = {.u=c}, r; \
    uint64_t flags; \
    for (unsigned op=0; op<7; ++op) { \
        switch (op) { \
        case 0: __asm__ volatile ("msr fpsr, xzr; fadd %" VIEW "0, %" VIEW "2, %" VIEW "3; mrs %1, fpsr" : "=w"(r.f), "=r"(flags) : "w"(x.f), "w"(y.f)); break; \
        case 1: __asm__ volatile ("msr fpsr, xzr; fsub %" VIEW "0, %" VIEW "2, %" VIEW "3; mrs %1, fpsr" : "=w"(r.f), "=r"(flags) : "w"(x.f), "w"(y.f)); break; \
        case 2: __asm__ volatile ("msr fpsr, xzr; fmul %" VIEW "0, %" VIEW "2, %" VIEW "3; mrs %1, fpsr" : "=w"(r.f), "=r"(flags) : "w"(x.f), "w"(y.f)); break; \
        case 3: __asm__ volatile ("msr fpsr, xzr; fdiv %" VIEW "0, %" VIEW "2, %" VIEW "3; mrs %1, fpsr" : "=w"(r.f), "=r"(flags) : "w"(x.f), "w"(y.f)); break; \
        case 4: __asm__ volatile ("msr fpsr, xzr; fmadd %" VIEW "0, %" VIEW "2, %" VIEW "3, %" VIEW "4; mrs %1, fpsr" : "=w"(r.f), "=r"(flags) : "w"(x.f), "w"(y.f), "w"(z.f)); break; \
        case 5: __asm__ volatile ("msr fpsr, xzr; fcmp %" VIEW "2, %" VIEW "3; cset %w0, eq; mrs %1, fpsr" : "=r"(r.u), "=r"(flags) : "w"(x.f), "w"(y.f) : "cc"); break; \
        default: __asm__ volatile ("msr fpsr, xzr; fcmpe %" VIEW "2, %" VIEW "3; cset %w0, mi; mrs %1, fpsr" : "=r"(r.u), "=r"(flags) : "w"(x.f), "w"(y.f) : "cc"); break; \
        } \
        printf("%u %u %u " HEX " " HEX " " HEX " " HEX " %02lx\n", W,mode,op,a,b,c,r.u,flags); \
    } \
}
RUN(32, float, uint32_t, "s", "%08x")
RUN(64, double, uint64_t, "d", "%016lx")

int main(void) {
    uint64_t original_control, original_status;
    __asm__ volatile ("mrs %0, fpcr; mrs %1, fpsr" : "=r"(original_control), "=r"(original_status));
    for (unsigned mode=0; mode<4; ++mode) {
        uint64_t control = (uint64_t)mode << 22;
        __asm__ volatile ("msr fpcr, %0; isb" : : "r"(control) : "memory");
        for (unsigned i=0; i<20; ++i) for (unsigned j=0; j<20; ++j) {
            unsigned k=(i+3*j)%20;
            run32(mode,values32[i],values32[j],values32[k]);
            run64(mode,values64[i],values64[j],values64[k]);
        }
        /* Fused-vs-separate, min-normal tininess and invalid/qNaN priority. */
        run32(mode,0x3f800001,0x3f7ffffe,0xbf800000);
        run64(mode,0x3ff0000000000001,0x3feffffffffffffe,0xbff0000000000000);
        run32(mode,0,0x7f800000,0x7fc00055);
        run64(mode,0,0x7ff0000000000000,0x7ff8000000000055);
        run32(mode,0x7fc00011,0x7fc00022,0x7fc00033);
        run64(mode,0x7ff8000000000011,0x7ff8000000000022,0x7ff8000000000033);
    }
    __asm__ volatile ("msr fpcr, %0; msr fpsr, %1; isb" : : "r"(original_control), "r"(original_status) : "memory");
    return 0;
}
