#include <stdio.h>

int program_main(void);

int array_pair(int first, int second);

int main(void)
{
    printf("%d\n", array_pair(1, 2));
    printf("%d\n", array_pair(0, 0));
    printf("%d\n", array_pair(-3, 5));
    printf("%d\n", array_pair(7, 9));
    printf("%d\n", array_pair(-8, -2));
    printf("%d\n", array_pair(25, -4));
    printf("%d\n", array_pair(100, 17));
    printf("%d\n", program_main());
    return 0;
}
