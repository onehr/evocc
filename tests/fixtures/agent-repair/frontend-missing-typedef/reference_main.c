#include <stdio.h>

int program_main(void);

int typed_value(int input);

int main(void)
{
    printf("%d\n", typed_value(-9));
    printf("%d\n", typed_value(-2));
    printf("%d\n", typed_value(0));
    printf("%d\n", typed_value(1));
    printf("%d\n", typed_value(7));
    printf("%d\n", typed_value(41));
    printf("%d\n", typed_value(120));
    printf("%d\n", program_main());
    return 0;
}
