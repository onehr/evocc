/* Return zero for a zero divisor; otherwise return the integer remainder. */
int remainder_or_zero(int a, int b);

int remainder_or_zero(int a, int b)
{
    if (b == 0) return 0;
    return a % b;
}

int main(void)
{
    return remainder_or_zero(9, 0) != 0;
}
