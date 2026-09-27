/* Diagnostic reduction for musl's split LL/SC templates, not an assertion
 * that arbitrary separate asm statements form an indivisible region. */
extern int printf(const char *, ...);

__attribute__((noinline)) int split_exclusive(volatile int *p) {
    for (int i = 0; i < 100000; i++) {
        int old, status;
        __asm__ volatile ("ldaxr %w0,%1" : "=r"(old) : "Q"(*p));
        if (old != 0) return 2;
        __asm__ volatile ("stlxr %w0,%w2,%1"
                          : "=&r"(status), "=Q"(*p) : "r"(1) : "memory");
        if (!status) return 0;
    }
    return 1;
}

__attribute__((noinline)) int joined_exclusive(volatile int *p) {
    for (int i = 0; i < 100000; i++) {
        int old, status;
        __asm__ volatile ("ldaxr %w0,%2\n\tstlxr %w1,%w3,%2"
                          : "=&r"(old), "=&r"(status), "+Q"(*p)
                          : "r"(1) : "memory");
        if (old != 0) return 2;
        if (!status) return 0;
    }
    return 1;
}

int main(void) {
    volatile int slots[128] = {0};
    int split_failures = 0, joined_failures = 0;
    for (int i = 0; i < 128; i++) split_failures += split_exclusive(&slots[i]) != 0;
    for (int i = 0; i < 128; i++) slots[i] = 0;
    for (int i = 0; i < 128; i++) joined_failures += joined_exclusive(&slots[i]) != 0;
    printf("split=%d joined=%d\n", split_failures, joined_failures);
    return joined_failures != 0;
}
