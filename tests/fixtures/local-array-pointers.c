static int view(int n, int *data) {
    int width = n, seen = 0;
    int (*a)[n++] = (seen = n, (void *)data), (*b)[n++];
    b = (void *)data;
    n = 99;
    if (seen != width + 1 || sizeof(*a) != 4UL * width ||
        sizeof(*b) != 4UL * (width + 1)) return 1;
    a[1][0] = 17;
    b[1][0] = 23;
    if (data[width] != 17 || data[width + 1] != 23) return 2;
    int (*slots[2])[width] = {a, a};
    slots[1][1][1] = 29;
    if (sizeof(slots) != 2 * sizeof(void *) || data[width + 1] != 29) return 3;
    int total = 0;
    for (int (*p)[width] = a; p < a + 2; ++p) total += sizeof(*p);
    register int (*r)[width] = a;
    return total != 8 * width || r[1][0] != 17;
}

static __attribute__((always_inline)) inline int inlined(int n, int *data) {
    int (*p)[n] = (void *)data;
    p[1][0] += n;
    return p[1][0];
}

int main(void) {
    int data[32] = {0};
    if (view(3, data) || view(5, data)) return 1;
    if (inlined(2, data) != 2 || inlined(8, data) != 8) return 2;
    return 0;
}
