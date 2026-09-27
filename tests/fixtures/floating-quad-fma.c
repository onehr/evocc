/* Audit target libm's explicit fused binary128 operation, not contraction. */
#include <stdint.h>
#include <stdio.h>
#include <math.h>
#include <errno.h>
typedef unsigned __int128 U;
union quad { long double f; U u; };
_Static_assert(sizeof(long double) == 16 && __LDBL_MANT_DIG__ == 113, "quad target");
static uint64_t control(void) { uint64_t x; __asm__ volatile("mrs %0,fpcr":"=r"(x)); return x; }
static uint64_t status(void) { uint64_t x; __asm__ volatile("mrs %0,fpsr":"=r"(x)); return x; }
static void set(uint64_t c, uint64_t s) {
    __asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory");
}
#ifdef EVO_QUAD_SUBJECT
extern long double fused(long double,long double,long double);
#else
static __attribute__((noinline)) long double fused(long double a, long double b, long double c) {
    return fmal(a, b, c);
}
#endif
int main(void) {
    const U one = (U)0x3fff000000000000ULL << 64;
    const U sign = (U)1 << 127, infinity = (U)0x7fff000000000000ULL << 64;
    const U quiet = (U)1 << 111;
    const U values[] = {0, sign, one, sign | one, one + 1, one - 2,
        (U)0x4000000000000000ULL << 64, (U)0x3f8e000000000000ULL << 64,
        1, sign | 1, (U)1 << 112, ((U)1 << 112) - 1,
        infinity - 1, sign | (infinity - 1), infinity, sign | infinity,
        infinity | quiet | 0x11, infinity | 0x33,
        sign | infinity | quiet | 0x22, sign | infinity | 0x44};
    const uint64_t oldc = control(), olds = status();
    for (unsigned mode = 0; mode < 4; ++mode)
    for (unsigned i = 0; i < 20; ++i)
    for (unsigned j = 0; j < 20; ++j)
    for (unsigned k = 0; k < 20; ++k) {
        union quad a = {.u=values[i]}, b = {.u=values[j]}, c = {.u=values[k]}, r;
        errno = 123;
        set((uint64_t)mode << 22, 0);
        r.f = fused(a.f, b.f, c.f);
        uint64_t flags = status(), controls = control();
        int saved_errno = errno;
        set(0, 0);
        if (controls != ((uint64_t)mode << 22)) return 2;
        if (saved_errno != 123) return 3;
        printf("128 %u 4 %016llx%016llx %016llx%016llx %016llx%016llx %016llx%016llx %02llx\n",
            mode, (unsigned long long)(a.u>>64), (unsigned long long)a.u,
            (unsigned long long)(b.u>>64), (unsigned long long)b.u,
            (unsigned long long)(c.u>>64), (unsigned long long)c.u,
            (unsigned long long)(r.u>>64), (unsigned long long)r.u,
            (unsigned long long)flags);
    }
    set(oldc, olds);
    return 0;
}
