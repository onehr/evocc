int runtime_convert(int value);
#define constant_convert(value) \
    (__builtin_constant_p(value) ? ((value) << 4) : runtime_convert(value))

static const unsigned char encoded = constant_convert(2);
static const unsigned long logarithm =
    __builtin_constant_p(100 / 10) ? 63 - __builtin_clzl(100 / 10) : 0;
static const int masks[] = {
    [0 ... 3] = __builtin_constant_p(~0) ? ~0 : runtime_convert(~0)
};
static int untouched;
static const int no_evaluation = __builtin_constant_p(untouched++);

int main(void)
{
    return encoded != 32 || logarithm != 3 || masks[2] != -1 ||
        no_evaluation != 0 || untouched != 0;
}
