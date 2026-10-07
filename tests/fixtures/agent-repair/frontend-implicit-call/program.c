int function_difference(int a, int b)
{
    return foo(a, b);
}

int foo(int a, int b)
{
    return 2 + a - b;
}

int main(void)
{
    return function_difference(1, 3);
}
