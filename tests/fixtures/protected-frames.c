unsigned long __stack_chk_guard = 0x8badf00d;
__attribute__((noinline)) void use(void *p) { __asm__ volatile ("" : : "r"(p) : "memory"); }
int plain(int a) { int n = a; return n; }
void array(void) { char a[2]; use(a); }
void scalar(void) { int n = 1; use(&n); }
void heap(int *p) { use(&*p); }
void parameter(int a) { use(&a); }
__attribute__((no_stack_protector)) void exempt(void) { char a[20]; use(a); }
void persistent(void) { static char a[20]; use(a); }
int multiple(int c) { char a[16]; use(a); if (c) return 1; return 2; }
void *address(void) { return __builtin_return_address(0); }
__attribute__((noinline)) int overalign(void) {
    char a[32] __attribute__((aligned(64))); use(a);
    return (unsigned long)a % 64;
}
__attribute__((noinline)) int dynamic(unsigned n) {
    char *p=__builtin_alloca(n); p[0]=3; use(p); return p[0];
}
int main(void) {
    array(); scalar(); int n=3; heap(&n); parameter(4); exempt(); persistent();
    return multiple(0) != 2 || multiple(1) != 1 || plain(9) != 9 || !address()
        || overalign() || dynamic(91) != 3;
}
