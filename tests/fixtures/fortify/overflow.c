/* A fortified call whose destination is too small for the first argument.
 * The second argument selects the call. A checked call stops the program. */
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    char small[8];
    if (argc < 3)
        return 2;
    if (!strcmp(argv[2], "memcpy"))
        memcpy(small, argv[1], strlen(argv[1]) + 1);
    else if (!strcmp(argv[2], "snprintf"))
        snprintf(small, 64, "%s", argv[1]);
    else
        strcpy(small, argv[1]);
    printf("%s\n", small);
    return 0;
}
