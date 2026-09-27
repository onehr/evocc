/* Function-wide allocation with calls, pressure, and multiple loop/branch blocks. */
static volatile unsigned calls;
__attribute__((noinline)) int value(int x)
{
    ++calls;
    return 3 * x + 1;
}

__attribute__((noinline)) int batch(int x)
{
    int a = value(x), b = value(x + 1), c = value(x + 2);
    int d = value(x + 3), e = value(x + 4), f = value(x + 5);
    int g = value(x + 6), h = value(x + 7), i = value(x + 8);
    int j = value(x + 9), k = value(x + 10), l = value(x + 11);
    int sum = a + b + c + d + e + f + g + h + i + j + k + l;
    for (unsigned n = 0; n < 8; ++n)
        sum += n & 1 ? a : l;
    return sum;
}

int main(void)
{
    return batch(2) != 470 || calls != 12;
}
