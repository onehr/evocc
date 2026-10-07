#include <stdio.h>

int program_main(void);

int negate_saturated(int a);

int main(void)
{
    printf("%d\n", negate_saturated((-2147483647 - 1)));
    printf("%d\n", negate_saturated(2147483647));
    printf("%d\n", negate_saturated(-1));
    printf("%d\n", negate_saturated(0));
    printf("%d\n", negate_saturated(1));
    printf("%d\n", negate_saturated(19));
    printf("%d\n", negate_saturated(-19));
    printf("%d\n", negate_saturated(-2147483647));
    printf("%d\n", program_main());
    return 0;
}
