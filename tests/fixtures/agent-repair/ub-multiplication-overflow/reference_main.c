#include <stdio.h>

int program_main(void);

int multiply_saturated(int a, int b);

int main(void)
{
    printf("%d\n", multiply_saturated(2147483647, 2));
    printf("%d\n", multiply_saturated((-2147483647 - 1), 2));
    printf("%d\n", multiply_saturated((-2147483647 - 1), -1));
    printf("%d\n", multiply_saturated(0, 2147483647));
    printf("%d\n", multiply_saturated(12, 13));
    printf("%d\n", multiply_saturated(-7, 2));
    printf("%d\n", multiply_saturated(-17, -19));
    printf("%d\n", multiply_saturated(46340, 46340));
    printf("%d\n", program_main());
    return 0;
}
