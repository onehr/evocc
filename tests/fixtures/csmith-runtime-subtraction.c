#include "csmith.h"

int main(void) {
    int32_t values32[] = {INT32_MIN, INT32_MIN + 1, -7, -1, 0, 1, 7, INT32_MAX};
    int64_t values64[] = {INT64_MIN, INT64_MIN + 1, -7, -1, 0, 1, 7, INT64_MAX};
    for (int i = 0; i < 8; ++i) {
        for (int j = 0; j < 8; ++j) {
            int32_t a = values32[i], b = values32[j], x, y;
            int32_t expected = ((b > 0 && a < INT32_MIN + b)
                || (b < 0 && a > INT32_MAX + b)) ? a : a - b;
            if (safe_sub_func_int32_t_s_s(x, a, y, b) != expected) return 1;
            int64_t c = values64[i], d = values64[j], u, v;
            int64_t wide = ((d > 0 && c < INT64_MIN + d)
                || (d < 0 && c > INT64_MAX + d)) ? c : c - d;
            if (safe_sub_func_int64_t_s_s(u, c, v, d) != wide) return 2;
        }
    }
    return 0;
}
