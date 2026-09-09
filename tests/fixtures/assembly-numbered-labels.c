static int branch(int n) {
    asm goto("cbnz %w0, %l1" : : "r"(n) : : hit);
    return 3;
hit:
    return 7;
}
static int modify(int n) {
    int value = 3;
    asm goto("cbnz %w1, %l3\n\tadd %w0, %w0, #5" : "+r"(value) : "r"(n) : : hit);
    return value;
hit:
    return value + 10;
}
static int select(int a, int b) {
    asm goto("cbnz %w0, %l2\n\tcbnz %w1, %l3" : : "r"(a), "r"(b) : : first, second);
    return 1;
first:
    return 2;
second:
    return 3;
}
static int double_digit(int n) {
    asm goto("b %l10" : : "r"(n), "r"(n), "r"(n), "r"(n), "r"(n), "r"(n), "r"(n), "r"(n), "r"(n), "r"(n) : : hit);
    return 1;
hit:
    return 0;
}
int main(void) {
    return branch(0) != 3 || branch(1) != 7 || modify(0) != 8 || modify(1) != 13
        || select(0, 0) != 1 || select(1, 0) != 2 || select(0, 1) != 3 || double_digit(1);
}
