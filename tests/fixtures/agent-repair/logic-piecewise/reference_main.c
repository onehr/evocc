#include <stdio.h>

int program_main(void);

int piecewise_value(int value);
int main(void);

int main(void)
{
    printf("%d\n", piecewise_value(0));
    printf("%d\n", piecewise_value(1));
    printf("%d\n", piecewise_value(3));
    printf("%d\n", piecewise_value(4));
    printf("%d\n", piecewise_value(5));
    printf("%d\n", piecewise_value(6));
    printf("%d\n", piecewise_value(9));
    printf("%d\n", piecewise_value(100));
    printf("%d\n", program_main());
    return 0;
}
