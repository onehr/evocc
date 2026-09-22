#ifdef TEST_PRIVATE
typedef __Poly8_t poly8_t;
typedef __Poly16_t poly16_t;
typedef __Poly64_t poly64_t;
typedef __Poly128_t poly128_t;
#else
#include <arm_neon.h>
#endif

int main(void)
{
    poly8_t bytes[2] = { 161, 255 };
    poly8_t got;
    poly8_t expected;
    got = bytes[0];
    expected = 161;
    if (got != expected || (unsigned char)bytes[1] != 255)
        return 1;
    poly16_t half = 65535;
    poly64_t word = 0x123456789abcdef0ULL;
    poly128_t wide;
    wide = (poly128_t)((unsigned __int128)1 << 100);
    return (unsigned short)half != 65535
        || (unsigned long long)word != 0x123456789abcdef0ULL
        || (unsigned __int128)wide != ((unsigned __int128)1 << 100);
}
