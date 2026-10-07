/* Saturate the one overflowing integer quotient to 2147483647. The divisor is nonzero. */
int quotient(int a, int b);

int quotient(int a, int b)
{
    if (a == -2147483647 - 1 && b == -1) return 2147483647;
    return a / b;
}

int main(void)
{
    return quotient((-2147483647 - 1), -1) != 2147483647;
}
