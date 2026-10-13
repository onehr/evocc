/* L06 probe worker: the callee side of l06-tiny-vector-abi.h. */
#include <stdarg.h>
#include "l06-tiny-vector-abi.h"

volatile unsigned seen;

void take4(int lead, v4 x, v2 y, v1 z)
{
    seen = (unsigned)lead + x[0] + x[3] * 256u + y[1] * 65536u + z[0] * 16777216u;
}
v4 give4(void) { return (v4){ 11, 22, 33, 44 }; }
v2 give2(void) { return (v2){ 55, 66 }; }
s2 give_s2(void) { return (s2){ 777, 888 }; }
v4 echo4(v4 x) { return x + 1; }
struct holder echo_holder(struct holder h) { h.x += 1; h.tag += 1; return h; }
unsigned sum_variadic(int count, ...)
{
    va_list ap;
    unsigned sum = 0;
    va_start(ap, count);
    for (int i = 0; i < count; i++) {
        v4 x = va_arg(ap, v4);
        sum += x[0] + x[1] + x[2] + x[3];
    }
    va_end(ap);
    return sum;
}
