struct mixed { int value; float unused; };
__attribute__((noinline))
static int read(int n, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap, n);
    struct mixed x = __builtin_va_arg(ap, struct mixed);
    __builtin_va_end(ap);
    return x.value;
}
int main(void) {
    struct mixed x;
    x.value = 37;
    return read(1, x) != 37;
}
