#include <stdio.h>

int program_main(void);

int multiply_then_add(int a, int b, int c);
int main(void);

int main(void)
{
    printf("%d\n", multiply_then_add(12, 34, 56));
    printf("%d\n", multiply_then_add(1, 2, 3));
    printf("%d\n", multiply_then_add(-5, 2, 4));
    printf("%d\n", multiply_then_add(7, 9, 0));
    printf("%d\n", multiply_then_add(0, 8, 3));
    printf("%d\n", multiply_then_add(5, -2, 6));
    printf("%d\n", multiply_then_add(-3, -4, -2));
    printf("%d\n", multiply_then_add(100, 100, 100));
    printf("%d\n", program_main());
    return 0;
}
