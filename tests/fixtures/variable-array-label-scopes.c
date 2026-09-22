/* A label in a syntactically untaken nested scope precedes a VLA. */
static void *volatile escaped;
static int hidden_label(int limit) {
    int n = 0;
    if (0) {
again:;
    }
    int a[n % 7 + 1];
    a[0] = n;
    escaped = a;
    if (a[0] != n) return 1;
    if (++n < limit) goto again;
    return n != limit;
}

static int siblings(int limit) {
    int n = 0;
again:;
    {
        int a[n % 7 + 1];
        a[0] = n;
        escaped = a;
        if (a[0] != n) return 1;
        ++n;
    }
    {
        int b[n % 5 + 1];
        b[0] = n;
        escaped = b;
        if (b[0] != n) return 2;
        ++n;
    }
    if (n < limit) goto again;
    return n != limit;
}

int main(void) { return hidden_label(64) || siblings(64); }
