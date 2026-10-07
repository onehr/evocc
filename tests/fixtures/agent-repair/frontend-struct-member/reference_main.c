#include <stdio.h>

int program_main(void);

int struct_difference(int first, int second);

int main(void)
{
    printf("%d\n", struct_difference(3, 5));
    printf("%d\n", struct_difference(0, 0));
    printf("%d\n", struct_difference(-3, 5));
    printf("%d\n", struct_difference(7, 9));
    printf("%d\n", struct_difference(-8, -2));
    printf("%d\n", struct_difference(25, -4));
    printf("%d\n", struct_difference(100, 17));
    printf("%d\n", program_main());
    return 0;
}
