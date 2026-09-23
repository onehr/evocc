/* Reduced from the repeated row/column addressing in PolyBench floyd-warshall. */
int same_stride(const int *p, const int *q, int i) {
    return p[i] ^ q[i];
}
int different_strides(const int (*p)[3], const int (*q)[5], int i) {
    return p[i][0] ^ q[i][0];
}
int across_guard(const int *p, const int *q, int i, int n) {
    int a = p[i];
    int next = n + 1;
    return a ^ q[i] ^ next;
}
int main(void) {
    int a[3] = {11, 17, 29}, b[3] = {31, 37, 41};
    int rows3[2][3] = {{1, 2, 3}, {43, 5, 6}};
    int rows5[2][5] = {{7, 8, 9, 10, 11}, {47, 13, 14, 15, 16}};
    if (same_stride(a + 2, b + 2, -1) != (17 ^ 37)) return 1;
    if (different_strides(rows3, rows5, 1) != (43 ^ 47)) return 2;
    if (across_guard(a, b, 1, 4) != (17 ^ 37 ^ 5)) return 3;
    return 0;
}
