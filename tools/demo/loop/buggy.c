divide(int selector, int unused)
{
    int divisor = 0;
    if (selector > 0)
        divisor = selector;
    return 12 / divisor;
}

int main(void)
{
    return divide(0, 7);
}
