#include <stdio.h>

int program_main(void);

int add_saturated(int a, int b);

int main(void)
{
    printf("%d\n", add_saturated(2147483647, 1));
    printf("%d\n", add_saturated((-2147483647 - 1), -1));
    printf("%d\n", add_saturated(0, 0));
    printf("%d\n", add_saturated(3, 9));
    printf("%d\n", add_saturated(-7, 2));
    printf("%d\n", add_saturated(2147483647, -1));
    printf("%d\n", add_saturated((-2147483647 - 1), 2147483647));
    printf("%d\n", add_saturated(1234, -234));
    printf("%d\n", program_main());
    return 0;
}
