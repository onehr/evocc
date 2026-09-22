#include <stdio.h>
int main(void) {
    int n = 3, calls = 0;
    int a[n];
    __typeof__((calls++, &a)) p = &a;
    printf("pointer=%d/%lu\n", calls, (unsigned long)sizeof(*p));
    calls = 0;
    __typeof__(*(calls++, &a)) b;
    printf("array=%d/%lu\n", calls, (unsigned long)sizeof b);
    calls = 0;
    __typeof__(({ calls++; &a; })) q = &a;
    printf("pointer-statement=%d/%lu\n", calls, (unsigned long)sizeof(*q));
    calls = 0;
    __typeof__(({ calls++; a; })) r = a;
    printf("decayed=%d/%lu\n", calls, (unsigned long)sizeof r);
    return 0;
}
