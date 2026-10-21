/* Complex <tgmath.h> calls (L04 and D20). Each line prints the expression,
 * the size of its type and both parts of its value. Build it with and
 * without -D_GNU_SOURCE; glibc's macros then list more functions. */
#include <stdio.h>
#include <tgmath.h>
static volatile float _Complex vf = 0.5f + 0.25if;
static volatile double _Complex vd = -1.5 + 2.0i;
static volatile long double _Complex vl = 3.0L - 0.75iL;
static volatile _Complex int vi = 2 + 3i;
static volatile double vr = 0.75;
#define SHOW(e)                                                                     \
    do {                                                                            \
        long double _Complex value = (e);                                           \
        printf("%-24s %2zu %La %La\n", #e, sizeof(e), __real__ value, __imag__ value); \
    } while (0)
int main(void)
{
    float _Complex f = vf;
    double _Complex d = vd;
    long double _Complex l = vl;
    _Complex int i = vi;
    double r = vr;
    SHOW(sqrt(f)); SHOW(sqrt(d)); SHOW(sqrt(l)); SHOW(sqrt(i));
    SHOW(exp(f)); SHOW(log(d)); SHOW(sin(l)); SHOW(cos(f));
    SHOW(tan(d)); SHOW(asin(l)); SHOW(acos(f)); SHOW(atan(d));
    SHOW(sinh(l)); SHOW(cosh(f)); SHOW(tanh(d)); SHOW(asinh(l));
    SHOW(acosh(f)); SHOW(atanh(d));
    SHOW(pow(f, f)); SHOW(pow(d, r)); SHOW(pow(r, l)); SHOW(pow(f, 2));
    SHOW(fabs(f)); SHOW(fabs(d)); SHOW(fabs(l)); SHOW(fabs(i));
    SHOW(carg(f)); SHOW(carg(d)); SHOW(carg(l)); SHOW(carg(r));
    SHOW(cimag(f)); SHOW(creal(d)); SHOW(conj(l)); SHOW(cproj(d));
    SHOW(conj(r)); SHOW(cimag(r)); SHOW(sqrt(r)); SHOW(exp(sqrt(d)));
    return 0;
}
