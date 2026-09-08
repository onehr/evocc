struct pair { int first; long second; };
struct large { long first, second, third; };

struct pair small(int first, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, first);
    int second = __builtin_va_arg(ap, int);
    long third = __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return (struct pair){first, second + third};
}

struct large big(long first, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, first);
    long second = __builtin_va_arg(ap, long);
    long third = __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return (struct large){first, second, third};
}

int main(void) {
    struct pair a = small(3, (unsigned char)4, 5L);
    struct large (*fp)(long, ...) = big;
    struct large b = fp(10L, 20L, 30L);
    if (a.first != 3 || a.second != 9) return 1;
    if (b.first != 10 || b.second != 20 || b.third != 30) return 2;
    return 0;
}
