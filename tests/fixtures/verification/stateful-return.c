int stateful_return(int x, int y)
{
    return (x = y, x ? x + 1 : y - 1);
}
