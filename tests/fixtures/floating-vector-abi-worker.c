/* Worker side of the floating-vector crossed ABI probe (L05). Every value
 * is a small integer or half, so each operation is exact, fused or not. */
#include "floating-vector-abi.h"

__attribute__((noinline)) f2 w_f2(f2 a, f2 b) { return a + b * 2; }
__attribute__((noinline)) f4 w_f4(f4 a, float s, f4 b) { return a * s - b; }
__attribute__((noinline)) d1 w_d1(d1 a, double s, d1 b) { return a / s + b; }
__attribute__((noinline)) d2 w_d2(d2 a, d2 b) { return a - b; }

/* Scalars and vectors of both sizes share the SIMD argument cursor. */
__attribute__((noinline)) f4 w_mix(float x, d1 a, double y, f2 b, d2 c, f4 d)
{
    f4 r = d;
    r[0] += x;
    r[1] += (float)a[0];
    r[2] += (float)y + b[0] + b[1];
    r[3] += (float)(c[0] + c[1]);
    return r;
}

/* Eight vectors fill v0 to v7; the ninth and tenth go to the stack, the
 * 16-byte one aligned to 16. */
__attribute__((noinline)) d2 w_stack(f4 a0, f4 a1, f4 a2, f4 a3, f4 a4, f4 a5,
                                     f4 a6, d1 a7, d2 s0, f2 s1)
{
    f4 sum = a0 + a1 + a2 + a3 + a4 + a5 + a6;
    d2 r = s0 * 1000;
    r[0] += sum[0] + a7[0] + s1[0] * 100;
    r[1] += sum[3] + s1[1] * 100;
    return r;
}

__attribute__((noinline)) double w_va(int n, ...)
{
    va_list ap;
    double sum = 0;
    va_start(ap, n);
    for (int k = 0; k < n; ++k) {
        f2 a = va_arg(ap, f2);
        d1 b = va_arg(ap, d1);
        f4 c = va_arg(ap, f4);
        d2 d = va_arg(ap, d2);
        sum = sum * 2 + a[0] + a[1] * 2 + b[0] * 4 + c[0] * 8 + c[3] * 16 + d[0] * 32
            + d[1] * 64;
    }
    va_end(ap);
    return sum;
}

__attribute__((noinline)) struct h2 w_h2(struct h2 h, f2 v)
{
    h.a += v;
    h.b[0] -= v[1];
    return h;
}

__attribute__((noinline)) struct h3 w_h3(struct h3 h)
{
    struct h3 r = {h.a * 2, h.b * 3, h.c + 4};
    return r;
}

__attribute__((noinline)) struct h4 w_h4(struct h4 h, d1 v)
{
    struct h4 r = {{h.a[3] + v, h.a[2] + v, h.a[1] + v, h.a[0] + v}};
    return r;
}

/* Five members make no HVA: the record travels by reference and returns
 * through x8. */
__attribute__((noinline)) struct h5 w_h5(struct h5 h)
{
    for (int k = 0; k < 5; ++k)
        h.a[k] = h.a[k] * (float)(k + 1);
    return h;
}

__attribute__((noinline)) struct hd w_hd(struct hd h)
{
    struct hd r = {h.d, h.c, h.b, h.a};
    r.a[1] += 0.5;
    return r;
}

__attribute__((noinline)) struct mixed w_mixed(struct mixed m)
{
    m.a = m.a - m.b[1];
    m.b[2] = m.a[0];
    return m;
}

__attribute__((noinline)) union lanes w_union(union lanes u)
{
    u.d[1] = -u.d[1];
    return u;
}

/* The record is 16 bytes but its member is 8: it is no HVA and travels in
 * general registers. */
__attribute__((noinline)) struct padded w_padded(struct padded p, f2 v)
{
    p.a = p.a * v;
    return p;
}

/* Six vectors use v0 to v5; the three-member HVA does not fit in v6 and
 * v7, so it goes wholly to the stack, and the later vector does not take
 * v6 back (AAPCS64 C.3). */
__attribute__((noinline)) d2 w_spill(f4 a0, f4 a1, f4 a2, f4 a3, f4 a4, f4 a5,
                                     struct h3 h, d1 late)
{
    f4 sum = a0 + a1 + a2 + a3 + a4 + a5;
    d2 r = h.b;
    r[0] += sum[0] + h.a[1] + (double)h.c[2] + late[0] * 10;
    r[1] += sum[3];
    return r;
}

/* Each variadic function takes records of one type. With both sides built
 * by Apple clang 21, an anonymous HVA of 16-byte vectors after a record
 * passed by reference arrives at the wrong place (measured on this host),
 * so this probe does not mix record types. */
#define RECORDS(name, type, weight)                                          \
    __attribute__((noinline)) double name(int n, ...)                        \
    {                                                                        \
        va_list ap;                                                          \
        double sum = 0;                                                      \
        va_start(ap, n);                                                     \
        for (int k = 0; k < n; ++k) {                                        \
            type r = va_arg(ap, type);                                       \
            sum = sum * 2 + weight(r);                                       \
        }                                                                    \
        va_end(ap);                                                          \
        return sum;                                                          \
    }
#define H2(r) ((r).a[1] + (r).b[0] * 2)
#define H3(r) ((r).a[3] + (r).b[1] * 4 + (r).c[0] * 16)
#define HD(r) ((r).a[0] + (r).d[1] * 32)
#define UNION(r) ((r).d[0] + (r).f[3] * 256)
#define MIXED(r) ((r).a[0] * 64 + (r).b[3] * 128)
RECORDS(w_va_h2, struct h2, H2)
RECORDS(w_va_h3, struct h3, H3)
RECORDS(w_va_hd, struct hd, HD)
RECORDS(w_va_union, union lanes, UNION)
RECORDS(w_va_mixed, struct mixed, MIXED)

__attribute__((noinline)) f4 w_callback(f4 (*f)(f4, d2), f4 a, d2 b)
{
    f4 kept = a * 2;
    f4 r = f(a, b);
    return r + kept;
}
