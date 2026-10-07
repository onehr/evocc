/* Return the mathematical product, clamped to the signed 32-bit range. */
int multiply_saturated(int a, int b);

int multiply_saturated(int a, int b)
{
    return a * b;
}

int main(void)
{
    return multiply_saturated(2147483647, 2) != 2147483647;
}
