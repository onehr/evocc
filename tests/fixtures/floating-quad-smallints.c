/* Defined-domain 32/64-bit conversions, using the existing wide value checker. */
#include <stdint.h>
#include <stdio.h>
typedef unsigned __int128 U;
typedef __int128 S;
union quad { long double f; U u; };
#ifdef EVO_QUAD_SUBJECT
extern long double quad_from_i32(int), quad_from_u32(unsigned);
extern long double quad_from_i64(long), quad_from_u64(unsigned long);
extern int quad_to_i32(long double);
extern unsigned quad_to_u32(long double);
extern long quad_to_i64(long double);
extern unsigned long quad_to_u64(long double);
#else
#include "floating-quad-smallint-source.c"
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
static void row(unsigned mode,const char *op,U a,U r,uint64_t flags,uint64_t control) {
    set(0,0);
    printf("128 %u %s %016lx%016lx %016lx%016lx %lx %lx\n",mode,op,
        (uint64_t)(a>>64),(uint64_t)a,(uint64_t)(r>>64),(uint64_t)r,flags,control);
}
int main(void) {
    uint64_t values[600]; unsigned count=0;
    for (unsigned bit=1;bit<64;++bit) for(int delta=-3;delta<=3;++delta)
        values[count++]=(UINT64_C(1)<<bit)+(uint64_t)delta;
    uint64_t seed=0xd1b54a32d192ed03ULL;
    for(unsigned i=0;i<128;++i) {seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;values[count++]=seed;}
    const long double inputs[]={0.L,-0.L,0.5L,-0.5L,1.5L,-1.5L,
        0x1.fffffffdp30L,-0x1.000000018p31L,-0x1p31L,
        0x1.fffffffdp31L,0x1.fffffffffffffffep62L,
        -0x1.0000000000000001p63L,-0x1p63L,0x1.fffffffffffffffep63L};
    uint64_t oldc=reg(0),olds=reg(1);
    for(unsigned mode=0;mode<4;++mode) {
        for(unsigned i=0;i<count;++i) {
            uint64_t x=values[i]; union quad q;
            set((uint64_t)mode<<22,0); q.f=quad_from_i32((int)x);
            row(mode,"from_s",(U)(S)(int)x,q.u,reg(1),reg(0));
            set((uint64_t)mode<<22,0); q.f=quad_from_u32((unsigned)x);
            row(mode,"from_u",(unsigned)x,q.u,reg(1),reg(0));
            set((uint64_t)mode<<22,0); q.f=quad_from_i64((long)x);
            row(mode,"from_s",(U)(S)(long)x,q.u,reg(1),reg(0));
            set((uint64_t)mode<<22,0); q.f=quad_from_u64(x);
            row(mode,"from_u",x,q.u,reg(1),reg(0));
        }
        for(unsigned i=0;i<sizeof(inputs)/sizeof(*inputs);++i) {
            union quad q={.f=inputs[i]}; U r;
            if(q.f>-0x1.00000002p31L && q.f<0x1p31L) {
                set((uint64_t)mode<<22,0); r=(U)(S)quad_to_i32(q.f);
                row(mode,"to_s",q.u,r,reg(1),reg(0));
            }
            if(q.f>-1.L && q.f<0x1p32L) {
                set((uint64_t)mode<<22,0); r=quad_to_u32(q.f);
                row(mode,"to_u",q.u,r,reg(1),reg(0));
            }
            if(q.f>-0x1.0000000000000002p63L && q.f<0x1p63L) {
                set((uint64_t)mode<<22,0); r=(U)(S)quad_to_i64(q.f);
                row(mode,"to_s",q.u,r,reg(1),reg(0));
            }
            if(q.f>-1.L && q.f<0x1p64L) {
                set((uint64_t)mode<<22,0); r=quad_to_u64(q.f);
                row(mode,"to_u",q.u,r,reg(1),reg(0));
            }
        }
    }
    set(oldc,olds);
    return 0;
}
