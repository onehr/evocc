extern void constant_failed(void) __attribute__((error("constant index is out of range")));
static __attribute__((always_inline)) inline unsigned checked_index(unsigned index)
{
    if (index >= 1491)
        constant_failed();
    return index;
}
static __attribute__((noinline)) unsigned map(int *p, int index)
{
    ++*p;
    return checked_index(index) + *p;
}
int main(void)
{
    int value = 0;
    unsigned a = map(&value, 3), b = map(&value, 3);
    return value != 2 || a != 4 || b != 5;
}
