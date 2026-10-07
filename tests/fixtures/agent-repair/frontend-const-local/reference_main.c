#include <stdio.h>

int program_main(void);

int compound_steps(int input);

int main(void)
{
    printf("%d\n", compound_steps(-9));
    printf("%d\n", compound_steps(-3));
    printf("%d\n", compound_steps(-2));
    printf("%d\n", compound_steps(0));
    printf("%d\n", compound_steps(1));
    printf("%d\n", compound_steps(7));
    printf("%d\n", compound_steps(41));
    printf("%d\n", compound_steps(120));
    printf("%d\n", program_main());
    return 0;
}
