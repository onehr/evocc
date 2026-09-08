int isdigit(int value) { return value + 7; }
static int digit(int value) { return __builtin_isdigit(value); }
int main(void) {
    for (int value = 0; value < 256; ++value) {
        int expected = value >= 48 && value <= 57;
        if (digit(value) != expected) return 1;
    }
    if (digit(-1) || digit(0x130) || digit(-2147483647 - 1)
        || digit(2147483647)) return 2;
    int side = 47;
    if (__builtin_isdigit(++side) != 1 || side != 48) return 3;
    // A compatible function body does not replace the builtin operation.
    if (__builtin_isdigit(42) != 0) return 4;
    return 0;
}
