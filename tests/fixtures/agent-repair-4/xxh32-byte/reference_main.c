unsigned long long evaluate(unsigned seed);
int program_main(void);
int printf(const char *format, ...);
int main(void)
{
    unsigned seed;
    for (seed = 0; seed < 12; ++seed)
        printf("%u %llu\n", seed, evaluate(seed));
    printf("main %d\n", program_main());
    return 0;
}
