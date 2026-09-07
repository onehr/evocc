#include "csmith.h"

int main(void) {
    int8_t a8; int16_t a16; int32_t a32; int64_t a64;
    int count;
    if (safe_lshift_func_int8_t_s_s(a8, -1, count, 1) != -1) return 1;
    if (safe_lshift_func_int16_t_s_s(a16, -1, count, 1) != -1) return 2;
    if (safe_lshift_func_int32_t_s_s(a32, -1, count, 1) != -1) return 3;
    if (safe_lshift_func_int64_t_s_s(a64, -1, count, 1) != -1) return 4;
    if (safe_lshift_func_int8_t_s_u(a8, -1, count, 1u) != -1) return 5;
    if (safe_lshift_func_int16_t_s_u(a16, -1, count, 1u) != -1) return 6;
    if (safe_lshift_func_int32_t_s_u(a32, -1, count, 1u) != -1) return 7;
    if (safe_lshift_func_int64_t_s_u(a64, -1, count, 1u) != -1) return 8;
    /* Narrow operands promote to int; these shifts themselves are defined. */
    if (safe_lshift_func_int8_t_s_s(a8, 64, count, 1) != (int8_t)128) return 9;
    if (safe_lshift_func_int16_t_s_s(a16, 16384, count, 1) != (int16_t)32768) return 10;
    if (safe_lshift_func_int32_t_s_s(a32, INT32_MAX, count, 1) != INT32_MAX) return 11;
    if (safe_lshift_func_int64_t_s_u(a64, INT64_MAX, count, 1u) != INT64_MAX) return 12;
    if (safe_lshift_func_int32_t_s_u(a32, 7, count, 2u) != 28) return 13;
    if (safe_lshift_func_int32_t_s_s(a32, 7, count, -1) != 7) return 14;
    if (safe_lshift_func_int64_t_s_u(a64, 7, count, 64u) != 7) return 15;
    int value = 7, shift = 2;
    if (safe_lshift_func_int32_t_s_s(a32, value++, count, shift++) != 28
        || value != 8 || shift != 3) return 16;
    return 0;
}
