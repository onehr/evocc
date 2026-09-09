static volatile unsigned long inputs[15] = {
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
};

__attribute__((noinline)) int check(int depth) {
    unsigned char slot[17] __attribute__((aligned(256)));
    slot[0] = 7;
    slot[16] = 9;
    int bad = ((unsigned long)slot & 255) != 0;
    unsigned long a = inputs[0], b = inputs[1], c = inputs[2], d = inputs[3];
    unsigned long e = inputs[4], f = inputs[5], g = inputs[6], h = inputs[7];
    unsigned long i = inputs[8], j = inputs[9], k = inputs[10], l = inputs[11];
    unsigned long m = inputs[12], n = inputs[13], o = inputs[14];
    if (depth) bad |= check(depth - 1);
    if (a + b + c + d + e + f + g + h + i + j + k + l + m + n + o != 120) return 2;
    return bad || slot[0] != 7 || slot[16] != 9;
}

int main(void) { return check(31); }
