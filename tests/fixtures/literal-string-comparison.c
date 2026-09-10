static int calls;
static int strcmp(void) { ++calls; return 123; }

int main(void) {
    if (__builtin_strcmp("alpha", "omega") != -1) return 1;
    if (__builtin_strcmp("omega", "alpha") != 1) return 2;
    if (__builtin_strcmp("same", "same") != 0) return 3;
    if (__builtin_strcmp("", "x") != -1) return 4;
    if (__builtin_strcmp("x", "") != 1) return 5;
    if (__builtin_strcmp("X", "X\376") != -1) return 6;
    if (__builtin_strcmp("\200", "a") != 1) return 7;
    if (__builtin_strcmp("a\0z", "a\0x") != 0) return 8;
    if (sizeof(__builtin_strcmp("a", "b")) != sizeof(int)) return 9;
    return calls;
}
