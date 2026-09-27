/* Fixed-target audit of integer128 helpers, restricted to defined C casts. */
#include <stdint.h>
#include <stdio.h>
typedef unsigned __int128 U;
typedef __int128 S;
union F {float f; uint32_t bits;};
union D {double f; uint64_t bits;};
#ifdef EVO_WIDE_SUBJECT
#define fs wide_fs
#define fu wide_fu
#define ds wide_ds
#define du wide_du
#define sf wide_sf
#define uf wide_uf
#define sd wide_sd
#define ud wide_ud
extern float fs(S),fu(U);
extern double ds(S),du(U);
extern S sf(float),sd(double);
extern U uf(float),ud(double);
#else
static __attribute__((noinline)) float fs(S x) {return x;}
static __attribute__((noinline)) float fu(U x) {return x;}
static __attribute__((noinline)) double ds(S x) {return x;}
static __attribute__((noinline)) double du(U x) {return x;}
static __attribute__((noinline)) S sf(float x) {return x;}
static __attribute__((noinline)) U uf(float x) {return x;}
static __attribute__((noinline)) S sd(double x) {return x;}
static __attribute__((noinline)) U ud(double x) {return x;}
#endif
static uint64_t reg(unsigned which) {uint64_t v;if(which) __asm__ volatile("mrs %0,fpsr":"=r"(v)); else __asm__ volatile("mrs %0,fpcr":"=r"(v));return v;}
static void set(uint64_t c,uint64_t s) {__asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory");}
static void row(unsigned width,unsigned mode,const char *op,U a,U r,uint64_t flags,uint64_t control) {
    set(0,0);
    printf("%u %u %s %016lx%016lx %016lx%016lx %lx %lx\n",width,mode,op,
        (uint64_t)(a>>64),(uint64_t)a,(uint64_t)(r>>64),(uint64_t)r,flags,control);
}
int main(void) {
    U inputs[2304];unsigned count=0;
    inputs[count++]=0;inputs[count++]=(U)-1;
    for(unsigned bit=1;bit<128;++bit) for(int delta=-3;delta<=3;++delta) {
        U value=((U)1<<bit)+(U)(S)delta;
        inputs[count++]=value;inputs[count++]=0-value;
    }
    U seed=((U)0xd1b54a32d192ed03ULL<<64)|0x94d049bb133111ebULL;
    for(unsigned i=0;i<512;++i) {
        seed^=seed<<23;seed^=seed>>17;seed^=seed<<26;inputs[count++]=seed;
    }
    uint64_t oldc=reg(0),olds=reg(1);
    for(unsigned mode=0;mode<4;++mode) {
        for(unsigned i=0;i<count;++i) {
            U a=inputs[i];union F f;union D d;
            set((uint64_t)mode<<22,0);f.f=fs((S)a);row(32,mode,"from_s",a,f.bits,reg(1),reg(0));
            set((uint64_t)mode<<22,0);f.f=fu(a);row(32,mode,"from_u",a,f.bits,reg(1),reg(0));
            set((uint64_t)mode<<22,0);d.f=ds((S)a);row(64,mode,"from_s",a,d.bits,reg(1),reg(0));
            set((uint64_t)mode<<22,0);d.f=du(a);row(64,mode,"from_u",a,d.bits,reg(1),reg(0));
        }
        const double values[]={0.0,-0.0,0.5,-0.5,1.5,0x1p64,0x1p126};
        for(unsigned i=0;i<sizeof(values)/sizeof(values[0]);++i) {
            union D d={.f=values[i]};union F f={.f=(float)values[i]};U r;
            set((uint64_t)mode<<22,0);r=(U)sf(f.f);row(32,mode,"to_s",f.bits,r,reg(1),reg(0));
            set((uint64_t)mode<<22,0);r=uf(f.f);row(32,mode,"to_u",f.bits,r,reg(1),reg(0));
            set((uint64_t)mode<<22,0);r=(U)sd(d.f);row(64,mode,"to_s",d.bits,r,reg(1),reg(0));
            set((uint64_t)mode<<22,0);r=ud(d.f);row(64,mode,"to_u",d.bits,r,reg(1),reg(0));
        }
    }
    set(oldc,olds);
}
