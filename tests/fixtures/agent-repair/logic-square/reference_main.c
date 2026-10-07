#include <stdio.h>

int program_main(void);

int square_value(int value);
int main(void);

int main(void)
{
    printf("%d\n", square_value(-12));
    printf("%d\n", square_value(-3));
    printf("%d\n", square_value(-1));
    printf("%d\n", square_value(0));
    printf("%d\n", square_value(1));
    printf("%d\n", square_value(2));
    printf("%d\n", square_value(7));
    printf("%d\n", square_value(15));
    printf("%d\n", program_main());
    return 0;
}
