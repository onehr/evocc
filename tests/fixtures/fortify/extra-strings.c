/* Exercise the actual checked entry points with a runtime bound. The three
 * functions have different return types and different stopping conditions. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv) {
    char buffer[8] = "a";
    unsigned long count = argc > 1 ? strtoul(argv[1], 0, 10) : sizeof buffer;
    int operation = argc > 2 ? atoi(argv[2]) : 0;
    if (operation == 0) {
        char *end = __builtin___memccpy_chk(buffer, "abcde", 'c' + 256, count, sizeof buffer);
        printf("memccpy %ld %c%c%c\n", end ? end - buffer : -1L, buffer[0], buffer[1], buffer[2]);
    } else if (operation == 1) {
        unsigned long length = __builtin___strlcpy_chk(buffer, "123456789", count, sizeof buffer);
        printf("strlcpy %lu %s\n", length, buffer);
    } else {
        unsigned long length = __builtin___strlcat_chk(buffer, "123456789", count, sizeof buffer);
        printf("strlcat %lu %s\n", length, buffer);
    }
    return 0;
}
