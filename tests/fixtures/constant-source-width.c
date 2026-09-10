int main(void) {
    int b = -2147483647;
    if (!(b >= (unsigned)(2147483647 + 2))) return 1;
    if (2147483647 + 2 != -2147483647) return 2;
    if ((-2147483647 - 1) - 1 != 2147483647) return 3;
    if (1073741824 * 3 != -1073741824) return 4;
    if (9223372036854775807L + 1L != (-9223372036854775807L - 1L)) return 5;
    if ((unsigned char)255 + (unsigned char)1 != 256) return 6;
    if ((unsigned)2147483647 + 2 != 2147483649U) return 7;
    if (-(-2147483647 - 1) != (-2147483647 - 1)) return 8;
    if (((__int128)1 << 126) * 3 != -((__int128)1 << 126)) return 9;
    volatile int one = 1;
    if (one + (2147483647 + 2) != -2147483646) return 10;
    int effects = 0;
    if ((effects = 6) + (2147483647 + 2) != -2147483641) return 11;
    if (effects != 6) return 12;
    if ((one ? 2147483647 + 2 : effects++) != -2147483647) return 13;
    if (effects != 6) return 14;
    return 0;
}
