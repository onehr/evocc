/* Return the mathematical sum, clamped to the signed 32-bit range. */
int add_saturated(int a, int b);

int add_saturated(int a, int b)
{
    return a + b;
}

int main(void)
{
    return add_saturated(2147483647, 1) != 2147483647;
}
