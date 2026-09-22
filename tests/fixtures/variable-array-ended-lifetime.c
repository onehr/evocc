static int run(int n) {
    int *saved;
    {
        int a[n];
        a[0] = 7;
        saved = a;
    }
    return *saved;
}
int main(void) { return run(3); }
