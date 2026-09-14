struct padded { unsigned char a; unsigned long b; unsigned char c; };
union small { unsigned char a; unsigned long b[8]; };
union bits { unsigned char a : 1; unsigned long b; };

static int padding(const struct padded *p) {
    const unsigned char *bytes = (const unsigned char *)p;
    for (unsigned i = 1; i < 8; ++i)
        if (bytes[i]) return 1;
    for (unsigned i = 17; i < sizeof(*p); ++i)
        if (bytes[i]) return 2;
    return 0;
}

int main(void) {
    volatile unsigned long value = 23;
    struct padded empty = {};
    struct padded zero = {0};
    struct padded partial = {.b = value++};
    struct padded all = {.a = 3, .b = value++, .c = 7};
    struct padded array[2] = {{.b = 19}, {.a = 4, .c = 8}};
    union small small = {.a = 5};
    union bits bits = {.a = 1};
    if (padding(&empty) || padding(&zero) || padding(&partial) || padding(&all)
        || padding(&array[0]) || padding(&array[1])) return 1;
    if (partial.b != 23 || all.a != 3 || all.b != 24 || all.c != 7 || value != 25)
        return 2;
    const unsigned char *bytes = (const unsigned char *)&small;
    if (bytes[0] != 5) return 3;
    for (unsigned i = 1; i < sizeof(small); ++i)
        if (bytes[i]) return 4;
    bytes = (const unsigned char *)&bits;
    if (bytes[0] != 1) return 5;
    for (unsigned i = 1; i < sizeof(bits); ++i)
        if (bytes[i]) return 6;
    return 0;
}
