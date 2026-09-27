/* Hosted integer128/binary128 helper audit, restricted to defined C casts. */
#include <stdint.h>
#include <stdio.h>
typedef unsigned __int128 U;
typedef __int128 S;
union quad { long double f; U bits; };
_Static_assert(sizeof(long double) == 16 && __LDBL_MANT_DIG__ == 113, "quad target");
#ifdef EVO_QUAD_SUBJECT
extern long double quad_from_signed(S), quad_from_unsigned(U);
extern S quad_to_signed(long double);
extern U quad_to_unsigned(long double);
#else
static __attribute__((noinline)) long double quad_from_signed(S x) { return x; }
static __attribute__((noinline)) long double quad_from_unsigned(U x) { return x; }
static __attribute__((noinline)) S quad_to_signed(long double x) { return x; }
static __attribute__((noinline)) U quad_to_unsigned(long double x) { return x; }
#endif
static uint64_t reg(unsigned which) {
    uint64_t v;
    if (which) __asm__ volatile("mrs %0,fpsr":"=r"(v));
    else __asm__ volatile("mrs %0,fpcr":"=r"(v));
    return v;
}
static void set(uint64_t c, uint64_t s) {
    __asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory");
}
static void row(unsigned mode, const char *op, U a, U r, uint64_t flags, uint64_t control) {
    set(0, 0);
    printf("128 %u %s %016lx%016lx %016lx%016lx %lx %lx\n", mode, op,
        (uint64_t)(a>>64), (uint64_t)a, (uint64_t)(r>>64), (uint64_t)r, flags, control);
}
int main(void) {
    U inputs[2304]; unsigned count = 0;
    inputs[count++] = 0; inputs[count++] = (U)-1;
    for (unsigned bit = 1; bit < 128; ++bit) for (int delta = -3; delta <= 3; ++delta) {
        U value = ((U)1 << bit) + (U)(S)delta;
        inputs[count++] = value; inputs[count++] = 0 - value;
    }
    U seed = ((U)0xd1b54a32d192ed03ULL << 64) | 0x94d049bb133111ebULL;
    for (unsigned i = 0; i < 512; ++i) {
        seed ^= seed << 23; seed ^= seed >> 17; seed ^= seed << 26;
        inputs[count++] = seed;
    }
    const long double values[] = {0.0L, -0.0L, 0.5L, -0.5L, 1.5L,
        0x1p64L, 0x1p112L, 0x1.0000000000000000000000000001p112L,
        0x1.ffffffffffffffffffffffffffffp126L, -0x1p127L};
    uint64_t oldc = reg(0), olds = reg(1);
    for (unsigned mode = 0; mode < 4; ++mode) {
        for (unsigned i = 0; i < count; ++i) {
            U a = inputs[i]; union quad q;
            set((uint64_t)mode << 22, 0); q.f = quad_from_signed((S)a);
            row(mode, "from_s", a, q.bits, reg(1), reg(0));
            set((uint64_t)mode << 22, 0); q.f = quad_from_unsigned(a);
            row(mode, "from_u", a, q.bits, reg(1), reg(0));
        }
        for (unsigned i = 0; i < sizeof(values)/sizeof(values[0]); ++i) {
            union quad q = {.f=values[i]}; U r;
            set((uint64_t)mode << 22, 0); r = (U)quad_to_signed(q.f);
            row(mode, "to_s", q.bits, r, reg(1), reg(0));
            if (q.f > -1.0L) {
                set((uint64_t)mode << 22, 0); r = quad_to_unsigned(q.f);
                row(mode, "to_u", q.bits, r, reg(1), reg(0));
            }
        }
    }
    set(oldc, olds);
    return 0;
}
