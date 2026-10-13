/* The reproducer of the fortified-call defect, with the macOS 15 guard
 * pattern from guard15.h instead of the SDK headers. */
#include "guard15.h"

static int format(char *buffer, size_t size, const char *pattern, ...) {
    va_list list;
    va_start(list, pattern);
    int written = vsnprintf(buffer, size, pattern, list);
    va_end(list);
    return written;
}

int main(void) {
    char first[32], second[32];
    snprintf(first, sizeof first, "x=%d", 42);
    memcpy(second, first, sizeof first);
    strcpy(second, first);
    format(second, sizeof second, "%s!", first);
    puts(second);
    return 0;
}
