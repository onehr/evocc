/* All four checked output builtins must preserve the format flag, including
 * the library's rejection of %n in a writable format string. */
#include <stdio.h>
#include <stdarg.h>

static int print_list(int flag, const char *format, ...) {
    va_list arguments;
    va_start(arguments, format);
    int result = __builtin___vprintf_chk(flag, format, arguments);
    va_end(arguments);
    return result;
}

static int print_stream(FILE *stream, int flag, const char *format, ...) {
    va_list arguments;
    va_start(arguments, format);
    int result = __builtin___vfprintf_chk(stream, flag, format, arguments);
    va_end(arguments);
    return result;
}

int main(int argc, char **argv) {
    if (argc > 1) {
        char format[] = "written%n";
        int count = -1;
        if (argv[1][0] == 'p') __builtin___printf_chk(1, format, &count);
        if (argv[1][0] == 'f') __builtin___fprintf_chk(stdout, 1, format, &count);
        if (argv[1][0] == 'v') print_list(1, format, &count);
        if (argv[1][0] == 's') print_stream(stdout, 1, format, &count);
        return count != 7;
    }
    int count = __builtin___printf_chk(0, "%s=%d\n", "printf", 12);
    count += __builtin___fprintf_chk(stdout, 1, "%s=%d\n", "fprintf", 34);
    count += print_list(1, "%s=%d\n", "vprintf", 56);
    count += print_stream(stdout, 1, "%s=%d\n", "vfprintf", 78);
    printf("total=%d\n", count);
    return 0;
}
