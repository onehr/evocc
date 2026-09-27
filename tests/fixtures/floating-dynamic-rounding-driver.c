/* Reference caller for the unchanged EvoCC-compiled source fixture. */
#include <stdint.h>
#include <stdio.h>

#define ENTRY(name) extern unsigned long name(void)
ENTRY(runtime_constant); ENTRY(runtime_volatile); ENTRY(translation_constant);
ENTRY(runtime_integer_conversion); ENTRY(runtime_narrowing);
ENTRY(runtime_branch_conversion); ENTRY(runtime_condition);
ENTRY(quiet_nan_comparison); ENTRY(signaling_nan_comparison);
ENTRY(ordered_nan_comparison);

static uint64_t control(void) { uint64_t v; __asm__ volatile("mrs %0,fpcr":"=r"(v)); return v; }
static uint64_t flags(void) { uint64_t v; __asm__ volatile("mrs %0,fpsr":"=r"(v)); return v; }
static void set(uint64_t c,uint64_t s) { __asm__ volatile("msr fpcr,%0; msr fpsr,%1; isb"::"r"(c),"r"(s):"memory"); }

int main(void) {
    const struct {
        const char *name;
        unsigned long (*call)(void);
        uint64_t nearest;
        unsigned status, upward_increment;
    } cases[] = {
        {"constant",runtime_constant,0x3ff0000000000000ULL,16,1},
        {"volatile",runtime_volatile,0x3ff0000000000000ULL,16,1},
        {"static",translation_constant,0x3ff0000000000000ULL,0,0},
        {"integer",runtime_integer_conversion,0x4340000000000000ULL,16,1},
        {"narrowing",runtime_narrowing,0x4b800000,16,1},
        {"branch",runtime_branch_conversion,0x4340000000000000ULL,16,1},
        {"condition",runtime_condition,0,16,1},
        {"quiet",quiet_nan_comparison,0,0,0},
        {"signaling",signaling_nan_comparison,0,1,0},
        {"ordered",ordered_nan_comparison,0,1,0},
    };
    uint64_t saved_control=control(), saved_flags=flags();
    unsigned checks=0;
    for (unsigned mode=0;mode<4;++mode) {
        for (unsigned i=0;i<sizeof(cases)/sizeof(cases[0]);++i) {
            set((uint64_t)mode<<22,2);
            uint64_t actual=cases[i].call(), status=flags(), c=control();
            set(saved_control,saved_flags);
            if (actual!=cases[i].nearest+(mode==1 ? cases[i].upward_increment : 0)
                || status!=(cases[i].status|2) || c!=((uint64_t)mode<<22)) {
                printf("FAIL %s mode=%u bits=%016lx status=%lx control=%lx\n",
                    cases[i].name,mode,actual,status,c);
                return 1;
            }
            ++checks;
        }
    }
    printf("strict C native: %u checks PASS\n",checks);
    return 0;
}
