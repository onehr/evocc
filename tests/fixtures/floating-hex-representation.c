/* Keep source-format rounding separate from storage-format conversion. */
static float values[] = {
    0x1.00000100000000000001p0f, 0x1p-149f, 0x1p-150f,
    -0x0p0f, 0x1.fffffep127f, 0x1p-126f,
};
static unsigned int expected[] = {
    0x3f800001U, 1, 0, 0x80000000U, 0x7f7fffffU, 0x00800000U,
};
static double wide[] = { 0x1p-1074, -0x0p0, 0x1.fffffffffffffp1023 };
static unsigned long wide_expected[] = { 1, 0x8000000000000000UL,
                                         0x7fefffffffffffffUL };
int main(void) {
    for (int i = 0; i < 6; ++i) {
        unsigned int bits;
        __builtin_memcpy(&bits, &values[i], 4);
        if (bits != expected[i]) return i + 1;
    }
    for (int i = 0; i < 3; ++i) {
        unsigned long bits;
        __builtin_memcpy(&bits, &wide[i], 8);
        if (bits != wide_expected[i]) return i + 10;
    }
    return 0;
}
