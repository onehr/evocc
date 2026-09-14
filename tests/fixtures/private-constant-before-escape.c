extern void invalid(void);

__attribute__((noinline)) static void replace(int *p) { *p = 7; }

static __attribute__((always_inline)) inline _Bool error_pointer(const void *p) {
    return __builtin_expect(!!((unsigned long)(void *)p >= (unsigned long)-4095), 0);
}

int main(void) {
    int initial = 32;
    int result = __builtin_ffs(6);
    do { if (initial < 32) invalid(); } while (0);
    asm volatile("" : : "r"(&initial), "r"(&result) : "memory");
    replace(&initial);
    if (initial != 7 || result != 2) return 1;
    do { if (error_pointer((void *)0xffff0123456789abUL)) invalid(); } while (0);
    if (!error_pointer((void *)-1L)) return 2;
    if (error_pointer((void *)0)) return 3;
    return 0;
}
