/* Bit-exact target arithmetic corpus. Compile with -ffp-contract=off, no fast math. */
extern int printf(const char *, ...);
typedef unsigned long u64;
static const u64 cases64[] = {
    0, 0x8000000000000000ul, 1, 0x8000000000000001ul,
    0x000ffffffffffffful, 0x0010000000000000ul,
    0x3feffffffffffffful, 0x3ff0000000000000ul, 0x3ff0000000000001ul,
    0x7feffffffffffffful, 0x7ff0000000000000ul, 0xfff0000000000000ul,
    0x7ff8000000000011ul, 0xfff8000000000022ul,
    0x7ff0000000000011ul, 0xfff0000000000022ul
};
static const unsigned cases32[] = {
    0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000,
    0x3f7fffff, 0x3f800000, 0x3f800001, 0x7f7fffff,
    0x7f800000, 0xff800000, 0x7fc00011, 0xffc00022, 0x7f800011, 0xff800022
};
static void run32(unsigned a, unsigned b) {
    union { unsigned u; float f; } x = {.u=a}, y = {.u=b}, r;
    volatile float p = x.f, q = y.f;
    for (int op=0; op<4; ++op) {
        if (op==0) r.f=p+q;
        if (op==1) r.f=p-q;
        if (op==2) r.f=p*q;
        if (op==3) r.f=p/q;
        printf("32 %d %08x %08x %08x\n", op,a,b,r.u);
    }
}
static void run64(u64 a, u64 b) {
    union { u64 u; double f; } x = {.u=a}, y = {.u=b}, r;
    volatile double p = x.f, q = y.f;
    for (int op=0; op<4; ++op) {
        if (op==0) r.f=p+q;
        if (op==1) r.f=p-q;
        if (op==2) r.f=p*q;
        if (op==3) r.f=p/q;
        printf("64 %d %016lx %016lx %016lx\n", op,a,b,r.u);
    }
}
int main(void) {
    for (unsigned i=0;i<16;++i) for(unsigned j=0;j<16;++j) {
        run32(cases32[i],cases32[j]); run64(cases64[i],cases64[j]);
    }
    u64 state=0x153968ac01234567ul;
    for(unsigned i=0;i<128;++i) {
        state=state*6364136223846793005ul+1442695040888963407ul;
        u64 a=state;
        state=state*6364136223846793005ul+1442695040888963407ul;
        run32((unsigned)a,(unsigned)state); run64(a,state);
    }
    return 0;
}
