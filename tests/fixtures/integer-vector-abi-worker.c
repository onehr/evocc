#include "integer-vector-abi.h"
i4 vector_add(i4 a, i4 b) { return a + b; }
s4 vector_small(s4 a, s4 b) { return a + b; }
long vector_ninth(int n, i4 a, i4 b, i4 c, i4 d, i4 e, i4 f, i4 g, i4 h, i4 i, long tail)
{ return n + a[0] + b[1] + c[2] + d[3] + e[0] + f[1] + g[2] + h[3] + i[2] + tail; }
struct hva vector_hva(struct hva a, i4 b) {
    a.x = a.x + b; a.y = a.y + b; a.z = a.z + b; return a;
}
i8 vector_wide(i8 a) { return a + 3; }
i4 vector_preserved(i4 a) {
    i4 b = a + 2;
    int n = vector_callback(7);
    return b + n;
}
long vector_mixed_stack(long a,long b,long c,long d,long e,long f,long g,long h,
                       s4 v0,s4 v1,s4 v2,s4 v3,s4 v4,s4 v5,s4 v6,s4 v7,s4 v8,__int128 last)
{
    return a+b+c+d+e+f+g+h+v0[0]+v1[0]+v2[0]+v3[0]+v4[0]+v5[0]+v6[0]+v7[0]+v8[0]
        +(long)last+(long)(last>>64);
}
struct hva vector_hva_stack(i4 a,i4 b,i4 c,i4 d,i4 e,i4 f,struct hva h,long n)
{
    h.x += a+b+c; h.y += d+e+f; h.z += (int)n;
    return h;
}
