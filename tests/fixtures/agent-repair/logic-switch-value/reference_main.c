#include <stdio.h>

int program_main(void);

int case_value(int value);
int main(void);

int main(void)
{
    printf("%d\n", case_value(-2));
    printf("%d\n", case_value(-1));
    printf("%d\n", case_value(0));
    printf("%d\n", case_value(1));
    printf("%d\n", case_value(2));
    printf("%d\n", case_value(3));
    printf("%d\n", case_value(4));
    printf("%d\n", case_value(5));
    printf("%d\n", program_main());
    return 0;
}
