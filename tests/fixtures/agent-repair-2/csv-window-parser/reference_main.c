unsigned long long evaluate(unsigned seed);
int program_main(void);
int printf(const char *format, ...);
int main(void)
{
    unsigned i;
    for (i = 0; i < 12; ++i)
        printf("%u %llu\n", i, evaluate(i));
    printf("main %d\n", program_main());
    return 0;
}
