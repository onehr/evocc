int statement_return(int x, unsigned y)
{
    const unsigned a = y + 1U;
    int z = (int)a;
    {
        int t = x;
        z += t;
    }
    if (x)
        z += 2;
    else
        z -= 3;
    return z;
}
