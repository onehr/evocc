int evaluate(int seed);
int program_main(void);
int printf(const char *format, ...);

int main(void)
{
    int seed;
    for (seed = 0; seed < 12; seed++) {
        printf("%d\n", evaluate(seed));
    }
    printf("%d\n", program_main());
    return 0;
}
