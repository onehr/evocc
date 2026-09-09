typedef __int128 I;
typedef unsigned __int128 U;

static int left32(int value, int count) { return value << count; }
static long left64(long value, int count) { return value << count; }
static I left128(I value, int count) { return value << count; }

int main(void) {
    if (left32(-1, 1) != -2 || left32(1, 31) != (-2147483647 - 1)) return 1;
    if (left32(3, 30) != -1073741824 || (left32(-4, 6) >> 4) != -16) return 2;
    if (left32(-2147483647 - 1, 1) != 0 || left32(-1, 0) != -1) return 3;
    if (left64(1, 63) != (-9223372036854775807L - 1) || left64(-1, 1) != -2) return 4;
    if ((U)left128(1, 127) != ((U)1 << 127) || left128(-1, 1) != -2) return 5;
    if (left128((I)((U)1 << 127), 1) != 0) return 6;
    int a = -1;
    a <<= 1;
    volatile long b = -4;
    b <<= 6;
    I c = 1;
    I *p = &c;
    *p <<= 127;
    return a != -2 || (b >> 4) != -16 || (U)c != ((U)1 << 127);
}
