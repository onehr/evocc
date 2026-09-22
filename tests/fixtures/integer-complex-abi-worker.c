__attribute__((noinline)) _Complex short complex_short(_Complex short x) {
    return x + (2 + 3i);
}
struct pair { _Complex char a, b; };
__attribute__((noinline)) struct pair complex_pair(struct pair x) {
    x.a += 2 + 3i;
    x.b -= 1 + 2i;
    return x;
}
__attribute__((noinline)) _Complex int complex_int(int before, _Complex int x, int after) {
    return before + x + after;
}
__attribute__((noinline)) _Complex long complex_long(long a, long b, long c, long d,
        long e, long f, long g, _Complex long x, long h) {
    return x + a + b + c + d + e + f + g + h;
}
