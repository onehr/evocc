#include <stdio.h>

int main(void)
{
    unsigned long a = 0, b = 1;
    puts("Hello from a TinyCC-compiled C program!");
    puts("TinyCC itself was built with EvoCC.");
    printf("Fibonacci:");
    for (int i = 0; i < 12; ++i) {
        printf(" %lu", a);
        unsigned long next = a + b;
        a = b;
        b = next;
    }
    puts("");
    return a == 144 ? 0 : 1;
}
