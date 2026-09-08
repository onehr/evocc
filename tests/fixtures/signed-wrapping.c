volatile int high = 2147483647;
volatile long low = (-9223372036854775807L - 1L);
int main(void) {
    int a = high, b = high;
    if (a++ != 2147483647 || a != (-2147483647 - 1)) return 1;
    if (++b != (-2147483647 - 1)) return 2;
    a -= 1;
    if (a != 2147483647) return 3;
    b *= 3;
    if (b != (-2147483647 - 1)) return 4;
    long c = low;
    if (-c != c || (0L - c) != c || (c + c) != 0) return 5;
    unsigned __int128 all = ~(unsigned __int128)0;
    __int128 max = (__int128)(all >> 1);
    __int128 min = max + 1;
    if ((unsigned __int128)min != ((unsigned __int128)1 << 127)) return 6;
    if (-min != min || min * 3 != min) return 7;
    return 0;
}
