magnitude(int a)
{
    int unused = 7;
    return a < 0 ? -a : a;
}

int main(void)
{
    return magnitude(-2147483647 - 1);
}
