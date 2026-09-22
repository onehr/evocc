int main(void) {
    int n = 9;
    int a[n];
    if (a[0] || a[n - 1]) return 1;
    a[3] = 7;
    if (a[3] != 7) return 2;
    int nested[n][2];
    if (nested[0][0] || nested[n-1][1]) return 3;
    struct item { void *p; char c; long x; } objects[n];
    if (objects[0].p || objects[n-1].p || objects[n-1].x) return 4;
    unsigned char *bytes = (unsigned char *)objects;
    for (unsigned long i = 0; i < sizeof objects; i++)
        if (bytes[i]) return 5;
    int value = 19;
    objects[2].p = &value;
    if (*(int *)objects[2].p != 19) return 6;
    double unused[n];
    unsigned long representation;
    __builtin_memcpy(&representation, &unused[n-1], sizeof representation);
    return representation != 0;
}
