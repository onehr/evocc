#include <stdio.h>

int program_main(void);

int truth_value(int value);
int main(void);

int main(void)
{
    printf("%d\n", truth_value(-99));
    printf("%d\n", truth_value(-4));
    printf("%d\n", truth_value(-1));
    printf("%d\n", truth_value(0));
    printf("%d\n", truth_value(1));
    printf("%d\n", truth_value(2));
    printf("%d\n", truth_value(7));
    printf("%d\n", truth_value(99));
    printf("%d\n", program_main());
    return 0;
}
