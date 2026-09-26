typedef unsigned int lanes __attribute__((vector_size(16)));
struct record {
    char tag;
    unsigned bits : 3;
    long number;
    int *pointer;
    lanes vector;
    _Complex int pair;
    float single;
    double wide;
};

__attribute__((noinline)) int inspect(struct record *p) {
    if (p->tag || p->bits || p->number || p->pointer)
        return 1;
    if (p->vector[0] || p->vector[3] || __real__ p->pair || __imag__ p->pair)
        return 2;
    if (p->single != 0.0f || p->wide != 0.0)
        return 3;
    const unsigned char *bytes = (const unsigned char *)p;
    for (unsigned i = 0; i < sizeof(*p); ++i)
        if (bytes[i]) return 4;
    return 0;
}

__attribute__((noinline)) int dynamic(unsigned n) {
    struct record records[n];
    for (unsigned i = 0; i < n; ++i)
        if (inspect(&records[i])) return 5;
    return 0;
}

int main(void) {
    struct record object;
    int *pointers[3];
    if (inspect(&object) || pointers[0] || pointers[2]) return 6;
    return dynamic(2);
}
