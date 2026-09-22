__attribute__((noinline))
static unsigned long update(unsigned long value) {
    __asm__ volatile ("" : "+g"(value));
    return value;
}
int main(void) { return update(0xfedcba9876543210UL) != 0xfedcba9876543210UL; }
