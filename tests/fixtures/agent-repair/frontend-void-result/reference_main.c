#include <stdio.h>

int program_main(void);

int count_primes(int limit);

int main(void)
{
    printf("%d\n", count_primes(0));
    printf("%d\n", count_primes(2));
    printf("%d\n", count_primes(3));
    printf("%d\n", count_primes(4));
    printf("%d\n", count_primes(10));
    printf("%d\n", count_primes(20));
    printf("%d\n", count_primes(30));
    printf("%d\n", count_primes(50));
    printf("%d\n", count_primes(100));
    printf("%d\n", program_main());
    return 0;
}
