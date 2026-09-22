typedef int lanes __attribute__((vector_size(8)));

static inline long pack(lanes value) {
    return (long)value;
}

static inline lanes advance(lanes value) {
    value += (lanes){2, 3};
    return value;
}

int main(void) {
    lanes original = {1, -1};
    int evaluations = 0;
    long bits = pack((evaluations++, original));
    lanes result = advance(original);
    lanes nested = ({ lanes local = ({ lanes inner = {4, 5}; inner; }); local; });
    return bits != (long)0xffffffff00000001UL || evaluations != 1
        || original[0] != 1 || original[1] != -1
        || result[0] != 3 || result[1] != 2
        || nested[0] != 4 || nested[1] != 5;
}
