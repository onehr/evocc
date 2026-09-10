static int prototype_calls;
static int outer_calls;
static int inner_calls;

int matrix(int n, int a[(++prototype_calls, 2)][(++prototype_calls, n)]);
int matrix(int n, int a[(++outer_calls, 2)][(++inner_calls, n)]) {
    int width = n;
    n = 99;
    a[1][width - 1] = 41;
    return sizeof(*a) != 4UL * width || a[1][width - 1] != 41;
}

static int pointer_form(int n, int (*a)[n++]) {
    int width = n - 1;
    n = 99;
    a[1][0] = 17;
    return sizeof(*a) != 4UL * width || a[1][0] != 17;
}

static int cube(int m, int n, int a[2][m][n]) {
    int rows = m, columns = n;
    m = n = 99;
    a[1][rows - 1][columns - 1] = 23;
    return sizeof(*a) != 4UL * rows * columns ||
           sizeof(**a) != 4UL * columns || a[1][rows - 1][columns - 1] != 23;
}

static __attribute__((always_inline)) inline int inlined(int n, int a[2][n]) {
    ++a[1][0];
    return sizeof(*a) + a[1][0];
}

static int invoke_inlined(int n, void *values) { return inlined(n, values); }
static int dependent(int n, int a[2][n], int b[sizeof(*a)]) {
    n = 99;
    return sizeof(*a) != 12 || sizeof(b) != sizeof(int *) || b[0] != 7;
}

static int recursive(int n, int a[2][n]) {
    int width = n;
    if (n > 1 && recursive(n - 1, (void *)a)) return 1;
    a[1][0] = width;
    return (char *)&a[1] - (char *)a != 4 * width;
}

static int unnamed(int n, int [2][(++inner_calls, n)]) { return n; }
static int through_callback(int n, int (*callback)(int m, int a[2][m]),
                            int a[2][n]) {
    return callback(n, a);
}
static int callback(int n, int a[2][n]) { return sizeof(*a) != 4UL * n; }
int star(int n, int a[*][*]);
int star(int n, int a[2][n]) { return sizeof(*a) != 4UL * n; }

int main(void) {
    int matrix(int n, int a[2][(++prototype_calls, n)]);
    int a[2][3] = {{0}}, b[2][5] = {{0}}, c[2][3][4] = {{{0}}};
    int buffer[24] = {0};
    if (prototype_calls || outer_calls || inner_calls) return 1;
    if (matrix(3, a) || matrix(5, b)) return 2;
    if (prototype_calls || outer_calls != 2 || inner_calls != 2) return 3;
    if (pointer_form(3, a) || pointer_form(5, b)) return 4;
    if (cube(3, 4, c) || c[1][2][3] != 23) return 5;
    if (inlined(3, a) != 30 || inlined(5, b) != 38) return 6;
    if (a[1][0] != 18 || b[1][0] != 18) return 7;
    if (recursive(4, (void *)buffer)) return 8;
    if (buffer[1] != 1 || buffer[2] != 2 || buffer[3] != 3 || buffer[4] != 4) return 9;
    if (unnamed(3, a) != 3 || inner_calls != 3) return 10;
    if (through_callback(5, callback, b) || star(3, a)) return 11;
    buffer[0] = 7;
    if (dependent(3, a, buffer)) return 12;
    if (invoke_inlined(3, a) != 31 || invoke_inlined(5, b) != 39) return 13;
    return prototype_calls != 0;
}
