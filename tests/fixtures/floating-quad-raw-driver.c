/* Raw IR conversions are total; these are calls, not invalid C casts. */
#include <stdint.h>
#include <stdio.h>
typedef unsigned __int128 U;
typedef __int128 S;
extern int quad_raw_s32(long double);
extern unsigned char quad_raw_u8(long double);
extern S quad_raw_s128(long double);
union quad { long double f; U u; };
static uint64_t reg(unsigned status) { uint64_t x; if(status) __asm__ volatile("mrs %0,fpsr":"=r"(x)); else __asm__ volatile("mrs %0,fpcr":"=r"(x)); return x; }
static void set(uint64_t c,uint64_t s) { __asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory"); }
int main(void) {
    struct sample { long double x; S s32,u8,s128; unsigned f32,f8,f128; } values[]={
        {0,0,0,0,0,0,0}, {-0.L,0,0,0,0,0,0}, {0.5L,0,0,0,16,16,16},
        {-0.5L,0,0,0,16,16,16}, {-1.5L,-1,0,-1,16,1,16},
        {255.5L,255,255,255,16,16,16}, {256.L,256,0,256,0,1,0},
        {0x1p31L,0,0,(S)1<<31,1,1,0}, {-0x1p31L,-2147483647-1,0,-((S)1<<31),0,1,0},
        {0x1p127L,0,0,0,1,1,1}, {-0x1p127L,0,0,(S)((U)1<<127),1,1,0},
        {0,0,0,0,1,1,1}, {0,0,0,0,1,1,1}, {0,0,0,0,1,1,1},
    };
    union quad q;
    q.u=(U)0x7fff000000000000ULL<<64; values[11].x=q.f;
    q.u|=17; values[12].x=q.f;
    q.u|=(U)1<<111; values[13].x=q.f;
    uint64_t oldc=reg(0),olds=reg(1); unsigned count=0;
    for(unsigned mode=0;mode<4;++mode) for(unsigned prior=0;prior<32;++prior)
    for(unsigned i=0;i<sizeof(values)/sizeof(values[0]);++i) for(unsigned op=0;op<3;++op) {
        struct sample v=values[i];
        set((uint64_t)mode<<22,prior);
        S result=op==0 ? quad_raw_s32(v.x) : op==1 ? quad_raw_u8(v.x) : quad_raw_s128(v.x);
        uint64_t flags=reg(1),control=reg(0); set(0,0);
        S expected=op==0 ? v.s32 : op==1 ? v.u8 : v.s128;
        unsigned raised=op==0 ? v.f32 : op==1 ? v.f8 : v.f128;
        if(result!=expected||flags!=(prior|raised)||control!=((uint64_t)mode<<22)) {
            printf("FAIL mode=%u prior=%u case=%u op=%u flags=%lx\n",mode,prior,i,op,(unsigned long)flags); return 1;
        }
        ++count;
    }
    set(oldc,olds); printf("quad raw IR: %u checks PASS\n",count); return 0;
}
