/* An asm tie binds a register representation, including an address. */
static int object;
static void function(void) {}

__attribute__((noinline)) static unsigned address_low(const void *input) {
    unsigned result;
    __asm__ volatile ("" : "=g"(result) : "0"(input));
    return result;
}

__attribute__((noinline)) static unsigned code_low(void (*input)(void)) {
    unsigned result;
    __asm__ volatile ("" : "=r"(result) : "0"(input));
    return result;
}

int main(void) {
    const void *pointer = &object;
    unsigned word = 0x80000001U;
    __asm__ volatile ("" : "+g"(pointer), "+g"(word));
    if (pointer != &object || word != 0x80000001U) return 1;
    if (address_low(pointer) != (unsigned)(unsigned long)&object) return 2;
    return code_low(function) != (unsigned)(unsigned long)function;
}
