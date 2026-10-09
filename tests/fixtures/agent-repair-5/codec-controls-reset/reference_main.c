unsigned long long evaluate(unsigned seed);
int program_main(void);
int printf(const char *format, ...);
int main(void)
{
    unsigned seeds[16] = {0, 1, 2, 3, 4, 5, 6, 7, 15, 31, 255, 65535, 65536, 2147483647u, 2147483648u, 4294967295u};
    unsigned index;
    for (index = 0; index < 16; ++index)
        printf("%u %llu\n", seeds[index], evaluate(seeds[index]));
    printf("main %d\n", program_main());
    return 0;
}
