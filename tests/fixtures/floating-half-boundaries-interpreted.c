/* The fixed batch probe floating-half-boundaries.c, unchanged, for the EvoCC
 * interpreters, which do not call the C library's printf. A local function
 * takes the place of printf and keeps the line that the probe prints; it
 * formats the two conversions that the probe uses, %zu and %02x. main
 * returns the status of the probe, plus 2 when the line differs from the
 * reference line of GCC 14 and Clang 19 for the same KIND. */
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

static char line[96];
static unsigned length;

static void put(char c)
{
    if (length + 1 < sizeof line)
        line[length++] = c;
}

static int record(const char *format, ...)
{
    va_list arguments;
    va_start(arguments, format);
    for (const char *p = format; *p; ++p) {
        if (p[0] == '%' && p[1] == 'z' && p[2] == 'u') {
            size_t value = va_arg(arguments, size_t);
            char digits[24];
            unsigned count = 0;
            do {
                digits[count++] = (char)('0' + value % 10);
                value /= 10;
            } while (value);
            while (count)
                put(digits[--count]);
            p += 2;
        } else if (p[0] == '%' && p[1] == '0' && p[2] == '2' && p[3] == 'x') {
            unsigned value = va_arg(arguments, unsigned);
            put("0123456789abcdef"[value >> 4 & 15]);
            put("0123456789abcdef"[value & 15]);
            p += 3;
        } else {
            put(*p);
        }
    }
    va_end(arguments);
    return 0;
}

#define printf record
#define main probe_main
#include "floating-half-boundaries.c"
#undef main
#undef printf

#if KIND == 0
#define REFERENCE "size=2 align=2 unary=2 bytes=4000 input=3e00\n"
#elif KIND == 1
#define REFERENCE "size=2 align=2 unary=4 bytes=4000 input=3e00\n"
#else
#define REFERENCE "size=2 align=2 unary=2 bytes=4000 input=3fc0\n"
#endif

int main(void)
{
    int status = probe_main();
    const char *reference = REFERENCE;
    unsigned i = 0;
    while (reference[i] && i < length && reference[i] == line[i])
        ++i;
    return status | (reference[i] != 0 || i != length) << 1;
}
