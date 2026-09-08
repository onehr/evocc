static int key;
static inline __attribute__((always_inline)) int branch(int * const p, const _Bool polarity) {
    char *k = &((char *)p)[polarity];
    asm goto("// %c0\n cbnz %w1, %l[yes]" : : "i"(k), "r"(polarity) : : yes);
    return 3;
yes:
    return 7;
}
int main(void) { return branch(&key, 0) != 3 || branch(&key, 1) != 7; }
