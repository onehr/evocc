#include <stdio.h>

int program_main(void);

int fibonacci_ceiling(int limit);
int main(void);

int main(void)
{
    printf("%d\n", fibonacci_ceiling(1));
    printf("%d\n", fibonacci_ceiling(2));
    printf("%d\n", fibonacci_ceiling(3));
    printf("%d\n", fibonacci_ceiling(4));
    printf("%d\n", fibonacci_ceiling(5));
    printf("%d\n", fibonacci_ceiling(10));
    printf("%d\n", fibonacci_ceiling(100));
    printf("%d\n", fibonacci_ceiling(1000));
    printf("%d\n", program_main());
    return 0;
}
