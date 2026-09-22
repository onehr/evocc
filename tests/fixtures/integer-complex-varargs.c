/* pr38151 reduction: integer complex is a composite, not a floating HFA. */
struct small { unsigned int tag; _Complex int value; };
struct aligned { unsigned int tag; _Complex int value; struct {} __attribute__((aligned)) empty; };
struct large { long prefix; _Complex long values[2]; };

#ifndef DRIVER_ONLY
__attribute__((noinline)) int read_values(int exhausted, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, exhausted);
    if (exhausted)
        for (int i = 0; i < 9; ++i)
            if (__builtin_va_arg(ap, long) != i + 1) return 1;
    struct small a = __builtin_va_arg(ap, struct small);
    struct aligned b = __builtin_va_arg(ap, struct aligned);
    struct large c = __builtin_va_arg(ap, struct large);
    _Complex int d = __builtin_va_arg(ap, _Complex int);
    __builtin_va_end(ap);
    if (a.tag != 41 || __real__ a.value != 7 || __imag__ a.value != -3) return 2;
    if (b.tag != 42 || __real__ b.value != -9 || __imag__ b.value != 5) return 3;
    if (c.prefix != 43 || __real__ c.values[1] != 12 || __imag__ c.values[1] != -11) return 4;
    return (__real__ d != 13 || __imag__ d != -6) ? 5 : 0;
}
#else
int read_values(int exhausted, ...);
#endif

#ifndef WORKER_ONLY
int main(void) {
    struct small a = { 41, 7 - 3i };
    struct aligned b = { 42, -9 + 5i, {} };
    struct large c = { 43, { 1 + 2i, 12 - 11i } };
    int result = read_values(0, a, b, c, 13 - 6i);
    if (result) return result;
    return read_values(1, 1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, a, b, c, 13 - 6i);
}
#endif
