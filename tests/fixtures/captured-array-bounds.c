static int calls;
static int bound(int n) { ++calls; return n; }

static unsigned long sizes(int n) {
    typedef int A[bound(n)];
    typedef A B[2];
    _Static_assert(__builtin_types_compatible_p(A, int[99]), "variable bound compatibility");
    unsigned long before = sizeof(A);
    n = 99;
    return before + sizeof(A) + sizeof(B) + __alignof__(A);
}

static int ordered(int n) {
    typedef int A[n++], B[n++];
    return sizeof(A) != 12 || sizeof(B) != 16 || n != 5;
}

static __attribute__((always_inline)) inline unsigned long inlined(int n) {
    typedef long A[n++];
    return sizeof(A) + n;
}

static int jump_out_and_reenter(int n) {
    int count = 0;
again:;
    typedef int A[bound(n + count)];
    if (sizeof(A) != (unsigned long)(n + count) * sizeof(int)) return 1;
    if (++count < 2) goto again;
    goto out;
out:
    switch (count) { case 2: return 0; default: return 2; }
}

int main(void) {
    int i;
    if (sizes(3) != 52 || calls != 1) return 1;
    if (sizes(5) != 84 || calls != 2) return 2;
    if (ordered(3)) return 3;
    for (i = 1; i != 5; ++i) {
        typedef int A[bound(i)];
        if (sizeof(A) != 4UL * i) return 4;
    }
    if (calls != 6) return 5;
    if (inlined(2) != 19 || inlined(4) != 37) return 6;
    if (jump_out_and_reenter(3) || calls != 8) return 7;
    if (sizeof(({typedef int A[bound(20)]; 42;})) != 4 || calls != 8) return 8;
    return 0;
}
