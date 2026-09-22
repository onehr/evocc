/* Reduced from the oversized-vector and narrow-remainder torture cases. */
typedef long wide __attribute__((vector_size(128)));
typedef signed char bytes __attribute__((vector_size(16)));
static wide next, saved;
static volatile int choose;

__attribute__((noinline)) static wide combine(wide input) {
    wide old = input;
    input = next;
    saved = input;
    return old + input;
}

int main(void) {
    wide input;
    for (int i = 0; i < 16; ++i) {
        input[i] = i + 1;
        next[i] = 100;
    }
    wide result = combine(input);
    for (int i = 0; i < 16; ++i)
        if (result[i] != i + 101 || saved[i] != 100) return 1;
    wide a, b = result, c;
    if (choose) a = b; else a = next;
    saved = a;
    b = input;
    if (choose) c = b; else c = result;
    next = c;
    for (int i = 0; i < 16; ++i)
        if (saved[i] != 100 || next[i] != i + 101) return 2;
    int count = 0;
again:
    result = ~result;
    if (++count != 2) goto again;
    for (int i = 0; i < 16; ++i)
        if (result[i] != i + 101) return 3;
    bytes small = {-128, -127, 127, 31};
    small %= (signed char)-128;
    if (small[0] != 0 || small[1] != -127 || small[2] != 127) return 4;
    small %= (signed char)-19;
    return small[3] != 12;
}
