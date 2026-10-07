#include <stdio.h>

int program_main(void);

int switch_trace(int value);
int main(void);

int main(void)
{
    printf("%d\n", switch_trace(-5));
    printf("%d\n", switch_trace(-1));
    printf("%d\n", switch_trace(0));
    printf("%d\n", switch_trace(1));
    printf("%d\n", switch_trace(2));
    printf("%d\n", switch_trace(3));
    printf("%d\n", switch_trace(4));
    printf("%d\n", switch_trace(99));
    printf("%d\n", program_main());
    return 0;
}
