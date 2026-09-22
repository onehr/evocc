/* The same finite constant evaluator serves path selection and object bytes. */
static float single[] = {
    16777216.0f + 1.0f - 16777216.0f,
    0x1.000002p0f * 0x1.fffffcp-1f - 1.0f,
    0x1p-149f * 2.0f,
    1.0f / 10.0f,
};
static unsigned int single_bits[] = { 0, 0, 2, 0x3dcccccdU };
static double mixed[] = {
    1.0f + 16777217.0, 16777217.0 + 1.0f,
    (double)((unsigned __int128)1 << 127),
    1.0 / 10.0,
};
static unsigned long mixed_bits[] = {
    0x4170000020000000UL, 0x4170000020000000UL,
    0x47e0000000000000UL, 0x3fb999999999999aUL,
};
static int selected = (16777216.0f + 1.0f == 16777216.0f) ? 1 : -1;
int main(void) {
    for (int i = 0; i < 4; ++i) {
        unsigned int narrow;
        unsigned long wide;
        __builtin_memcpy(&narrow, &single[i], 4);
        __builtin_memcpy(&wide, &mixed[i], 8);
        if (narrow != single_bits[i]) return i + 1;
        if (wide != mixed_bits[i]) return i + 10;
    }
    return selected != 1;
}
