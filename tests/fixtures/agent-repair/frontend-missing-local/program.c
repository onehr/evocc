int arithmetic_chain(int input)
{

    x = input;
    x = x * 10;
    x = x / 2;
    x = x % 3;
    return x - 2;
}

int main(void)
{
    return arithmetic_chain(1);
}
