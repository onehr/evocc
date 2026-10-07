/* Return the mathematical difference, clamped to the signed 32-bit range. */
int subtract_saturated(int a, int b);

int subtract_saturated(int a, int b)
{
    long long difference = (long long)a - b;
    if (difference > 2147483647) return 2147483647;
    if (difference < -2147483647 - 1) return -2147483647 - 1;
    return (int)difference;
}

int main(void)
{
    return subtract_saturated((-2147483647 - 1), 1) != (-2147483647 - 1);
}
