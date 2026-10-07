/* Return the negated argument, clamped to the signed 32-bit range. */
int negate_saturated(int a);

int negate_saturated(int a)
{
    if (a == -2147483647 - 1) return 2147483647;
    return -a;
}

int main(void)
{
    return negate_saturated((-2147483647 - 1)) != 2147483647;
}
