int compound_steps(int input)
{
    int x = input;

    x += 2;
    x += 2;
    x -= 1;
    x *= 2;
    return x;
}

int main(void)
{
    return compound_steps(0);
}
