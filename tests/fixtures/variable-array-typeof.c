int main(void) {
    int n = 2;
    __typeof__(int[n++]) a;
    if (n != 3 || sizeof a != 2 * sizeof(int)) return 1;
    a[0] = 7;
    a[1] = 9;
    typedef __typeof__(a) A;
    A b;
    b[0] = a[0];
    b[1] = a[1];
    __typeof__(int (*)[n++]) p = 0;
    if (n != 4 || sizeof(*p) != 3 * sizeof(int)) return 2;
    return b[0] != 7 || b[1] != 9 || sizeof b != sizeof a;
}
