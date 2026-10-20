/* L06: the caller side of tiny-vector-abi-shapes.h. Exit 0 when every call
 * passes; otherwise one bit per failing group: 1 b1, 2 b2, 4 b4, 8 h1, 16 h2
 * and 32 i1 (result and round trip), 64 register and anonymous arguments,
 * 128 stack arguments and records. */
#include "tiny-vector-abi-shapes.h"

int main(void)
{
    int fail = 0;
    b1 a = make_b1(), ea = echo_b1((b1){ 254 });
    fail |= (a[0] != 201 || ea[0] != 255) << 0;
    b2 b = make_b2(), eb = echo_b2((b2){ 1, 255 });
    fail |= (b[0] != 55 || b[1] != 66 || eb[0] != 2 || eb[1] != 0) << 1;
    b4 c = make_b4(), ec = echo_b4((b4){ 1, 2, 3, 4 });
    fail |= (c[0] != 11 || c[1] != 22 || c[2] != 33 || c[3] != 44
             || ec[0] != 2 || ec[1] != 3 || ec[2] != 4 || ec[3] != 5) << 2;
    h1 d = make_h1(), ed = echo_h1((h1){ -9 });
    fail |= (d[0] != -3 || ed[0] != -8) << 3;
    h2 e = make_h2(), ee = echo_h2((h2){ 65535, 7 });
    fail |= (e[0] != 777 || e[1] != 65000 || ee[0] != 0 || ee[1] != 8) << 4;
    i1 f = make_i1(), ef = echo_i1((i1){ -2 });
    fail |= (f[0] != -5 || ef[0] != -1) << 5;

    b1 pa = { 7 };
    b2 pb = { 8, 9 };
    b4 pc = { 10, 11, 12, 13 };
    h1 pd = { -14 };
    h2 pe = { 15, 16 };
    i1 pf = { -17 };
    unsigned long hash = mix(0, pa[0]);
    hash = mix(mix(hash, pb[0]), pb[1]);
    hash = mix(mix(mix(mix(hash, pc[0]), pc[1]), pc[2]), pc[3]);
    hash = mix(hash, pd[0]);
    hash = mix(mix(hash, pe[0]), pe[1]);
    hash = mix(hash, pf[0]);
    unsigned long anonymous = mix(mix(mix(5, pa[0]), pb[0]), pb[1]);
    anonymous = mix(mix(mix(anonymous, pd[0]), pe[0]), pe[1]);
    anonymous = mix(anonymous, pf[0]);
    fail |= (take_all(pa, pb, pc, pd, pe, pf) != hash
             || take_anonymous(5, pa, pb, pd, pe, pf) != anonymous) << 6;

    struct tagged t = { { 21, 22, 23, 24 }, -25 };
    unsigned long stacked = 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8;
    stacked = mix(stacked, pa[0]);
    stacked = mix(mix(mix(mix(stacked, pc[0]), pc[1]), pc[2]), pc[3]);
    stacked = mix(mix(stacked, pd[0]), pf[0]);
    stacked = mix(mix(mix(mix(stacked, t.x[0]), t.x[1]), t.x[2]), t.x[3]);
    stacked = mix(stacked, t.tag);
    struct tagged u = echo_tagged(t);
    fail |= (take_stacked(1, 2, 3, 4, 5, 6, 7, 8, pa, pc, pd, pf, t) != stacked
             || u.x[0] != 22 || u.x[3] != 25 || u.tag != -24) << 7;
    return fail;
}
