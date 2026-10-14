#include "shapes.h"

int takes_sc(sc c) { return c + 10; }
int takes_char(char c) { return c + 10; }
int takes_short(short s) { return s + 10; }
int takes_uc(uc c) { return c + 10; }
int takes_us(us s) { return s + 10; }
int takes_bool(_Bool b) { return b + 10; }
int takes_mixed(sc a, short b, uc c, us d, char e) { return a + b * 3 + c * 5 + d * 7 + e * 11; }

sc gives_sc(int x) { return (sc)x; }
char gives_char(int x) { return (char)x; }
short gives_short(int x) { return (short)x; }
uc gives_uc(int x) { return (uc)x; }
us gives_us(int x) { return (us)x; }
_Bool gives_bool(int x) { return x != 0; }

narrow_fn pick_takes_sc(void) { return takes_sc; }
narrow_result_fn pick_gives_sc(void) { return gives_sc; }

long pair_after_one(long a, __int128 b) { return a * 1000 + (long)b + (long)(b >> 64) * 100; }
long pair_after_three(long a, long b, long c, __int128 d, long e)
{
    return a + b * 2 + c * 3 + (long)d * 4 + (long)(d >> 64) * 5 + e * 6;
}
__int128 pair_result(long a, __int128 b) { return b * a; }

float tenth(float a, float b, float c, float d, float e, float f, float g, float h, float i,
            float j)
{
    return a + b + c + d + e + f + g + h + i * 100 + j * 1000;
}

long ints(long a0, long a1, long a2, long a3, long a4, long a5, long a6, long a7, sc s8, uc u9,
          short s10, us u11, int i12, sc s13, long l14, char c15)
{
    return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + s8 * 3 + u9 * 5 + s10 * 7 + u11 * 11
           + i12 * 13 + s13 * 17 + l14 * 19 + c15 * 23;
}

double floats(double d0, double d1, double d2, double d3, double d4, double d5, double d6,
              double d7, float f8, double d9, float f10, float f11, double d12, float f13,
              float f14, double d15)
{
    return d0 + d1 + d2 + d3 + d4 + d5 + d6 + d7 + f8 * 3 + d9 * 5 + f10 * 7 + f11 * 11
           + d12 * 13 + f13 * 17 + f14 * 19 + d15 * 23;
}

long records(long a0, long a1, long a2, long a3, long a4, long a5, long a6, long a7, char c8,
             struct r3 r, char c9, struct r12 q, char c10, struct a16 al, char c11)
{
    return a0 + a7 + c8 * 3 + r.c[0] * 5 + r.c[2] * 7 + c9 * 11 + q.a * 13 + q.c * 17
           + c10 * 19 + al.a * 23 + al.b * 29 + c11 * 31;
}

double floating_records(double d0, double d1, double d2, double d3, double d4, double d5,
                        double d6, double d7, float f8, struct h3 h, float f9, struct h2d g,
                        float f10)
{
    return d0 + d7 + f8 * 3 + h.a * 5 + h.c * 7 + f9 * 11 + g.a * 13 + g.b * 17 + f10 * 19;
}

long pair_on_stack(long a0, long a1, long a2, long a3, long a4, long a5, long a6, long a7,
                   char c8, __int128 w, char c9)
{
    return a0 + a7 + c8 * 3 + (long)w * 5 + (long)(w >> 64) * 7 + c9 * 11;
}

long record_split(long a0, long a1, long a2, long a3, long a4, long a5, long a6, struct r12 q,
                  long z)
{
    return a0 + a6 * 2 + q.a * 3 + q.b * 5 + q.c * 7 + z * 11;
}

long interleaved(int i0, double d0, sc i1, float d1, short i2, double d2, int i3, float d3,
                 long i4, double d4, uc i5, float d5, us i6, double d6, int i7, float d7,
                 sc i8, float d8, short i9, double d9, char i10, float d10)
{
    return i0 + i1 * 2 + i2 * 3 + i3 * 4 + i4 * 5 + i5 * 6 + i6 * 7 + i7 * 8 + i8 * 9
           + i9 * 10 + i10 * 11
           + (long)(d0 + d1 * 2 + d2 * 3 + d3 * 4 + d4 * 5 + d5 * 6 + d6 * 7 + d7 * 8
                    + d8 * 9 + d9 * 10 + d10 * 11);
}

long vectors(v4i a, v4i b, v4i c, v4i d, v4i e, v4i f, v4i g, v4i h, v2i i, v4i j, v2i k)
{
    return a[0] + b[1] + c[2] + d[3] + e[0] + f[1] + g[2] + h[3] + i[0] * 3 + i[1] * 5
           + j[0] * 7 + j[3] * 11 + k[0] * 13 + k[1] * 17;
}
