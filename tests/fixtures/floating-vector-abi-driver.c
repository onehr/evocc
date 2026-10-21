/* Driver side of the floating-vector crossed ABI probe (L05). Exit 0
 * means every call agreed; otherwise the exit status names the first
 * failed check. */
#include "floating-vector-abi.h"

static f4 swap(f4 a, d2 b)
{
    f4 r = {a[3], a[2], (float)b[1], (float)b[0]};
    return r;
}

int main(void)
{
    f2 a2 = {1, 2}, b2 = {3, 4};
    f4 a4 = {1, 2, 3, 4}, b4 = {10, 20, 30, 40};
    d1 a1 = {6}, b1 = {0.5};
    d2 ad = {1.5, 2.5}, bd = {0.25, 0.5};

    f2 r2 = w_f2(a2, b2);
    if (r2[0] != 7 || r2[1] != 10) return 1;
    f4 r4 = w_f4(a4, 3, b4);
    if (r4[0] != -7 || r4[3] != -28) return 2;
    d1 r1 = w_d1(a1, 4, b1);
    if (r1[0] != 2) return 3;
    d2 rd = w_d2(ad, bd);
    if (rd[0] != 1.25 || rd[1] != 2) return 4;
    r4 = w_mix(0.5f, a1, 2, a2, ad, b4);
    if (r4[0] != 10.5f || r4[1] != 26 || r4[2] != 35 || r4[3] != 44) return 5;
    rd = w_stack(a4, a4, a4, a4, a4, a4, a4, a1, bd, b2);
    if (rd[0] != 250 + 7 + 6 + 300 || rd[1] != 500 + 28 + 400) return 6;
    if (w_va(2, a2, a1, a4, ad, b2, b1, b4, bd)
        != 2 * (1 + 4 + 24 + 8 + 64 + 48 + 160) + (3 + 8 + 2 + 80 + 640 + 8 + 32))
        return 7;

    struct h2 h2 = {a2, a1};
    h2 = w_h2(h2, b2);
    if (h2.a[0] != 4 || h2.a[1] != 6 || h2.b[0] != 2) return 8;
    struct h3 h3 = {a4, ad, {7, 8, 9, 10}};
    h3 = w_h3(h3);
    if (h3.a[3] != 8 || h3.b[1] != 7.5 || h3.c[0] != 11 || h3.c[3] != 14) return 9;
    struct h4 h4 = {{{1}, {2}, {3}, {4}}};
    h4 = w_h4(h4, b1);
    if (h4.a[0][0] != 4.5 || h4.a[3][0] != 1.5) return 10;
    struct h5 h5 = {{{1, 2}, {3, 4}, {5, 6}, {7, 8}, {9, 10}}};
    h5 = w_h5(h5);
    if (h5.a[0][1] != 2 || h5.a[2][0] != 15 || h5.a[4][1] != 50) return 11;
    struct hd hd = {{1, 2}, {3, 4}, {5, 6}, {7, 8}};
    hd = w_hd(hd);
    if (hd.a[0] != 7 || hd.a[1] != 8.5 || hd.b[1] != 6 || hd.d[0] != 1) return 12;
    struct mixed m = {a2, b4};
    m = w_mixed(m);
    if (m.a[0] != -19 || m.a[1] != -18 || m.b[2] != -19 || m.b[3] != 40) return 13;
    union lanes u;
    u.d = ad;
    u = w_union(u);
    if (u.d[0] != 1.5 || u.d[1] != -2.5) return 14;
    struct padded pad = {a2};
    pad = w_padded(pad, b2);
    if (pad.a[0] != 3 || pad.a[1] != 8) return 22;
    struct h3 spill = {b4, bd, {1, 2, 3, 4}};
    rd = w_spill(a4, a4, a4, a4, a4, a4, spill, a1);
    if (rd[0] != 0.25 + 6 + 20 + 3 + 60 || rd[1] != 0.5 + 24) return 15;

    struct h2 va = {b2, b1}, va2 = {a2, a1};
    struct h3 vb = {b4, bd, {5, 6, 7, 8}}, vb2 = {a4, ad, {1, 2, 3, 4}};
    struct hd vc = {{1, 2}, {3, 4}, {5, 6}, {7, 8}}, vc2 = {{9, 10}, {}, {}, {11, 12}};
    struct mixed vd = {a2, b4}, vd2 = {b2, a4};
    union lanes ve, ve2;
    ve.d = ad;
    ve2.f = b4;
    if (w_va_h2(2, va, va2) != 2 * (4 + 1) + (2 + 12)) return 16;
    if (w_va_h3(2, vb, vb2) != 2 * (40 + 2 + 80) + (4 + 10 + 16)) return 17;
    if (w_va_hd(2, vc, vc2) != 2 * (1 + 256) + (9 + 384)) return 18;
    if (w_va_union(2, ve, ve2) != 2 * (1.5 + 256 * ve.f[3]) + (ve2.d[0] + 256 * 40))
        return 19;
    if (w_va_mixed(2, vd, vd2) != 2 * (64 + 5120) + (192 + 512)) return 20;
    r4 = w_callback(swap, a4, ad);
    if (r4[0] != 6 || r4[1] != 7 || r4[2] != 8.5f || r4[3] != 9.5f) return 21;
    return 0;
}
