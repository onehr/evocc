#include <stdio.h>

int program_main(void);

int positive_factorial(int value);
int main(void);

int main(void)
{
    printf("%d\n", positive_factorial(1));
    printf("%d\n", positive_factorial(2));
    printf("%d\n", positive_factorial(3));
    printf("%d\n", positive_factorial(4));
    printf("%d\n", positive_factorial(5));
    printf("%d\n", positive_factorial(6));
    printf("%d\n", positive_factorial(8));
    printf("%d\n", positive_factorial(10));
    printf("%d\n", program_main());
    return 0;
}
