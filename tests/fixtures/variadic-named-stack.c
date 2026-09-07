/* Named stack arguments must not be read again by va_arg. */
static int nine(long a, long b, long c, long d, long e, long f,
                long g, long h, long i, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, i);
    long first = __builtin_va_arg(ap, long);
    long second = __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return first != 10 || second != 11;
}

static int ten(long a, long b, long c, long d, long e, long f,
               long g, long h, long i, long j, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, j);
    long first = __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return first != 11;
}

/* The eighth register is skipped for the aligned pair, on the stack. */
static int wide(long a, long b, long c, long d, long e, long f,
                long g, __int128 h, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, h);
    long first = __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return first != 11;
}

int main(void) {
    return nine(1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L, 11L)
        | ten(1L, 2L, 3L, 4L, 5L, 6L, 7L, 8L, 9L, 10L, 11L) << 1
        | wide(1L, 2L, 3L, 4L, 5L, 6L, 7L, (__int128)9, 11L) << 2;
}
