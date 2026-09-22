/* Reduced from LLVM conditional-gnu-ext; parameter storage survives CFG edges. */
static int calls;
__attribute__((noinline)) static _Complex int choose(_Complex int value, int c) {
    if (c) return value;
    return value + 1;
}
__attribute__((noinline)) static _Complex int next(_Complex int value) {
    if (calls++) return value + 10;
    return value;
}
__attribute__((noinline)) static _Complex int omitted(void) {
    _Complex int never_read;
    return next(1 + 2i) ?: never_read;
}
typedef long wide __attribute__((vector_size(32)));
__attribute__((noinline)) static wide vector_branch(wide value, int c) {
    if (c) value += 1;
    else value += 2;
    return value;
}
int main(void) {
#ifdef READ_UNINITIALIZED
    _Complex int uninitialized;
    _Complex int result = choose(0, 1) ?: uninitialized;
    return __real__ result;
#endif
    _Complex int z = choose(3 + 4i, 1);
    if (__real__ z != 3 || __imag__ z != 4) return 1;
    z = choose(3 + 4i, 0);
    if (__real__ z != 4 || __imag__ z != 4) return 2;
    z = omitted();
    if (__real__ z != 1 || __imag__ z != 2 || calls != 1) return 3;
    calls = 0;
    z = next(0) ?: (7 + 8i);
    if (__real__ z != 7 || __imag__ z != 8 || calls != 1) return 4;
    z = 3 ?: next(9 + 10i);
    if (__real__ z != 3 || __imag__ z != 0 || calls != 1) return 5;
    _Complex short narrow = 0;
    _Complex long larger = narrow ?: (_Complex long)(5 + 6i);
    if (__real__ larger != 5 || __imag__ larger != 6) return 6;
    wide v = {10, 20, 30, 40};
    v = vector_branch(v, 1);
    v = vector_branch(v, 0);
    return v[0] != 13 || v[1] != 23 || v[2] != 33 || v[3] != 43;
}
