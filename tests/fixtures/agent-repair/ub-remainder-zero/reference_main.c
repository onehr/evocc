#include <stdio.h>

int program_main(void);

int remainder_or_zero(int a, int b);

int main(void)
{
    printf("%d\n", remainder_or_zero(9, 0));
    printf("%d\n", remainder_or_zero(9, 4));
    printf("%d\n", remainder_or_zero(-9, 4));
    printf("%d\n", remainder_or_zero(9, -4));
    printf("%d\n", remainder_or_zero(0, 0));
    printf("%d\n", remainder_or_zero(0, 3));
    printf("%d\n", remainder_or_zero(17, 1));
    printf("%d\n", remainder_or_zero(-17, -5));
    printf("%d\n", program_main());
    return 0;
}
