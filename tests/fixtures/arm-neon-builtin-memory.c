typedef __Uint64x2_t words2;
typedef __Uint8x16_t bytes16;

int main(void) {
    unsigned long input[2] = { 11, 29 }, output[2] = { 0, 0 };
    words2 value = __builtin_aarch64_ld1v2di_us((const long *)input);
    __builtin_aarch64_st1v2di_su((long *)output, value);
    if (output[0] != 11 || output[1] != 29) return 1;
    bytes16 table = { 7, 6, 5, 4, 3, 2, 1, 0, 15, 14, 13, 12, 11, 10, 9, 8 };
    bytes16 indices = { 0, 15, 16, 255, 3, 2, 1, 0, 7, 8, 9, 10, 11, 12, 13, 14 };
    bytes16 result = __builtin_aarch64_qtbl1v16qi_uuu(table, indices);
    if (result[0] != 7 || result[1] != 8 || result[2] != 0 || result[3] != 0)
        return 2;
    result = __builtin_aarch64_lshrv16qi_uus(table, 8);
    for (int i = 0; i != 16; ++i)
        if (result[i] != 0) return 3;
    return 0;
}
