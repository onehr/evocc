static int bounds;
static unsigned long size(unsigned long n) { ++bounds; return n; }
static long sum(long a, long b, long c, long d, long e, long f,
                long g, long h, long i, long j, long k, long l) {
    return a+b+c+d+e+f+g+h+i+j+k+l;
}

static int exercise(unsigned long n) {
    unsigned long fixed[3] __attribute__((aligned(64))) = {17, 29, 41};
    unsigned char *a;
    {
        a = __builtin_alloca(size(n));
        a[0] = 7;
        a[n-1] = 11;
    }
    unsigned char *b = __builtin_alloca(size(n+13));
    b[0] = 19;
    b[n+12] = 23;
    if (sum(1,2,3,4,5,6,7,8,9,10,11,12) != 78) return 1;
    if (a[0] != 7 || a[n-1] != 11 || b[0] != 19 || b[n+12] != 23) return 2;
    if (fixed[0] != 17 || fixed[1] != 29 || fixed[2] != 41) return 3;
    if ((unsigned long)fixed % 64) return 4;
    unsigned char *objects[7];
    for (int i = 0; i < 7; ++i) {
        objects[i] = __builtin_alloca(n+i);
        objects[i][0] = i+31;
    }
    for (int i = 0; i < 7; ++i)
        if (objects[i][0] != i+31) return 5;
    return 0;
}

static int recursive(int n) {
    unsigned char *p = __builtin_alloca(n+5);
    p[0] = n+37;
    if (n && recursive(n-1)) return 1;
    return p[0] != n+37;
}

int main(void) {
    if (exercise(19) || exercise(35) || bounds != 4) return 1;
    if (recursive(5)) return 2;
    return 0;
}
