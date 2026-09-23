volatile unsigned calls;
unsigned next(void) { return ++calls; }

unsigned changed(volatile unsigned (*p)[4], int i, int j)
{
    unsigned a = p[i][j];
    p[i][j] = 9;
    return a + p[i][j];
}

unsigned swapped(volatile unsigned (*p)[4], int i, int j)
{
    return p[i][j] + p[j][i];
}

unsigned repeated(unsigned x, unsigned y)
{
    return (x + y) * (x + y);
}

int main(void)
{
    volatile unsigned p[4][4] = {{0}, {0, 0, 3}, {0, 5}};
    if (swapped(p, 1, 2) != 8 || changed(p, 1, 2) != 12)
        return 1;
    int i = 1;
    if (p[i++][2] != 9 || p[i++][1] != 5 || i != 3)
        return 2;
    if (repeated(5, 7) != 144 || repeated(~0u, 1) != 0)
        return 3;
    if ((next() + 1) + (next() + 1) != 5 || calls != 2)
        return 4;
    unsigned sum = 0;
    for (int k = 0; k != 4; ++k)
        sum += (k + 2u) * (k + 2u);
    return sum != 54;
}
