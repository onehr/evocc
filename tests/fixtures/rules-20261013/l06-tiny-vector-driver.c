/* L06 probe driver: exit 0 when every crossed call passes; otherwise the bit
 * of each failing direction is set: 1 arguments, 2 v4 result, 4 v2 result,
 * 8 two-lane short result, 16 both directions, 32 record member, 64 varargs. */
#include "l06-tiny-vector-abi.h"

int main(void)
{
    int fail = 0;
    take4(1, (v4){ 2, 0, 0, 3 }, (v2){ 0, 4 }, (v1){ 5 });
    fail |= (seen != 1u + 2u + 3u * 256u + 4u * 65536u + 5u * 16777216u) << 0;
    v4 a = give4();
    fail |= (a[0] != 11 || a[1] != 22 || a[2] != 33 || a[3] != 44) << 1;
    v2 b = give2();
    fail |= (b[0] != 55 || b[1] != 66) << 2;
    s2 c = give_s2();
    fail |= (c[0] != 777 || c[1] != 888) << 3;
    v4 d = echo4((v4){ 1, 2, 3, 4 });
    fail |= (d[0] != 2 || d[3] != 5) << 4;
    struct holder h = echo_holder((struct holder){ { 1, 2, 3, 4 }, 9 });
    fail |= (h.x[0] != 2 || h.x[3] != 5 || h.tag != 10) << 5;
    fail |= (sum_variadic(2, (v4){ 1, 2, 3, 4 }, (v4){ 5, 6, 7, 8 }) != 36) << 6;
    return fail;
}
