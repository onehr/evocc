void count_primes(int limit)
{
    int n;
    int t;
    int c;
    int p;

    c = 0;
    n = 2;
    while (n < limit) {
        t = 2;
        p = 1;
        while (t * t <= n) {
            if (n % t == 0)
                p = 0;
            t++;
        }
        n++;
        if (p)
            c++;
    }
    return c;
}

int main(void)
{
    return count_primes(20);
}
