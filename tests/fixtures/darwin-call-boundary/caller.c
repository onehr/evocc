#include "shapes.h"

/* Opaque values, so that each argument is computed at run time. */
static volatile int minus_one = -1, big = 255, wide = 65535, high = 200, six = 6;

#define CHECK(n, condition) \
    do {                    \
        if (!(condition))   \
            return n;       \
    } while (0)

int main(void)
{
    int m = minus_one, b = big, w = wide, h = high;

    CHECK(1, takes_sc((sc)m) == 9);
    CHECK(2, takes_char((char)m) == 9);
    CHECK(3, takes_short((short)m) == 9);
    CHECK(4, takes_uc((uc)b) == 265);
    CHECK(5, takes_us((us)w) == 65545);
    CHECK(6, takes_bool((_Bool)m) == 11);
    CHECK(7, takes_mixed((sc)m, (short)m, (uc)h, (us)w, (char)m) == -1 - 3 + 1000 + 458745 - 11);

    CHECK(8, gives_sc(b) + 10 == 9);
    CHECK(9, gives_char(b) + 10 == 9);
    CHECK(10, gives_short(w) + 10 == 9);
    CHECK(11, gives_uc(b) + 10 == 265);
    CHECK(12, gives_us(w) + 10 == 65545);
    CHECK(13, gives_bool(h) + 10 == 11);

    CHECK(14, pick_takes_sc()((sc)m) == 9);
    CHECK(15, pick_gives_sc()(b) + 10 == 9);

    __int128 pair = ((__int128)3 << 64) + 2;
    CHECK(16, pair_after_one(1, pair) == 1302);
    CHECK(17, pair_after_three(1, 2, 3, pair, 4) == 1 + 4 + 9 + 8 + 15 + 24);
    __int128 product = pair_result(six, pair);
    CHECK(18, (long)product == 12 && (long)(product >> 64) == 18);

    CHECK(19, tenth(1, 2, 3, 4, 5, 6, 7, 8, 9, 10) == 36 + 900 + 10000);
    CHECK(20, ints(0, 1, 2, 3, 4, 5, 6, 7, (sc)m, (uc)h, (short)m, (us)w, -12, (sc)m, -14,
                   (char)m)
                  == 28 - 3 + 1000 - 7 + 720885 - 156 - 17 - 266 - 23);
    CHECK(21, floats(0, 1, 2, 3, 4, 5, 6, 7, 8.5f, 9.5, 10.5f, 11.5f, 12.5, 13.5f, 14.5f, 15.5)
                  == 28 + 25.5 + 47.5 + 73.5 + 126.5 + 162.5 + 229.5 + 275.5 + 356.5);

    struct r3 r = { { 1, 2, 3 } };
    struct r12 q = { 4, 5, 6 };
    struct a16 al = { 7, 8 };
    CHECK(22, records(m, 1, 2, 3, 4, 5, 6, h, 9, r, 10, q, 11, al, 12)
                  == -1 + 200 + 27 + 5 + 21 + 110 + 52 + 102 + 209 + 161 + 232 + 372);
    struct h3 hh = { 1.5f, 2.5f, 3.5f };
    struct h2d g = { 4.25, 5.75 };
    CHECK(23, floating_records(1, 2, 3, 4, 5, 6, 7, 8, 9.5f, hh, 10.5f, g, 11.5f)
                  == 1 + 8 + 28.5 + 7.5 + 24.5 + 115.5 + 55.25 + 97.75 + 218.5);
    CHECK(24, pair_on_stack(1, 2, 3, 4, 5, 6, 7, 8, 9, pair, (char)m)
                  == 1 + 8 + 27 + 10 + 21 - 11);
    CHECK(25, record_split(1, 2, 3, 4, 5, 6, 7, q, 9) == 1 + 14 + 12 + 25 + 42 + 99);
    CHECK(26, interleaved(1, 1.0, (sc)m, 2.0f, (short)m, 3.0, 4, 4.0f, 5, 5.0, (uc)h, 6.0f,
                          (us)w, 7.0, 8, 8.0f, (sc)m, 9.0f, (short)m, 10.0, (char)m, 11.0f)
                  == 1 - 2 - 3 + 16 + 25 + 1200 + 458745 + 64 - 9 - 10 - 11 + 506);

    v4i va = { 1, 2, 3, 4 };
    v2i vb = { 5, 6 };
    v4i vj = { 7, 8, 9, 10 };
    v2i vk = { 11, 12 };
    CHECK(27, vectors(va, va, va, va, va, va, va, va, vb, vj, vk)
                  == 1 + 2 + 3 + 4 + 1 + 2 + 3 + 4 + 15 + 30 + 49 + 110 + 143 + 204);
    return 0;
}
