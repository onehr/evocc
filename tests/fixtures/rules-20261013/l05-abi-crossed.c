/* L05 probe: floating-vector calls across separately compiled units.
 * Build the same file twice: PART=1 defines the callees, PART=2 the
 * caller; link one object from each compiler, in both directions.
 * CASE=0: 8/16-byte short vectors, a shared SIMD argument cursor with
 * scalars, a ninth vector on the stack, HVAs of two and four members,
 * an HVA whose members have different lane types (AAPCS64 tests only the
 * 128-bit short-vector type), a non-HVA record, a 32-byte vector,
 * anonymous vector arguments and a vector live across a call.
 * CASE=1: a 4-byte float vector, which is not an AAPCS64 short vector.
 * Exit status 0 means every check passed. */
#include <stdarg.h>
#ifndef CASE
#define CASE 0
#endif
#ifndef PART
#define PART 2
#endif
typedef float f4 __attribute__((vector_size(16)));
typedef float f2 __attribute__((vector_size(8)));
typedef float f1 __attribute__((vector_size(4)));
typedef double d2 __attribute__((vector_size(16)));
typedef double d1 __attribute__((vector_size(8)));
typedef float f8 __attribute__((vector_size(32)));
struct hf { f4 x, y; };
struct hd { d2 a, b, c, d; };
struct hmix { f4 x; d2 y; };
struct nonh { f4 x; float s; };

#if CASE == 0
f4 vadd(f4, f4);
d2 dadd(d2, d2);
f2 sadd(f2, f2);
d1 dneg(d1);
float mix(float, f4, double, d2, f2);
f4 ninth(f4, f4, f4, f4, f4, f4, f4, f4, f4);
struct hf hfa(struct hf, f4);
struct hd hd4(struct hd);
struct hmix mixed(struct hmix);
struct nonh nonhva(struct nonh);
f8 wide(f8);
double vsum(int, ...);
f4 keep(f4);
f4 callback(f4);

#if PART == 1
f4 vadd(f4 a, f4 b) { return a + b; }
d2 dadd(d2 a, d2 b) { return a + b; }
f2 sadd(f2 a, f2 b) { return a + b; }
d1 dneg(d1 a) { return -a; }
float mix(float a, f4 b, double c, d2 d, f2 e)
{
    return a + b[3] + (float)c + (float)d[1] + e[1];
}
f4 ninth(f4 a, f4 b, f4 c, f4 d, f4 e, f4 f, f4 g, f4 h, f4 i)
{
    return a + b + c + d + e + f + g + h + i * 100;
}
struct hf hfa(struct hf h, f4 v)
{
    h.x += v;
    h.y -= v;
    return h;
}
struct hd hd4(struct hd h)
{
    struct hd r = {h.d, h.c, h.b, h.a};
    return r;
}
struct hmix mixed(struct hmix h)
{
    h.x *= 2;
    h.y *= 3;
    return h;
}
struct nonh nonhva(struct nonh h)
{
    h.x -= h.s;
    h.s = h.x[3];
    return h;
}
f8 wide(f8 a) { return a + a; }
double vsum(int n, ...)
{
    va_list ap;
    double s = 0;
    va_start(ap, n);
    for (int k = 0; k < n; ++k) {
        f4 x = va_arg(ap, f4);
        d2 y = va_arg(ap, d2);
        s += x[0] + x[3] + y[0] + y[1];
    }
    va_end(ap);
    return s;
}
f4 keep(f4 a)
{
    f4 b = a * 2;
    f4 c = callback(a);
    return b + c;
}
#else
f4 callback(f4 a)
{
    /* Disturb the SIMD registers that the callee may hold live. */
    volatile f4 t = a * 3;
    return t - a;
}
int main(void)
{
    f4 a = {1, 2, 3, 4}, b = {10, 20, 30, 40};
    f4 r = vadd(a, b);
    if (r[0] != 11 || r[3] != 44) return 1;
    d2 d = dadd((d2){1.5, 2.5}, (d2){0.25, 0.5});
    if (d[0] != 1.75 || d[1] != 3.0) return 2;
    f2 s = sadd((f2){1, 2}, (f2){3, 4});
    if (s[0] != 4 || s[1] != 6) return 3;
    d1 n = dneg((d1){2.5});
    if (n[0] != -2.5) return 4;
    if (mix(1, a, 2, (d2){3, 4}, (f2){5, 6}) != 1 + 4 + 2 + 4 + 6) return 5;
    r = ninth(a, a, a, a, a, a, a, a, b);
    if (r[0] != 8 + 1000 || r[3] != 32 + 4000) return 6;
    struct hf h = hfa((struct hf){a, b}, a);
    if (h.x[1] != 4 || h.y[2] != 27) return 7;
    struct hd q = hd4((struct hd){{1, 2}, {3, 4}, {5, 6}, {7, 8}});
    if (q.a[0] != 7 || q.b[1] != 6 || q.c[0] != 3 || q.d[1] != 2) return 8;
    struct hmix m = mixed((struct hmix){a, (d2){1, 2}});
    if (m.x[2] != 6 || m.y[1] != 6) return 9;
    struct nonh o = nonhva((struct nonh){b, 0.5f});
    if (o.x[0] != 9.5f || o.s != 39.5f) return 13;
    f8 w = wide((f8){1, 2, 3, 4, 5, 6, 7, 8});
    if (w[0] != 2 || w[7] != 16) return 10;
    if (vsum(2, a, (d2){5, 6}, b, (d2){7, 8}) != 1 + 4 + 5 + 6 + 10 + 40 + 7 + 8)
        return 11;
    r = keep(a);
    if (r[0] != 2 + 2 || r[3] != 8 + 8) return 12;
    return 0;
}
#endif
#elif CASE == 1
f1 tiny(f1, f1);
#if PART == 1
f1 tiny(f1 a, f1 b) { return a + b; }
#else
int main(void)
{
    f1 r = tiny((f1){1.5f}, (f1){2.0f});
    return r[0] != 3.5f;
}
#endif
#else
#error unsupported CASE
#endif
