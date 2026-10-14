#include <string.h>
#include "shapes.h"

static volatile int minus_one = -1;

#define CHECK(n, condition) \
    do {                    \
        if (!(condition))   \
            return n;       \
    } while (0)

int main(void)
{
    int m = minus_one;
    long seven = 7;

    CHECK(1, v_doubles(3, 1.5, 2.25, -4.0) == 1.5 + 4.5 - 12.0);
    CHECK(2, v_mixed(1, 2, 3L, 4.5, &seven, (char)m, 1.5f) == 2 + 9 + 22 + 49 - 11 + 19);

    __int128 w = ((__int128)3 << 64) + 2;
    CHECK(3, v_int128(2, 1, w, 4, w) == 1 + 6 + 15 + 4 + 6 + 15);

    long double third = 1.0L / 3;
    CHECK(4, v_long_double(2, 1, third, 2, (long double)0.5) == 1 + 3 * third + 2 + 1.5L);

    struct h3 h = { 1.5f, 2.5f, 3.5f };
    struct h4d d = { 0.25, 0.5, 0.75, 1.0 };
    CHECK(5, v_hfa(2, h, d, h, d) == 2 * (1.5 + 7.5 + 17.5 + 1.75 + 11.0));

    struct hv2 hv = { { 1, 2, 3, 4 }, { 5, 6, 7, 8 } };
    struct hv1 g = { { 9, 10 } };
    v4i v = { 11, 12, 13, 14 };
    CHECK(6, v_hva(1, hv, g, v) == 1 + 24 + 50 + 91);

    struct r3 r = { { 1, 2, 3 } };
    struct r12 q = { 4, 5, 6 };
    struct r16 s = { 7, 8 };
    struct a16 a = { 9, 10 };
    struct big b = { { 11, 12, 13, 14, 15 } };
    CHECK(7, v_records(1, r, q, s, a, b) == 3 + 18 + 40 + 63 + 110 + 195);

    CHECK(8, v_copy(3, 10L, 20L, 30L) == (10 + 40 + 90) * 1000 + 60);

    char buffer[64];
    int length = v_format(buffer, sizeof buffer, "%d %s %.2f %ld %c %Lg", -5, "abc", 2.5,
                          1234567890123L, 'x', third);
    CHECK(9, length == (int)strlen("-5 abc 2.50 1234567890123 x 0.333333")
                 && strcmp(buffer, "-5 abc 2.50 1234567890123 x 0.333333") == 0);

    CHECK(10, v_after_stack(1, 2, 3, 4, 5, 6, 7, 8, (char)m, 9, 2.5, 100L, -3)
                  == 1 + 8 - 3 + 45 + 17 + 1100 - 39);
    CHECK(11, forward(3, 10L, 20L, 30L) == 10 + 40 + 90);
    return 0;
}
