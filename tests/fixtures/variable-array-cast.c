static int evaluations;
static int bound(int n) { ++evaluations; return n; }

static int inspect(int n, void *data) {
    int (*p)[n] = (int (*)[bound(n)])data;
    if (evaluations != 1 || p[1][1] != 5) return 1;
    if (0 && (int (*)[bound(n)])data) return 2;
    if (evaluations != 1) return 3;
    if (sizeof(*(int (*)[bound(n)])data) != (unsigned long)n * sizeof(int)) return 4;
    if (evaluations != 2) return 5;
    if (sizeof((int (*)[bound(n)])data) != sizeof(void *) || evaluations != 2) return 6;
    if (sizeof *p++ != (unsigned long)n * sizeof(int) || p[0][1] != 5) return 7;
    for (int i = 0; i < 3; ++i) {
        if (((int (*)[bound(n)])data)[1][2] != 6) return 8;
    }
    if (evaluations != 5) return 9;
    if ((n ? (int (*)[bound(n)])data : (int (*)[bound(n)])data)[1][0] != 4) return 10;
    if (evaluations != 6) return 11;
#ifdef WITH_ASM
    __asm__ volatile ("" : : "m" (*(char (*)[n])data) :);
#endif
    return 0;
}

int main(void) {
    int data[6] = {1, 2, 3, 4, 5, 6};
    return inspect(3, data);
}
