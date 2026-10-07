#include <stdio.h>

int program_main(void);

int series_sum(int limit);
int main(void);

int main(void)
{
    printf("%d\n", series_sum(0));
    printf("%d\n", series_sum(1));
    printf("%d\n", series_sum(2));
    printf("%d\n", series_sum(3));
    printf("%d\n", series_sum(4));
    printf("%d\n", series_sum(8));
    printf("%d\n", series_sum(20));
    printf("%d\n", series_sum(100));
    printf("%d\n", program_main());
    return 0;
}
