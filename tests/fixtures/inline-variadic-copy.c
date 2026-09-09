typedef __builtin_va_list argument_list;

static inline __attribute__((always_inline)) int one(argument_list ap)
{
    return __builtin_va_arg(ap, int);
}

static inline __attribute__((always_inline)) int pair(argument_list ap)
{
    int first = __builtin_va_arg(ap, int);
    return first + 10 * one(ap);
}

static int take(int count, ...)
{
    argument_list ap;
    __builtin_va_start(ap, count);
    int result = pair(ap);
    __builtin_va_end(ap);
    return result;
}

static int take_member(int count, ...)
{
    struct holder { argument_list ap; } h;
    __builtin_va_start(h.ap, count);
    int result = one(h.ap);
    __builtin_va_end(h.ap);
    return result;
}

int main(void)
{
    return take(2, 7, 9) != 97 || take(2, 3, 4) != 43
        || take_member(1, 13) != 13;
}
