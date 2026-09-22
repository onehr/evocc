#ifndef CASE
#define CASE 0
#endif
int main(void) {
    volatile double lower = -2147483648.75;
    volatile double upper = 2147483647.75;
    volatile double neg = -0.75;
    volatile double i8 = -128.75;
    volatile double u8 = 255.75;
    volatile double i128 = -0x1p127;
    volatile double u128 = 0x1.fffffffffffffp127;
    if ((int)lower != (-2147483647 - 1) || (int)upper != 2147483647) return 1;
    if ((unsigned int)neg || (unsigned long)neg || (unsigned __int128)neg) return 2;
    if ((signed char)i8 != -128 || (unsigned char)u8 != 255) return 3;
    if ((__int128)i128 != -((__int128)1 << 126) * 2) return 4;
    if ((unsigned __int128)u128 != ~(unsigned __int128)0 - (((unsigned __int128)1 << 75) - 1)) return 5;
#if CASE == 1
    volatile double bad = 0x1p31; return (int)bad;
#elif CASE == 2
    volatile double bad = -2147483649.0; return (int)bad;
#elif CASE == 3
    volatile double bad = -1.0; return (unsigned int)bad;
#elif CASE == 4
    volatile double bad = 0x1p64; return (unsigned long)bad;
#elif CASE == 5
    volatile double bad = 0x1p127; return (__int128)bad;
#elif CASE == 6
    volatile double bad = 0x1p128; return (unsigned __int128)bad;
#elif CASE == 7
    volatile double bad = __builtin_nan("1"); return (int)bad;
#elif CASE == 8
    volatile double bad = __builtin_inf(); return (unsigned __int128)bad;
#elif CASE == 9
    volatile double bad = 128.0; return (signed char)bad;
#elif CASE == 10
    volatile double bad = 256.0; return (unsigned char)bad;
#elif CASE == 11
    double missing; return (int)missing;
#endif
    return 0;
}
