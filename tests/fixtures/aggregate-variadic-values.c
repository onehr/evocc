struct Small { int x; };
struct Pair { long x, y; };
struct Large { long x, y, z; };

long aggregate_sum(int tag, ...) {
    __builtin_va_list ap, copy;
    __builtin_va_start(ap, tag);
    __builtin_va_copy(copy, ap);
    struct Small a = __builtin_va_arg(ap, struct Small);
    struct Pair b = __builtin_va_arg(ap, struct Pair);
    struct Large c = __builtin_va_arg(ap, struct Large);
    long tail = __builtin_va_arg(ap, long);
    struct Small again = __builtin_va_arg(copy, struct Small);
    __builtin_va_end(copy);
    __builtin_va_end(ap);
    return tag + a.x + b.x + b.y + c.x + c.y + c.z + tail + again.x;
}

long aggregate_stacked(long a, long b, long c, long d, long e, long f, long g, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, g);
    struct Pair pair = __builtin_va_arg(ap, struct Pair);
    struct Large large = __builtin_va_arg(ap, struct Large);
    long tail = __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return a + b + c + d + e + f + g + pair.x + pair.y + large.x + large.y + large.z + tail;
}

int main(void) {
    struct Small a = {2};
    struct Pair b = {3, 5};
    struct Large c = {7, 11, 13};
    long (*indirect)(int, ...) = aggregate_sum;
    return sizeof(a) != 4 || sizeof(b) != 16 || sizeof(c) != 24 ||
           aggregate_sum(1, a, b, c, 17L) != 61 ||
           indirect(1, a, b, c, 17L) != 61 ||
           aggregate_stacked(1, 2, 3, 4, 5, 6, 7, b, c, 17L) != 84;
}
