/* Exact object representations; no runtime floating arithmetic. */
static float above = 1.0000000596046447753906250000000000000001f;
static float tie = 1.000000059604644775390625f;
static float below = 1.0000001788139343261718749999999999999999f;
static float odd_tie = 1.000000178813934326171875f;
static float promoted = 16777217.0f;

int main(void) {
    unsigned int bits;
    __builtin_memcpy(&bits, &above, 4);
    if (bits != 0x3f800001U) return 1;
    __builtin_memcpy(&bits, &tie, 4);
    if (bits != 0x3f800000U) return 2;
    __builtin_memcpy(&bits, &below, 4);
    if (bits != 0x3f800001U) return 3;
    __builtin_memcpy(&bits, &odd_tie, 4);
    if (bits != 0x3f800002U) return 4;
    __builtin_memcpy(&bits, &promoted, 4);
    if (bits != 0x4b800000U) return 5;
    /* The source suffix rounds before a subsequent conversion to double. */
    double widened = 16777217.0f;
    unsigned long wide;
    __builtin_memcpy(&wide, &widened, 8);
    if (wide != 0x4170000000000000UL) return 6;
    return 0;
}
