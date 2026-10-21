/* Worker side of the floating-complex crossed ABI probe (L04). The
 * functions use only per-part arithmetic, so no runtime helper is called. */
#include "floating-complex-abi.h"

__attribute__((noinline)) cf w_f(cf a, cf b) { return a + b * 2; }
__attribute__((noinline)) cd w_d(cd a, double x, cd b) { return a * x + b; }
__attribute__((noinline)) cq w_q(cq a, cq b) { return a - b; }

/* Five complex doubles need ten SIMD registers; the fifth goes on the stack. */
__attribute__((noinline)) cd w_many(cd a0, cd a1, cd a2, cd a3, cd a4)
{
    return a0 + a1 * 2 + a2 * 4 + a3 * 8 + a4 * 16;
}

/* Mixed scalar and complex arguments share the SIMD cursor. */
__attribute__((noinline)) cd w_mixed(float f, cd a, cf b, cq c, double d)
{
    return f + a + (cd)b + (cd)c + d;
}

/* After seven used SIMD registers a two-member HFA goes to the stack
 * (AAPCS64 C.3), and the later complex float does not backfill v7. */
__attribute__((noinline)) cf w_spill(double r0, double r1, double r2, double r3,
                                     double r4, double r5, double r6, cq c, cf g)
{
    return (cf)(r0 + r1 + r2 + r3 + r4 + r5 + r6) + (cf)c + g;
}

__attribute__((noinline)) double w_va(int n, ...)
{
    va_list ap;
    double sum = 0;
    va_start(ap, n);
    for (int i = 0; i < n; ++i) {
        cd z = va_arg(ap, cd);
        cf f = va_arg(ap, cf);
        cq q = va_arg(ap, cq);
        sum += __real__ z * 1 + __imag__ z * 10 + __real__ f * 100 + __imag__ f * 1000
             + (double)__real__ q * 10000 + (double)__imag__ q * 100000;
    }
    va_end(ap);
    return sum;
}

__attribute__((noinline)) cd w_struct(struct tagged t, struct hfa3 h, struct hq q)
{
    return t.z * t.tag + h.z + h.w + (cd)q.z;
}

__attribute__((noinline)) struct hfa3 w_ret3(cd z)
{
    struct hfa3 h = { z * 2, 5 };
    return h;
}

__attribute__((noinline)) struct hq w_retq(cq z)
{
    struct hq h = { z + 1 };
    return h;
}

__attribute__((noinline)) struct hf w_retf(cf a, cf b)
{
    struct hf h = { b, a };
    return h;
}

__attribute__((noinline)) cf w_records(struct hf h, struct mixed m)
{
    return h.a - h.b + m.a * m.c + m.b;
}

__attribute__((noinline)) cd w_cb(cd (*f)(cd, cd), cd a, cd b) { return f(a, b) * 3; }

/* A variadic long double complex result after variadic records. */
__attribute__((noinline)) cq w_va_q(int n, ...)
{
    va_list ap;
    cq sum = 0;
    va_start(ap, n);
    for (int i = 0; i < n; ++i) {
        struct hf h = va_arg(ap, struct hf);
        struct hq q = va_arg(ap, struct hq);
        sum += q.z + (cq)h.a + (cq)h.b * 10;
    }
    va_end(ap);
    return sum;
}
