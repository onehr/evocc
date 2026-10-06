/* Variadic calls in both directions across the C library boundary. */
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

static int total(int count, ...) {
    va_list list;
    int sum = 0;
    va_start(list, count);
    for (int index = 0; index < count; index++) {
        sum += va_arg(list, int);
    }
    va_end(list);
    return sum;
}

static int format(char *buffer, size_t size, const char *pattern, ...) {
    va_list list;
    va_list copy;
    va_start(list, pattern);
    va_copy(copy, list);
    int length = vsnprintf(buffer, size, pattern, copy);
    va_end(copy);
    va_end(list);
    return length;
}

int main(void) {
    char buffer[64];
    format(buffer, sizeof buffer, "%d-%s-%.2f", 7, "seven", 0.25);
    printf("%s %d %ld\n", buffer, total(9, 1, 2, 3, 4, 5, 6, 7, 8, 9), 1234567890123L);
    return strcmp(buffer, "7-seven-0.25") == 0 ? 0 : 1;
}
