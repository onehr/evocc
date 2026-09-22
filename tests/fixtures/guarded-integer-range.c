extern void impossible_range(void);
void check(int x) {
    if (!(x <= 0 || (unsigned long)(unsigned)(x - 1) < 4294967295UL))
        impossible_range();
    if (x > 0 && (unsigned)(x - 1) >= 4294967295U)
        impossible_range();
}
int main(void) {
    check(-2147483647-1); check(-1); check(0); check(1); check(2147483647);
    return 0;
}
