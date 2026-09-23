volatile int input;
int calls;
__attribute__((noinline)) int next(void) { ++calls; return input; }
__attribute__((noinline)) int normalize(int n) { return !!!!n; }
__attribute__((noinline)) int invert(int n) { return !!!n; }

int main(void)
{
    for (int n = -3; n <= 3; ++n) {
        input = n;
        calls = 0;
        if (!!!!next() != (n != 0) || calls != 1)
            return 1;
        if (normalize(n) != (n != 0) || invert(n) != (n == 0))
            return 2;
    }
    int n = 0;
    if (!!!!n++ || n != 1 || !!!!!!!n++ || n != 2)
        return 3;
    int *p = &n;
    if (!!!!p != 1 || !!!!(int *)0 != 0)
        return 4;
    unsigned __int128 wide = (unsigned __int128)1 << 100;
    if (!!!!wide != 1)
        return 5;
    volatile double zero = -0.0, one = 1.0;
    double nan = zero / zero;
    if (!!!!zero || !!!!one != 1 || !!!!nan != 1 || !!!nan)
        return 6;
    int x = 0;
    if (!!!!(input || (x = 7)) != 1 || x != 0)
        return 7;
    return 0;
}
