/* Destination/source/mode/input/output/status/control, all bits in hex. */
#include <stdint.h>
#include <stdio.h>
typedef unsigned __int128 U;
union quad { long double f; U u; };
union single { float f; uint32_t u; };
union binary64 { double f; uint64_t u; };
#ifdef EVO_QUAD_SUBJECT
extern float quad_narrow32(long double);
extern double quad_narrow64(long double);
extern long double quad_widen32(float), quad_widen64(double);
#else
#include "floating-quad-format-source.c"
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
static void row(unsigned dst,unsigned src,unsigned mode,U a,U r,uint64_t flags,uint64_t control) {
    set(0,0);
    printf("%u %u %u %016lx%016lx %016lx%016lx %lx %lx\n",dst,src,mode,
        (uint64_t)(a>>64),(uint64_t)a,(uint64_t)(r>>64),(uint64_t)r,flags,control);
}
int main(void) {
    const U sign=(U)1<<127, inf=(U)0x7fff000000000000ULL<<64;
    const U one=(U)0x3fff000000000000ULL<<64, quiet=(U)1<<111;
    const U quad[] = {0,sign,1,sign|1,(U)1<<112,((U)1<<112)-1,
        one,sign|one,one+1,one-1,inf-1,sign|(inf-1),inf,sign|inf,
        inf|quiet|1,inf|2,sign|inf|quiet|3,sign|inf|4,
        one+((U)1<<88)+1,one+((U)1<<59)+1,
        (U)0x3f69000000000000ULL<<64,(U)0x3bcd000000000000ULL<<64};
    const uint32_t single[]={0,0x80000000,1,0x80000001,0x800000,0x7fffff,
        0x3f800000,0xbf800000,0x3f800001,0x3f7fffff,0x7f7fffff,0xff7fffff,
        0x7f800000,0xff800000,0x7fc12345,0xff812345};
    const uint64_t binary64[]={0,0x8000000000000000ULL,1,0x8000000000000001ULL,
        0x10000000000000ULL,0xfffffffffffffULL,0x3ff0000000000000ULL,0xbff0000000000000ULL,
        0x3ff0000000000001ULL,0x3fefffffffffffffULL,0x7fefffffffffffffULL,0xffefffffffffffffULL,
        0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0xfff0123456789abcULL};
    uint64_t oldc=reg(0),olds=reg(1);
    for(unsigned mode=0;mode<4;++mode) {
        for(unsigned i=0;i<sizeof(quad)/sizeof(*quad);++i) {
            union quad a={.u=quad[i]}; union single s; union binary64 d;
            set((uint64_t)mode<<22,0); s.f=quad_narrow32(a.f);
            row(32,128,mode,a.u,s.u,reg(1),reg(0));
            set((uint64_t)mode<<22,0); d.f=quad_narrow64(a.f);
            row(64,128,mode,a.u,d.u,reg(1),reg(0));
        }
        for(unsigned i=0;i<16;++i) {
            union single s={.u=single[i]}; union binary64 d={.u=binary64[i]}; union quad q;
            set((uint64_t)mode<<22,0); q.f=quad_widen32(s.f);
            row(128,32,mode,s.u,q.u,reg(1),reg(0));
            set((uint64_t)mode<<22,0); q.f=quad_widen64(d.f);
            row(128,64,mode,d.u,q.u,reg(1),reg(0));
        }
    }
    set(oldc,olds);
    return 0;
}
