/* Qualify the hosted helper result encodings and signaling behavior. */
#include <stdint.h>
#include <stdio.h>
typedef unsigned __int128 U;
union quad { long double f; U u; };
extern int __eqtf2(long double, long double), __netf2(long double, long double);
extern int __letf2(long double, long double), __lttf2(long double, long double);
extern int __getf2(long double, long double), __gttf2(long double, long double);
extern int __unordtf2(long double, long double);
static uint64_t control(void) { uint64_t x; __asm__ volatile("mrs %0,fpcr":"=r"(x)); return x; }
static uint64_t status(void) { uint64_t x; __asm__ volatile("mrs %0,fpsr":"=r"(x)); return x; }
static void set(uint64_t c, uint64_t s) {
    __asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory");
}
int main(void) {
    const U sign = (U)1 << 127, inf = (U)0x7fff000000000000ULL << 64;
    const U one = (U)0x3fff000000000000ULL << 64, quiet = (U)1 << 111;
    const U values[] = {0, sign, 1, sign | 1, one, sign | one, one + 1,
        one - 1, inf - 1, sign | (inf - 1), inf, sign | inf,
        inf | quiet | 1, inf | 2, sign | inf | quiet | 3, sign | inf | 4};
    int (*helpers[])(long double, long double) = {
        __eqtf2, __netf2, __letf2, __lttf2, __getf2, __gttf2, __unordtf2};
    uint64_t oldc = control(), olds = status();
    unsigned checks = 0, failures = 0;
    for (unsigned mode=0; mode<4; ++mode)
    for (unsigned prior=0; prior<32; ++prior)
    for (unsigned i=0; i<16; ++i)
    for (unsigned j=0; j<16; ++j)
    for (unsigned op=0; op<7; ++op) {
        union quad a = {.u=values[i]}, b = {.u=values[j]};
        U ma=a.u & ~sign, mb=b.u & ~sign;
        int na=ma>inf, nb=mb>inf;
        int signaling=(na && !(a.u&quiet)) || (nb && !(b.u&quiet));
        int equal=a.u==b.u || (!ma && !mb);
        int less=(a.u & sign) ? ((b.u & sign) ? ma>mb : !equal)
                             : ((b.u & sign) ? 0 : ma<mb);
        int order=equal ? 0 : less ? -1 : 1;
        int expected = op<2 ? (!equal || na || nb) : op==6 ? (na || nb)
            : (na || nb) ? (op<4 ? 2 : -2) : order;
        unsigned raised=(op>=2 && op<6) ? (na || nb) : signaling;
        set((uint64_t)mode<<22, prior);
        int actual=helpers[op](a.f,b.f);
        uint64_t flags=status(), controls=control();
        set(0,0);
        ++checks;
        if (actual!=expected || flags!=(prior|raised) || controls!=((uint64_t)mode<<22)) {
            if (++failures<=8) printf("FAIL op=%u mode=%u prior=%u i=%u j=%u got=%d/%lx expected=%d/%x control=%lx\n",
                op,mode,prior,i,j,actual,(unsigned long)flags,expected,prior|raised,(unsigned long)controls);
        }
    }
    set(oldc,olds);
    printf("quad comparison helpers: %u checks, %u failures\n",checks,failures);
    return failures!=0;
}
