#ifdef COMBINED
#include "mixed-record-varargs-worker.c"
#else
#include "mixed-record-varargs-abi.h"
#endif
int main(void) {
    struct small_mixed a;
    struct medium_mixed b;
    struct large_mixed c;
    a.value = 3;
    b.value = 5;
    c.first = 7;
    c.last = 11;
    if (read_mixed(0, a, b, c, 13L) != 39) return 1;
    if (read_mixed(7, 1L, 2L, 3L, 4L, 5L, 6L, 7L, a, b, c, 13L) != 39) return 2;
    return 0;
}
