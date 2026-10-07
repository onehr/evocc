#include <stdio.h>

int program_main(void);

int lookup(int index);

int main(void)
{
    printf("%d\n", lookup(-1));
    printf("%d\n", lookup(0));
    printf("%d\n", lookup(1));
    printf("%d\n", lookup(2));
    printf("%d\n", lookup(3));
    printf("%d\n", lookup(4));
    printf("%d\n", lookup(5));
    printf("%d\n", lookup(-8));
    printf("%d\n", program_main());
    return 0;
}
