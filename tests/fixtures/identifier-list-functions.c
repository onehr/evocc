int narrow(c, s, p)
    signed char c;
    unsigned short s;
    int *p;
{
    signed char *q = &c;
    return sizeof(c) != 1 || sizeof(s) != 2 || *q != -1 || s != 65535 || *p != 7;
}

int arrays(a, f)
    int a[3];
    int f(int);
{
    return sizeof(a) != sizeof(int *) || f(a[1]) != 9;
}

int add(int x) { return x + 2; }
int known(int);
int known(c)
    signed char c;
{
    return c;
}

int factorial(n)
    register int n;
{
    return n ? n * factorial(n - 1) : 1;
}

int prior_narrow(signed char);
int prior_narrow(c)
    signed char c;
{
    return sizeof(c) != 1 || c != -1;
}

int main(void)
{
    int a[3] = {0, 7, 0};
    int (*p)(int, int, int *) = narrow;
    return p(-1, 65535, &a[1]) || arrays(a, add) || known(255) != -1 || factorial(5) != 120 || prior_narrow(255);
}
