/* Driver side of the floating-complex crossed ABI probe (L04). Exit 0
 * means every call agreed; otherwise the exit status names the first
 * failed check. */
#include "floating-complex-abi.h"

static cd sub(cd a, cd b) { return a - b; }
#define C(x, y) __builtin_complex(x, y)

int main(void)
{
    if (w_f(C(1.0f, 2.0f), C(3.0f, 4.0f)) != C(7.0f, 10.0f)) return 1;
    if (w_d(C(1.0, 2.0), 3.0, C(4.0, 5.0)) != C(7.0, 11.0)) return 2;
    if (w_q(C(9.0L, 8.0L), C(1.0L, 2.0L)) != C(8.0L, 6.0L)) return 3;
    if (w_many(C(1.0, 1.0), C(1.0, 2.0), C(1.0, 3.0), C(1.0, 4.0), C(1.0, 5.0))
        != C(31.0, 129.0)) return 4;
    if (w_mixed(1.0f, C(2.0, 3.0), C(4.0f, 5.0f), C(6.0L, 7.0L), 8.0) != C(21.0, 15.0))
        return 5;
    if (w_spill(1, 2, 3, 4, 5, 6, 7, C(10.0L, 20.0L), C(100.0f, 200.0f))
        != C(138.0f, 220.0f)) return 6;
    if (w_va(2, C(1.0, 2.0), C(3.0f, 4.0f), C(5.0L, 6.0L),
             C(1.0, 0.0), C(0.0f, 0.0f), C(0.0L, 0.0L)) != 654322.0) return 7;
    struct tagged t = { 3, C(1.0, 2.0) };
    struct hfa3 h = { C(10.0, 20.0), 5.0 };
    struct hq q = { C(100.0L, 200.0L) };
    if (w_struct(t, h, q) != C(118.0, 226.0)) return 8;
    struct hfa3 r = w_ret3(C(1.5, -2.5));
    if (r.z != C(3.0, -5.0) || r.w != 5.0) return 9;
    if (w_cb(sub, C(5.0, 5.0), C(1.0, 2.0)) != C(12.0, 9.0)) return 10;
    struct hq rq = w_retq(C(-1.0L, 2.0L));
    if (rq.z != C(0.0L, 2.0L)) return 11;
    struct hf rf = w_retf(C(1.0f, 2.0f), C(3.0f, 4.0f));
    if (rf.a != C(3.0f, 4.0f) || rf.b != C(1.0f, 2.0f)) return 12;
    struct mixed m = { C(1.0f, -1.0f), 0.5f, 4 };
    if (w_records(rf, m) != C(6.5f, -2.0f)) return 13;
    struct hf hv = { C(1.0f, 2.0f), C(3.0f, 4.0f) };
    struct hq qv = { C(100.0L, 200.0L) };
    if (w_va_q(2, hv, qv, hv, qv) != C(262.0L, 484.0L)) return 14;
    return 0;
}
