typedef unsigned long U;
typedef unsigned __int128 Wide;
typedef U Vec __attribute__((vector_size(16)));
extern U boundary(U, U, U, U);

U choose(unsigned n, U x, U y) {
    U a;
    if (n & 1) a = x; else a = y;
    return a + 7;
}

U rotate(unsigned n, U a, U b, U c) {
    for (unsigned i = 0; i < n; ++i) {
        U old = a;
        a = b;
        b = c;
        c = old;
    }
    return (a + b * 3) ^ c;
}

U dispatch(unsigned n, U a, U b) {
    void *labels[] = {&&left, &&right};
    goto *labels[n & 1];
left:
    return a ^ (b + 17);
right:
    return b ^ (a + 29);
}

U shuffled(U a, U b, U c) {
    U keep = a + 17;
    return boundary(c, a, b, keep) ^ keep;
}

Wide wide_across(Wide x, U y) {
    U z = boundary(y, y + 1, y + 2, y + 3);
    return x ^ ((Wide)z << 64) ^ z;
}

Vec vector_across(Vec x, U y) {
    U z = boundary(y, y + 1, y + 2, y + 3);
    return x + (Vec){z, z ^ 0xdeadbeefUL};
}
