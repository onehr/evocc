static long words[2] = {11, 37};
long external_word = 23;
int target(int x) { return x + 5; }

unsigned long member_address(void) {
    unsigned long address;
    __asm__ volatile("adrp %0, %c1\nadd %0, %0, :lo12:%c1"
                     : "=r"(address) : "i"(&words[1]));
    return address;
}
unsigned long object_address(void) {
    unsigned long address;
    __asm__ volatile("adrp %0, %c1\nadd %0, %0, :lo12:%c1"
                     : "=r"(address) : "i"(&external_word));
    return address;
}
unsigned long function_address(void) {
    unsigned long address;
    __asm__ volatile("adrp %0, %c[name]\nadd %0, %0, :lo12:%c[name]"
                     : "=r"(address) : [name] "i"(target));
    return address;
}
int numeric(void) {
    int result;
    __asm__ volatile("mov %w0, %c1" : "=r"(result) : "i"(7));
    return result;
}
int main(void) {
    return member_address() != (unsigned long)&words[1]
        || object_address() != (unsigned long)&external_word
        || function_address() != (unsigned long)target || numeric() != 7;
}
