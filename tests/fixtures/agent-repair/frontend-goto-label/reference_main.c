#include <stdio.h>

int program_main(void);

int goto_route(int input);

int main(void)
{
    printf("%d\n", goto_route(-9));
    printf("%d\n", goto_route(-2));
    printf("%d\n", goto_route(0));
    printf("%d\n", goto_route(1));
    printf("%d\n", goto_route(7));
    printf("%d\n", goto_route(41));
    printf("%d\n", goto_route(120));
    printf("%d\n", program_main());
    return 0;
}
