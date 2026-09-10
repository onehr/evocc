#include <stdio.h>

static int calls;
static int bound(int n) { ++calls; return n; }
static int legacy(n, a)
    int n;
    int a[2][n++];
{
    int width = n - 1;
    printf("increment: n=%d size=%lu stride=%ld\n", n,
           (unsigned long)sizeof(*a), (long)((char *)&a[1] - (char *)a));
    n = 99;
    a[1][0] += width;
    printf("changed: n=%d size=%lu stride=%ld\n", n,
           (unsigned long)sizeof(*a), (long)((char *)&a[1] - (char *)a));
    return 0;
}
static int named(n, a)
    int n;
    int a[2][bound(n)];
{
    printf("call: n=%d size=%lu stride=%ld calls=%d\n", n,
           (unsigned long)sizeof(*a), (long)((char *)&a[1] - (char *)a), calls);
    return 0;
}
int main(void) {
    int data[24] = {0};
    legacy(3, (void *)data);
    printf("stored=%d\n", data[3]);
    named(3, (void *)data);
    return 0;
}
