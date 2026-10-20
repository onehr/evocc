/* L06: the callee side of tiny-vector-abi-shapes.h. */
#include <stdarg.h>
#include "tiny-vector-abi-shapes.h"

b1 make_b1(void) { return (b1){ 201 }; }
b2 make_b2(void) { return (b2){ 55, 66 }; }
b4 make_b4(void) { return (b4){ 11, 22, 33, 44 }; }
h1 make_h1(void) { return (h1){ -3 }; }
h2 make_h2(void) { return (h2){ 777, 65000 }; }
i1 make_i1(void) { return (i1){ -5 }; }
b1 echo_b1(b1 x) { return x + 1; }
b2 echo_b2(b2 x) { return x + 1; }
b4 echo_b4(b4 x) { return x + 1; }
h1 echo_h1(h1 x) { return x + 1; }
h2 echo_h2(h2 x) { return x + 1; }
i1 echo_i1(i1 x) { return x + 1; }

unsigned long take_all(b1 a, b2 b, b4 c, h1 d, h2 e, i1 f)
{
    unsigned long hash = mix(0, a[0]);
    hash = mix(mix(hash, b[0]), b[1]);
    hash = mix(mix(mix(mix(hash, c[0]), c[1]), c[2]), c[3]);
    hash = mix(hash, d[0]);
    hash = mix(mix(hash, e[0]), e[1]);
    return mix(hash, f[0]);
}

unsigned long take_stacked(long r0, long r1, long r2, long r3, long r4,
                           long r5, long r6, long r7, b1 a, b4 b, h1 c,
                           i1 d, struct tagged t)
{
    unsigned long hash = (unsigned long)(r0 + r1 + r2 + r3 + r4 + r5 + r6 + r7);
    hash = mix(hash, a[0]);
    hash = mix(mix(mix(mix(hash, b[0]), b[1]), b[2]), b[3]);
    hash = mix(mix(hash, c[0]), d[0]);
    hash = mix(mix(mix(mix(hash, t.x[0]), t.x[1]), t.x[2]), t.x[3]);
    return mix(hash, t.tag);
}

unsigned long take_anonymous(int count, ...)
{
    va_list ap;
    va_start(ap, count);
    unsigned long hash = (unsigned long)count;
    b1 a = va_arg(ap, b1);
    b2 b = va_arg(ap, b2);
    h1 c = va_arg(ap, h1);
    h2 d = va_arg(ap, h2);
    i1 e = va_arg(ap, i1);
    va_end(ap);
    hash = mix(mix(mix(hash, a[0]), b[0]), b[1]);
    hash = mix(mix(mix(hash, c[0]), d[0]), d[1]);
    return mix(hash, e[0]);
}

struct tagged echo_tagged(struct tagged t)
{
    t.x += 1;
    t.tag += 1;
    return t;
}
