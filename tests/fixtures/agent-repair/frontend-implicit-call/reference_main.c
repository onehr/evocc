#include <stdio.h>

int program_main(void);

int function_difference(int a, int b);

int main(void)
{
    printf("%d\n", function_difference(1, 3));
    printf("%d\n", function_difference(0, 0));
    printf("%d\n", function_difference(-3, 5));
    printf("%d\n", function_difference(7, 9));
    printf("%d\n", function_difference(-8, -2));
    printf("%d\n", function_difference(25, -4));
    printf("%d\n", function_difference(100, 17));
    printf("%d\n", program_main());
    return 0;
}
