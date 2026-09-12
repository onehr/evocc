static int keys[8], effects;
static inline __attribute__((always_inline)) int branch(int *key) {
    asm goto("// %c0" : : "i"(key) : : yes);
    return 0;
yes:
    return 1;
}

__attribute__((noinline)) int hook(unsigned int pf, unsigned int index) {
    if (__builtin_constant_p(pf) && __builtin_constant_p(index) &&
        branch(&keys[pf + index]))
        return 1;
    return 0;
}

int unused(int *p) {
    int a = (++effects, 0) && branch(p);
    int b = (++effects, 1) || branch(p);
    return a != 0 || b != 1;
}

int main(void) {
    return hook(0, 1) || unused(keys) || effects != 2;
}
