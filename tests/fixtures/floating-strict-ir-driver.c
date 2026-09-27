/* Reference-compiled caller of the unchanged strict-world EvoIR fixture.
 * This is an IR/backend check, not C strict-lowering acceptance.
 */
#include <stdint.h>
#include <stdio.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>
#include <signal.h>

extern double strict_add(double,double);
extern unsigned char strict_trunc(double);
extern _Bool defined_only(double);
static uint64_t bits(double x) { union {double d; uint64_t u;} v={.d=x}; return v.u; }
static double from(uint64_t x) { union {double d; uint64_t u;} v={.u=x}; return v.d; }
static uint64_t control(void) { uint64_t v; __asm__ volatile("mrs %0,fpcr":"=r"(v)); return v; }
static uint64_t flags(void) { uint64_t v; __asm__ volatile("mrs %0,fpsr":"=r"(v)); return v; }
static void set(uint64_t c,uint64_t s) { __asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory"); }

int main(void) {
    uint64_t original_control=control(), original_flags=flags();
    unsigned checks=0;
    for (unsigned mode=0;mode<4;++mode) {
        set((uint64_t)mode<<22,2);
        uint64_t actual=bits(strict_add(1.0,0x1p-53));
        uint64_t status=flags();
        if (actual != 0x3ff0000000000000ULL+(mode==1) || status!=18 || control()!=((uint64_t)mode<<22)) return 1;
        ++checks;
        set((uint64_t)mode<<22,0);
        actual=bits(strict_add(from(0x7ff0000000000033),from(0x7ff8000000000011)));
        if (actual!=0x7ff8000000000033 || flags()!=1) return 2;
        ++checks;
        set((uint64_t)mode<<22,0);
        actual=bits(strict_add(0.0,-0.0));
        if (actual!=(mode==2 ? 0x8000000000000000ULL:0) || flags()!=0) return 3;
        ++checks;
    }
    set(original_control,original_flags);
    const uint64_t conversion_inputs[]={
        0x405fe00000000000ULL, /* 127.5 -> 127, inexact */
        0x4060000000000000ULL, /* 128 -> invalid in bits<8> */
        0xc060100000000000ULL, /* -128.5 -> -128, inexact */
        0x7ff0000000000011ULL, /* sNaN -> invalid; predicate stays pure */
        0x7ff8000000000011ULL, /* qNaN -> invalid */
        0x7ff0000000000000ULL  /* infinity -> invalid */
    };
    const unsigned expected_values[]={127,0,128,0,0,0};
    const unsigned expected_flags[]={16,1,16,1,1,1};
    for (unsigned i=0;i<6;++i) {
        for (unsigned mode=0;mode<4;++mode) {
            set((uint64_t)mode<<22,2);
            unsigned value=strict_trunc(from(conversion_inputs[i]));
            if (value!=expected_values[i] || flags()!=(expected_flags[i]|2)) return 6;
            ++checks;
            set((uint64_t)mode<<22,2);
            unsigned defined=defined_only(from(conversion_inputs[i]));
            if (defined!=(expected_flags[i]==16) || flags()!=2) return 7;
            ++checks;
        }
    }
    set(original_control,original_flags);
    /* Fail-closed FZ and DN are observed in disposable child processes. */
    for (unsigned bit=24;bit<=25;++bit) {
        pid_t child=fork();
        if (child<0) return 4;
        if (!child) {
            struct rlimit limit={0,0}; setrlimit(RLIMIT_CORE,&limit);
            set(1ULL<<bit,0);
            (void)strict_add(1.0,0x1p-53);
            _exit(99);
        }
        int status;
        if (waitpid(child,&status,0)!=child || !WIFSIGNALED(status) || WTERMSIG(status)!=SIGTRAP) return 5;
        ++checks;
    }
    printf("strict IR native: %u checks PASS\n",checks);
    return 0;
}
