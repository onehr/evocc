#include <stdio.h>

int program_main(void);

int quotient(int a, int b);

int main(void)
{
    printf("%d\n", quotient((-2147483647 - 1), -1));
    printf("%d\n", quotient((-2147483647 - 1), 1));
    printf("%d\n", quotient(2147483647, -1));
    printf("%d\n", quotient(9, 3));
    printf("%d\n", quotient(-9, 3));
    printf("%d\n", quotient(7, -2));
    printf("%d\n", quotient(0, -1));
    printf("%d\n", quotient(1, 1));
    printf("%d\n", program_main());
    return 0;
}
