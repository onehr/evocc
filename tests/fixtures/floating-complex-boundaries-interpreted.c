/* Run floating-complex-boundaries.c unchanged in the EvoCC interpreters.
 * The interpreters execute no printf, so a small formatter records the
 * probe's output line. EXPECTED is a string macro that holds the recorded
 * reference line (floating-complex-boundaries.out). Exit 0 means the probe
 * returned 0 and printed EXPECTED; 1 means another line; 2 means the probe
 * returned nonzero; 3 means a format that the formatter does not know. */
#include <stdarg.h>
#include <stddef.h>
#define main boundaries_main
#define printf boundaries_printf
#include "floating-complex-boundaries.c"
#undef printf
#undef main

static char line[256];
static size_t used;
static int unknown;

static void put(char c)
{
    if (used + 1 < sizeof line)
        line[used++] = c;
}

static void number(unsigned long long value, unsigned base, int width)
{
    char digits[32];
    int count = 0;
    do {
        digits[count++] = "0123456789abcdef"[value % base];
        value /= base;
    } while (value);
    while (count < width)
        digits[count++] = '0';
    while (count)
        put(digits[--count]);
}

int boundaries_printf(const char *restrict format, ...)
{
    va_list ap;
    va_start(ap, format);
    for (const char *p = format; *p; ++p) {
        if (*p != '%') {
            put(*p);
        } else if (p[1] == '0' && p[2] == '2' && p[3] == 'x') {
            number(va_arg(ap, unsigned), 16, 2);
            p += 3;
        } else if (p[1] == 'x') {
            number(va_arg(ap, unsigned), 16, 0);
            p += 1;
        } else if (p[1] == 'z' && p[2] == 'u') {
            number(va_arg(ap, size_t), 10, 0);
            p += 2;
        } else {
            unknown = 1;
            break;
        }
    }
    va_end(ap);
    return 0;
}

int main(void)
{
    static const char expected[] = EXPECTED "\n";
    int status = boundaries_main();
    if (unknown)
        return 3;
    if (status)
        return 2;
    if (used != sizeof expected - 1)
        return 1;
    for (size_t i = 0; i < used; ++i)
        if (line[i] != expected[i])
            return 1;
    return 0;
}
