#include <stdio.h>

int program_main(void);

int grid_count(int width, int height, int depth);
int main(void);

int main(void)
{
    printf("%d\n", grid_count(0, 3, 3));
    printf("%d\n", grid_count(1, 1, 1));
    printf("%d\n", grid_count(2, 3, 3));
    printf("%d\n", grid_count(3, 2, 4));
    printf("%d\n", grid_count(1, 4, 2));
    printf("%d\n", grid_count(5, 0, 4));
    printf("%d\n", grid_count(3, 3, 0));
    printf("%d\n", grid_count(10, 10, 10));
    printf("%d\n", program_main());
    return 0;
}
