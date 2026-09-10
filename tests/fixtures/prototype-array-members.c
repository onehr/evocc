struct callbacks {
    int (*array)(int n, int a[n][n]);
    int (*pointer)(int n, int (*a)[n]);
};

static int read_row(int n, int a[n][n]) { return a[1][0]; }

int main(void) {
    int data[4] = {1, 2, 3, 4};
    typedef int callback(int n, int a[n][n]);
    struct local { callback *call; } local = {read_row};
    struct callbacks c = {read_row, read_row};
    return sizeof(c) != 16 || sizeof(local) != 8 ||
        c.array(2, (void *)data) != 3 || c.pointer(2, (void *)data) != 3 ||
        local.call(2, (void *)data) != 3;
}
