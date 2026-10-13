/* GNU argument-pack forwarding, as in glibc's fortified printf wrappers.
 * Each anonymous argument is evaluated once, at the substituted call. */
static int calls;

static int sum(int count, ...) {
    __builtin_va_list list;
    __builtin_va_start(list, count);
    int total = 0;
    for (int index = 0; index < count; index++)
        total += __builtin_va_arg(list, int);
    __builtin_va_end(list);
    return total;
}

static int next(void) { return ++calls; }

extern __inline __attribute__((__always_inline__, __gnu_inline__)) int
forward(int count, ...) {
    return sum(count, __builtin_va_arg_pack()) * 10 + __builtin_va_arg_pack_len();
}

static inline __attribute__((always_inline)) int twice(int count, ...) {
    return forward(count, __builtin_va_arg_pack()) + forward(count, __builtin_va_arg_pack());
}

int main(void) {
    char small = 2;
    int first = forward(3, 1, small, next());
    int second = twice(2, next(), 5);
    return first + second + calls;
}
