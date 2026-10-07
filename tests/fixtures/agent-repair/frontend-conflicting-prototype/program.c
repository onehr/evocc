int bit_or_offset(int *input);

int bit_or_offset(int input)
{
    int x;

    x = input;
    x = x | 4;
    return x - 5;
}

int main(void)
{
    return bit_or_offset(1);
}
