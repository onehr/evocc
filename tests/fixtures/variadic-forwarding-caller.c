#include <stdio.h>
#include <string.h>

int wrap(char *, unsigned, const char *, ...);

int main(void) {
    char buf[256];
    if (wrap(buf, sizeof buf, "%d %g %d", 7, 1e9, 11) != 10) return 1;
    if (strcmp(buf, "7 1e+09 11")) return 2;
    if (wrap(buf, sizeof buf, "%g %g %g %g %g %g %g %g %g %g %d",
             1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 13) != 23) return 3;
    return strcmp(buf, "1 2 3 4 5 6 7 8 9 10 13") != 0;
}
