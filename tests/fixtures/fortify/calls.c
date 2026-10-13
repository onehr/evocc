/* Fortified C library calls through the platform headers. The output does
 * not depend on the platform. */
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

static int format(char *buffer, size_t size, const char *pattern, ...) {
    va_list list;
    va_start(list, pattern);
    int written = vsnprintf(buffer, size, pattern, list);
    va_end(list);
    return written;
}

static int replace(char *buffer, const char *pattern, ...) {
    va_list list;
    va_start(list, pattern);
    int written = vsprintf(buffer, pattern, list);
    va_end(list);
    return written;
}

int main(int argc, char **argv) {
    char first[32], second[32], third[64], fourth[16], fifth[16];
    (void)argv;
    snprintf(first, sizeof first, "x=%d", 42);
    memcpy(second, first, sizeof first);
    memmove(second + 1, second, 4);
    strcpy(second, first);
    format(third, sizeof third, "%s!", first);
    sprintf(third + 8, "%d-%s", argc, "y");
    strcat(third, "+");
    strncat(third, "abcdef", 3);
    strncpy(fourth, "uvw", sizeof fourth);
    char *after = stpncpy(fifth, "ab", 4);
    memset(first, 'z', 4);
    first[4] = 0;
    char *end = stpcpy(second, "stp");
    replace(end + 1, "%c%c", 'p', 'q');
    printf("%s|%s|%s|%s|%s|%ld|%ld\n", first, second, third, third + 8, fourth,
           (long)(end - second), (long)(after - fifth));
    fprintf(stdout, "%d %u %c %s\n", -7, 7u, 'q', end + 1);
    char directory[512];
    printf("%d\n", getcwd(directory, sizeof directory) == directory);
    return 0;
}
