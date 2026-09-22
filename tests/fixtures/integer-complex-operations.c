typedef __complex__ int ci;
typedef _Complex short cs;
typedef __complex unsigned long cl;
static ci table[2] = { 2 + 3j, 9 - 1i };
static cs narrow(cs x) { return x + x; }
static cl wide(int lead, cl x, int tail) { return x + lead + tail; }
static int partial(void) {
    ci value;
    __real__ value = 7;
    int *part = &__real value;
    return *part;
}
int main(void) {
    cs x = 3 + 4i;
    _Static_assert(sizeof(+x) == 4 && sizeof(x + x) == 4, "complex width");
    x = narrow(x);
    if (__real__ x != 6 || __imag__ x != 8 || partial() != 7) return 1;
    cl y = 3 + 4i;
    y = wide(1, y, 2);
    if (__real__ y != 6 || __imag__ y != 4) return 2;
    ci z = 5 + 1i;
    if (z != 5 + 1i || ~z != 5 - 1i || -z != -5 - 1i) return 3;
    z += table[0];
    z -= 2;
    if (z != 5 + 4i || (int)z != 5 || !(_Bool)z) return 4;
    ci zero = 0;
    if (zero || (zero ? 1 : 0) || !z) return 5;
    int count = 0;
    if (__imag__ count++ != 0 || count != 1) return 6;
    (void)z;
    if (!__builtin_constant_p(__real__ (3 + 4i))) return 7;
    return 0;
}
