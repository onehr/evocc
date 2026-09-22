#include "mixed-record-varargs-abi.h"
int read_mixed(unsigned skip, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, skip);
    for (unsigned i = 0; i < skip; ++i)
        if (__builtin_va_arg(ap, long) != i + 1) return 100;
    struct small_mixed a = __builtin_va_arg(ap, struct small_mixed);
    struct medium_mixed b = __builtin_va_arg(ap, struct medium_mixed);
    struct large_mixed c = __builtin_va_arg(ap, struct large_mixed);
    long tail = __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return a.value + b.value + c.first + c.last + tail;
}
