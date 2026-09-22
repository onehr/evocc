typedef unsigned int U __attribute__((vector_size(16)));
typedef unsigned char B __attribute__((vector_size(16)));
typedef unsigned long L __attribute__((vector_size(16)));

static U add(U value, int scalar) { return value + scalar; }
static B small(B value, signed char scalar) { return value + scalar; }
static L wide(L value, int scalar) { return value + scalar; }

int main(void) {
    U input = {3, 5, 7, 11};
    U quotient = -18 / -input;
    int once = -1;
    U sum = add(input, once++);
    B narrow = (B){0} + -1L;
    B signed_min = (B){0} + -128;
    B from_char = small((B){1}, -2);
    L from_int = wide((L){2, 3}, -3);
    return quotient[0] != -18U / -3U
        || quotient[3] != -18U / -11U
        || once != 0 || sum[0] != 2 || sum[3] != 10
        || narrow[0] != 255 || narrow[15] != 255
        || signed_min[0] != 128 || signed_min[15] != 128
        || from_char[0] != 255 || from_char[15] != 254
        || from_int[0] != ~0UL || from_int[1] != 0;
}
