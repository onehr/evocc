static int view(int n, int *data, int branch) {
    int original = n;
    typedef int Row[n++];
    typedef Row Pair[2];
    Row *p = (Row *)data;
    Pair *g = (Pair *)data;
    if (branch) n += 100; else n += 200;
    p[1][original - 1] = 41;
    Row *q = p + 2;
    if (q - p != 2) return 1;
    if ((char *)q - (char *)p != 8 * original) return 2;
    if (q[-1][original - 1] != 41) return 3;
    --q;
    q -= 1;
    if (q != p) return 4;
    q++;
    (*q)[0] = 17;
    if (p[1][0] != 17) return 5;
    g[1][1][0] = 23;
    if (p[3][0] != 23) return 6;
    if (sizeof(*q) != (unsigned long)original * 4) return 7;
    if (sizeof(*g) != (unsigned long)original * 8) return 8;
    return n != original + 1 + (branch ? 100 : 200);
}

static __attribute__((always_inline)) inline int inlined_view(int n, int *data) {
    typedef int Row[n];
    Row *p = (Row *)data;
    p[1][0] += n;
    return p[1][0];
}

int main(void) {
    int data[24] = {0};
    if (view(3, data, 1)) return 11;
    if (view(4, data, 0)) return 12;
    if (inlined_view(2, data) != 2) return 13;
    if (inlined_view(6, data) != 6) return 14;
    return 0;
}
