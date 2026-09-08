int initialized;

int forward(int skip) {
    if (skip) goto inside;
    {
        int a[2] = {++initialized, 8};
    inside:
        a[0] = 3;
        return a[0] != 3;
    }
}

int backward(void) {
    int n = 0;
    int *p = 0;
again:;
    int a[2] = {5, 6};
    if (n++ == 0) {
        p = &a[0];
        goto again;
    }
    return *p != 5 || p != &a[0];
}

int entries(int choice) {
    if (choice == 1) goto first;
    if (choice == 2) goto second;
    {
        int a[2] = {1, 2};
    first:
        a[0] = 4;
    second:
        a[0] = 7;
        return a[0] != 7;
    }
}

int main(void) {
    if (forward(0) || initialized != 1) return 1;
    if (forward(1) || initialized != 1) return 2;
    if (backward()) return 3;
    for (int i = 0; i < 3; i++) if (entries(i)) return 4;
    return 0;
}
