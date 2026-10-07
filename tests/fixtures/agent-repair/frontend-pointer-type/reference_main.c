#include <stdio.h>

int program_main(void);

int pointer_store(int initial, int value);

int main(void)
{
    printf("%d\n", pointer_store(4, 0));
    printf("%d\n", pointer_store(0, 7));
    printf("%d\n", pointer_store(-3, 11));
    printf("%d\n", pointer_store(99, -5));
    printf("%d\n", pointer_store(-8, -8));
    printf("%d\n", pointer_store(1, 31));
    printf("%d\n", pointer_store(100, -100));
    printf("%d\n", program_main());
    return 0;
}
