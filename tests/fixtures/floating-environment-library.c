/* Frozen AArch64 Linux GNU fenv ABI: glibc and musl use these same encodings,
 * but glibc raise/update may additionally raise INEXACT for overflow/underflow.
 * Explicit declarations let the two abstract interpreters execute the same
 * unchanged probe; the native driver independently checks the system header.
 */
typedef struct { unsigned control, status; } Environment;
extern int fegetround(void), fesetround(int);
extern int feclearexcept(int), feraiseexcept(int), fetestexcept(int);
extern int fegetexceptflag(unsigned *, int), fesetexceptflag(const unsigned *, int);
extern int fegetenv(Environment *), fesetenv(const Environment *);
extern int feholdexcept(Environment *), feupdateenv(const Environment *);

int environment_probe(void) {
    Environment original, saved;
    unsigned flag;
    if (fegetenv(&original)) return 1;
    for (int rounding=0;rounding<4;++rounding) {
        if (fesetround(rounding<<22) || fegetround()!=(rounding<<22)) return 2;
        for (unsigned mask=0;mask<32;++mask) {
            unsigned raised=mask | ((mask&12) ? 16 : 0);
            if (feclearexcept(31) || feraiseexcept(mask) || fetestexcept(31)!=(int)raised) return 3;
            if (fegetexceptflag(&flag,21) || flag!=(raised&21)) return 4;
            if (feclearexcept(21) || fetestexcept(31)!=(int)(raised&10)) return 5;
            if (fesetexceptflag(&flag,21) || fetestexcept(31)!=(int)raised) return 6;
            if (feholdexcept(&saved) || saved.control!=(unsigned)(rounding<<22)
                || saved.status!=raised || fetestexcept(31)!=0) return 7;
            if (feraiseexcept(2) || feupdateenv(&saved)
                || fetestexcept(31)!=(int)(raised|2)) return 8;
            if (fesetenv(&saved) || fetestexcept(31)!=(int)raised) return 9;
            flag=mask;
            if (fesetexceptflag(&flag,31) || fetestexcept(31)!=(int)mask) return 13;
        }
    }
    /* Invalid rounding request returns nonzero and leaves the state alone. */
    if (fesetround(1)==0 || fegetround()!=0xc00000) return 10;
    if (fesetenv((const Environment *)-1L) || fegetround()!=0 || fetestexcept(31)!=0) return 11;
    if (fesetenv(&original)) return 12;
    return 0;
}

int environment_arithmetic(void) {
    Environment original;
    if (fegetenv(&original) || fesetenv((const Environment *)-1L)) return 20;
    volatile double one=1.0, half=0x1p-53, zero=0.0, three=3.0;
    if (fesetround(0x400000)) return 21;
    union { double value; unsigned long bits; } up={.value=one+half};
    if (up.bits!=0x3ff0000000000001UL || fetestexcept(31)!=16) return 22;
    volatile double infinity=one/zero;
    if (fetestexcept(31)!=18) return 23;
    volatile double nan=zero/zero;
    if (fetestexcept(31)!=19) return 24;
    if (feclearexcept(31)) return 25;
    volatile double inexact=one/three;
    if (fetestexcept(31)!=16) return 26;
    (void)infinity; (void)nan; (void)inexact;
    return fesetenv(&original);
}
