#include <stdint.h>
#include <stdio.h>
typedef unsigned __int128 U;
extern double strict_wide_from(U);
extern U strict_wide_to(double);
union D {double d; uint64_t u;};
static uint64_t status(void) {uint64_t v;__asm__ volatile("mrs %0,fpsr":"=r"(v));return v;}
static uint64_t control(void) {uint64_t v;__asm__ volatile("mrs %0,fpcr":"=r"(v));return v;}
static void set(uint64_t c,uint64_t s) {__asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory");}
int main(void) {
    uint64_t oldc=control(),olds=status();unsigned checks=0;
    for(unsigned mode=0;mode<4;++mode) for(unsigned prior=0;prior<32;++prior) {
        set((uint64_t)mode<<22,prior);
        union D r={.d=strict_wide_from(((U)1<<64)+1)};
        if(r.u!=0x43f0000000000000ULL+(mode==1) || status()!=(prior|16)
            || control()!=((uint64_t)mode<<22)) return 1;
        ++checks;
        const uint64_t input[]={0x3ff8000000000000ULL,0x43f0000000000000ULL,
            0x7ff0000000000011ULL,0x7ff8000000000011ULL,0x47e0000000000000ULL};
        const U expected[]={1,(U)1<<64,0,0,0};
        const unsigned flags[]={16,0,1,1,1};
        for(unsigned i=0;i<5;++i) {
            set((uint64_t)mode<<22,prior);
            union D a={.u=input[i]};U result=strict_wide_to(a.d);
            if(result!=expected[i] || status()!=(prior|flags[i])
                || control()!=((uint64_t)mode<<22)) return 2;
            ++checks;
        }
    }
    set(oldc,olds);
    printf("strict wide IR: %u checks PASS\n",checks);
}
