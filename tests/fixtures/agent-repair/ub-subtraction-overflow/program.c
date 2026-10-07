/* Return the mathematical difference, clamped to the signed 32-bit range. */
int subtract_saturated(int a, int b);

int subtract_saturated(int a, int b)
{
    return a - b;
}

int main(void)
{
    return subtract_saturated((-2147483647 - 1), 1) != (-2147483647 - 1);
}
