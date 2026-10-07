#include <stdio.h>

int program_main(void);

int bit_or_offset(int input);

int main(void)
{
    printf("%d\n", bit_or_offset(0));
    printf("%d\n", bit_or_offset(1));
    printf("%d\n", bit_or_offset(2));
    printf("%d\n", bit_or_offset(3));
    printf("%d\n", bit_or_offset(4));
    printf("%d\n", bit_or_offset(7));
    printf("%d\n", bit_or_offset(8));
    printf("%d\n", bit_or_offset(16));
    printf("%d\n", bit_or_offset(31));
    printf("%d\n", program_main());
    return 0;
}
