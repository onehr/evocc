unsigned evaluate(unsigned seed);
int program_main(void);
int printf(const char *format, ...);
int main(void)
{
    unsigned seeds[12] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11};
    unsigned i;
    for (i = 0; i < 12; ++i)
    {
        printf("%u %u\n", seeds[i], evaluate(seeds[i]));
    }
    printf("main %d\n", program_main());
    return 0;
}
