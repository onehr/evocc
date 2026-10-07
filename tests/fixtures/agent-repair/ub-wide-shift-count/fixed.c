/* Return a shifted left by count; return zero for a count outside 0..30. Inputs a are 0 or 1. */
int shift_or_zero(int a, int count);

int shift_or_zero(int a, int count)
{
    if (count < 0 || count > 30) return 0;
    return a << count;
}

int main(void)
{
    return shift_or_zero(1, 32) != 0;
}
