/* Defined-input twin; the overflow negative substitutes the initial value. */
int main(void) {
    volatile int x = 40;
    return (x + 2) != 42;
}
