static int evaluations;
static int extent(int n) { ++evaluations; return n; }

static int matrix(int n) {
    int a[extent(n)][extent(n + 1)];
    unsigned long bytes = sizeof a;
    ++n;
    for (int i = 0; i < n - 1; ++i)
        for (int j = 0; j < n; ++j) a[i][j] = 10 * i + j;
    return bytes != (unsigned long)(n - 1) * n * sizeof(int)
        || a[n - 2][n - 1] != 10 * (n - 2) + n - 1;
}

static void *volatile escaped;
static int reenter(int repetitions) {
    int count = 0;
again:;
    int a[1000 + count % 3];
    a[0] = count;
    escaped = a;
    if (a[0] != count) return 1;
    if (++count < repetitions) goto again;
    return 0;
}

static int check(int repetitions) {
    if (matrix(3) || evaluations != 2) return 1;
    if (matrix(5) || evaluations != 4) return 2;
    return reenter(repetitions);
}

int short_run(void) { return check(64); }
int main(void) { return check(4096); }
