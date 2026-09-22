__attribute__((noinline))
static int low(long long value) {
    int result;
    __asm__ volatile ("" : "=r"(result) : "0"(value));
    return result;
}
int main(void) { return low(0x1234567880000001LL) != (-2147483647); }
