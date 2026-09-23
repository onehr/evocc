volatile unsigned observations;

unsigned tick(unsigned n)
{
    observations += n;
    return n;
}

unsigned paths(unsigned n)
{
    volatile unsigned a[4] = {7, 2, 9, 1};
    unsigned sum = 0;
    for (unsigned j = 0; j < n; ++j) {
        unsigned old = a[j];
        a[j] = old < 5 ? tick(old + 1) : tick(old - 1);
        sum += a[j];
    }
    return sum;
}

int labels(int n)
{
    static void *table[] = {&&again, &&done};
    int x = 0;
    if (n < 0)
        goto *table[1];
again:
    if (x++ < n)
        goto *table[0];
done:
    return x;
}

int scoped(int count)
{
    int sum = 0;
    for (int n = 1; n <= count; ++n) {
        int a[n];
        int *p = &a[n - 1];
        if (n & 1)
            *p = n;
        else
            *p = n * 2;
        sum += *p;
    }
    return sum;
}

int main(void)
{
    if (paths(0) != 0 || observations != 0)
        return 1;
    if (paths(4) != 19 || observations != 19)
        return 2;
    if (labels(-1) != 0 || labels(3) != 4)
        return 3;
    return scoped(4) != 16;
}
