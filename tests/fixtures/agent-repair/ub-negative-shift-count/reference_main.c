#include <stdio.h>

int program_main(void);

int shift_or_zero(int a, int count);

int main(void)
{
    printf("%d\n", shift_or_zero(1, -1));
    printf("%d\n", shift_or_zero(1, -32));
    printf("%d\n", shift_or_zero(1, 0));
    printf("%d\n", shift_or_zero(1, 1));
    printf("%d\n", shift_or_zero(1, 30));
    printf("%d\n", shift_or_zero(1, 31));
    printf("%d\n", shift_or_zero(0, -1));
    printf("%d\n", shift_or_zero(0, 10));
    printf("%d\n", program_main());
    return 0;
}
