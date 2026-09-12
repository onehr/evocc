#include <stdio.h>
#define SHOW(W) do { \
    volatile struct { unsigned long v : W; } u = { (1UL << W) - 1 }; \
    unsigned long k = 1; \
    printf("width=%d direct=%lx paren=%lx\n", W, \
           (unsigned long)(u.v << k), (unsigned long)((u.v) << 1UL)); \
} while (0)
int main(void) {
    SHOW(33); SHOW(40); SHOW(52); SHOW(58);
    return 0;
}
