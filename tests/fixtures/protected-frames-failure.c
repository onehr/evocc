unsigned long __stack_chk_guard = 0x8badf00d;
extern void _Exit(int);
__attribute__((no_stack_protector, noinline)) void __stack_chk_fail(void) { _Exit(77); }
__attribute__((no_stack_protector, noinline)) void change_guard(void *p) {
    __asm__ volatile ("" : : "r"(p) : "memory");
    __stack_chk_guard ^= 1;
}
__attribute__((noinline)) int checked(void) {
    char a[16];
    change_guard(a);
    return 0;
}
int main(void) { return checked(); }
