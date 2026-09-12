extern int backtrace(void **, int);
extern int absent __attribute__((weak));
static int datum = 7;
__attribute__((noinline)) int *local_address(void) { return &datum; }
__attribute__((noinline)) int *weak_address(void) { return &absent; }
__attribute__((noinline)) void retain(void *p) {
    __asm__ volatile ("" : : "r"(p) : "memory");
}
__attribute__((noinline)) int capture(unsigned size) {
    char aligned[32] __attribute__((aligned(64)));
    char *dynamic = __builtin_alloca(size);
    void *frames[20];
    aligned[0] = 5; dynamic[0] = 9;
    retain(aligned); retain(dynamic);
    int depth = backtrace(frames, 20);
    return depth < 3 || aligned[0] != 5 || dynamic[0] != 9;
}
int main(void) {
    return capture(83) || *local_address() != 7 || weak_address() != 0;
}
