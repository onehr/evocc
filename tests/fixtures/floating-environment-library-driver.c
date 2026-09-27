#include <fenv.h>
#include <stddef.h>
#include <stdio.h>
_Static_assert(sizeof(fenv_t)==8 && _Alignof(fenv_t)==4,"fenv ABI");
_Static_assert(offsetof(fenv_t,__fpcr)==0 && offsetof(fenv_t,__fpsr)==4,"fenv fields");
_Static_assert(sizeof(fexcept_t)==4,"except ABI");
_Static_assert(FE_ALL_EXCEPT==31 && FE_UPWARD==0x400000 && FE_DOWNWARD==0x800000
    && FE_TOWARDZERO==0xc00000 && FE_TONEAREST==0,"fenv constants");
extern int environment_probe(void),environment_arithmetic(void);
int main(void) {
    for (unsigned mask=0;mask<32;++mask) {
        feclearexcept(FE_ALL_EXCEPT);
        feraiseexcept(mask);
        printf("raise %u -> %d\n",mask,fetestexcept(FE_ALL_EXCEPT));
    }
    feclearexcept(FE_ALL_EXCEPT);
    int library=environment_probe(),arithmetic=environment_arithmetic();
    printf("fenv library=%d arithmetic=%d\n",library,arithmetic);
    return library || arithmetic;
}
