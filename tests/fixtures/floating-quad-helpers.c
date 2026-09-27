/* Hosted AArch64 binary128 helper qualification, not EvoCC acceptance.
 * Keep exact object bits, current FPCR, and FPSR in every record. */
#include <stdint.h>
#include <stdio.h>
typedef unsigned __int128 U;
union quad { long double f; U u; };
_Static_assert(sizeof(long double)==16 && __LDBL_MANT_DIG__==113, "binary128 target");
static uint64_t read_control(void) { uint64_t v; __asm__ volatile("mrs %0,fpcr":"=r"(v)); return v; }
static uint64_t read_status(void) { uint64_t v; __asm__ volatile("mrs %0,fpsr":"=r"(v)); return v; }
static void set(uint64_t c,uint64_t s) { __asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory"); }
#ifdef EVO_QUAD_SUBJECT
extern long double arithmetic(unsigned,long double,long double);
#else
static __attribute__((noinline)) long double arithmetic(unsigned op,long double a,long double b) {
    switch(op) {case 0:return a+b; case 1:return a-b; case 2:return a*b; default:return a/b;}
}
#endif
int main(void) {
    const U one=(U)0x3fff000000000000ULL<<64;
    const U sign=(U)1<<127;
    const U infinity=(U)0x7fff000000000000ULL<<64;
    const U quiet=(U)1<<111;
    const U inputs[]={0,sign,one,one|sign,one+1,one-1,
        (U)0x4000000000000000ULL<<64,(U)0x3f8e000000000000ULL<<64,
        1,sign|1,(U)1<<112,((U)1<<112)-1,
        infinity-1,sign|(infinity-1),infinity,sign|infinity,
        infinity|quiet|0x11,infinity|0x33,sign|infinity|quiet|0x22,sign|infinity|0x44};
    const uint64_t oldc=read_control(), olds=read_status();
    for(unsigned mode=0;mode<4;++mode) for(unsigned op=0;op<4;++op)
    for(unsigned i=0;i<20;++i) for(unsigned j=0;j<20;++j) {
        union quad a={.u=inputs[i]},b={.u=inputs[j]},r;
        set((uint64_t)mode<<22,0);
        r.f=arithmetic(op,a.f,b.f);
        uint64_t flags=read_status(), controls=read_control();
        set(0,0); /* Formatting must not contribute to the measured operation. */
        printf("128 %u %u %016llx%016llx %016llx%016llx %016llx%016llx %02llx %08llx\n",
            mode,op,(unsigned long long)(a.u>>64),(unsigned long long)a.u,
            (unsigned long long)(b.u>>64),(unsigned long long)b.u,
            (unsigned long long)(r.u>>64),(unsigned long long)r.u,
            (unsigned long long)flags,(unsigned long long)controls);
    }
    set(oldc,olds);
    return 0;
}
