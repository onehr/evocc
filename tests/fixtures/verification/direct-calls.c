long converted_callee(int a, unsigned int b)
{
    return (long)a + (long)b;
}

int converted_caller(long a, unsigned long b)
{
    return converted_callee(a, b);
}

int checked_callee(int numerator, int divisor)
{
    return numerator / divisor;
}

int checked_caller(int x, int divisor)
{
    return checked_callee(x, divisor) + 1;
}

int nested_caller(int x, int divisor)
{
    return checked_caller(checked_callee(x, 2), divisor);
}
