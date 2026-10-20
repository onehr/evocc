/* AArch64 native control: write a function, execute it, then replace its
 * return value. The builtin must call the target runtime, not act as a hint.
 * Pages are writable or executable, never both at the same time. */
#include <sys/mman.h>
#include <unistd.h>
#include <stdio.h>

static int evaluations;
static void *once(void *pointer) {
    ++evaluations;
    return pointer;
}

int main(void) {
    long page_size = sysconf(_SC_PAGESIZE);
    if (page_size <= 0) return 1;
    unsigned *code = mmap(0, page_size, PROT_READ | PROT_WRITE,
                         MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (code == MAP_FAILED) return 2;
    for (unsigned value = 17; value <= 18; ++value) {
        if (mprotect(code, page_size, PROT_READ | PROT_WRITE)) return 3;
        code[0] = 0x52800000u | value << 5; /* mov w0, #value */
        code[1] = 0xd65f03c0u;             /* ret */
        __builtin___clear_cache(once(code), once(code + 2));
        __builtin___clear_cache(code, code);
        if (mprotect(code, page_size, PROT_READ | PROT_EXEC)) return 4;
        int result = ((int (*)(void))code)();
        if (result != (int)value) return 5;
        printf("returned=%d\n", result);
    }
    if (evaluations != 4) return 6;
    return munmap(code, page_size) != 0;
}
