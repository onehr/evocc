/* Fortified unistd.h calls with lengths that are not constants. The second
 * argument names a call whose length exceeds its buffer. */
#include <stdio.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    char buffer[16], link[32], directory[256], path[64];
    size_t extra = argc > 1 ? strlen(argv[1]) : 0;
    if (argc > 2 && !strcmp(argv[2], "read"))
        return (int)read(-1, buffer, sizeof buffer + extra);
    printf("%zd\n", read(-1, buffer, sizeof buffer - extra));
    printf("%zd\n", pread(-1, buffer, sizeof buffer - extra, 0));
    printf("%zd\n", readlink("/evocc-no-such-link", link, sizeof link - extra));
    printf("%d\n", getcwd(directory, sizeof directory - extra) == directory);
    printf("%d\n", confstr(_CS_PATH, path, sizeof path - extra) > 0);
    printf("%d\n", gethostname(path, sizeof path - extra) == 0);
    return 0;
}
