#include <stdio.h>

int program_main(void);

int arithmetic_chain(int input);

int main(void)
{
    printf("%d\n", arithmetic_chain(-8));
    printf("%d\n", arithmetic_chain(-3));
    printf("%d\n", arithmetic_chain(-1));
    printf("%d\n", arithmetic_chain(0));
    printf("%d\n", arithmetic_chain(1));
    printf("%d\n", arithmetic_chain(2));
    printf("%d\n", arithmetic_chain(3));
    printf("%d\n", arithmetic_chain(7));
    printf("%d\n", arithmetic_chain(12));
    printf("%d\n", program_main());
    return 0;
}
