static int check(int n) {
    int *saved;
    {
        int a[n];
        a[n - 1] = 21;
        saved = __builtin_alloca(sizeof(int));
        *saved = a[n - 1];
    }
    {
        int b[n + 1];
        b[n] = 16;
        if (*saved + b[n] != 37) return 1;
    }
    return *saved != 21;
}
int main(void) { return check(7) || check(13); }
