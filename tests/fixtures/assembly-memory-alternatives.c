static unsigned long divide(unsigned long dividend, unsigned divisor, unsigned long *remainder) {
    unsigned result = 0;
    while (dividend >= divisor) {
        asm("" : "+rm"(dividend));
        dividend -= divisor;
        result++;
    }
    *remainder = dividend;
    return result;
}
static long effects(long *values, int *calls) {
    asm volatile("" : "+rm"(values[(*calls)++]));
    asm volatile("" : "+mr"(values[(*calls)++]));
    long result = 11;
    asm volatile("" : "+&rm"(result) : "rm"(values[(*calls)++]));
    asm volatile("" : "+&mr"(result) : "mr"(values[(*calls)++]));
    return result;
}
static long outputs(long input) {
    register long result asm("x8");
    asm volatile("add %0, %1, #3" : "=rm"(result) : "r"(input));
    long a = result;
    asm volatile("add %0, %1, #4" : "=&mr"(result) : "r"(input));
    return a + result;
}
static long branches(long value, int branch) {
    asm goto("cbnz %w1, %l[yes]" : "+rm"(value) : "r"(branch) : : yes);
    return value + 1;
yes:
    return value + 2;
}
int main(void) {
    unsigned long remainder = 0;
    if (divide(39, 7, &remainder) != 5 || remainder != 4) return 1;
    long values[4] = {3, 5, 7, 9};
    int calls = 0;
    if (effects(values, &calls) != 11 || calls != 4) return 2;
    if (values[0] != 3 || values[1] != 5) return 3;
    if (outputs(6) != 19) return 4;
    if (branches(12, 0) != 13 || branches(12, 1) != 14) return 5;
    return 0;
}
