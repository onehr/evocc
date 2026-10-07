/* Return the mathematical product, clamped to the signed 32-bit range. */
int multiply_saturated(int a, int b);

int multiply_saturated(int a, int b)
{
    long long product = (long long)a * b;
    if (product > 2147483647) return 2147483647;
    if (product < -2147483647 - 1) return -2147483647 - 1;
    return (int)product;
}

int main(void)
{
    return multiply_saturated(2147483647, 2) != 2147483647;
}
