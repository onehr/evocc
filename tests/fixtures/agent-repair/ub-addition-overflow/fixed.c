/* Return the mathematical sum, clamped to the signed 32-bit range. */
int add_saturated(int a, int b);

int add_saturated(int a, int b)
{
    long long sum = (long long)a + b;
    if (sum > 2147483647) return 2147483647;
    if (sum < -2147483647 - 1) return -2147483647 - 1;
    return (int)sum;
}

int main(void)
{
    return add_saturated(2147483647, 1) != 2147483647;
}
