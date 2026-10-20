static int g(int a, int b)
{
    int result = a + b;
    return result;
}

int f(int a)
{
    return g(a, 3) + g(a + 1, 3);
}
