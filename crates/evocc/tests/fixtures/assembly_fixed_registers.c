static long input(long n) {
    __asm__ volatile("mov x8, #5" : : : "x8");
    register long wanted __asm__("x8") = n;
    long result;
    __asm__ volatile("mov %0, x8" : "=r"(result) : "r"(wanted));
    return result;
}
static long output(void) {
    register long result __asm__("x8");
    __asm__ volatile("mov x8, #42" : "=r"(result));
    return result;
}
static long tied(long n) {
    register long result __asm__("x8");
    __asm__ volatile("add x8, x8, #3" : "=r"(result) : "0"(n));
    return result;
}
static long readwrite(long n) {
    register long result __asm__("x8") = n;
    __asm__ volatile("add x8, x8, #9" : "+r"(result));
    return result;
}
static unsigned narrow(unsigned n) {
    register unsigned result __asm__("w8") = n;
    __asm__ volatile("add w8, w8, #7" : "+r"(result));
    return result;
}
static long branch(int take, long n) {
    register long result __asm__("x8") = n;
    __asm__ goto("add x8, x8, #5\ncbnz %w1, %l[done]"
                 : "+&r"(result) : "r"(take) : : done);
    return result + 1;
done:
    return result + 2;
}
int main(void) {
    if (input(37) != 37 || input(103) != 103) return 1;
    if (output() != 42) return 2;
    if (tied(37) != 40) return 3;
    if (readwrite(37) != 46) return 4;
    if (narrow(37) != 44) return 5;
    if (branch(0, 37) != 43 || branch(1, 37) != 44) return 6;
    return 0;
}
