int division_loop(int value, int count, int divisor)
{
    while (count) {
        value /= divisor;
        --count;
    }
    return value;
}

int division_order(int a, int b, int c)
{
    return (a / b) % c;
}

int division_choice(int choose, int a, int b)
{
    return choose ? a / b : a % b;
}
