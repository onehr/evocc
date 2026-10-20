/* VM operands execute once at their declaration, not once per typedef use.
 * Unevaluated operands must not leak the hidden evaluation into the block. */
int main(void)
{
    int n = 3, calls = 0;
    int a[n];
    for (int i = 0; i < 3; ++i) {
        typedef __typeof__((calls++, &a)) pointer;
        if (calls != i + 1) return 1;
        pointer p = &a, q = &a;
        if (sizeof(*p) != 3 * sizeof(int) || p != q) return 2;
    }
    if (calls != 3) return 3;
    unsigned long size = sizeof(__typeof__((calls++, &a)));
    if (size != sizeof(void *) || calls != 3) return 4;
    int selected = __builtin_choose_expr(1, 7,
        ({ __typeof__((calls++, &a)) p = &a; sizeof(*p); }));
    if (selected != 7 || calls != 3) return 5;

    __typeof_unqual__((calls++, &a)) p = (calls == 4 ? &a : 0);
    if (p != &a || calls != 4) return 6;
    p = (__typeof__((calls++, &a))) &a;
    if (p != &a || calls != 5) return 7;
    int matrix[1][n];
    __typeof__((calls++, matrix)) row = matrix;
    if (row != matrix || calls != 6 || sizeof(*row) != sizeof a) return 8;
    __typeof__((calls++, a)) element = a;
    if (element != a || calls != 6) return 9;

    __typeof__(({ struct private_tag { int x; }; calls++; &a; })) q = &a;
    struct private_tag { long y; } object;
    object.y = 7;
    if (q != p || calls != 7 || object.y != 7) return 10;
    if (__alignof__(__typeof__((calls++, &a))) != __alignof__(void *) || calls != 7) return 11;
    if (!__builtin_types_compatible_p(__typeof__((calls++, &a)), __typeof__(&a)) || calls != 7) return 12;
    size = sizeof(__typeof__(*(calls++, &a)));
    if (size != sizeof a || calls != 8) return 13;
    size = 0 && sizeof(__typeof__(*(calls++, &a)));
    if (size != 0 || calls != 8) return 14;
    size = sizeof(sizeof(__typeof__(*(calls++, &a))));
    if (size != sizeof(unsigned long) || calls != 8) return 15;

    int (*ended)[n];
    {
        int temporary[n];
        ended = &temporary;
    }
    size = sizeof(__typeof__(ended));
    if (size != sizeof(void *)) return 16;
    __typeof__(ended) after = 0;
    if (after != 0) return 17;
    _Alignas(__typeof__((calls++, &a))) int aligned = 0;
    if (calls != 8 || aligned != 0) return 18;
    return 0;
}
