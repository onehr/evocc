static int prototype_calls;
static int entry_calls;
static int nested_calls;

int bounded(int n, int a[(++prototype_calls, n)]);
int bounded(int n, int a[(++entry_calls, n)]) {
    n = 1;
    return a[n] + (sizeof a == sizeof(int *));
}

static int bound(void) { ++entry_calls; return 3; }
static int postincrement(int n, int a[n++]) { return n + sizeof a; }
static int call_bound(int n, int a[bound()]) { return n + a[0]; }
static int unused_bound(int n, int [bound()]) { return n; }

static int callback(int n, int a[(++nested_calls, n)]) { return a[0]; }
static int nested(int n, int (*f)(int m, int a[(++prototype_calls, m)]),
                  int a[n]) {
    return f(n, a);
}

static inline __attribute__((always_inline))
int inline_bound(int n, int a[(++entry_calls, n)]) { return a[0]; }

static int recursive(int n, int a[(++entry_calls, 3)]) {
    if (n) return recursive(n - 1, a);
    return a[0];
}

int main(void) {
    int bounded(int n, int a[(++prototype_calls, n)]);
    int a[3] = {1, 7, 11};
    if (prototype_calls || entry_calls || nested_calls) return 1;
    if (bounded(3, a) != 8 || entry_calls != 1 || prototype_calls) return 2;
    if (postincrement(3, a) != 12) return 3;
    if (call_bound(3, a) != 4 || entry_calls != 2) return 4;
    if (unused_bound(3, a) != 3 || entry_calls != 3) return 5;
    if (nested(3, callback, a) != 1 || nested_calls != 1 || prototype_calls) return 6;
    if (inline_bound(3, a) != 1 || entry_calls != 4) return 7;
    if (recursive(3, a) != 1 || entry_calls != 8) return 8;
    return 0;
}
